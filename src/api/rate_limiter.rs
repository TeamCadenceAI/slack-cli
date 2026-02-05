//! Rate limiting for Slack API requests
//!
//! Implements Tier 2 rate limits: 20 requests per minute with burst of 3.

use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter as GovernorRateLimiter,
};
use std::num::NonZeroU32;
use std::sync::Arc;

/// Rate limiter for Slack API requests
///
/// Uses governor crate with Tier 2 limits:
/// - 20 requests per minute
/// - Burst capacity of 3
#[derive(Clone)]
pub struct RateLimiter {
    limiter: Arc<GovernorRateLimiter<NotKeyed, InMemoryState, DefaultClock>>,
}

impl RateLimiter {
    /// Create a new rate limiter with Tier 2 defaults
    pub fn new() -> Self {
        Self::with_config(20, 3)
    }

    /// Create a rate limiter with custom configuration
    ///
    /// # Arguments
    /// * `requests_per_minute` - Maximum requests per minute (must be > 0)
    /// * `burst` - Burst capacity (allows this many requests immediately, must be > 0)
    ///
    /// If either argument is 0, safe defaults are used (1 for the invalid value).
    pub fn with_config(requests_per_minute: u32, burst: u32) -> Self {
        // Use safe defaults for invalid inputs
        let requests_per_minute = if requests_per_minute == 0 {
            1
        } else {
            requests_per_minute
        };
        let burst = if burst == 0 { 1 } else { burst };

        // Calculate period between requests in milliseconds
        // 60_000 ms / requests_per_minute
        let period_ms = 60_000 / requests_per_minute;

        // SAFETY: We ensured burst > 0 above, and period_ms is guaranteed valid since
        // requests_per_minute is guaranteed > 0
        let burst_nz = NonZeroU32::new(burst).unwrap_or(NonZeroU32::MIN);
        let quota = Quota::with_period(std::time::Duration::from_millis(period_ms as u64))
            .unwrap_or_else(|| Quota::per_second(NonZeroU32::MIN))
            .allow_burst(burst_nz);

        let limiter = GovernorRateLimiter::direct(quota);

        Self {
            limiter: Arc::new(limiter),
        }
    }

    /// Acquire a permit to make a request
    ///
    /// This method is non-blocking - it waits asynchronously until a permit
    /// is available according to the rate limit configuration.
    pub async fn acquire(&self) {
        self.limiter.until_ready().await;
    }

    /// Try to acquire a permit without waiting
    ///
    /// Returns `true` if a permit was acquired, `false` if rate limited.
    pub fn try_acquire(&self) -> bool {
        self.limiter.check().is_ok()
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[tokio::test]
    async fn test_rate_limiter_creation() {
        let limiter = RateLimiter::new();
        // Should be able to acquire immediately
        limiter.acquire().await;
    }

    #[tokio::test]
    async fn test_rate_limiter_burst_allowance() {
        // Create a limiter with very low rate but burst of 3
        let limiter = RateLimiter::with_config(1, 3);

        // Should be able to acquire 3 times immediately due to burst
        let start = Instant::now();
        for _ in 0..3 {
            assert!(limiter.try_acquire());
        }
        let elapsed = start.elapsed();

        // All 3 should complete almost instantly
        assert!(elapsed < Duration::from_millis(100));

        // 4th should fail (no more burst capacity)
        assert!(!limiter.try_acquire());
    }

    #[tokio::test]
    async fn test_rate_limiter_try_acquire() {
        let limiter = RateLimiter::with_config(60, 1); // 1 per second, burst of 1

        // First should succeed
        assert!(limiter.try_acquire());

        // Second should fail (burst exhausted)
        assert!(!limiter.try_acquire());
    }

    #[tokio::test]
    async fn test_rate_limiter_async_acquire() {
        // Very fast rate for testing
        let limiter = RateLimiter::with_config(600, 1); // 10 per second

        let start = Instant::now();

        // First is immediate
        limiter.acquire().await;

        // Second should wait
        limiter.acquire().await;

        let elapsed = start.elapsed();

        // Should have taken at least some time for the second request
        // With 600 req/min = 100ms per request
        assert!(elapsed >= Duration::from_millis(50));
    }

    #[tokio::test]
    async fn test_rate_limiter_default() {
        let limiter = RateLimiter::default();
        // Should work same as new()
        limiter.acquire().await;
    }

    #[tokio::test]
    async fn test_rate_limiter_clone() {
        let limiter1 = RateLimiter::new();
        let limiter2 = limiter1.clone();

        // Both should share the same internal state
        assert!(limiter1.try_acquire());
        assert!(limiter1.try_acquire());
        assert!(limiter1.try_acquire());

        // limiter2 should see the exhausted burst
        // (they share the Arc)
        assert!(!limiter2.try_acquire());
    }
}
