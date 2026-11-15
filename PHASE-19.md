# Phase 19: Storage-Node HTTP Health & Metrics

## Objective

The coordinator gained a dedicated HTTP surface in Phase 14, but storage nodes
only exposed gRPC. Orchestrators therefore had no way to check a node's liveness
or readiness, and Prometheus had no storage-node scrape target. This phase gives
each storage node a small HTTP sidecar with liveness, readiness, and metrics.

## What Was Implemented

### `storage_node::http`
- `GET /health` — liveness; `200 {"status":"ok","service":"storage-node"}`.
- `GET /ready` — readiness; probes the storage volume via `capacity()` and
  requires the node to have registered with the coordinator. `503` with
  `{"status":"not_ready","reason":"storage volume unavailable"}` otherwise.
- `GET /metrics` — Prometheus text exposition of the process registry.
- `router` / `serve` / `serve_listener` mirror the coordinator API so tests can
  bind an ephemeral `TcpListener`.

### Startup ordering
The HTTP server is started **before** registration. A storage node that cannot
reach its coordinator still answers `/health`, and `/ready` stays `503` until
registration succeeds. This makes Kubernetes liveness probes work during
coordinator outages instead of failing every probe.

### Binary wiring
- `STORAGE_NODE_HTTP_ADDR` (default `0.0.0.0:8082`).
- The binary now calls `observability::register_metrics()` so `/metrics` is not
  empty.
- `docker-compose.yml` maps `8082`, `8083`, and `8084` for the three nodes.

## Configuration

| Variable | Default | Purpose |
|---|---|---|
| `STORAGE_NODE_HTTP_ADDR` | `0.0.0.0:8082` | HTTP health/readiness/metrics listener |

## Endpoints

```
$ curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8082/health
200
$ curl -s http://localhost:8082/health
{"status":"ok","service":"storage-node"}

# Before the coordinator is reachable:
$ curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8082/ready
503
```

## Crate Changes

- `crates/storage-node/src/http.rs` — router, handlers, serve helpers, tests
- `crates/storage-node/src/lib.rs` — export the `http` module
- `crates/storage-node/src/main.rs` — spawn the HTTP server, register metrics
- `crates/storage-node/Cargo.toml` — `axum = "0.7"`
- `README.md`, `.env.example`, `docker-compose.yml`

## Verification

- `cargo test --workspace` — 50 pass (was 45)
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean
- Live smoke test with the coordinator unreachable:
  - `/health` → `200`
  - `/ready` → `503`
  - `/metrics` → Prometheus text (15 `rustystore_*` series)

## What Was Deferred (Future Work)

- Storage-node-specific gauges (chunks stored, bytes stored) rather than only
  the shared registry.
- Healthchecks in `docker-compose.yml` that drive orchestrator restarts.
- Authentication on the storage-node HTTP port (it is intended for a private
  network, like the coordinator's).

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
