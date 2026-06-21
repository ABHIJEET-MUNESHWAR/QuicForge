//! Binary entry point for `quicforge-node`.

use clap::Parser;

use quicforge_node::config::Cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load `.env` if present (ignored when absent).
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();
    quicforge_node::telemetry::init_tracing(cli.log_json);
    quicforge_node::run(cli).await
}
