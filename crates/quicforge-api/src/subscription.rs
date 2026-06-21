//! Subscription root — the live read side over the run-event stream.

use async_graphql::{Context, Subscription};
use futures::{Stream, StreamExt};

use crate::schema::ApiContext;
use crate::types::RunEventObject;

/// Streaming entry points.
pub struct SubscriptionRoot;

#[Subscription]
impl SubscriptionRoot {
    /// Live stream of run lifecycle events (started → progress → completed/failed).
    ///
    /// Each subscriber gets an independent broadcast receiver; a slow consumer
    /// only drops its own backlog and never stalls the engine.
    async fn run_progress(
        &self,
        ctx: &Context<'_>,
    ) -> impl Stream<Item = RunEventObject> + 'static {
        ctx.data_unchecked::<ApiContext>()
            .events
            .subscribe()
            .map(RunEventObject::from)
    }
}
