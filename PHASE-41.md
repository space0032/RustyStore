# Phase 41: Shared Rate Limiting

## Objective

The gateway rate limiter kept one bucket per process, so three gateway replicas
allowed three times the configured request rate for the same client. Phase 41
moves the authoritative token bucket into PostgreSQL behind a coordinator RPC,
making limits cluster-wide while preserving a local fallback during coordinator
outages.

## Design

### 1. `CheckRateLimit` RPC

`ClusterService` gains:

```proto
rpc CheckRateLimit(RateLimitRequest) returns (RateLimitResponse);
```

The request carries the bucket key (the gateway sends the client IP), the
sustained `rps`, and the `burst` capacity. The response returns whether one
token was consumed and, when rejected, the minimum retry delay. `rps = 0`
bypasses state entirely; a positive rate requires a non-empty key and a positive
burst.

### 2. PostgreSQL token bucket

`rate_limit_buckets` stores one row per key with the remaining tokens and the
last update time. `MetadataStore::check_rate_limit` locks the row
(`SELECT ... FOR UPDATE`), refills it based on elapsed wall-clock time, consumes
a token when available, and upserts the new state in the same transaction. The
lock serializes concurrent gateway replicas, so a burst cannot be over-admitted.

The leader's lease monitor deletes buckets unused for one hour. Bucket state is
reconstructible, so cleanup only bounds table growth.

### 3. Gateway integration

`SharedRateLimiter` replaces the process-local limiter in the middleware. When
`RATE_LIMIT_RPS > 0` it asks the coordinator for a token using the configured
RPS/burst. If the coordinator is unreachable or the RPC fails, it falls back to
the existing local `RateLimiter`; a coordinator outage therefore degrades to
per-process limiting instead of failing every request. A rejection returns `429`
with `Retry-After` derived from the coordinator's retry delay. Health, readiness,
and metrics endpoints remain exempt.

### 4. Compatibility

The protobuf change is additive. Existing gateways and coordinators continue to
work during a rolling upgrade; a new gateway talking to an old coordinator gets
an `Unimplemented` status and falls back to its local bucket. Existing
`RATE_LIMIT_RPS` and `RATE_LIMIT_BURST` variables keep their meaning.

## Verification

- `cargo fmt --all`
- `cargo check --workspace --all-targets`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`: 109 tests passed

Added `shared_rate_limit_buckets_enforce_burst_per_key`, a PostgreSQL-backed
test covering per-key bursts, key independence, disabled limiting, and bucket
cleanup. PostgreSQL-backed tests skip locally without `TEST_DATABASE_URL` and
run in CI.

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
| 14 | Coordinator HTTP Metrics/Health | Complete |
| 15 | Structured Errors & Request IDs | Complete |
| 16 | Upload Size Limits | Complete |
| 17 | Readiness Probes | Complete |
| 18 | Storage-Node gRPC Integration Tests | Complete |
| 19 | Storage-Node HTTP Health & Metrics | Complete |
| 20 | Validated Startup Configuration | Complete |
| 21 | Graceful Shutdown (SIGINT/SIGTERM) | Complete |
| 22 | Storage-Node Transfer Metrics | Complete |
| 23 | End-to-End Upload/Download Test + ID Fix | Complete |
| 24 | File Deletion End to End | Complete |
| 25 | Coordinator-Authoritative Chunking/Replication | Complete |
| 26 | Request-ID Propagation to gRPC | Complete |
| 27 | Durable Replica Recording on Commit | Complete |
| 28 | Replication Task Deduplication | Complete |
| 29 | Orphan-Chunk GC & Stale Upload Reaper | Complete |
| 30 | PostgreSQL-Backed Integration Tests | Complete |
| 31 | Paginated File Listing | Complete |
| 32 | Graceful HTTP Shutdown, CI, Container Builds | Complete |
| 33 | Recovery Source Failover | Complete |
| 34 | Recovery Target Failover & Shutdown-Aware Registration | Complete |
| 35 | Full-Stack & S3 Integration Tests | Complete |
| 36 | Recovery Race, Provenance, Priority & Throttle | Complete |
| 37 | Coordinator HA-Lite, Gateway Failover & Image Publishing | Complete |
| 38 | Legacy RPC & Workspace Cleanup | Complete |
| 39 | Dead RPC, Task Recovery & Docs Cleanup | Complete |
| 40 | gRPC TLS/mTLS | Complete |
| 41 | Shared Rate Limiting | Complete |
