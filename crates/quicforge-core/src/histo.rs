//! HDR-histogram helpers for latency aggregation.
//!
//! `hdrhistogram` records values in a fixed range with constant relative error,
//! so percentile queries (p50/p99/p99.9) are O(1) and memory is bounded — ideal
//! for high-rate latency capture.

use hdrhistogram::Histogram;

use quicforge_types::prelude::*;

/// Create a histogram spanning 1 ns … ~1 hour with 3 significant figures.
pub fn new_histogram() -> Histogram<u64> {
    Histogram::<u64>::new_with_bounds(1, 3_600_000_000_000, 3).expect("valid histogram bounds")
}

/// Derive summary statistics, returning `None` when no samples were recorded.
pub fn stats_from_histogram(h: &Histogram<u64>) -> Option<LatencyStats> {
    if h.is_empty() {
        return None;
    }
    Some(LatencyStats {
        samples: h.len(),
        min_ns: h.min(),
        mean_ns: h.mean() as u64,
        p50_ns: h.value_at_quantile(0.50),
        p90_ns: h.value_at_quantile(0.90),
        p99_ns: h.value_at_quantile(0.99),
        p999_ns: h.value_at_quantile(0.999),
        max_ns: h.max(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_histogram_has_no_stats() {
        assert!(stats_from_histogram(&new_histogram()).is_none());
    }

    #[test]
    fn records_and_summarizes() {
        let mut h = new_histogram();
        for ns in 1..=1000 {
            h.saturating_record(ns * 1000);
        }
        let stats = stats_from_histogram(&h).unwrap();
        assert_eq!(stats.samples, 1000);
        assert!(stats.min_ns <= stats.p50_ns);
        assert!(stats.p50_ns <= stats.p99_ns);
        assert!(stats.p99_ns <= stats.max_ns);
    }
}
