# Phase 13: Per-IP Rate Limiting

## Objective

Phase 10 left rate limiting as a documented gap ("`tower-http` API mismatch").
An unauthenticated or compromised client could hammer the gateway and exhaust
coordinator, storage-node, and database resources. This phase adds a
self-contained token-bucket rate limiter as Axum middleware.

## What Was Implemented

### Token-bucket limiter (`gateway::rate_limit`)
- One bucket per client IP, guarded by a `Mutex<HashMap<IpAddr, Bucket>>`.
- Refills continuously at `RATE_LIMIT_RPS` tokens/second up to a burst
  capacity of `RATE_LIMIT_BURST`.
- Bounded memory: stale buckets are pruned once the map exceeds 10k entries.
- A disabled limiter (`RPS=0`) is a cheap pass-through.

### Middleware
- Rejects with `429 Too Many Requests` and `Retry-After: 1` when the bucket is
  empty.
- Exempts `/metrics` and `/api/v1/health` so probes and Prometheus scrapes are
  never throttled.
- Regular IP extraction from `ConnectInfo`; the server now uses
  `into_make_service_with_connect_info::<SocketAddr>()`.

### Configuration
| Variable | Default | Meaning |
|---|---|---|
| `RATE_LIMIT_RPS` | `0` (disabled) | Sustained requests/second per client IP |
| `RATE_LIMIT_BURST` | `2 × RPS` | Bucket capacity (max burst) |

The limiter is enabled only when `RATE_LIMIT_RPS > 0`.

## Behavior

With `RATE_LIMIT_RPS=1`, `RATE_LIMIT_BURST=1`, rapid requests to a normal
endpoint produce:

```
GET /nope -> 404
GET /nope -> 429 (retry-after: 1)
GET /nope -> 429 (retry-after: 1)
...
```

Each distinct client IP gets an independent bucket.

## Crate Changes

- `crates/gateway/src/rate_limit.rs` — new module (config, limiter, middleware,
  tests)
- `crates/gateway/src/lib.rs` — exports, layer wiring, connect-info service

## Verification

- `cargo build -p gateway --bins` — clean
- `cargo clippy -p gateway --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean
- `cargo test -p gateway` — 5/5 pass (3 chunk + 2 rate-limit)
- Live smoke test: burst of rapid requests yields `429` with `retry-after: 1`;
  `/api/v1/health` stays `200`.

> Note: the limiter is correct but only *observable* when requests arrive faster
> than the refill rate. Endpoints that block on a slow coordinator (≈2s on a
> connection failure) naturally refill the bucket between calls.

## What Was Deferred (Future Work)

- **Distributed rate limiting** — the current bucket is per-process, so N
  gateway replicas allow N × the limit. A Redis-backed or coordinator-backed
  limiter would make it cluster-wide.
- **Per-route limits** — e.g. tighter limits on uploads than reads.
- **`X-Forwarded-For` awareness** — trust a configured proxy and key on the
  forwarded client IP instead of the immediate peer.

## Completed Phases

| Phase | Description | Status |
|---|---|---|
| 1 | Architecture & Foundation | Complete |
| 2 | Storage Engine | Complete |
| 3 | Gateway (HTTP API) | Complete |
| 4 | Coordinator (metadata, health, leases) | Complete |
| 5 | Storage Node gRPC | Complete |
| 6 | Replication (durable acks, recovery) | Complete |
| 7 | Fault Tolerance (concurrent recovery) | Complete |
| 8 | Hybrid Storage (S3/MinIO abstraction) | Complete |
| 9 | Testing & Benchmarking | Complete |
| 10 | Production Hardening | Complete |
| 11 | Runnable Services & Deployment | Complete |
| 12 | Metrics Endpoint & Counter Wiring | Complete |
| 13 | Per-IP Rate Limiting | Complete |
