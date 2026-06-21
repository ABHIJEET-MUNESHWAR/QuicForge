//! `quicforge-types` — pure domain value objects for the QUIC latency lab.
//!
//! No I/O, no framework dependencies. Invariants (payload bounds, non-zero
//! counts) are enforced at construction so illegal parameters are unrepresentable.
#![forbid(unsafe_code)]

pub mod error;
pub mod params;
pub mod run;
pub mod stats;

/// Common imports.
pub mod prelude {
    pub use crate::error::InvalidParam;
    pub use crate::params::{BenchParams, ConnectionCount, PayloadSize, RequestCount};
    pub use crate::run::{FailureReason, RunId, RunStatus, RunSummary};
    pub use crate::stats::{LatencyNs, LatencyStats, Throughput};
}

pub use prelude::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_size_validates_range() {
        assert!(PayloadSize::new(0).is_err());
        assert!(PayloadSize::new(1).is_ok());
        assert_eq!(PayloadSize::new(1232).unwrap().get(), 1232);
        assert!(PayloadSize::new(PayloadSize::MAX).is_ok());
        assert!(PayloadSize::new(PayloadSize::MAX + 1).is_err());
        assert_eq!(PayloadSize::default().get(), 1232);
    }

    #[test]
    fn counts_reject_zero() {
        assert_eq!(ConnectionCount::new(0), Err(InvalidParam::ZeroConnections));
        assert_eq!(RequestCount::new(0), Err(InvalidParam::ZeroRequests));
        assert_eq!(ConnectionCount::new(8).unwrap().get(), 8);
        assert_eq!(RequestCount::new(5).unwrap().get(), 5);
    }

    #[test]
    fn total_requests_multiplies() {
        let params = BenchParams::new(
            "127.0.0.1:9000".parse().unwrap(),
            ConnectionCount::new(4).unwrap(),
            RequestCount::new(250).unwrap(),
            PayloadSize::default(),
        );
        assert_eq!(params.total_requests(), 1000);
    }

    #[test]
    fn run_status_serializes_tagged() {
        let json = serde_json::to_string(&RunStatus::Running { completed: 7 }).unwrap();
        assert_eq!(json, r#"{"state":"running","completed":7}"#);
        let failed = serde_json::to_string(&RunStatus::Failed {
            reason: FailureReason::ConnectTimeout,
        })
        .unwrap();
        assert_eq!(failed, r#"{"state":"failed","reason":"connect_timeout"}"#);
    }

    #[test]
    fn latency_conversions() {
        let l = LatencyNs::new(1_500_000);
        assert_eq!(l.get(), 1_500_000);
        assert!((l.as_micros_f64() - 1500.0).abs() < f64::EPSILON);
        assert!((l.as_millis_f64() - 1.5).abs() < f64::EPSILON);
    }
}
