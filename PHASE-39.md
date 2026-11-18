# Phase 39: Dead RPC, Task Recovery & Docs Cleanup

## Objective

Phase 38 removed the legacy coordinator-to-node RPC surface, but a second
unimplemented RPC and several dead APIs remained. Recovery tasks also had two
correctness gaps: retry limits ignored each task's `max_attempts`, and tasks
abandoned by a crashed coordinator stayed `in_progress` forever. Operations
documentation still described removed health checks, wrong metrics ports, and a
manual migration step. This phase removes the remaining dead surface and fixes
the recovery and documentation gaps.

## Design

### 1. Remove `ChunkService::ReplicateChunk`

Replication is orchestrated by the coordinator: it claims a task, streams the
chunk from a healthy source with `RetrieveChunk`, and writes it to the target
with `StoreChunk`. The `ReplicateChunk` RPC was a leftover node-to-node design
that always returned `unimplemented`.

`proto/storage.proto` no longer declares `ReplicateChunk` or its request/response
messages, and the storage-node service no longer implements the stub. The
generated `ChunkService` surface is now exactly:

- `StoreChunk` (streaming)
- `RetrieveChunk` (streaming)
- `DeleteChunk`
- `ListChunks` (streaming)

### 2. Remove dead `StorageNode` address API

`StorageNode::register` was unused because registration is performed by the node
binary as a coordinator client. The node also stored `addr`/`grpc_addr` fields
that no caller read after registration moved out of the node state.

`StorageNode::new` now takes only the storage path. The `register`, `addr`, and
`grpc_addr` methods and their `RwLock` fields are gone; all call sites
(main binary, storage-node tests, gateway e2e test, full-stack tests) use the
path-only constructor.

### 3. Honor per-task retry limits

`MetadataStore::mark_task_failed` previously compared attempts against a
hardcoded `5`. It now compares against the task row's `max_attempts` in SQL, so
per-task retry policy is authoritative and no Rust-side constant can drift.

### 4. Fail abandoned recovery tasks

A coordinator that crashes after claiming a task leaves it `in_progress`. The
under-replication detector already treats such rows as inactive after five
minutes, but the rows themselves never changed state.

The leader's lease monitor now calls `fail_stale_replication_tasks(300)` on each
tick. Rows stuck `in_progress` for more than five minutes are marked `failed`
with `attempts = max_attempts`, which makes the detector enqueue fresh work and
prevents an old row from being reclaimed as a duplicate later. Fresh
`in_progress` rows are untouched.

### 5. Correct operations documentation and config

`docs/operations/README.md` now reflects the actual services:

- The coordinator self-migrates on startup; there is no separate `sqlx migrate`
  step.
- Storage nodes no longer expose gRPC `Check`; health and readiness are HTTP
  endpoints on the storage-node HTTP port.
- Metrics are served on the gateway (`:8080`), coordinator (`:8081`), and
  storage-node (`:8082`) HTTP ports, not a single `:9090` endpoint.

The unused `PROMETHEUS_ADDR`, `HEARTBEAT_INTERVAL_MS`, and `LEASE_TTL_MS`
entries are removed from `.env.example` (the coordinator hardcodes its lease
timing). The dead `common::Config` struct and its `envy` dependency are removed;
the `serde` dependency remains because `common::types` uses it.

The replication sequence diagram in `docs/architecture/overview.md` now shows
the coordinator's `RetrieveChunk` + `StoreChunk` orchestration instead of the
removed node-to-node `ReplicateChunk` call.

## Verification

- `cargo fmt --all`
- `cargo check --workspace --all-targets`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`: 106 tests passed

Added `replication_task_retry_limit_and_stale_cleanup`, a PostgreSQL-backed test
covering `max_attempts` and stale `in_progress` cleanup. PostgreSQL-backed and
full-stack tests still return early locally without a database and run in CI.

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
