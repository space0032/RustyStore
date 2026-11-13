# Phase 17: Readiness Probes

## Objective

`/health` only proved the process was alive. Orchestrators (Kubernetes,
load balancers) need a separate signal that a dependency is actually reachable
before routing traffic. This phase splits liveness from readiness and makes the
readiness probes dependency-aware.

## What Was Implemented

### Gateway `/api/v1/ready`
- Probes the coordinator (`ListNodes`) under a 2s timeout.
- `200 {"status":"ready"}` when the coordinator answers.
- `503 {"status":"not_ready","reason":"coordinator unavailable"}` otherwise
  (fails closed).
- Always public: exempt from auth and rate limiting alongside `/health`.

### Coordinator `/ready`
- Pings PostgreSQL (`SELECT 1`) through the new `MetadataStore::ping`.
- `200 {"status":"ready"}` when the database responds, else `503`.
- Readiness is injected as a `ReadyCheck` closure so the HTTP layer does not
  depend on the database directly (and stays unit-testable).

### Liveness unchanged
- Gateway `/api/v1/health` and coordinator `/health` still report process
  liveness without touching dependencies.

## Configuration

No new variables. The coordinator HTTP surface (including `/ready`) listens on
`COORDINATOR_HTTP_ADDR` (default `0.0.0.0:8081`).

## Endpoints

```
$ curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8080/api/v1/ready
503
$ curl -s http://localhost:8080/api/v1/ready
{"status":"not_ready","reason":"coordinator unavailable"}

$ curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8081/ready
200
```

## Crate Changes

- `crates/metadata/src/queries.rs` — `MetadataStore::ping`
- `crates/coordinator/src/http.rs` — `ReadyCheck`, `/ready`, updated serve signatures, tests
- `crates/coordinator/src/main.rs` — build ready check from the store
- `crates/gateway/src/handlers/health.rs` — `ready` handler + tests
- `crates/gateway/src/routes.rs` — `/api/v1/ready` route
- `crates/gateway/src/auth.rs`, `rate_limit.rs` — readiness is always public
- `docs/api/openapi.yaml`, `README.md`

## Verification

- `cargo clippy -p coordinator -p gateway --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean
- `cargo test -p coordinator -p gateway` — 10 + 18 pass
- Live smoke test: gateway `/api/v1/ready` returns `503` with no coordinator;
  `/api/v1/health` still returns `200`

## What Was Deferred (Future Work)

- Gateway readiness also probing at least one healthy storage node.
- Caching the readiness result briefly to avoid a coordinator round-trip on
  every probe.
- Kubernetes probe examples in the deployment docs.

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
