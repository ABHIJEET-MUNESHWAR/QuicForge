//! GraphQL DTOs and conversions from domain types.
//!
//! The domain (`quicforge-core`/`quicforge-types`) stays free of
//! `async-graphql`; this module is the anti-corruption layer mapping run
//! summaries and events onto the wire schema.

use async_graphql::{InputObject, SimpleObject};
use chrono::{DateTime, Utc};

use quicforge_core::events::RunEvent;
use quicforge_types::prelude::*;

/// Parameters for starting a benchmark run.
#[derive(InputObject)]
pub struct BenchParamsInput {
    /// Target QUIC echo server as `host:port`.
    pub target: String,
    /// Number of concurrent connections (>= 1).
    #[graphql(default = 1)]
    pub connections: u32,
    /// Request/response round-trips per connection (>= 1).
    #[graphql(default = 1000)]
    pub requests_per_connection: u64,
    /// Payload size in bytes (1..=1048576). Defaults to Solana's TPU max.
    #[graphql(default = 1232)]
    pub payload_bytes: u32,
}

/// Latency distribution (nanoseconds plus micro-second convenience fields).
#[derive(SimpleObject, Clone, Copy, Debug)]
pub struct LatencyStatsObject {
    pub samples: u64,
    pub min_ns: u64,
    pub mean_ns: u64,
    pub p50_ns: u64,
    pub p90_ns: u64,
    pub p99_ns: u64,
    pub p999_ns: u64,
    pub max_ns: u64,
    pub p50_micros: f64,
    pub p99_micros: f64,
    pub p999_micros: f64,
}

impl From<LatencyStats> for LatencyStatsObject {
    fn from(s: LatencyStats) -> Self {
        Self {
            samples: s.samples,
            min_ns: s.min_ns,
            mean_ns: s.mean_ns,
            p50_ns: s.p50_ns,
            p90_ns: s.p90_ns,
            p99_ns: s.p99_ns,
            p999_ns: s.p999_ns,
            max_ns: s.max_ns,
            p50_micros: s.p50_ns as f64 / 1_000.0,
            p99_micros: s.p99_ns as f64 / 1_000.0,
            p999_micros: s.p999_ns as f64 / 1_000.0,
        }
    }
}

/// Sustained throughput, including a derived megabit/s figure.
#[derive(SimpleObject, Clone, Copy, Debug)]
pub struct ThroughputObject {
    pub requests_per_sec: f64,
    pub bytes_per_sec: f64,
    pub megabits_per_sec: f64,
}

impl From<Throughput> for ThroughputObject {
    fn from(t: Throughput) -> Self {
        Self {
            requests_per_sec: t.requests_per_sec,
            bytes_per_sec: t.bytes_per_sec,
            megabits_per_sec: t.bytes_per_sec * 8.0 / 1_000_000.0,
        }
    }
}

/// A benchmark run summary as exposed over GraphQL.
#[derive(SimpleObject, Clone, Debug)]
pub struct RunSummaryObject {
    pub id: String,
    pub target: String,
    pub connections: u32,
    pub requests_per_connection: u64,
    pub payload_bytes: u32,
    pub total_requests: u64,
    /// `pending` / `running` / `completed` / `failed`.
    pub state: String,
    pub completed: Option<u64>,
    pub failure_reason: Option<String>,
    pub stats: Option<LatencyStatsObject>,
    pub throughput: Option<ThroughputObject>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl From<&RunSummary> for RunSummaryObject {
    fn from(r: &RunSummary) -> Self {
        let (state, completed, failure_reason) = match &r.status {
            RunStatus::Pending => ("pending".to_string(), None, None),
            RunStatus::Running { completed } => ("running".to_string(), Some(*completed), None),
            RunStatus::Completed => ("completed".to_string(), None, None),
            RunStatus::Failed { reason } => ("failed".to_string(), None, Some(reason.to_string())),
        };
        Self {
            id: r.id.as_uuid().to_string(),
            target: r.params.target.to_string(),
            connections: r.params.connections.get(),
            requests_per_connection: r.params.requests_per_connection.get(),
            payload_bytes: r.params.payload.get(),
            total_requests: r.params.total_requests(),
            state,
            completed,
            failure_reason,
            stats: r.stats.map(LatencyStatsObject::from),
            throughput: r.throughput.map(ThroughputObject::from),
            started_at: r.started_at,
            finished_at: r.finished_at,
        }
    }
}

/// A run lifecycle event delivered over the subscription (flattened union).
#[derive(SimpleObject, Clone, Debug)]
pub struct RunEventObject {
    /// `started` / `progress` / `completed` / `failed`.
    pub kind: String,
    pub run_id: String,
    pub total: Option<u64>,
    pub completed: Option<u64>,
    pub stats: Option<LatencyStatsObject>,
    pub throughput: Option<ThroughputObject>,
    pub failure_reason: Option<String>,
    pub at: DateTime<Utc>,
}

impl From<RunEvent> for RunEventObject {
    fn from(ev: RunEvent) -> Self {
        let base = |kind: &str, run_id: RunId, at: DateTime<Utc>| RunEventObject {
            kind: kind.to_string(),
            run_id: run_id.as_uuid().to_string(),
            total: None,
            completed: None,
            stats: None,
            throughput: None,
            failure_reason: None,
            at,
        };
        match ev {
            RunEvent::Started { id, total, at } => RunEventObject {
                total: Some(total),
                ..base("started", id, at)
            },
            RunEvent::Progress {
                id,
                completed,
                total,
                at,
            } => RunEventObject {
                total: Some(total),
                completed: Some(completed),
                ..base("progress", id, at)
            },
            RunEvent::Completed {
                id,
                stats,
                throughput,
                at,
            } => RunEventObject {
                stats: Some(stats.into()),
                throughput: Some(throughput.into()),
                ..base("completed", id, at)
            },
            RunEvent::Failed { id, reason, at } => RunEventObject {
                failure_reason: Some(reason.to_string()),
                ..base("failed", id, at)
            },
        }
    }
}
