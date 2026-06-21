//! Engine configuration.

use std::time::Duration;

/// Tunables for the [`BenchEngine`](crate::engine::BenchEngine).
#[derive(Debug, Clone, Copy)]
pub struct EngineConfig {
    /// Maximum time to establish a QUIC connection.
    pub connect_timeout: Duration,
    /// Maximum time for a single request/response round-trip.
    pub request_timeout: Duration,
    /// How many times to attempt connection establishment.
    pub connect_retries: u32,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(2),
            connect_retries: 3,
        }
    }
}
