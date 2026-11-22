# Phase 12: Metrics Endpoint & Counter Wiring

## Objective

The `observability` crate defined Prometheus counters and gauges, but nothing
registered them, incremented them, or exposed them over HTTP. This phase wires
the metrics end to end so the running system is actually observable.

## What Was Implemented

### Idempotent registration (`observability`)
- `register_metrics()` now tolerates `prometheus::Error::AlreadyReg`, so it is
  safe to call from multiple entry points or tests in one process.
- Added a unit test asserting the rendered exposition contains the expected
  counter/gauge names.

### `/metrics` HTTP endpoint (`gateway`)
- New `GET /metrics` handler returns the Prometheus text exposition format
  (`text/plain; version=0.0.4`).
- Registered in the router; the gateway calls `register_metrics()` at startup.
- `/metrics` and `/api/v1/health` are always public — the auth middleware now
  bypasses them so probes and scrapers work even when `REQUIRE_AUTH=true`.

### Gateway counters
- `rustystore_uploaded_bytes_total` — incremented per chunk after all replicas
  acknowledge durable storage.
- `rustystore_downloaded_bytes_total` — incremented per chunk streamed to the
  client after SHA-256 verification.
- `rustystore_upload_failures_total` — incremented on any failed upload.

### Coordinator gauges
- `rustystore_active_storage_nodes` — refreshed by the lease monitor from the
  count of healthy nodes.
- `rustystore_under_replicated_chunks` — set by the under-replication detector.
- The coordinator registers metrics at startup.

## Metrics Reference

| Metric | Type | Source | Description |
|---|---|---|---|
| `rustystore_uploaded_bytes_total` | counter | gateway | Total bytes stored across replicas |
| `rustystore_downloaded_bytes_total` | counter | gateway | Total bytes streamed to clients |
| `rustystore_upload_failures_total` | counter | gateway | Failed upload attempts |
| `rustystore_active_storage_nodes` | gauge | coordinator | Healthy storage nodes |
| `rustystore_under_replicated_chunks` | gauge | coordinator | Chunks below their replication factor |

## Crate Changes

- `observability` — idempotent `register_metrics`, `register()` helper, test
- `gateway` — `handlers/health.rs` (`metrics`), `routes.rs`, `auth.rs` (public
  paths), `lib.rs` (register at startup), `handlers/upload.rs`,
  `handlers/download.rs` (counters)
- `coordinator` — `service.rs` (gauges), `main.rs` (register at startup)
- `docs/api/openapi.yaml` — documented `/metrics`

## Verification

- `cargo build --workspace --bins` — clean
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean
- `cargo test --workspace` — 22/22 tests pass (observability metrics test added)
- Smoke test: `GET /metrics` on a running gateway returned
  `rustystore_uploaded_bytes_total`, `rustystore_downloaded_bytes_total`,
  `rustystore_upload_failures_total`, `rustystore_active_storage_nodes`,
  `rustystore_under_replicated_chunks`.

## What Was Deferred (Future Work)

- **Coordinator metrics endpoint** — gauges are registered in the coordinator
  process but not yet exposed over HTTP (only the gateway serves `/metrics`).
- **Histograms** for request latency / chunk size distributions.
- **Grafana dashboard** JSON.

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
