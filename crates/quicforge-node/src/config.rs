//! Runtime configuration: a `clap` CLI with `serve` and `bench` subcommands.

use clap::{Args, Parser, Subcommand, ValueEnum};

/// Transport backend used to drive round-trips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum Transport {
    /// In-process simulator (no network); deterministic, always available.
    #[default]
    Loopback,
    /// Real QUIC via `quinn` (requires the `quic` build feature).
    Quic,
}

/// QuicForge — a QUIC/kernel round-trip latency lab.
#[derive(Debug, Parser)]
#[command(
    name = "quicforge-node",
    version,
    about = "QUIC round-trip latency lab (GraphQL server + CLI benchmark)"
)]
pub struct Cli {
    /// Emit logs as JSON (recommended in production).
    #[arg(
        long,
        env = "QUICFORGE_LOG_JSON",
        global = true,
        default_value_t = false
    )]
    pub log_json: bool,

    /// Subcommand to run.
    #[command(subcommand)]
    pub command: Command,
}

/// Top-level subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Serve the GraphQL API (HTTP + WebSocket subscriptions).
    Serve(ServeArgs),
    /// Run a benchmark from the CLI and print an HDR latency report.
    Bench(BenchArgs),
}

/// Arguments for the `serve` subcommand.
#[derive(Debug, Clone, Args)]
pub struct ServeArgs {
    /// Interface to bind (host name or IP).
    #[arg(long, env = "QUICFORGE_HOST", default_value = "127.0.0.1")]
    pub host: String,

    /// TCP port to bind.
    #[arg(long, env = "QUICFORGE_PORT", default_value_t = 8080)]
    pub port: u16,

    /// Transport used for benchmarks started over the API.
    #[arg(long, value_enum, env = "QUICFORGE_TRANSPORT", default_value_t = Transport::Loopback)]
    pub transport: Transport,
}

impl Default for ServeArgs {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            transport: Transport::Loopback,
        }
    }
}

/// Arguments for the `bench` subcommand.
#[derive(Debug, Clone, Args)]
pub struct BenchArgs {
    /// Target QUIC echo server as `host:port` (ignored by loopback).
    #[arg(long, default_value = "127.0.0.1:0")]
    pub target: String,

    /// Number of concurrent connections.
    #[arg(long, default_value_t = 1)]
    pub connections: u32,

    /// Round-trips per connection.
    #[arg(long, default_value_t = 1000)]
    pub requests: u64,

    /// Payload bytes per request.
    #[arg(long, default_value_t = 1232)]
    pub payload: u32,

    /// Transport backend.
    #[arg(long, value_enum, default_value_t = Transport::Loopback)]
    pub transport: Transport,

    /// (quic) Spawn a local echo server and benchmark against it.
    #[arg(long, default_value_t = false)]
    pub spawn_echo: bool,

    /// (loopback) Simulated per-round-trip delay in microseconds.
    #[arg(long, default_value_t = 0)]
    pub sim_delay_us: u64,
}

impl Default for BenchArgs {
    fn default() -> Self {
        Self {
            target: "127.0.0.1:0".to_string(),
            connections: 1,
            requests: 1000,
            payload: 1232,
            transport: Transport::Loopback,
            spawn_echo: false,
            sim_delay_us: 0,
        }
    }
}
