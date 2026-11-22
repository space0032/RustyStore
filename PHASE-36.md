# Phase 36: Recovery Race, Provenance, Priority & Throttle

## Objective

Phase 33 and 34 made recovery resilient to a bad source or target, but it still
tried replicas one at a time, discarded which nodes actually completed the
transfer, treated every task as equal priority, and offered no way to limit
replication bandwidth. This phase makes recovery faster, observable, and
operationally controllable.

## Design

### 1. Parallel source race

`execute_recovery_task` now builds a `FuturesUnordered` of source transfers and
uses the first SHA-256-verified response:

```rust
let mut fetches = FuturesUnordered::new();
for source_node_id in sources {
    fetches.push(async move {
        let result = Self::retrieve_verified_chunk(...).await;
        (source_node_id, result)
    });
}

while let Some((source_node_id, result)) = fetches.next().await {
    if let Ok(data) = result {
        source_and_data = Some((source_node_id, data));
        break;
    }
}
```

Dropping the remaining futures cancels their in-flight transfers, so a slow or
corrupt replica cannot delay recovery. Sources are raced because reading the
same chunk from multiple replicas is safe. Targets are still tried
sequentially: racing writes could create duplicate replicas.

### 2. Persisted recovery provenance

A completed task now records the nodes that actually served the transfer, not
just the nodes named when the task was created:

```rust
store
    .mark_task_done_with_nodes(task.task_id, Some(source_node_id), target_node_id)
    .await?;
```

`MetadataStore::mark_task_done_with_nodes` updates `status`, `source_node_id`,
and `target_node_id`. The older `mark_task_done` remains for tests and the
legacy code path. The full-stack recovery test asserts the completed row names
the surviving source and the spare target.

### 3. Priority queue

`replication_tasks` gained a `priority INTEGER NOT NULL DEFAULT 0` column. The
schema includes `ALTER TABLE ... ADD COLUMN IF NOT EXISTS` for existing
databases, and a queue index:

```sql
CREATE INDEX IF NOT EXISTS idx_replication_tasks_queue
    ON replication_tasks(status, priority DESC, created_at ASC);
```

The under-replication detector sets priority to the number of missing replicas
(`required - actual`, minimum 1). Task claiming and pending-task listing order
by `priority DESC, created_at ASC`, so the most under-replicated chunks are
recovered first.

### 4. Optional recovery bandwidth throttle

`RECOVERY_BANDWIDTH_BYTES_PER_SEC` is parsed in
`CoordinatorServer::serve_with_listener` and passed through the scheduler to
both source retrieval and target push. Unset or `0` means unlimited. The helper
converts bytes and rate into a delay:

```rust
fn throttle_delay(rate: Option<u64>, bytes: usize) -> Option<Duration> {
    let rate = rate.filter(|rate| *rate > 0)?;
    (bytes > 0).then(|| Duration::from_secs_f64(bytes as f64 / rate as f64))
}
```

The throttle is per transfer, not an aggregate cluster-wide budget. That keeps
the implementation simple and avoids a shared token bucket in the coordinator.

## Verification

- `cargo fmt --all` - clean
- `cargo clippy --workspace --all-targets -- -D warnings` - clean
- `cargo clippy -p storage-engine --features s3 --all-targets -- -D warnings` -
  clean
- `cargo test --workspace` - 98 pass (81 unit + 10 integration + 7 PostgreSQL),
  up from 96:
  - `throttle_delay_matches_configured_rate` (coordinator unit test)
  - `replication_task_priority_order_and_actual_nodes` (PostgreSQL)
  - the full-stack recovery test now also verifies persisted source/target

The PostgreSQL and full-stack tests return early locally when no database is
configured. CI provides PostgreSQL, so those paths run there.

## What Was Deferred (Future Work)

- Target failover remains sequential; a hedged write could be added later if
  duplicate-replica handling is designed first.
- Bandwidth throttling is per transfer, not a shared recovery budget.
- Priority is a single integer based on missing replicas; there is no aging or
  per-file/per-tenant priority.
- Coordinator HA/leader election is still future work.

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
