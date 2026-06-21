//! Schema assembly: the shared context and the depth/complexity-limited schema.

use std::sync::Arc;

use async_graphql::Schema;

use quicforge_core::ports::RunEventStream;
use quicforge_core::BenchEngine;

use crate::mutation::MutationRoot;
use crate::query::QueryRoot;
use crate::subscription::SubscriptionRoot;

/// Shared state injected into every resolver via [`async_graphql::Context`].
#[derive(Clone)]
pub struct ApiContext {
    /// The benchmark engine (write side + read model).
    pub engine: BenchEngine,
    /// Subscribable event source for the `runProgress` subscription.
    pub events: Arc<dyn RunEventStream>,
}

impl ApiContext {
    /// Assemble a context from the engine and event stream.
    pub fn new(engine: BenchEngine, events: Arc<dyn RunEventStream>) -> Self {
        Self { engine, events }
    }
}

/// The fully-typed schema for this service.
pub type QuicForgeSchema = Schema<QueryRoot, MutationRoot, SubscriptionRoot>;

/// Build the schema with the given context.
///
/// Depth and complexity limits cap the cost of any single query — a cheap,
/// always-on guard against pathological/abusive documents (DoS resilience).
pub fn build_schema(context: ApiContext) -> QuicForgeSchema {
    Schema::build(QueryRoot, MutationRoot, SubscriptionRoot)
        .data(context)
        .limit_depth(12)
        .limit_complexity(256)
        .finish()
}
