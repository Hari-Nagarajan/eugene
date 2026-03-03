//! Global rate limiter for NVD API calls.
//!
//! Shared across all parallel executor agents via `Arc` (NvdClient holds `Arc<RateLimiter>`).
//! OSV.dev has no rate limits, so only NVD requests need limiting.

use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant, sleep};

/// Token-bucket-style rate limiter using a mutex-guarded last-request timestamp.
///
/// # Intervals
/// - Without API key: 6 seconds between requests (5 req/30s NVD limit)
/// - With API key: 1 second between requests (50 req/30s NVD limit)
#[derive(Clone)]
pub struct RateLimiter {
    last_request: Arc<Mutex<Instant>>,
    min_interval: Duration,
}

impl RateLimiter {
    /// Create a new rate limiter.
    ///
    /// `has_api_key = true` uses 1-second interval (authenticated NVD).
    /// `has_api_key = false` uses 6-second interval (unauthenticated NVD).
    pub fn new(has_api_key: bool) -> Self {
        let min_interval = if has_api_key {
            Duration::from_secs(1)
        } else {
            Duration::from_secs(6)
        };
        Self {
            // Initialize to "past" so the first request proceeds immediately
            last_request: Arc::new(Mutex::new(Instant::now() - min_interval)),
            min_interval,
        }
    }

    /// Wait until at least `min_interval` has elapsed since the last request.
    ///
    /// This method is safe to call concurrently from multiple tasks -- the mutex
    /// serializes access and each caller sleeps for the correct remaining duration.
    pub async fn wait(&self) {
        let mut last = self.last_request.lock().await;
        let elapsed = last.elapsed();
        if elapsed < self.min_interval {
            sleep(self.min_interval - elapsed).await;
        }
        *last = Instant::now();
    }

    /// Return the configured minimum interval (for testing).
    pub fn min_interval(&self) -> Duration {
        self.min_interval
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_unauthenticated_interval() {
        let limiter = RateLimiter::new(false);
        assert_eq!(limiter.min_interval(), Duration::from_secs(6));
    }

    #[test]
    fn test_rate_limiter_authenticated_interval() {
        let limiter = RateLimiter::new(true);
        assert_eq!(limiter.min_interval(), Duration::from_secs(1));
    }

    #[test]
    fn test_rate_limiter_clone_shares_state() {
        let limiter = RateLimiter::new(true);
        let cloned = limiter.clone();
        // Both should reference the same Arc<Mutex<Instant>>
        assert!(Arc::ptr_eq(&limiter.last_request, &cloned.last_request));
    }

    #[tokio::test]
    async fn test_rate_limiter_first_call_immediate() {
        let limiter = RateLimiter::new(true);
        let start = Instant::now();
        limiter.wait().await;
        // First call should be nearly immediate (< 50ms)
        assert!(start.elapsed() < Duration::from_millis(50));
    }

    #[tokio::test]
    async fn test_rate_limiter_second_call_delayed() {
        let limiter = RateLimiter::new(true); // 1-second interval
        limiter.wait().await; // first call -- immediate
        let start = Instant::now();
        limiter.wait().await; // second call -- should wait ~1 second
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(900),
            "Second call should wait ~1s, waited {:?}",
            elapsed
        );
    }
}
