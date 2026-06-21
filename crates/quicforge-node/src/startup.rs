//! Composition root: wire adapters into the engine, build the schema, serve HTTP/WS.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use async_graphql::http::GraphiQLSource;
use async_graphql_axum::{GraphQLRequest, GraphQLResponse, GraphQLSubscription};
use axum::{
    extract::Extension,
    response::{Html, IntoResponse},
    routing::get,
    Json, Router,
};
use metrics_exporter_prometheus::PrometheusHandle;
use tower_http::trace::TraceLayer;

use quicforge_api::{build_schema, ApiContext, QuicForgeSchema};
use quicforge_core::ports::{QuicConnector, RunEventStream};
use quicforge_core::{BenchDeps, BenchEngine, EngineConfig};
use quicforge_infra::{BroadcastEventSink, LoopbackConnector, MemoryRunRepository, SystemClock};

use crate::config::{ServeArgs, Transport};

/// Build a connector for the requested transport.
///
/// Selecting a backend is a localized change here; the engine and API are
/// unaffected because both adapters satisfy the same `QuicConnector` port
/// (hexagonal architecture).
pub(crate) fn build_connector(
    transport: Transport,
    sim_delay: Duration,
) -> anyhow::Result<Arc<dyn QuicConnector>> {
    match transport {
        Transport::Loopback => {
            if sim_delay.is_zero() {
                Ok(Arc::new(LoopbackConnector::new()))
            } else {
                Ok(Arc::new(LoopbackConnector::with_delay(sim_delay)))
            }
        }
        #[cfg(feature = "quic")]
        Transport::Quic => Ok(Arc::new(quicforge_infra::QuinnConnector::new()?)),
        #[cfg(not(feature = "quic"))]
        Transport::Quic => {
            anyhow::bail!("quic transport requires building with `--features quic`")
        }
    }
}

/// Wire adapters into an engine, returning the engine and its event bus.
pub(crate) fn build_engine(
    connector: Arc<dyn QuicConnector>,
) -> (BenchEngine, Arc<BroadcastEventSink>) {
    let events = Arc::new(BroadcastEventSink::default());
    let deps = BenchDeps {
        connector,
        repo: Arc::new(MemoryRunRepository::new()),
        events: events.clone(),
        clock: Arc::new(SystemClock),
    };
    (BenchEngine::new(deps, EngineConfig::default()), events)
}

/// Build the GraphQL schema for the `serve` configuration.
pub fn build_schema_from_args(args: &ServeArgs) -> anyhow::Result<QuicForgeSchema> {
    let connector = build_connector(args.transport, Duration::ZERO)?;
    let (engine, events) = build_engine(connector);
    let events: Arc<dyn RunEventStream> = events;
    Ok(build_schema(ApiContext::new(engine, events)))
}

/// Assemble the axum application (routes + middleware). Pure and unit-testable.
pub fn build_app(schema: QuicForgeSchema, metrics: PrometheusHandle) -> Router {
    Router::new()
        .route("/graphql", get(graphiql).post(graphql_handler))
        .route_service("/graphql/ws", GraphQLSubscription::new(schema.clone()))
        .route("/health/live", get(health_live))
        .route("/health/ready", get(health_ready))
        .route("/metrics", get(metrics_handler))
        .layer(Extension(schema))
        .layer(Extension(metrics))
        .layer(TraceLayer::new_for_http())
}

/// Run the server until a shutdown signal (SIGTERM / Ctrl-C) arrives.
pub async fn run_server(args: ServeArgs) -> anyhow::Result<()> {
    let metrics = crate::telemetry::init_metrics()?;
    let schema = build_schema_from_args(&args)?;
    let app = build_app(schema, metrics);

    let listener = tokio::net::TcpListener::bind((args.host.as_str(), args.port))
        .await
        .with_context(|| format!("bind {}:{}", args.host, args.port))?;
    let addr = listener.local_addr().context("resolve local addr")?;
    tracing::info!(%addr, transport = ?args.transport, "quicforge-node listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server terminated unexpectedly")?;
    Ok(())
}

async fn graphql_handler(
    Extension(schema): Extension<QuicForgeSchema>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    metrics::counter!("quicforge_graphql_requests_total").increment(1);
    schema.execute(req.into_inner()).await.into()
}

async fn graphiql() -> impl IntoResponse {
    Html(
        GraphiQLSource::build()
            .endpoint("/graphql")
            .subscription_endpoint("/graphql/ws")
            .finish(),
    )
}

async fn health_live() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "live" }))
}

async fn health_ready() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ready" }))
}

async fn metrics_handler(Extension(handle): Extension<PrometheusHandle>) -> impl IntoResponse {
    handle.render()
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(e) => tracing::error!(error = %e, "failed to install SIGTERM handler"),
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("received Ctrl-C, shutting down"),
        _ = terminate => tracing::info!("received SIGTERM, shutting down"),
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use metrics_exporter_prometheus::PrometheusBuilder;
    use tower::ServiceExt;

    use super::*;

    fn test_app() -> Router {
        let schema = build_schema_from_args(&ServeArgs::default()).unwrap();
        let handle = PrometheusBuilder::new().build_recorder().handle();
        build_app(schema, handle)
    }

    #[tokio::test]
    async fn health_live_returns_ok() {
        let res = test_app()
            .oneshot(
                Request::builder()
                    .uri("/health/live")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn graphql_api_version_executes() {
        let res = test_app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/graphql")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"query":"{ apiVersion }"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(v["data"]["apiVersion"].is_string());
    }

    #[tokio::test]
    async fn metrics_endpoint_renders() {
        let res = test_app()
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }
}
