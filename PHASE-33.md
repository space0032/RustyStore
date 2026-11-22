# Phase 33: Recovery Source Failover

## Objective

Replication tasks record a source node when the under-replication detector
creates them. Between creation and execution, that node can die, lose its
replica, or return corrupt bytes. Previously any of those cases failed the task
outright — even when another healthy durable replica of the same chunk was
available — and the task sat in `pending` until a later attempt. This phase
tries every usable replica before giving up, and removes the dead
`crates/replication` stub.

## Design

### 1. Ordered source candidates

`execute_recovery_task` no longer trusts `task.source_node_id`. It builds a
deduplicated, ordered list:

1. the task's recorded source, if that node is healthy;
2. every other healthy node with a `durable = true` replica of the chunk.

The target node is excluded (it is the destination), as are unhealthy nodes and
non-durable replicas. The pure helper is unit-testable:

```rust
fn source_candidates(
    preferred: Option<Uuid>,
    replicas: &[ChunkReplicaRow],
    healthy: &HashSet<Uuid>,
    target: Uuid,
) -> Vec<Uuid>
```

### 2. Per-source verification and failover

Retrieval moved into `retrieve_verified_chunk`, which connects to one node,
streams the chunk, and checks SHA-256 before returning the bytes. The executor
loops over the candidate list:

- on success, it logs when an alternate source was used and continues to the
  target;
- on any error (connection refused, gRPC status, truncated stream, hash
  mismatch), it logs the failed source and tries the next candidate;
- only when every candidate fails does it call `mark_task_failed`, preserving
  the existing retry/backoff behaviour.

Because verification happens per source, a replica with corrupt bytes is
skipped rather than written to the target and then rejected.

### 3. Dead crate removal

`crates/replication` contained only an unused `RecoveryScheduler` placeholder;
the real scheduler lives in `crates/coordinator`. The crate is deleted and
removed from the workspace members, dropping the workspace from 12 to 11
crates. No code depended on it, so `Cargo.lock` did not change.

## Verification

- `cargo fmt --all` — clean
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo test --workspace` — 85 pass (73 unit + 6 integration + 6 PostgreSQL),
  up from 82:
  - `source_candidates_prefers_recorded_source`
  - `source_candidates_falls_back_when_recorded_source_unhealthy`
  - `source_candidates_excludes_target_and_non_durable_replicas`
- The coordinator crate now has 21 tests; the removed `replication` crate
  contributed none.

## What Was Deferred (Future Work)

- Candidate sources are tried sequentially. Trying two sources in parallel and
  using the first verified response would reduce tail latency for large chunks.
- A successful failover does not rewrite `replication_tasks.source_node_id`;
  the row is marked done, so the stale source is only visible in logs.
- If the target itself has died since task creation, the transfer fails and the
  task is retried; target re-selection is not implemented.

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
