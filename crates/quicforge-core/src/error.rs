//! Core error types.

use thiserror::Error;

/// Failure at an adapter boundary (transport, repository, …).
#[derive(Debug, Error, Clone)]
pub enum PortError {
    /// A transport-level error (connection refused, reset, …).
    #[error("transport error: {0}")]
    Transport(String),
    /// The operation exceeded its deadline.
    #[error("operation timed out")]
    Timeout,
    /// A requested entity was not found.
    #[error("not found")]
    NotFound,
    /// The dependency is temporarily unavailable.
    #[error("unavailable: {0}")]
    Unavailable(String),
}

impl PortError {
    /// Whether retrying the operation could plausibly succeed.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            PortError::Transport(_) | PortError::Timeout | PortError::Unavailable(_)
        )
    }
}

/// Domain-level error surfaced by the engine.
#[derive(Debug, Error, Clone)]
pub enum CoreError {
    /// The request was structurally invalid.
    #[error("invalid request: {0}")]
    Invalid(String),
    /// An underlying port failed.
    #[error(transparent)]
    Port(#[from] PortError),
}
