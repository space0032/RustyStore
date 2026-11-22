//! Per-client-IP rate limiting middleware.
//!
//! Requests are checked against the coordinator's shared token-bucket limiter,
//! so bursts are bounded across gateway replicas instead of per process. If the
//! coordinator is unreachable the gateway falls back to its local bucket rather
//! than failing every request. When either bucket is empty the request is
//! rejected with `429 Too Many Requests`. Health and metrics endpoints are
//! always exempt so probes and scrapers are never throttled.
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

use crate::AppState;

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

/// Result of a shared rate-limit check.
#[derive(Clone, Copy, Debug)]
pub struct RateDecision {
    pub allowed: bool,
    pub retry_after_ms: u64,
}

/// Cluster-wide limiter backed by the coordinator, with a local fallback.
#[derive(Clone)]
pub struct SharedRateLimiter {
    config: RateLimitConfig,
    local: Arc<RateLimiter>,
    state: Arc<AppState>,
}

impl SharedRateLimiter {
    pub fn new(config: RateLimitConfig, state: Arc<AppState>) -> Self {
        Self {
            local: Arc::new(RateLimiter::new(config.rps, config.burst)),
            config,
            state,
        }
    }

    pub async fn check(&self, ip: IpAddr) -> RateDecision {
        if !self.config.is_enabled() {
            return RateDecision {
                allowed: true,
                retry_after_ms: 0,
            };
        }

        match self.state.coord_client().await {
            Ok(mut client) => {
                match client
                    .check_rate_limit(&ip.to_string(), self.config.rps, self.config.burst)
                    .await
                {
                    Ok((allowed, retry_after_ms)) => {
                        return RateDecision {
                            allowed,
                            retry_after_ms,
                        };
                    }
                    Err(e) => {
                        tracing::debug!("shared rate limiter unavailable, using local bucket: {e}")
                    }
                }
            }
            Err(e) => {
                tracing::debug!("shared rate limiter unavailable, using local bucket: {e}")
            }
        }

        let allowed = self.local.try_acquire(ip);
        RateDecision {
            allowed,
            retry_after_ms: if allowed { 0 } else { 1000 },
        }
    }
}

/// Middleware enforcing the per-IP rate limit.
pub async fn rate_limit_middleware(
    State(limiter): State<Arc<SharedRateLimiter>>,
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

    let decision = limiter.check(ip).await;
    if decision.allowed {
        next.run(request).await
    } else {
        let retry_after = decision.retry_after_ms.div_ceil(1000).max(1).to_string();
        (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", retry_after)],
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
