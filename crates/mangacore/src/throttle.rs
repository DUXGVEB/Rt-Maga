//! Shared async GCRA token-bucket rate limiter enforcing a strict global aggregate network
//! speed ceiling across all concurrent HTML, image, preload, and background update downloads.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::Instant;

/// Maximum configurable speed limit in KiB/s (100 MiB/s).
pub const MAX_SPEED_LIMIT_KBPS: u32 = 102_400;

/// Maximum burst credit window (200ms) so idle connections start responsively without spikes.
const MAX_BURST_WINDOW: Duration = Duration::from_millis(200);

/// Thread-safe global bandwidth throttle shared across all concurrent [`crate::HttpClient`] streams.
///
/// Uses a reservation-based GCRA schedule so N concurrent streams strictly share the exact
/// aggregate byte-per-second ceiling without overlapping sleep windows.
/// A limit of `0` means **unlimited** (zero locking on the hot path).
#[derive(Debug, Clone)]
pub struct BandwidthLimiter {
    bytes_per_sec: Arc<AtomicU64>,
    next_available: Arc<Mutex<Instant>>,
}

impl Default for BandwidthLimiter {
    fn default() -> Self {
        Self::new(0)
    }
}

impl BandwidthLimiter {
    /// Creates a new [`BandwidthLimiter`] with an initial limit in KiB/s (`0` = unlimited).
    pub fn new(limit_kbps: u32) -> Self {
        Self {
            bytes_per_sec: Arc::new(AtomicU64::new(kbps_to_bps(limit_kbps))),
            next_available: Arc::new(Mutex::new(Instant::now())),
        }
    }

    /// Updates the global speed ceiling live (`0` = unlimited).
    pub fn set_limit_kbps(&self, limit_kbps: u32) {
        self.bytes_per_sec
            .store(kbps_to_bps(limit_kbps), Ordering::Release);
    }

    /// Returns the currently configured speed limit in KiB/s (`0` = unlimited).
    pub fn limit_kbps(&self) -> u32 {
        let bps = self.bytes_per_sec.load(Ordering::Acquire);
        u32::try_from(bps / 1024).unwrap_or(MAX_SPEED_LIMIT_KBPS)
    }

    /// Reserves aggregate bandwidth for `bytes` downloaded and sleeps outside the lock if needed.
    pub async fn acquire(&self, bytes: usize) {
        if bytes == 0 {
            return;
        }
        let bps = self.bytes_per_sec.load(Ordering::Acquire);
        if bps == 0 {
            return;
        }

        let cost_us = u128::try_from(bytes)
            .unwrap_or(u128::MAX)
            .saturating_mul(1_000_000)
            .div_ceil(u128::from(bps));
        let cost = Duration::from_micros(u64::try_from(cost_us).unwrap_or(5_000_000));

        let wait_dur = {
            let mut slot = self.next_available.lock().await;
            let now = Instant::now();
            let floor = now.checked_sub(MAX_BURST_WINDOW).unwrap_or(now);
            let start = (*slot).max(floor);
            let finish = start + cost;
            *slot = finish;
            finish.saturating_duration_since(now)
        };

        if !wait_dur.is_zero() {
            tokio::time::sleep(wait_dur).await;
        }
    }
}

fn kbps_to_bps(limit_kbps: u32) -> u64 {
    if limit_kbps == 0 {
        0
    } else {
        u64::from(limit_kbps.min(MAX_SPEED_LIMIT_KBPS)).saturating_mul(1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unlimited_limiter_does_not_delay() {
        let limiter = BandwidthLimiter::new(0);
        let started = Instant::now();
        limiter.acquire(512 * 1024).await;
        assert!(
            started.elapsed() < Duration::from_millis(80),
            "unlimited mode must return immediately"
        );
    }

    #[tokio::test]
    async fn aggregate_limit_throttles_concurrent_streams() {
        // Limit: 64 KiB/s. Two concurrent streams acquiring 24 KiB each = 48 KiB total.
        // At 64 KiB/s with 200ms burst credit (~12.8 KiB), net paced transfer >= 35.2 KiB -> >= 550ms.
        let limiter = BandwidthLimiter::new(64);
        let started = Instant::now();
        let a = limiter.clone();
        let b = limiter.clone();
        let ((), ()) = tokio::join!(a.acquire(24 * 1024), b.acquire(24 * 1024));
        let elapsed = started.elapsed();
        assert!(
            elapsed >= Duration::from_millis(450),
            "concurrent transfers must obey aggregate speed limit (elapsed: {elapsed:?})"
        );
    }
}
