//! Criterion benchmark: end-to-end engine throughput over the loopback transport.
//!
//! Isolates the orchestration + HDR-aggregation overhead from real network cost
//! by using the in-process `LoopbackConnector`, so regressions in the engine
//! hot path are visible independent of QUIC/socket noise.

use std::sync::Arc;

use criterion::{criterion_group, criterion_main, Criterion};
use pprof::criterion::{Output, PProfProfiler};

use quicforge_core::{BenchDeps, BenchEngine, EngineConfig};
use quicforge_infra::{BroadcastEventSink, LoopbackConnector, MemoryRunRepository, SystemClock};
use quicforge_types::prelude::*;

fn loopback_engine() -> BenchEngine {
    let events = Arc::new(BroadcastEventSink::default());
    let deps = BenchDeps {
        connector: Arc::new(LoopbackConnector::new()),
        repo: Arc::new(MemoryRunRepository::new()),
        events,
        clock: Arc::new(SystemClock),
    };
    BenchEngine::new(deps, EngineConfig::default())
}

fn bench_loopback(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let engine = loopback_engine();
    let params = BenchParams::new(
        "127.0.0.1:9000".parse().unwrap(),
        ConnectionCount::new(4).unwrap(),
        RequestCount::new(256).unwrap(),
        PayloadSize::new(1232).unwrap(),
    );

    c.bench_function("loopback_4x256_round_trips", |b| {
        b.iter(|| {
            rt.block_on(async { engine.run(params).await.unwrap() });
        });
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().with_profiler(PProfProfiler::new(1_000, Output::Flamegraph(None)));
    targets = bench_loopback
}
criterion_main!(benches);
