//! `quicforge-resilience` — timeouts and retry-with-backoff.
//!
//! Deliberately lean: a latency lab is a *client load driver*, so the relevant
//! guards are a bounded handshake/round-trip **timeout** and a jittered **retry**
//! on connection establishment. (Circuit breaking / rate limiting are omitted on
//! purpose — they would distort the very latency the lab is built to measure.)
#![forbid(unsafe_code)]

use std::future::Future;
use std::time::Duration;

use thiserror::Error;

/// The wrapped future did not complete within the allotted time.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[error("operation timed out after {0:?}")]
pub struct TimeoutError(pub Duration);

/// Run `fut`, failing with [`TimeoutError`] if it exceeds `dur`.
pub async fn with_timeout<T, F>(dur: Duration, fut: F) -> Result<T, TimeoutError>
where
    F: Future<Output = T>,
{
    tokio::time::timeout(dur, fut)
        .await
        .map_err(|_| TimeoutError(dur))
}

/// Exponential-backoff retry policy with equal jitter.
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// Maximum total attempts (including the first).
    pub max_attempts: u32,
    /// Base delay before the first retry.
    pub base_delay: Duration,
    /// Upper bound on any single delay.
    pub max_delay: Duration,
    /// Growth factor per attempt.
    pub multiplier: f64,
    /// Whether to apply equal jitter.
    pub jitter: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(50),
            max_delay: Duration::from_secs(2),
            multiplier: 2.0,
            jitter: true,
        }
    }
}

impl RetryPolicy {
    /// A policy that retries up to `n` times with no delay (useful in tests).
    pub fn immediate(n: u32) -> Self {
        Self {
            max_attempts: n,
            base_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
            multiplier: 1.0,
            jitter: false,
        }
    }

    /// Delay before the given 1-based retry `attempt`.
    pub fn delay_for(&self, attempt: u32) -> Duration {
        if self.base_delay.is_zero() {
            return Duration::ZERO;
        }
        let exp = attempt.saturating_sub(1) as i32;
        let raw = self.base_delay.as_secs_f64() * self.multiplier.powi(exp);
        let capped = raw.min(self.max_delay.as_secs_f64());
        let secs = if self.jitter {
            // Equal jitter: half fixed, half random in [0, capped/2].
            let half = capped / 2.0;
            half + half * pseudo_random_fraction()
        } else {
            capped
        };
        Duration::from_secs_f64(secs)
    }
}

/// Retry `op` according to `policy`, treating every error as retryable.
pub async fn retry<T, E, Fut, Op>(policy: &RetryPolicy, op: Op) -> Result<T, E>
where
    Op: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    retry_if(policy, op, |_| true).await
}

/// Retry `op` according to `policy`, but stop early when `is_retryable` is false.
///
/// `op` receives the zero-based attempt index.
pub async fn retry_if<T, E, Fut, Op, P>(
    policy: &RetryPolicy,
    mut op: Op,
    is_retryable: P,
) -> Result<T, E>
where
    Op: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<T, E>>,
    P: Fn(&E) -> bool,
{
    let mut attempt: u32 = 0;
    loop {
        match op(attempt).await {
            Ok(value) => return Ok(value),
            Err(err) => {
                attempt += 1;
                if attempt >= policy.max_attempts || !is_retryable(&err) {
                    return Err(err);
                }
                let delay = policy.delay_for(attempt);
                tracing::debug!(attempt, ?delay, "retrying after error");
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }
}

/// A cheap pseudo-random fraction in `[0, 1)` derived from the system clock.
///
/// Jitter only needs to de-correlate retries; cryptographic quality is unnecessary.
fn pseudo_random_fraction() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos % 1_000) as f64 / 1_000.0
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    use super::*;

    #[tokio::test]
    async fn timeout_returns_value_when_fast() {
        let v = with_timeout(Duration::from_millis(100), async { 42 })
            .await
            .unwrap();
        assert_eq!(v, 42);
    }

    #[tokio::test]
    async fn timeout_elapses_for_slow_future() {
        let res = with_timeout(Duration::from_millis(10), async {
            tokio::time::sleep(Duration::from_millis(200)).await;
            1
        })
        .await;
        assert_eq!(res, Err(TimeoutError(Duration::from_millis(10))));
    }

    #[tokio::test]
    async fn retry_succeeds_after_transient_failures() {
        let calls = Arc::new(AtomicU32::new(0));
        let c = calls.clone();
        let policy = RetryPolicy::immediate(5);
        let out: Result<u32, &str> = retry(&policy, move |_attempt| {
            let c = c.clone();
            async move {
                let n = c.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    Err("transient")
                } else {
                    Ok(n)
                }
            }
        })
        .await;
        assert_eq!(out, Ok(2));
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_if_stops_on_non_retryable() {
        let calls = Arc::new(AtomicU32::new(0));
        let c = calls.clone();
        let policy = RetryPolicy::immediate(5);
        let out: Result<(), &str> = retry_if(
            &policy,
            move |_| {
                let c = c.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err("fatal")
                }
            },
            |e| *e != "fatal",
        )
        .await;
        assert_eq!(out, Err("fatal"));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retry_exhausts_attempts() {
        let calls = Arc::new(AtomicU32::new(0));
        let c = calls.clone();
        let policy = RetryPolicy::immediate(3);
        let out: Result<(), &str> = retry(&policy, move |_| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err("always")
            }
        })
        .await;
        assert_eq!(out, Err("always"));
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn delay_grows_and_caps() {
        let policy = RetryPolicy {
            max_attempts: 10,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(400),
            multiplier: 2.0,
            jitter: false,
        };
        assert_eq!(policy.delay_for(1), Duration::from_millis(100));
        assert_eq!(policy.delay_for(2), Duration::from_millis(200));
        assert_eq!(policy.delay_for(3), Duration::from_millis(400));
        // Capped at max_delay.
        assert_eq!(policy.delay_for(8), Duration::from_millis(400));
    }
}
