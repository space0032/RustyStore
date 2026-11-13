# Phase 14: Coordinator HTTP Metrics & Health

## Objective

Phase 12 registered coordinator gauges (`rustystore_active_storage_nodes`,
`rustystore_under_replicated_chunks`) but only the gateway exposed them over
HTTP, so the coordinator's own state was not scrapable. This phase adds a small
HTTP surface to the coordinator on a dedicated port, separate from gRPC.

## What Was Implemented

### `coordinator::http`
- `GET /metrics` — Prometheus text exposition (`text/plain; version=0.0.4`).
- `GET /health` — JSON liveness probe (`{"status":"ok","service":"coordinator"}`).
- `serve(addr)` binds a `TcpListener` and serves until exit.
- `serve_listener(listener)` accepts an already-bound listener (testable).

### Wiring (`main.rs`)
- The coordinator spawns the HTTP server as a background task alongside the
  gRPC server.
- Address comes from `COORDINATOR_HTTP_ADDR` (default `0.0.0.0:8081`).

### Dependency
- Added `axum = "0.7"` to the coordinator (already used by the gateway, so the
  version unifies across the workspace).

## Configuration

| Variable | Default | Purpose |
|---|---|---|
| `COORDINATOR_HTTP_ADDR` | `0.0.0.0:8081` | Metrics/health HTTP listen address |

## Endpoints

```
$ curl -s http://localhost:8081/health
{"status":"ok","service":"coordinator"}

$ curl -s http://localhost:8081/metrics | grep rustystore_
rustystore_active_storage_nodes 0
rustystore_under_replicated_chunks 0
...
```

## Crate Changes

- `crates/coordinator/src/http.rs` — new module (router, handlers, serve, tests)
- `crates/coordinator/src/lib.rs` — `pub mod http`
- `crates/coordinator/src/main.rs` — spawn HTTP server task
- `crates/coordinator/Cargo.toml` — `axum` dependency

## Verification

- `cargo build -p coordinator --bins` — clean
- `cargo clippy -p coordinator --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean
- `cargo test -p coordinator` — 8/8 pass (5 placement + 3 HTTP)
  - `metrics_render_prometheus_text` — gauges present in output
  - `health_returns_ok` — 200 with service name
  - `server_serves_metrics_over_http` — real `TcpListener` round-trip over HTTP

The HTTP round-trip test binds an ephemeral port and performs a raw
`GET /metrics`, so the endpoint is verified without PostgreSQL.

## What Was Deferred (Future Work)

- Serve coordinator HTTP with graceful shutdown tied to the gRPC server.
- Per-endpoint readiness that also probes the database connection.
- OpenMetrics content negotiation.

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
