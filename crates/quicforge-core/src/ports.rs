//! Ports — the abstract boundaries the engine depends on (DIP).

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::stream::BoxStream;

use quicforge_types::prelude::*;

use crate::error::PortError;
use crate::events::RunEvent;

/// A source of wall-clock time (injected for deterministic tests).
#[cfg_attr(test, mockall::automock)]
pub trait Clock: Send + Sync {
    /// The current instant.
    fn now(&self) -> DateTime<Utc>;
}

/// An established QUIC connection capable of request/response round-trips.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait QuicConnection: Send + Sync {
    /// Send `payload` on a fresh stream and await the server's echo/ack.
    async fn round_trip(&self, payload: &[u8]) -> Result<(), PortError>;
    /// Close the connection.
    async fn close(&self);
}

/// Establishes QUIC connections to a target endpoint.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait QuicConnector: Send + Sync {
    /// Open a new connection to `target`.
    async fn connect(&self, target: SocketAddr) -> Result<Arc<dyn QuicConnection>, PortError>;
}

/// Persistence for run summaries (read model).
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait RunRepository: Send + Sync {
    /// Insert a new summary.
    async fn save(&self, summary: &RunSummary) -> Result<(), PortError>;
    /// Update an existing summary.
    async fn update(&self, summary: &RunSummary) -> Result<(), PortError>;
    /// Fetch a summary by id.
    async fn get(&self, id: RunId) -> Result<Option<RunSummary>, PortError>;
    /// List the most recent summaries, newest first.
    async fn list_recent(&self, limit: usize) -> Result<Vec<RunSummary>, PortError>;
}

/// Fan-out sink for [`RunEvent`]s.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait EventSink: Send + Sync {
    /// Publish an event to all subscribers.
    async fn publish(&self, event: RunEvent);
}

/// A subscribable source of [`RunEvent`]s (read side, used by the API).
pub trait RunEventStream: Send + Sync {
    /// Subscribe to all subsequently published events.
    fn subscribe(&self) -> BoxStream<'static, RunEvent>;
}
