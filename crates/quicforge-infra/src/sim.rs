//! Loopback (in-process) QUIC simulator.
//!
//! Completes round-trips without touching the network, optionally after a fixed
//! delay. Lets the full hexagon run deterministically in tests and offline
//! benchmarks while the real `quic` adapter is reserved for live measurement.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use quicforge_core::error::PortError;
use quicforge_core::ports::{QuicConnection, QuicConnector};

/// A connector whose connections complete round-trips in-process.
#[derive(Debug, Clone, Default)]
pub struct LoopbackConnector {
    delay: Option<Duration>,
}

impl LoopbackConnector {
    /// A connector with instant round-trips.
    pub fn new() -> Self {
        Self::default()
    }

    /// A connector that sleeps `delay` on each round-trip (to model latency).
    pub fn with_delay(delay: Duration) -> Self {
        Self { delay: Some(delay) }
    }
}

#[async_trait]
impl QuicConnector for LoopbackConnector {
    async fn connect(&self, _target: SocketAddr) -> Result<Arc<dyn QuicConnection>, PortError> {
        Ok(Arc::new(LoopbackConnection { delay: self.delay }))
    }
}

#[derive(Debug)]
struct LoopbackConnection {
    delay: Option<Duration>,
}

#[async_trait]
impl QuicConnection for LoopbackConnection {
    async fn round_trip(&self, _payload: &[u8]) -> Result<(), PortError> {
        if let Some(delay) = self.delay {
            tokio::time::sleep(delay).await;
        }
        Ok(())
    }

    async fn close(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn loopback_round_trip_is_ok() {
        let connector = LoopbackConnector::new();
        let conn = connector
            .connect("127.0.0.1:9000".parse().unwrap())
            .await
            .unwrap();
        conn.round_trip(b"ping").await.unwrap();
        conn.close().await;
    }
}
