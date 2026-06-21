//! Mutation root — the write side: start a benchmark run.

use std::net::SocketAddr;

use async_graphql::{Context, Object, Result};

use quicforge_types::prelude::*;

use crate::schema::ApiContext;
use crate::types::{BenchParamsInput, RunSummaryObject};

/// Map any domain error into a GraphQL error without leaking internals.
fn to_err<E: std::fmt::Display>(e: E) -> async_graphql::Error {
    async_graphql::Error::new(e.to_string())
}

/// Validate and convert wire input into validated domain parameters.
fn to_params(input: BenchParamsInput) -> Result<BenchParams> {
    let target: SocketAddr = input.target.parse().map_err(to_err)?;
    let connections = ConnectionCount::new(input.connections).map_err(to_err)?;
    let requests = RequestCount::new(input.requests_per_connection).map_err(to_err)?;
    let payload = PayloadSize::new(input.payload_bytes).map_err(to_err)?;
    Ok(BenchParams::new(target, connections, requests, payload))
}

/// Write entry points.
pub struct MutationRoot;

#[Object]
impl MutationRoot {
    /// Run a benchmark against `target` and return the terminal summary.
    ///
    /// Transport faults are folded into the summary (`state: failed`); progress
    /// is also streamed live via the `runProgress` subscription.
    async fn start_benchmark(
        &self,
        ctx: &Context<'_>,
        input: BenchParamsInput,
    ) -> Result<RunSummaryObject> {
        let api = ctx.data_unchecked::<ApiContext>();
        let params = to_params(input)?;
        let summary = api.engine.run(params).await.map_err(to_err)?;
        Ok(RunSummaryObject::from(&summary))
    }
}
