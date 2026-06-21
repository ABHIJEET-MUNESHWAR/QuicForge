//! `quicforge-api` — the GraphQL surface (guideline #7: GraphQL over REST).
//!
//! Defines the schema (queries, a mutation, and a live subscription) plus the
//! [`ApiContext`] the composition root injects. Transport (axum + WebSocket)
//! lives in `quicforge-node`; this crate is transport-agnostic so the schema can
//! be unit-tested in-process against the simulated infra adapters.
#![forbid(unsafe_code)]

pub mod mutation;
pub mod query;
pub mod schema;
pub mod subscription;
pub mod types;

pub use mutation::MutationRoot;
pub use query::QueryRoot;
pub use schema::{build_schema, ApiContext, QuicForgeSchema};
pub use subscription::SubscriptionRoot;
pub use types::{
    BenchParamsInput, LatencyStatsObject, RunEventObject, RunSummaryObject, ThroughputObject,
};

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use quicforge_core::ports::RunEventStream;
    use quicforge_core::{BenchDeps, BenchEngine, EngineConfig};
    use quicforge_infra::{
        BroadcastEventSink, LoopbackConnector, MemoryRunRepository, SystemClock,
    };

    use super::*;

    fn test_schema() -> QuicForgeSchema {
        let events = Arc::new(BroadcastEventSink::default());
        let deps = BenchDeps {
            connector: Arc::new(LoopbackConnector::new()),
            repo: Arc::new(MemoryRunRepository::new()),
            events: events.clone(),
            clock: Arc::new(SystemClock),
        };
        let engine = BenchEngine::new(deps, EngineConfig::default());
        let events: Arc<dyn RunEventStream> = events;
        build_schema(ApiContext::new(engine, events))
    }

    #[tokio::test]
    async fn api_version_query_works() {
        let schema = test_schema();
        let res = schema.execute("{ apiVersion }").await;
        assert!(res.errors.is_empty(), "{:?}", res.errors);
    }

    #[tokio::test]
    async fn start_benchmark_completes_and_lists() {
        let schema = test_schema();
        let mutation = r#"
            mutation {
                startBenchmark(input: {
                    target: "127.0.0.1:9000",
                    connections: 2,
                    requestsPerConnection: 5,
                    payloadBytes: 256
                }) {
                    state
                    totalRequests
                    stats { samples }
                    throughput { requestsPerSec }
                }
            }
        "#;
        let res = schema.execute(mutation).await;
        assert!(res.errors.is_empty(), "{:?}", res.errors);
        let data = res.data.into_json().unwrap();
        assert_eq!(data["startBenchmark"]["state"], "completed");
        assert_eq!(data["startBenchmark"]["totalRequests"], 10);
        assert_eq!(data["startBenchmark"]["stats"]["samples"], 10);

        let res = schema.execute("{ runs { state } }").await;
        assert!(res.errors.is_empty(), "{:?}", res.errors);
        let data = res.data.into_json().unwrap();
        assert_eq!(data["runs"][0]["state"], "completed");
    }

    #[tokio::test]
    async fn invalid_target_is_rejected() {
        let schema = test_schema();
        let mutation = r#"
            mutation {
                startBenchmark(input: { target: "not-an-addr", connections: 1, requestsPerConnection: 1 }) {
                    state
                }
            }
        "#;
        let res = schema.execute(mutation).await;
        assert!(!res.errors.is_empty(), "invalid target must error");
    }
}
