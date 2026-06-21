//! Real QUIC client + echo server (feature `quic`) on `quinn` + `rustls`.
//!
//! The client mirrors a TPU-style flow: establish a QUIC connection, then issue
//! many request/response round-trips on fresh bidirectional streams. The echo
//! server is a self-contained target for the latency lab. Both endpoints'
//! UDP sockets are size-tuned via `socket2` to reflect low-latency deployments.

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use quinn::{Endpoint, EndpointConfig, ServerConfig, TokioRuntime};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use socket2::{Domain, Protocol, Socket, Type};

use quicforge_core::error::PortError;
use quicforge_core::ports::{QuicConnection, QuicConnector};

/// ALPN advertised by the lab's QUIC echo protocol.
const ECHO_ALPN: &[u8] = b"quicforge-echo";
/// Server name presented during TLS (ignored by the verifier).
const SERVER_NAME: &str = "quicforge";
/// Default UDP socket buffer size (4 MiB) for both directions.
const SOCKET_BUFFER_BYTES: usize = 4 * 1024 * 1024;
/// Upper bound on a single echo response (matches the max payload).
const MAX_ECHO_BYTES: usize = 1 << 20;

fn transport_err(ctx: &str, e: impl std::fmt::Display) -> PortError {
    PortError::Transport(format!("{ctx}: {e}"))
}

/// Build a UDP socket with enlarged send/recv buffers bound to `addr`.
///
/// Buffer sizing is best-effort: the kernel clamps to `net.core.{r,w}mem_max`.
fn tuned_udp_socket(addr: SocketAddr) -> Result<std::net::UdpSocket, PortError> {
    let domain = if addr.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };
    let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))
        .map_err(|e| transport_err("socket", e))?;
    let _ = socket.set_recv_buffer_size(SOCKET_BUFFER_BYTES);
    let _ = socket.set_send_buffer_size(SOCKET_BUFFER_BYTES);
    socket
        .set_nonblocking(true)
        .map_err(|e| transport_err("nonblocking", e))?;
    socket
        .bind(&addr.into())
        .map_err(|e| transport_err("bind", e))?;
    Ok(socket.into())
}

/// A reusable QUIC client endpoint.
#[derive(Debug, Clone)]
pub struct QuinnConnector {
    endpoint: Endpoint,
}

impl QuinnConnector {
    /// Build a client bound to an ephemeral local UDP port.
    pub fn new() -> Result<Self, PortError> {
        Self::bound("0.0.0.0:0".parse().expect("valid bind addr"))
    }

    /// Build a client bound to a specific local address.
    pub fn bound(bind: SocketAddr) -> Result<Self, PortError> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut crypto = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| transport_err("rustls versions", e))?
            .dangerous()
            .with_custom_certificate_verifier(SkipServerVerification::new())
            .with_no_client_auth();
        crypto.alpn_protocols = vec![ECHO_ALPN.to_vec()];

        let quic_crypto = QuicClientConfig::try_from(crypto)
            .map_err(|e| transport_err("quic client crypto", e))?;
        let client_config = quinn::ClientConfig::new(Arc::new(quic_crypto));

        let socket = tuned_udp_socket(bind)?;
        let mut endpoint = Endpoint::new(
            EndpointConfig::default(),
            None,
            socket,
            Arc::new(TokioRuntime),
        )
        .map_err(|e| transport_err("client endpoint", e))?;
        endpoint.set_default_client_config(client_config);

        Ok(Self { endpoint })
    }
}

#[async_trait]
impl QuicConnector for QuinnConnector {
    async fn connect(&self, target: SocketAddr) -> Result<Arc<dyn QuicConnection>, PortError> {
        let conn = self
            .endpoint
            .connect(target, SERVER_NAME)
            .map_err(|e| transport_err("connect setup", e))?
            .await
            .map_err(|e| transport_err("connect", e))?;
        Ok(Arc::new(QuinnConnection { conn }))
    }
}

/// An established QUIC connection that issues echo round-trips.
#[derive(Debug)]
struct QuinnConnection {
    conn: quinn::Connection,
}

#[async_trait]
impl QuicConnection for QuinnConnection {
    async fn round_trip(&self, payload: &[u8]) -> Result<(), PortError> {
        let (mut send, mut recv) = self
            .conn
            .open_bi()
            .await
            .map_err(|e| transport_err("open_bi", e))?;
        send.write_all(payload)
            .await
            .map_err(|e| transport_err("write", e))?;
        send.finish().map_err(|e| transport_err("finish", e))?;
        let echoed = recv
            .read_to_end(MAX_ECHO_BYTES)
            .await
            .map_err(|e| transport_err("read", e))?;
        if echoed.len() != payload.len() {
            return Err(PortError::Transport(format!(
                "echo length mismatch: sent {} got {}",
                payload.len(),
                echoed.len()
            )));
        }
        Ok(())
    }

    async fn close(&self) {
        self.conn.close(0u32.into(), b"done");
    }
}

/// A self-contained QUIC echo server used as the benchmark target.
#[derive(Debug)]
pub struct QuicEchoServer {
    endpoint: Endpoint,
    addr: SocketAddr,
}

impl QuicEchoServer {
    /// Start an echo server bound to `bind` (use port 0 for an ephemeral port).
    pub async fn start(bind: SocketAddr) -> Result<Self, PortError> {
        let (certs, key) = self_signed_cert()?;
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut server_crypto = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| transport_err("rustls versions", e))?
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| transport_err("server cert", e))?;
        server_crypto.alpn_protocols = vec![ECHO_ALPN.to_vec()];

        let quic_server = QuicServerConfig::try_from(server_crypto)
            .map_err(|e| transport_err("quic server crypto", e))?;
        let server_config = ServerConfig::with_crypto(Arc::new(quic_server));

        let socket = tuned_udp_socket(bind)?;
        let endpoint = Endpoint::new(
            EndpointConfig::default(),
            Some(server_config),
            socket,
            Arc::new(TokioRuntime),
        )
        .map_err(|e| transport_err("server endpoint", e))?;
        let addr = endpoint
            .local_addr()
            .map_err(|e| transport_err("local_addr", e))?;

        let accept_endpoint = endpoint.clone();
        tokio::spawn(async move { accept_loop(accept_endpoint).await });

        Ok(Self { endpoint, addr })
    }

    /// The address the server is listening on.
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Stop accepting and close existing connections.
    pub fn shutdown(&self) {
        self.endpoint.close(0u32.into(), b"shutdown");
    }
}

async fn accept_loop(endpoint: Endpoint) {
    while let Some(incoming) = endpoint.accept().await {
        tokio::spawn(async move {
            match incoming.await {
                Ok(conn) => serve_connection(conn).await,
                Err(e) => tracing::debug!(error = %e, "echo: handshake failed"),
            }
        });
    }
}

async fn serve_connection(conn: quinn::Connection) {
    // Loop ends when `accept_bi` errors (connection closed or lost).
    while let Ok((mut send, mut recv)) = conn.accept_bi().await {
        tokio::spawn(async move {
            if let Ok(data) = recv.read_to_end(MAX_ECHO_BYTES).await {
                let _ = send.write_all(&data).await;
                let _ = send.finish();
                let _ = send.stopped().await;
            }
        });
    }
}

/// Generate an ephemeral self-signed certificate for the echo server.
fn self_signed_cert() -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), PortError> {
    let cert =
        rcgen::generate_simple_self_signed(vec!["localhost".to_string(), SERVER_NAME.to_string()])
            .map_err(|e| transport_err("self-signed cert", e))?;
    let cert_der = cert.cert.der().clone();
    let key_der = PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());
    Ok((vec![cert_der], PrivateKeyDer::Pkcs8(key_der)))
}

/// Accept any server certificate (the lab uses ephemeral self-signed certs)
/// while still validating handshake signatures with the ring provider.
#[derive(Debug)]
struct SkipServerVerification(Arc<rustls::crypto::CryptoProvider>);

impl SkipServerVerification {
    fn new() -> Arc<Self> {
        Arc::new(Self(Arc::new(rustls::crypto::ring::default_provider())))
    }
}

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn echo_round_trip_works() {
        let server = QuicEchoServer::start("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        let addr = server.local_addr();

        let connector = QuinnConnector::new().unwrap();
        let conn = connector.connect(addr).await.unwrap();
        for _ in 0..5 {
            conn.round_trip(b"hello quic").await.unwrap();
        }
        conn.close().await;
        server.shutdown();
    }

    #[tokio::test]
    async fn engine_bench_against_echo_server() {
        use crate::{BroadcastEventSink, MemoryRunRepository, SystemClock};
        use quicforge_core::{BenchDeps, BenchEngine, EngineConfig};
        use quicforge_types::prelude::*;

        let server = QuicEchoServer::start("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        let addr = server.local_addr();

        let deps = BenchDeps {
            connector: Arc::new(QuinnConnector::new().unwrap()),
            repo: Arc::new(MemoryRunRepository::new()),
            events: Arc::new(BroadcastEventSink::default()),
            clock: Arc::new(SystemClock),
        };
        let engine = BenchEngine::new(deps, EngineConfig::default());
        let params = BenchParams::new(
            addr,
            ConnectionCount::new(2).unwrap(),
            RequestCount::new(10).unwrap(),
            PayloadSize::new(256).unwrap(),
        );

        let summary = engine.run(params).await.unwrap();
        assert_eq!(summary.status, RunStatus::Completed);
        assert_eq!(summary.stats.unwrap().samples, 20);
        server.shutdown();
    }
}
