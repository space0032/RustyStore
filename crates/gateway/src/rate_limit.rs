//! Per-client-IP rate limiting middleware.
//!
//! A token-bucket limiter keyed by client IP. When the bucket is empty the
//! request is rejected with `429 Too Many Requests`. Health and metrics
//! endpoints are always exempt so probes and scrapers are never throttled.
//!
//! Configure via `RATE_LIMIT_RPS` (0 disables limiting, the default) and
//! `RATE_LIMIT_BURST` (defaults to twice the RPS).

use axum::{
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Rate limiting configuration.
#[derive(Clone, Copy, Debug)]
pub struct RateLimitConfig {
    /// Sustained requests per second per client. `0` disables limiting.
    pub rps: u32,
    /// Maximum burst size (bucket capacity).
    pub burst: u32,
}

impl RateLimitConfig {
    pub fn try_from_env() -> Result<Self, common::config::ConfigError> {
        let rps: u32 = common::config::parse_env("RATE_LIMIT_RPS", 0)?;
        let burst: u32 = match std::env::var("RATE_LIMIT_BURST") {
            Ok(raw) => common::config::parse_value("RATE_LIMIT_BURST", &raw)?,
            Err(_) => rps.saturating_mul(2).max(1),
        };
        Ok(Self { rps, burst })
    }

    pub fn is_enabled(&self) -> bool {
        self.rps > 0
    }
}

struct Bucket {
    tokens: f64,
    last: Instant,
}

/// Token-bucket rate limiter with one bucket per client IP.
pub struct RateLimiter {
    refill_per_sec: f64,
    capacity: f64,
    buckets: Mutex<HashMap<IpAddr, Bucket>>,
}

impl RateLimiter {
    pub fn new(rps: u32, burst: u32) -> Self {
        Self {
            refill_per_sec: rps as f64,
            capacity: burst.max(1) as f64,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Attempt to take one token for `ip`. Returns `false` if the bucket is
    /// empty (request should be rejected). Disabled limiters always allow.
    pub fn try_acquire(&self, ip: IpAddr) -> bool {
        if self.refill_per_sec <= 0.0 {
            return true;
        }

        let now = Instant::now();
        let mut buckets = self.buckets.lock().unwrap();

        // Bound memory: drop stale buckets when the map grows large.
        if buckets.len() > 10_000 {
            buckets.retain(|_, b| now.duration_since(b.last).as_secs() < 600);
        }

        let bucket = buckets.entry(ip).or_insert(Bucket {
            tokens: self.capacity,
            last: now,
        });

        let elapsed = now.duration_since(bucket.last).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        bucket.last = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Middleware enforcing the per-IP rate limit.
pub async fn rate_limit_middleware(
    State(limiter): State<Arc<RateLimiter>>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    if path == "/metrics" || path == "/api/v1/health" || path == "/api/v1/ready" {
        return next.run(request).await;
    }

    let ip = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip())
        .unwrap_or(IpAddr::from([0, 0, 0, 0]));

    if limiter.try_acquire(ip) {
        next.run(request).await
    } else {
        (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", "1")],
            "rate limit exceeded",
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_limiter_allows_all() {
        let limiter = RateLimiter::new(0, 0);
        let ip = IpAddr::from([127, 0, 0, 1]);
        for _ in 0..100 {
            assert!(limiter.try_acquire(ip));
        }
    }

    #[test]
    fn burst_is_enforced_per_ip() {
        let limiter = RateLimiter::new(1, 3);
        let a = IpAddr::from([10, 0, 0, 1]);
        let b = IpAddr::from([10, 0, 0, 2]);

        // Burst of 3 allowed, 4th rejected (no time elapses).
        assert!(limiter.try_acquire(a));
        assert!(limiter.try_acquire(a));
        assert!(limiter.try_acquire(a));
        assert!(!limiter.try_acquire(a));

        // A different IP has its own bucket.
        assert!(limiter.try_acquire(b));
    }
}
