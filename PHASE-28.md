# Phase 28: Deduplicate Replication Tasks

## Objective

The under-replication detector runs every 10 seconds. For each chunk whose
durable replica count is below the file's replication factor it picked a target
and inserted a **new** `replication_tasks` row, without checking whether an
earlier task for that chunk was still pending or running.

When replication is slower than the detector interval — or repeatedly failing —
this produces a task storm:

- Every tick adds another pending task for the same chunk.
- The recovery scheduler then copies the same chunk to different targets,
  over-replicating and wasting bandwidth/disk.
- `attempts` bookkeeping is spread across duplicate rows instead of one task
  that eventually reaches `failed`.

There is also a crash case: `claim_replication_task` moves a task to
`in_progress`; if the coordinator dies mid-copy nothing resets it, so a naive
"skip if any task exists" check would block that chunk's recovery forever.

## The Fix

`MetadataStore::has_active_replication_task(chunk_id)`:

```sql
SELECT EXISTS (
    SELECT 1 FROM replication_tasks
    WHERE chunk_id=$1
      AND (
        status='pending'
        OR (status='in_progress' AND updated_at > now() - INTERVAL '5 minutes')
      )
)
```

- Pending tasks always suppress a new one.
- In-progress tasks suppress duplicates only while fresh; rows older than five
  minutes are treated as abandoned, so a crashed coordinator does not wedge
  recovery.
- `detect_under_replicated` skips chunks with an active task and re-evaluates on
  the next tick.

This intentionally replicates one target at a time per chunk: if a chunk needs
two more replicas, the next one is scheduled after the first completes. That is
backpressure, not a delay — chunks are only a few MB.

## Why It Was Not Caught Earlier

- The bug only appears when replication is slower than the 10 s detector tick,
  which needs a real database, storage nodes, and induced failure.
- There is no PostgreSQL-backed integration test yet (blocked on Docker in this
  environment).

## Verification

- `cargo test --workspace` — 67 pass (61 unit + 6 integration)
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean

The new SQL path requires PostgreSQL and is therefore not covered by the
in-process suite; it is a small `EXISTS` query exercised by the same recovery
loop that already had integration coverage at the algorithm level.

## What Was Deferred (Future Work)

- A partial unique index (`UNIQUE (chunk_id) WHERE status IN
  ('pending','in_progress')`) for multi-coordinator safety.
- A reaper that resets stale `in_progress` tasks to `pending` and increments
  `attempts`, instead of relying on the query's age filter.
- Making the five-minute staleness window configurable.
- PostgreSQL-backed recovery tests (under-replication → task → replica count).

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
