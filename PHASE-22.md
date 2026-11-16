# Phase 22: Storage-Node Transfer Metrics

## Objective

Phase 19 gave storage nodes a `/metrics` endpoint, but every series on it was a
process-wide metric registered by `observability::register_metrics()` (gateway
and coordinator counters). Nothing described what the node actually did. This
phase adds storage-specific counters so per-node dashboards and alerting can see
write/delete throughput and detect a node that has stopped receiving work.

## What Was Implemented

Three monotonic counters in `observability`:

| Metric | Type | Meaning |
|---|---|---|
| `rustystore_storage_chunks_stored_total` | counter | Chunks successfully stored by this node |
| `rustystore_storage_bytes_stored_total` | counter | Chunk bytes successfully stored |
| `rustystore_storage_chunks_deleted_total` | counter | Chunks deleted (only when a file was actually removed) |

- Registered by `register_metrics()`, which remains idempotent, so all three
  binaries still start with a complete metric set (zeros on services that never
  store chunks).
- Incremented in `StorageNodeService` **after** success only:
  - `store_chunk` bumps chunks + bytes after the atomic, SHA-256-verified write
    returns `Ok`.
  - `delete_chunk` bumps chunks deleted only when `delete_chunk` reports it
    actually removed the chunk, so deleting a missing chunk and retried deletes
    do not inflate the counter.

Counters (not gauges) are deliberate: a fresh process cannot know how many
chunks already exist on disk, so a "current chunks" gauge would be wrong after a
restart. Counters measure activity since process start, which is what transfer
dashboards need.

## Tests

- `observability::metrics_render_contains_counters_and_is_idempotent` asserts
  all three storage series render and re-registration is a no-op.
- `grpc_store_retrieve_delete_roundtrip` asserts the stored chunk and byte
  counters advanced after a real gRPC store, and that the deleted counter
  advanced after a real gRPC delete.
- `metrics_render_prometheus_text` (storage-node HTTP) asserts the node's
  `/metrics` body contains `rustystore_storage_chunks_stored_total`.

Assertions use `>=` / deltas because Rust runs tests in one process in parallel;
the counters are monotonic so a lower bound is race-free.

## Verification

- `cargo test --workspace` — 57 pass
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean

## What Was Deferred (Future Work)

- Per-node gauges for current chunk count and bytes on disk (needs a startup
  directory scan or a sidecar index).
- Delete byte counters (requires `delete_chunk` to return the removed size).
- Labelled metrics (node id, backend `fs`/`s3`) once a single process serves
  more than one backend.

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
