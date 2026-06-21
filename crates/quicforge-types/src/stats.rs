//! Latency samples and aggregated distribution statistics.

use serde::{Deserialize, Serialize};

/// A single round-trip latency sample, in nanoseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LatencyNs(u64);

impl LatencyNs {
    /// Wrap a nanosecond measurement.
    pub const fn new(ns: u64) -> Self {
        Self(ns)
    }

    /// The value in nanoseconds.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// The value in microseconds.
    pub fn as_micros_f64(self) -> f64 {
        self.0 as f64 / 1_000.0
    }

    /// The value in milliseconds.
    pub fn as_millis_f64(self) -> f64 {
        self.0 as f64 / 1_000_000.0
    }
}

/// Aggregated latency distribution for a completed run (all values in ns).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatencyStats {
    /// Number of recorded samples.
    pub samples: u64,
    /// Fastest round-trip.
    pub min_ns: u64,
    /// Arithmetic mean.
    pub mean_ns: u64,
    /// Median.
    pub p50_ns: u64,
    /// 90th percentile.
    pub p90_ns: u64,
    /// 99th percentile.
    pub p99_ns: u64,
    /// 99.9th percentile (tail latency).
    pub p999_ns: u64,
    /// Slowest round-trip.
    pub max_ns: u64,
}

/// Sustained throughput of a completed run.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Throughput {
    /// Completed round-trips per second.
    pub requests_per_sec: f64,
    /// Payload bytes moved per second (request direction).
    pub bytes_per_sec: f64,
}
