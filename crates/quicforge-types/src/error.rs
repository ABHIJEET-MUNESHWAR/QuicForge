//! Validation errors for constructing domain value objects.

use thiserror::Error;

/// Raised when a benchmark parameter violates its invariant.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum InvalidParam {
    /// Payload size outside the accepted byte range.
    #[error("payload size {0} bytes out of range (1..=1048576)")]
    PayloadSize(u32),
    /// Connection count was zero.
    #[error("connection count must be non-zero")]
    ZeroConnections,
    /// Request count was zero.
    #[error("requests-per-connection must be non-zero")]
    ZeroRequests,
}
