//! The benchmark orchestrator: drives concurrent QUIC connections, measures each
//! round-trip, and aggregates latencies into a single distribution.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use hdrhistogram::Histogram;

use quicforge_resilience::{retry_if, with_timeout, RetryPolicy};
use quicforge_types::prelude::*;

use crate::config::EngineConfig;
use crate::error::{CoreError, PortError};
use crate::events::RunEvent;
use crate::histo::{new_histogram, stats_from_histogram};
use crate::ports::{Clock, EventSink, QuicConnector, RunRepository};

/// Adapters the engine depends on, injected at the composition root.
pub struct BenchDeps {
    /// Establishes QUIC connections.
    pub connector: Arc<dyn QuicConnector>,
    /// Stores run summaries.
    pub repo: Arc<dyn RunRepository>,
    /// Publishes lifecycle events.
    pub events: Arc<dyn EventSink>,
    /// Wall-clock source.
    pub clock: Arc<dyn Clock>,
}

/// Orchestrates benchmark runs. Cheap to clone (`Arc` inside).
#[derive(Clone)]
pub struct BenchEngine {
    deps: Arc<BenchDeps>,
    config: EngineConfig,
}

impl BenchEngine {
    /// Construct an engine from its dependencies and configuration.
    pub fn new(deps: BenchDeps, config: EngineConfig) -> Self {
        Self {
            deps: Arc::new(deps),
            config,
        }
    }

    /// Execute a benchmark run to completion, persisting and returning its summary.
    ///
    /// Transport failures are folded into the returned [`RunSummary`] (status
    /// `Failed`) rather than surfaced as `Err`; `Err` is reserved for repository
    /// faults that prevent recording the outcome at all.
    pub async fn run(&self, params: BenchParams) -> Result<RunSummary, CoreError> {
        let id = RunId::generate();
        let started_at = self.deps.clock.now();
        let total = params.total_requests();

        let mut summary = RunSummary {
            id,
            params,
            status: RunStatus::Running { completed: 0 },
            stats: None,
            throughput: None,
            started_at,
            finished_at: None,
        };
        self.deps.repo.save(&summary).await?;
        self.deps
            .events
            .publish(RunEvent::Started {
                id,
                total,
                at: started_at,
            })
            .await;
        metrics::counter!("quicforge_runs_total").increment(1);

        let completed = Arc::new(AtomicU64::new(0));
        let wall_start = Instant::now();

        let mut handles = Vec::with_capacity(params.connections.get() as usize);
        for _ in 0..params.connections.get() {
            let deps = self.deps.clone();
            let config = self.config;
            let completed = completed.clone();
            handles.push(tokio::spawn(async move {
                run_connection(deps, config, id, params, total, completed).await
            }));
        }

        let mut merged = new_histogram();
        let mut first_err: Option<FailureReason> = None;
        for handle in handles {
            match handle.await {
                Ok(Ok(hist)) => {
                    let _ = merged.add(&hist);
                }
                Ok(Err(reason)) => {
                    first_err.get_or_insert(reason);
                }
                Err(_join) => {
                    first_err.get_or_insert(FailureReason::Internal);
                }
            }
        }

        let elapsed = wall_start.elapsed();
        let finished_at = self.deps.clock.now();

        if let Some(reason) = first_err {
            summary.status = RunStatus::Failed { reason };
            summary.stats = stats_from_histogram(&merged);
            summary.finished_at = Some(finished_at);
            self.deps.repo.update(&summary).await?;
            self.deps
                .events
                .publish(RunEvent::Failed {
                    id,
                    reason,
                    at: finished_at,
                })
                .await;
            metrics::counter!("quicforge_runs_failed_total").increment(1);
            return Ok(summary);
        }

        let Some(stats) = stats_from_histogram(&merged) else {
            // No samples despite no error — defensive; treat as internal failure.
            summary.status = RunStatus::Failed {
                reason: FailureReason::Internal,
            };
            summary.finished_at = Some(finished_at);
            self.deps.repo.update(&summary).await?;
            self.deps
                .events
                .publish(RunEvent::Failed {
                    id,
                    reason: FailureReason::Internal,
                    at: finished_at,
                })
                .await;
            return Ok(summary);
        };

        let done = completed.load(Ordering::SeqCst);
        let secs = elapsed.as_secs_f64().max(f64::MIN_POSITIVE);
        let throughput = Throughput {
            requests_per_sec: done as f64 / secs,
            bytes_per_sec: (done * params.payload.get() as u64) as f64 / secs,
        };

        summary.status = RunStatus::Completed;
        summary.stats = Some(stats);
        summary.throughput = Some(throughput);
        summary.finished_at = Some(finished_at);
        self.deps.repo.update(&summary).await?;
        self.deps
            .events
            .publish(RunEvent::Completed {
                id,
                stats,
                throughput,
                at: finished_at,
            })
            .await;
        metrics::counter!("quicforge_runs_completed_total").increment(1);

        Ok(summary)
    }

    /// Fetch a run summary by id.
    pub async fn get(&self, id: RunId) -> Result<Option<RunSummary>, CoreError> {
        Ok(self.deps.repo.get(id).await?)
    }

    /// List the most recent runs, newest first.
    pub async fn recent(&self, limit: usize) -> Result<Vec<RunSummary>, CoreError> {
        Ok(self.deps.repo.list_recent(limit).await?)
    }
}

/// Drive a single connection's share of the workload, returning its local histogram.
async fn run_connection(
    deps: Arc<BenchDeps>,
    config: EngineConfig,
    id: RunId,
    params: BenchParams,
    total: u64,
    completed: Arc<AtomicU64>,
) -> Result<Histogram<u64>, FailureReason> {
    let policy = RetryPolicy {
        max_attempts: config.connect_retries.max(1),
        ..RetryPolicy::default()
    };

    let connect_result = retry_if(
        &policy,
        |_attempt| {
            let connector = deps.connector.clone();
            let target = params.target;
            async move {
                match with_timeout(config.connect_timeout, connector.connect(target)).await {
                    Ok(inner) => inner,
                    Err(_timeout) => Err(PortError::Timeout),
                }
            }
        },
        |e| e.is_retryable(),
    )
    .await;

    let connection = match connect_result {
        Ok(conn) => conn,
        Err(PortError::Timeout) => return Err(FailureReason::ConnectTimeout),
        Err(_) => return Err(FailureReason::ConnectionLost),
    };

    let payload = vec![0u8; params.payload.get() as usize];
    let mut hist = new_histogram();

    for _ in 0..params.requests_per_connection.get() {
        let start = Instant::now();
        match with_timeout(config.request_timeout, connection.round_trip(&payload)).await {
            Ok(Ok(())) => {
                let ns = start.elapsed().as_nanos() as u64;
                hist.saturating_record(ns);
                metrics::histogram!("quicforge_round_trip_seconds").record(ns as f64 / 1e9);
                metrics::counter!("quicforge_requests_total").increment(1);
                completed.fetch_add(1, Ordering::SeqCst);
            }
            Ok(Err(_)) => {
                connection.close().await;
                return Err(FailureReason::Io);
            }
            Err(_timeout) => {
                connection.close().await;
                return Err(FailureReason::Io);
            }
        }
    }

    let done = completed.load(Ordering::SeqCst);
    deps.events
        .publish(RunEvent::Progress {
            id,
            completed: done,
            total,
            at: deps.clock.now(),
        })
        .await;
    connection.close().await;
    Ok(hist)
}
