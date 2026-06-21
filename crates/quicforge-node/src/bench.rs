//! CLI benchmark runner with an HDR latency report.

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::Context as _;

use quicforge_types::prelude::*;

use crate::config::BenchArgs;
use crate::startup::{build_connector, build_engine};

/// Execute a benchmark described by `args` and print a latency report.
pub async fn run_bench(args: BenchArgs) -> anyhow::Result<()> {
    let sim_delay = Duration::from_micros(args.sim_delay_us);

    // Optionally spawn a local echo server to target (quic only). The guard
    // keeps the server alive for the duration of the run.
    #[cfg(feature = "quic")]
    let mut _echo_guard: Option<quicforge_infra::QuicEchoServer> = None;

    let target: SocketAddr = {
        #[cfg(feature = "quic")]
        {
            if args.spawn_echo && args.transport == crate::config::Transport::Quic {
                let server =
                    quicforge_infra::QuicEchoServer::start("127.0.0.1:0".parse().expect("bind"))
                        .await
                        .context("start echo server")?;
                let addr = server.local_addr();
                println!("spawned local QUIC echo server on {addr}");
                _echo_guard = Some(server);
                addr
            } else {
                args.target.parse().context("parse --target")?
            }
        }
        #[cfg(not(feature = "quic"))]
        {
            if args.spawn_echo {
                anyhow::bail!("--spawn-echo requires building with `--features quic`");
            }
            args.target.parse().context("parse --target")?
        }
    };

    let connector = build_connector(args.transport, sim_delay)?;
    let (engine, _events) = build_engine(connector);

    let params = BenchParams::new(
        target,
        ConnectionCount::new(args.connections).context("--connections")?,
        RequestCount::new(args.requests).context("--requests")?,
        PayloadSize::new(args.payload).context("--payload")?,
    );

    println!(
        "QuicForge benchmark — {:?} transport\n  target: {}\n  {} connections x {} requests ({} total), {}-byte payload\n",
        args.transport,
        target,
        args.connections,
        args.requests,
        params.total_requests(),
        args.payload,
    );

    let summary = engine.run(params).await.context("run benchmark")?;
    print_report(&summary);
    Ok(())
}

/// Print a human-readable latency + throughput report.
fn print_report(summary: &RunSummary) {
    match &summary.status {
        RunStatus::Completed => {}
        RunStatus::Failed { reason } => {
            println!("benchmark FAILED: {reason}");
            return;
        }
        other => {
            println!("benchmark ended in unexpected state: {other:?}");
            return;
        }
    }

    let Some(stats) = summary.stats else {
        println!("no samples recorded");
        return;
    };
    let us = |ns: u64| ns as f64 / 1_000.0;

    println!("latency (microseconds):");
    println!("  samples : {}", stats.samples);
    println!("  min     : {:>12.3}", us(stats.min_ns));
    println!("  mean    : {:>12.3}", us(stats.mean_ns));
    println!("  p50     : {:>12.3}", us(stats.p50_ns));
    println!("  p90     : {:>12.3}", us(stats.p90_ns));
    println!("  p99     : {:>12.3}", us(stats.p99_ns));
    println!("  p99.9   : {:>12.3}", us(stats.p999_ns));
    println!("  max     : {:>12.3}", us(stats.max_ns));

    if let Some(t) = summary.throughput {
        println!("\nthroughput:");
        println!("  {:>12.1} req/s", t.requests_per_sec);
        println!("  {:>12.3} Mbit/s", t.bytes_per_sec * 8.0 / 1_000_000.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn loopback_bench_runs() {
        let args = BenchArgs {
            connections: 2,
            requests: 5,
            payload: 128,
            ..BenchArgs::default()
        };
        run_bench(args).await.unwrap();
    }
}
