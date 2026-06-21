//! Benchmark parameters — the validated knobs of a latency run.

use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use crate::error::InvalidParam;

/// Payload size in bytes for each request (`1..=1 MiB`).
///
/// Defaults to **1232 bytes** — Solana's maximum TPU packet size — so the lab
/// mirrors real transaction-ingress framing by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PayloadSize(u32);

impl PayloadSize {
    /// Smallest accepted payload.
    pub const MIN: u32 = 1;
    /// Largest accepted payload (1 MiB).
    pub const MAX: u32 = 1 << 20;

    /// Construct a payload size, validating the byte range.
    pub fn new(bytes: u32) -> Result<Self, InvalidParam> {
        if (Self::MIN..=Self::MAX).contains(&bytes) {
            Ok(Self(bytes))
        } else {
            Err(InvalidParam::PayloadSize(bytes))
        }
    }

    /// The size in bytes.
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl Default for PayloadSize {
    fn default() -> Self {
        Self(1232)
    }
}

/// Number of concurrent QUIC connections (non-zero).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ConnectionCount(u32);

impl ConnectionCount {
    /// Construct a non-zero connection count.
    pub fn new(n: u32) -> Result<Self, InvalidParam> {
        if n == 0 {
            Err(InvalidParam::ZeroConnections)
        } else {
            Ok(Self(n))
        }
    }

    /// The count.
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl Default for ConnectionCount {
    fn default() -> Self {
        Self(1)
    }
}

/// Number of request/response round-trips per connection (non-zero).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RequestCount(u64);

impl RequestCount {
    /// Construct a non-zero request count.
    pub fn new(n: u64) -> Result<Self, InvalidParam> {
        if n == 0 {
            Err(InvalidParam::ZeroRequests)
        } else {
            Ok(Self(n))
        }
    }

    /// The count.
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl Default for RequestCount {
    fn default() -> Self {
        Self(1000)
    }
}

/// The full, validated parameter set for a benchmark run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchParams {
    /// QUIC server endpoint to drive load against.
    pub target: SocketAddr,
    /// Concurrent connections.
    pub connections: ConnectionCount,
    /// Round-trips per connection.
    pub requests_per_connection: RequestCount,
    /// Bytes per request.
    pub payload: PayloadSize,
}

impl BenchParams {
    /// Assemble parameters for a target endpoint.
    pub fn new(
        target: SocketAddr,
        connections: ConnectionCount,
        requests_per_connection: RequestCount,
        payload: PayloadSize,
    ) -> Self {
        Self {
            target,
            connections,
            requests_per_connection,
            payload,
        }
    }

    /// Total round-trips across all connections.
    pub fn total_requests(&self) -> u64 {
        self.connections.get() as u64 * self.requests_per_connection.get()
    }
}
