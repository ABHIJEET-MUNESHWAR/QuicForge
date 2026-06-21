//! Domain events emitted over the run lifecycle.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use quicforge_types::prelude::*;

/// Lifecycle event for a benchmark run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum RunEvent {
    /// A run was accepted and started.
    Started {
        /// Run id.
        id: RunId,
        /// Total round-trips planned.
        total: u64,
        /// Timestamp.
        at: DateTime<Utc>,
    },
    /// Incremental progress (emitted as connections finish).
    Progress {
        /// Run id.
        id: RunId,
        /// Round-trips completed so far.
        completed: u64,
        /// Total round-trips planned.
        total: u64,
        /// Timestamp.
        at: DateTime<Utc>,
    },
    /// The run finished successfully.
    Completed {
        /// Run id.
        id: RunId,
        /// Final latency distribution.
        stats: LatencyStats,
        /// Sustained throughput.
        throughput: Throughput,
        /// Timestamp.
        at: DateTime<Utc>,
    },
    /// The run terminated with a failure.
    Failed {
        /// Run id.
        id: RunId,
        /// Why it failed.
        reason: FailureReason,
        /// Timestamp.
        at: DateTime<Utc>,
    },
}

impl RunEvent {
    /// The run this event belongs to.
    pub fn id(&self) -> RunId {
        match self {
            RunEvent::Started { id, .. }
            | RunEvent::Progress { id, .. }
            | RunEvent::Completed { id, .. }
            | RunEvent::Failed { id, .. } => *id,
        }
    }

    /// A stable discriminant string.
    pub fn kind(&self) -> &'static str {
        match self {
            RunEvent::Started { .. } => "started",
            RunEvent::Progress { .. } => "progress",
            RunEvent::Completed { .. } => "completed",
            RunEvent::Failed { .. } => "failed",
        }
    }
}
