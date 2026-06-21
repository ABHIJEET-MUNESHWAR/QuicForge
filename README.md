# QuicForge

**A QUIC / kernel round-trip latency lab in Rust.**

QuicForge measures TPU-style QUIC round-trip latency the way a Solana validator's
transaction-processing-unit ingress does: many concurrent QUIC connections, small
datagram-sized payloads, and a tail-latency-first view of the results. It records
every round-trip into an **HDR histogram**, tunes the UDP socket buffers, and exposes
the whole harness through both a **GraphQL API** (with live subscriptions) and a
**CLI**.

It is built as a production-grade, hexagonal Cargo workspace: a pure domain core,
swappable transport adapters (an in-process loopback simulator and a real `quinn`
QUIC stack), resilience primitives, full observability, and a comprehensive test
suite.

---

## Why this exists

Landing transactions on Solana is a latency game played over QUIC against the leader's
TPU. Before you can optimize that path you have to *measure* it precisely — including
the p99/p99.9 tail, which is where real money is lost. QuicForge is a focused lab for
exactly that: a reusable, well-instrumented QUIC round-trip benchmarker with the
ergonomics (GraphQL, metrics, HDR percentiles, socket tuning) you'd want in a real
low-latency networking team.

---

## Architecture

Hexagonal / ports-and-adapters. Dependencies point **inward**; the domain core knows
nothing about QUIC, axum, or GraphQL.

```
            ┌─────────────────────────────────────────────┐
            │                quicforge-node                │  composition root
            │   CLI (clap) · axum HTTP/WS · telemetry      │  + binary
            └───────────────┬───────────────┬─────────────┘
                            │               │
                   ┌────────▼──────┐  ┌──────▼────────────┐
                   │ quicforge-api │  │  quicforge-infra  │  adapters
                   │ async-graphql │  │ quinn · loopback  │
                   │ schema/types  │  │ memory repo · bus │
                   └────────┬──────┘  └──────┬────────────┘
                            │   implements   │ implements
                            │     ports      │   ports
                       ┌────▼────────────────▼────┐
                       │      quicforge-core       │  domain
                       │  BenchEngine · ports ·    │  (no I/O frameworks)
                       │  HDR aggregation · events │
                       └────────────┬──────────────┘
                                    │
                  ┌─────────────────┼─────────────────┐
          ┌───────▼────────┐               ┌───────────▼─────────┐
          │ quicforge-types │               │ quicforge-resilience│
          │ newtypes·stats  │               │ timeout · retry     │
          └─────────────────┘               └─────────────────────┘
```

| Crate | Responsibility | Depends on |
|---|---|---|
| **quicforge-types** | Domain newtypes (`PayloadSize`, `ConnectionCount`, `RequestCount`), `BenchParams`, latency `stats`, run lifecycle (`RunId`, `RunStatus`, `RunSummary`). Pure data, compile-time-validated invariants. | — |
| **quicforge-resilience** | `with_timeout`, `RetryPolicy` with exponential backoff + equal jitter. Deliberately **no** circuit breaker / rate limiter (they'd distort latency measurement). | — |
| **quicforge-core** | The `BenchEngine` orchestrator + ports (`QuicConnector`, `QuicConnection`, `RunRepository`, `EventSink`, `RunEventStream`, `Clock`), HDR histogram aggregation, domain events. No I/O frameworks. | types, resilience |
| **quicforge-infra** | Adapters: `QuinnConnector` + `QuicEchoServer` (real QUIC, `quic` feature), `LoopbackConnector` (simulator), `MemoryRunRepository`, `BroadcastEventSink`, `SystemClock`. | types, core |
| **quicforge-api** | `async-graphql` schema: queries (`run`, `runs`, `apiVersion`), mutation (`startBenchmark`), subscription (`runProgress`), and GraphQL object/input types. | types, core |
| **quicforge-node** | Composition root: clap CLI (`serve` / `bench`), axum HTTP + WS wiring, telemetry, graceful shutdown, the `quicforge-node` binary, and a criterion bench. | all of the above |

**Why no `ai` crate?** QuicForge is a pure-networking/latency tool — adding an LLM layer
would be scope creep. (Its sibling project SolLander demonstrates the AI-advisor layer.)

---

## The measurement model

- Each benchmark spawns **one worker task per connection** (`tokio::spawn`), so N
  connections run truly concurrently.
- A worker establishes its QUIC connection through a `RetryPolicy`-guarded,
  timeout-bounded connect, then issues `requests_per_connection` round-trips back to
  back, timing each with `Instant::now()`.
- Every round-trip is recorded into a per-worker **HDR histogram**
  (`hdrhistogram`, 3 significant figures, 1 ns … 1 h range). Workers' histograms are
  merged losslessly at the end, so percentiles are exact across all samples.
- A transport failure folds into the `RunSummary` as `RunStatus::Failed { reason }`
  rather than collapsing the whole call — `Err` is reserved for repository faults.

Reported percentiles: **min, mean, p50, p90, p99, p99.9, max** plus throughput
(req/s and Mbit/s).

---

## Quick start

### Prerequisites
- Rust **1.89.0** (pinned via `rust-toolchain.toml`).

### Build & test

```bash
cargo build --workspace
cargo test  --workspace                  # loopback adapters
cargo test  --workspace --all-features   # also exercises the real QUIC stack
```

### CLI benchmark

Loopback (no network — deterministic, always available):

```bash
cargo run -p quicforge-node -- bench \
  --transport loopback --connections 4 --requests 2000 --payload 1232 --sim-delay-us 5
```

Real QUIC against a self-spawned local echo server:

```bash
cargo run -p quicforge-node --features quic -- bench \
  --transport quic --spawn-echo --connections 4 --requests 500 --payload 1232
```

Sample output:

```
QuicForge benchmark — Quic transport
  target: 127.0.0.1:39835
  4 connections x 500 requests (2000 total), 1232-byte payload

latency (microseconds):
  samples :         2000
  min     :      188.672
  mean    :      414.693
  p50     :      395.775
  p90     :      551.423
  p99     :      766.975
  p99.9   :     1687.551
  max     :     2473.983

throughput:
        9248.3 req/s
        91.151 Mbit/s
```

### GraphQL server

```bash
cargo run -p quicforge-node -- serve --host 127.0.0.1 --port 8080
# open http://127.0.0.1:8080/graphql for the GraphiQL playground
```

Run a benchmark over the API:

```bash
curl -s -X POST http://127.0.0.1:8080/graphql \
  -H 'content-type: application/json' \
  -d '{"query":"mutation{ startBenchmark(input:{target:\"127.0.0.1:0\", connections:2, requestsPerConnection:1000, payloadBytes:1232}){ state totalRequests stats{ samples p50Micros p99Micros } throughput{ requestsPerSec megabitsPerSec } } }"}'
```

---

## GraphQL API

| Kind | Field | Description |
|---|---|---|
| Query | `apiVersion` | Crate version string. |
| Query | `run(id)` | Fetch a single run by UUID. |
| Query | `runs(limit)` | Most recent runs (limit clamped to 1..200). |
| Mutation | `startBenchmark(input)` | Run a benchmark and return its `RunSummary`. |
| Subscription | `runProgress` | Live `Started` / `Progress` / `Completed` / `Failed` events over WebSocket (`/graphql/ws`). |

A Postman collection is provided in [`postman/QuicForge.postman_collection.json`](postman/QuicForge.postman_collection.json).

---

## Operational endpoints

| Endpoint | Purpose |
|---|---|
| `GET /health/live` | Liveness probe. |
| `GET /health/ready` | Readiness probe. |
| `GET /metrics` | Prometheus exposition. |
| `GET/POST /graphql` | GraphiQL UI (GET) + GraphQL queries (POST). |
| `WS /graphql/ws` | GraphQL subscriptions. |

### Metrics

`quicforge_runs_total`, `quicforge_runs_completed_total`, `quicforge_runs_failed_total`,
`quicforge_requests_total`, `quicforge_round_trip_seconds` (histogram), and
`quicforge_graphql_requests_total`.

```bash
docker compose --profile monitoring up   # node + Prometheus on :9090
```

---

## Configuration

All flags have environment-variable equivalents (clap `env`):

| Variable | Flag | Default |
|---|---|---|
| `QUICFORGE_HOST` | `--host` | `127.0.0.1` |
| `QUICFORGE_PORT` | `--port` | `8080` |
| `QUICFORGE_TRANSPORT` | `--transport` | `loopback` |
| `QUICFORGE_LOG_JSON` | `--log-json` | `false` |
| `RUST_LOG` | — | `info` |

---

## Docker

```bash
docker build -t quicforge .
docker run --rm -p 8080:8080 quicforge                  # serve (default)
docker run --rm quicforge bench --transport quic --spawn-echo --requests 500
```

The image is multi-stage (`rust:1.89-slim` → `debian:bookworm-slim`), built with the
`quic` feature, and runs as a non-root user (`uid 10001`).

---

## Project layout

```
QuicForge/
├── crates/
│   ├── quicforge-types/        # domain newtypes, stats, run lifecycle
│   ├── quicforge-resilience/   # timeout + retry/backoff
│   ├── quicforge-core/         # BenchEngine, ports, HDR aggregation, events
│   ├── quicforge-infra/        # quinn + loopback adapters, memory repo, event bus
│   ├── quicforge-api/          # async-graphql schema/types
│   └── quicforge-node/         # CLI + axum server + binary + criterion bench
├── monitoring/prometheus.yml
├── postman/QuicForge.postman_collection.json
├── Dockerfile · docker-compose.yml
└── .github/workflows/ci.yml
```

---

## Testing & quality gates

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test  --workspace --all-features
cargo bench -p quicforge-node            # criterion: loopback_4x256_round_trips
```

- **32 tests** across the workspace (unit + adapter integration + a live QUIC
  echo round-trip), all green; `clippy` clean under both feature sets.
- `#![forbid(unsafe_code)]` in every crate.
- The `quic` integration tests stand up a real `quinn` endpoint on `127.0.0.1:0` and
  benchmark against it end-to-end.

See [`EVALUATION.md`](EVALUATION.md) for how the codebase maps to each engineering
guideline.

---

## License

Apache-2.0 © Abhijeet Ashok Muneshwar
