//! Run identity, lifecycle status, and the summary read model.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::params::BenchParams;
use crate::stats::{LatencyStats, Throughput};

/// Unique identifier for a benchmark run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(Uuid);

impl RunId {
    /// Generate a fresh random id.
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    /// Wrap an existing UUID.
    pub const fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    /// Borrow the inner UUID.
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Why a run failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureReason {
    /// The QUIC handshake did not complete within the timeout.
    ConnectTimeout,
    /// An established connection dropped mid-run.
    ConnectionLost,
    /// A socket/stream I/O error.
    Io,
    /// An unexpected internal error.
    Internal,
}

impl fmt::Display for FailureReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            FailureReason::ConnectTimeout => "connect_timeout",
            FailureReason::ConnectionLost => "connection_lost",
            FailureReason::Io => "io",
            FailureReason::Internal => "internal",
        };
        f.write_str(s)
    }
}

/// Lifecycle state of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RunStatus {
    /// Accepted, not yet started.
    Pending,
    /// In flight; `completed` round-trips so far.
    Running { completed: u64 },
    /// Finished successfully.
    Completed,
    /// Terminated with a failure.
    Failed { reason: FailureReason },
}

/// Immutable summary of a run (read model).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    /// Run id.
    pub id: RunId,
    /// Parameters used.
    pub params: BenchParams,
    /// Current lifecycle state.
    pub status: RunStatus,
    /// Latency distribution (present once samples exist).
    pub stats: Option<LatencyStats>,
    /// Throughput (present once completed).
    pub throughput: Option<Throughput>,
    /// When the run started.
    pub started_at: DateTime<Utc>,
    /// When the run reached a terminal state.
    pub finished_at: Option<DateTime<Utc>>,
}
