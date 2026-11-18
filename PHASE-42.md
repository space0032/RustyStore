# Phase 42: Leader Fencing and Recovery QoS

## Objective

Phase 37 serialized coordinator background work with a PostgreSQL lease, but a
leader that paused past lease expiry could still finish a recovery transfer and
publish metadata after a new leader took over. Recovery also had only a
per-transfer bandwidth limit, so concurrent repairs could exceed the intended
total budget, and strict priority ordering could starve low-priority repairs.

Phase 42 adds a monotonic leadership epoch, fences stale claims/completions, and
adds aggregate recovery bandwidth plus priority aging.

## Design

### 1. Leadership epoch

`coordinator_leader` gains an `epoch BIGINT NOT NULL DEFAULT 0` column, applied
idempotently for existing deployments. `try_acquire_leadership` returns
`Option<i64>`:

- A new leader inserts epoch `1`.
- The same leader renewing its lease keeps its epoch.
- A different leader taking over an expired lease increments the epoch.
- A standby that cannot take the lease returns `None`.

`Coordinator` tracks the current epoch in an atomic. The recovery scheduler
snapshots the epoch for each batch and refuses to claim work while the epoch is
zero or the process is not the leader. On an epoch change the new leader fails
`in_progress` tasks claimed by older epochs (or before epochs were recorded), so
the under-replication detector can enqueue fresh repairs without waiting for the
generic stale-task timeout.

### 2. Fenced claims and completions

`replication_tasks` gains `claimed_epoch BIGINT`, recorded by
`claim_replication_task(epoch, aging_secs)`. The claim query additionally
requires that the supplied epoch still matches the live, unexpired
`coordinator_leader` row, so a stale leader cannot take pending work.

Failure and completion are fenced:

- `mark_task_failed_fenced` updates only an `in_progress` task whose
  `claimed_epoch` matches the caller.
- `complete_replication_task_fenced` runs one transaction that verifies the
  leader id, epoch, and unexpired lease, upserts the durable replica, and marks
  the task done only when the claim epoch matches. It records the actual source
  and target used by the transfer.

If completion loses the race, the transaction rolls back. Bytes already written
to the target are not referenced by metadata and are reclaimed by orphan GC.

### 3. Aggregate recovery bandwidth

`RECOVERY_AGGREGATE_BYTES_PER_SEC` caps total recovery traffic across concurrent
transfers. `AggregateThrottle` reserves the next slice of wall-clock time under a
mutex, then sleeps outside the lock. When both aggregate and per-transfer limits
are set, the transfer sleeps for the larger delay rather than adding both, so
the effective rate is the tighter of the two limits.

`RECOVERY_BANDWIDTH_BYTES_PER_SEC` remains the per-transfer limit. Both default
to unlimited (`0` or unset).

### 4. Priority aging

`claim_replication_task` adds an aging bonus to priority:

```text
priority + min(1000, floor(queue_age_seconds / RECOVERY_PRIORITY_AGING_SECS))
```

`RECOVERY_PRIORITY_AGING_SECS` defaults to `60` and may be `0` to disable aging.
The bonus is capped so a very old task cannot outrank emergency work
indefinitely. Ties still resolve oldest-first.

## Verification

- `cargo fmt --all`
- `cargo check --workspace --all-targets`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`: 111 tests passed

PostgreSQL tests now acquire a leader before claiming tasks and cover:

- epoch stability on renewal and increment on takeover;
- stale epochs cannot claim pending work;
- a new leader fails tasks claimed by older epochs on takeover;
- stale leaders cannot fail tasks or publish replicas;
- the current leader completes the task and records two durable replicas;
- aged low-priority work outranks fresh high-priority work.

PostgreSQL-backed tests skip locally without `TEST_DATABASE_URL`; CI runs them
against PostgreSQL.

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
| 42 | Leader Fencing & Recovery QoS | Complete |
