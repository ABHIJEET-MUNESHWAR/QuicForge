//! `quicforge-node` — composition root + CLI (`serve` / `bench`).
//!
//! Wires the `quicforge-infra` adapters into the `quicforge-core` engine, exposes
//! the `quicforge-api` GraphQL schema over axum (HTTP + WebSocket), and provides
//! a CLI benchmark harness. This is the only crate that depends on every layer.
#![forbid(unsafe_code)]

pub mod bench;
pub mod config;
pub mod startup;
pub mod telemetry;

pub use config::{BenchArgs, Cli, Command, ServeArgs, Transport};

/// Dispatch a parsed CLI to the matching subcommand.
pub async fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Command::Serve(args) => startup::run_server(args).await,
        Command::Bench(args) => bench::run_bench(args).await,
    }
}
