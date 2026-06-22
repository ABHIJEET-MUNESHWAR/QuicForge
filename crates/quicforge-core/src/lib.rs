//! `quicforge-core` — domain core: ports, the benchmark engine, events, and
//! latency aggregation. No web or database dependencies (hexagonal inner ring).
#![forbid(unsafe_code)]

pub mod config;
pub mod engine;
pub mod error;
pub mod events;
pub mod histo;
pub mod ports;

pub use config::EngineConfig;
pub use engine::{BenchDeps, BenchEngine};
pub use error::{CoreError, PortError};
pub use events::RunEvent;
pub use ports::{Clock, EventSink, QuicConnection, QuicConnector, RunEventStream, RunRepository};

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use chrono::{DateTime, Utc};

    use quicforge_types::prelude::*;

    use super::ports::{MockQuicConnection, MockQuicConnector};
    use super::*;

    struct FakeClock;
    impl Clock for FakeClock {
        fn now(&self) -> DateTime<Utc> {
            Utc::now()
        }
    }

    #[derive(Default)]
    struct FakeRepo {
        rows: Mutex<Vec<RunSummary>>,
    }

    #[async_trait]
    impl RunRepository for FakeRepo {
        async fn save(&self, summary: &RunSummary) -> Result<(), PortError> {
            self.rows.lock().unwrap().push(summary.clone());
            Ok(())
        }
        async fn update(&self, summary: &RunSummary) -> Result<(), PortError> {
            self.rows.lock().unwrap().push(summary.clone());
            Ok(())
        }
        async fn get(&self, id: RunId) -> Result<Option<RunSummary>, PortError> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find(|s| s.id == id)
                .cloned())
        }
        async fn list_recent(&self, limit: usize) -> Result<Vec<RunSummary>, PortError> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .iter()
                .rev()
                .take(limit)
                .cloned()
                .collect())
        }
    }

    #[derive(Default)]
    struct FakeEvents {
        events: Mutex<Vec<RunEvent>>,
    }

    #[async_trait]
    impl EventSink for FakeEvents {
        async fn publish(&self, event: RunEvent) {
            self.events.lock().unwrap().push(event);
        }
    }

    fn always_ok_connector() -> Arc<dyn QuicConnector> {
        let mut connector = MockQuicConnector::new();
        connector.expect_connect().returning(|_| {
            let mut conn = MockQuicConnection::new();
            conn.expect_round_trip().returning(|_| Ok(()));
            conn.expect_close().returning(|| ());
            Ok(Arc::new(conn) as Arc<dyn QuicConnection>)
        });
        Arc::new(connector)
    }

    fn failing_connector() -> Arc<dyn QuicConnector> {
        let mut connector = MockQuicConnector::new();
        connector
            .expect_connect()
            .returning(|_| Err(PortError::Timeout));
        Arc::new(connector)
    }

    fn params(connections: u32, requests: u64) -> BenchParams {
        BenchParams::new(
            "127.0.0.1:9000".parse().unwrap(),
            ConnectionCount::new(connections).unwrap(),
            RequestCount::new(requests).unwrap(),
            PayloadSize::new(128).unwrap(),
        )
    }

    fn engine_with(connector: Arc<dyn QuicConnector>) -> (BenchEngine, Arc<FakeEvents>) {
        let events = Arc::new(FakeEvents::default());
        let deps = BenchDeps {
            connector,
            repo: Arc::new(FakeRepo::default()),
            events: events.clone(),
            clock: Arc::new(FakeClock),
        };
        (BenchEngine::new(deps, EngineConfig::default()), events)
    }

    #[tokio::test]
    async fn run_completes_and_aggregates_all_samples() {
        let (engine, events) = engine_with(always_ok_connector());
        let summary = engine.run(params(2, 3)).await.unwrap();

        assert_eq!(summary.status, RunStatus::Completed);
        let stats = summary.stats.expect("stats present");
        assert_eq!(stats.samples, 6);
        assert!(summary.throughput.is_some());

        let kinds: Vec<_> = events
            .events
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.kind())
            .collect();
        assert_eq!(kinds.first(), Some(&"started"));
        assert!(kinds.contains(&"completed"));
    }

    #[tokio::test]
    async fn connect_failure_marks_run_failed() {
        let config = EngineConfig {
            connect_retries: 1,
            ..Default::default()
        };
        let events = Arc::new(FakeEvents::default());
        let deps = BenchDeps {
            connector: failing_connector(),
            repo: Arc::new(FakeRepo::default()),
            events: events.clone(),
            clock: Arc::new(FakeClock),
        };
        let engine = BenchEngine::new(deps, config);

        let summary = engine.run(params(1, 5)).await.unwrap();
        assert_eq!(
            summary.status,
            RunStatus::Failed {
                reason: FailureReason::ConnectTimeout
            }
        );
    }

    #[tokio::test]
    async fn get_and_recent_round_trip() {
        let (engine, _events) = engine_with(always_ok_connector());
        let summary = engine.run(params(1, 2)).await.unwrap();
        let fetched = engine.get(summary.id).await.unwrap().unwrap();
        assert_eq!(fetched.id, summary.id);
        let recent = engine.recent(10).await.unwrap();
        assert!(recent.iter().any(|s| s.id == summary.id));
    }
}
