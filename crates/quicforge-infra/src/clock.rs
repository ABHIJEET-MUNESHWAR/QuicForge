//! System wall-clock adapter.

use chrono::{DateTime, Utc};

use quicforge_core::ports::Clock;

/// A [`Clock`] backed by the operating system clock.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_is_monotonic_enough() {
        let a = SystemClock.now();
        let b = SystemClock.now();
        assert!(b >= a);
    }
}
