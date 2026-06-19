// ── Monotonic Clock ───────────────────────────
//  Wraps `std::time::Instant` as a zero-cost
//  implementation of the application `Clock` trait.

use gaming_application::traits::Clock;

/// Nanoseconds-per-second constant.
const NS_PER_SEC: u64 = 1_000_000_000;

pub struct MonotonicClock {
    epoch: std::time::Instant,
}

impl MonotonicClock {
    pub fn new() -> Self {
        Self {
            epoch: std::time::Instant::now(),
        }
    }
}

impl Default for MonotonicClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for MonotonicClock {
    /// Returns nanoseconds since construction (monotonic, high resolution).
    fn now_ns(&self) -> u64 {
        let elapsed = self.epoch.elapsed();
        elapsed.as_secs() * NS_PER_SEC + elapsed.subsec_nanos() as u64
    }

    /// Returns seconds since UNIX_EPOCH (wall clock, best-effort monotonic).
    fn now_secs(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

// ──────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_monotonic_increases() {
        let clock = MonotonicClock::new();
        let a = clock.now_ns();
        std::thread::sleep(std::time::Duration::from_micros(100));
        let b = clock.now_ns();
        assert!(b > a);
    }

    #[test]
    fn now_secs_is_reasonable() {
        let clock = MonotonicClock::new();
        let secs = clock.now_secs();
        // Must be >= 2020 (1,577,836,800) and <= 2100.
        assert!(secs > 1_577_836_800);
        assert!(secs < 4_102_444_800u64);
    }
}
