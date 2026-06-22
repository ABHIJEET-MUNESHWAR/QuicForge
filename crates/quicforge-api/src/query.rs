//! Query root — the read side (CQRS).

use async_graphql::{Context, Object, Result};

use quicforge_types::prelude::*;

use crate::schema::ApiContext;
use crate::types::RunSummaryObject;

/// Map any domain error into a GraphQL error without leaking internals.
fn to_err<E: std::fmt::Display>(e: E) -> async_graphql::Error {
    async_graphql::Error::new(e.to_string())
}

/// Read-only entry points.
pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// The running API version (also serves as a liveness probe).
    async fn api_version(&self) -> &str {
        env!("CARGO_PKG_VERSION")
    }

    /// A single benchmark run by its id.
    async fn run(&self, ctx: &Context<'_>, id: String) -> Result<Option<RunSummaryObject>> {
        let api = ctx.data_unchecked::<ApiContext>();
        let uuid = uuid::Uuid::parse_str(&id).map_err(to_err)?;
        let summary = api.engine.get(RunId::from_uuid(uuid)).await.map_err(to_err)?;
        Ok(summary.as_ref().map(RunSummaryObject::from))
    }

    /// The most recent runs, newest first (bounded).
    async fn runs(
        &self,
        ctx: &Context<'_>,
        #[graphql(default = 20)] limit: i32,
    ) -> Result<Vec<RunSummaryObject>> {
        let api = ctx.data_unchecked::<ApiContext>();
        let limit = limit.clamp(1, 200) as usize;
        let summaries = api.engine.recent(limit).await.map_err(to_err)?;
        Ok(summaries.iter().map(RunSummaryObject::from).collect())
    }
}
