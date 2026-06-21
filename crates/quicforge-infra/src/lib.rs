//! `quicforge-infra` — concrete adapters implementing the `quicforge-core` ports.
//!
//! - [`SystemClock`] — OS wall-clock.
//! - [`MemoryRunRepository`] — in-memory run store.
//! - [`BroadcastEventSink`] — fan-out event bus (sink + subscribable stream).
//! - [`LoopbackConnector`] — in-process QUIC simulator (default feature).
//! - `quic::{QuinnConnector, QuicEchoServer}` — real QUIC (feature `quic`).
#![forbid(unsafe_code)]

pub mod clock;
pub mod events;
pub mod repo;
pub mod sim;

#[cfg(feature = "quic")]
pub mod quic;

pub use clock::SystemClock;
pub use events::BroadcastEventSink;
pub use repo::MemoryRunRepository;
pub use sim::LoopbackConnector;

#[cfg(feature = "quic")]
pub use quic::{QuicEchoServer, QuinnConnector};

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures::StreamExt;

    use quicforge_core::ports::RunEventStream;
    use quicforge_core::{BenchDeps, BenchEngine, EngineConfig};
    use quicforge_types::prelude::*;

    use super::*;

    #[tokio::test]
    async fn sim_bench_runs_end_to_end() {
        let events = Arc::new(BroadcastEventSink::default());
        let mut stream = events.subscribe();

        let deps = BenchDeps {
            connector: Arc::new(LoopbackConnector::new()),
            repo: Arc::new(MemoryRunRepository::new()),
            events: events.clone(),
            clock: Arc::new(SystemClock),
        };
        let engine = BenchEngine::new(deps, EngineConfig::default());
        let params = BenchParams::new(
            "127.0.0.1:9000".parse().unwrap(),
            ConnectionCount::new(3).unwrap(),
            RequestCount::new(4).unwrap(),
            PayloadSize::default(),
        );

        let summary = engine.run(params).await.unwrap();
        assert_eq!(summary.status, RunStatus::Completed);
        assert_eq!(summary.stats.unwrap().samples, 12);

        // The subscriber, created before the run, captures the lifecycle.
        let first = stream.next().await.unwrap();
        assert_eq!(first.kind(), "started");
    }
}
