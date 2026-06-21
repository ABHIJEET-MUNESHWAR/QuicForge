//! Telemetry wiring: structured `tracing` + a Prometheus metrics recorder.

use anyhow::Context;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

/// Install the global tracing subscriber.
///
/// Honors `RUST_LOG`; defaults to `info`. Set `json = true` for machine-readable
/// logs (one JSON object per line) suitable for log shippers.
pub fn init_tracing(json: bool) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let registry = tracing_subscriber::registry().with(filter);
    if json {
        registry
            .with(tracing_subscriber::fmt::layer().json().flatten_event(true))
            .init();
    } else {
        registry.with(tracing_subscriber::fmt::layer()).init();
    }
}

/// Install the global Prometheus recorder and return a render handle for `/metrics`.
pub fn init_metrics() -> anyhow::Result<PrometheusHandle> {
    PrometheusBuilder::new()
        .install_recorder()
        .context("install Prometheus recorder")
}
