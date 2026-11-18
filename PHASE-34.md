# Phase 34: Recovery Target Failover & Shutdown-Aware Registration

## Objective

Phase 33 made recovery resilient to a dead or corrupt **source** replica. The
**target** was still taken from the task row at creation time, so a target that
died before execution failed the transfer even when another healthy node could
hold the chunk. Separately, a storage node waiting for the coordinator kept
retrying registration forever, ignoring `SIGINT`/`SIGTERM`, so a shutdown during
coordinator downtime waited for the full retry cycle.

This phase treats the recorded target as a hint, tries other eligible nodes, and
lets registration be interrupted by the shared shutdown signal.

## Design

### 1. Ordered target candidates

`execute_recovery_task` now selects targets after it has verified a source:

```rust
fn target_candidates(
    preferred: Uuid,
    healthy: &[Uuid],
    replicas: &[ChunkReplicaRow],
    source: Uuid,
) -> Vec<Uuid>
```

The list is deduplicated and ordered:

1. the task's recorded target, if that node is healthy;
2. every other healthy node, in database order.

The source node and every node that already has a replica row (durable or not)
are excluded, so a target is attempted at most once and recovery does not
re-copy the chunk to a node that already holds it. The helper is pure and
unit-tested.

### 2. Target failover

The target transfer moved into `push_chunk_to_target`, which streams the chunk
to one node and requires a `durable` acknowledgement. The executor loops over
the candidate list:

- on success, it logs when an alternate target was used, records the durable
  replica, and marks the task done;
- on any error (connection refused, gRPC status, non-durable ack), it logs the
  failed target and tries the next candidate;
- only when every candidate fails does it call `mark_task_failed`, preserving
  the existing attempt/backoff behaviour.

Because the target is now chosen at execution time, a task created before a node
died can still complete instead of waiting for a retry.

### 3. Shutdown-aware registration

`register_with_retry` takes the shared `watch::Receiver<bool>` and returns
`Option<Uuid>`:

- if shutdown was already requested, it returns `None` immediately;
- otherwise it selects between `register_once` and `wait_for_shutdown`, so a
  signal interrupts a connect attempt;
- the 3-second retry backoff is also a `select!` against shutdown, so a signal
  during backoff returns immediately.

When registration is cancelled, `main` drains the HTTP probe/metrics server and
returns `Ok(())` instead of entering the gRPC server loop. The normal path is
unchanged: register, start heartbeats, serve gRPC, then drain HTTP.

## Verification

- `cargo fmt --all` — clean
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo test --workspace` — 90 pass (78 unit + 6 integration + 6 PostgreSQL),
  up from 85:
  - `target_candidates_prefers_recorded_target`
  - `target_candidates_skips_unhealthy_target_and_occupied_nodes`
  - `target_candidates_empty_when_no_eligible_node`
  - `registration_stops_when_shutdown_already_requested`
  - `registration_stops_when_shutdown_arrives_while_retrying`
- The coordinator crate now has 24 tests; the storage-node binary adds 2
  registration tests on top of its 11 library tests.

## What Was Deferred (Future Work)

- Candidate sources and targets are tried sequentially. Racing sources and
  using the first verified response would reduce tail latency for large chunks.
- A successful failover does not rewrite `replication_tasks.source_node_id` or
  `target_node_id`; the task is marked done and the actual nodes are only
  visible in logs.
- The heartbeat loop is still detached and stops when the process exits; it is
  not separately signalled.
- Recovery remains coordinator-local; there is no distributed task queue.

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
