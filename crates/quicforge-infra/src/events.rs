//! Broadcast-channel event bus implementing both the write port
//! ([`EventSink`]) and the read port ([`RunEventStream`]).

use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use quicforge_core::events::RunEvent;
use quicforge_core::ports::{EventSink, RunEventStream};

/// Fan-out event bus backed by a Tokio broadcast channel.
#[derive(Debug, Clone)]
pub struct BroadcastEventSink {
    tx: broadcast::Sender<RunEvent>,
}

impl BroadcastEventSink {
    /// Create a sink buffering up to `capacity` events per subscriber.
    pub fn new(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity.max(1));
        Self { tx }
    }

    /// Number of currently active subscribers.
    pub fn receiver_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

impl Default for BroadcastEventSink {
    fn default() -> Self {
        Self::new(1024)
    }
}

#[async_trait]
impl EventSink for BroadcastEventSink {
    async fn publish(&self, event: RunEvent) {
        // `send` errors only when there are no subscribers — not an error here.
        let _ = self.tx.send(event);
    }
}

impl RunEventStream for BroadcastEventSink {
    fn subscribe(&self) -> BoxStream<'static, RunEvent> {
        BroadcastStream::new(self.tx.subscribe())
            // Drop lagged markers; subscribers see a best-effort live feed.
            .filter_map(|res| async move { res.ok() })
            .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use quicforge_types::prelude::*;

    fn started() -> RunEvent {
        RunEvent::Started {
            id: RunId::generate(),
            total: 10,
            at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn subscriber_receives_published_events() {
        let sink = BroadcastEventSink::default();
        let mut stream = sink.subscribe();
        assert_eq!(sink.receiver_count(), 1);

        let event = started();
        sink.publish(event.clone()).await;

        let received = stream.next().await.unwrap();
        assert_eq!(received, event);
    }

    #[tokio::test]
    async fn publish_without_subscribers_is_ok() {
        let sink = BroadcastEventSink::new(4);
        sink.publish(started()).await; // must not panic
        assert_eq!(sink.receiver_count(), 0);
    }
}
