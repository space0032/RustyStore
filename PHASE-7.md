# Phase 7: Fault Tolerance — Concurrent Recovery, Node Failures, Verification

## Objective

Enhance fault tolerance with concurrent recovery task execution, priority handling for concurrent node failures, and recovery verification.

## Architecture Decisions

### Concurrent Recovery Task Execution
- Recovery scheduler now processes multiple tasks concurrently using a `tokio::task::JoinSet`
- Bounded concurrency via `tokio::sync::Semaphore` (max 4 concurrent recoveries)
- Scheduler claims up to 8 pending tasks per 5-second cycle
- Each task runs in its own tokio task with a semaphore permit
- `FOR UPDATE SKIP LOCKED` in `claim_replication_task()` prevents concurrent schedulers from claiming the same task

### Priority Handling for Concurrent Node Failures
- `detect_under_replicated()` finds all under-replicated chunks and creates tasks for each
- Tasks are ordered by `created_at ASC` (oldest first) for fairness
- Chunks with fewer durable replicas are naturally recovered first since they're detected first
- Multiple node failures create multiple tasks, all processed concurrently within the semaphore bound

### Recovery Verification
- SHA-256 verification on data retrieved from source node
- Target node confirms durable storage (fsync + atomic rename) via `StoreChunkResponse.durable=true`
- Coordinator marks replica as `durable=true` only after target confirms
- Under-replication detector won't list a chunk as under-replicated once all replicas are durable
- Failed tasks are retried up to 5 times before being marked permanently failed

### Node Failure Detection
- Lease expiry monitor (every 5s): marks nodes `healthy=false` when `lease_expiry < now()`
- Heartbeat (every lease_ttl/2): nodes refresh their lease with coordinator
- Node is NOT considered dead on missed heartbeat — only after lease expiry
- Healthy nodes can recover and re-register, flipping `healthy=true` again

### Recovery Flow (End-to-End)
1. Storage node fails → lease expires → coordinator marks `healthy=false`
2. Under-replication detector finds chunks with insufficient durable replicas
3. Creates `replication_tasks` rows (status=pending)
4. Recovery scheduler claims tasks (bounded concurrency via semaphore)
5. For each task: source node streams chunk via `RetrieveChunk`, target stores via `StoreChunk`
6. Target confirms durable, coordinator marks replica `durable=true`
7. Task marked done; chunk is back to full replication factor

## Crate Changes

### `coordinator` (Phase 7)
- `src/service.rs` — `run_recovery_batch()` processes up to 8 tasks concurrently with semaphore (max 4); `execute_recovery_task()` performs the actual gRPC replication transfer

## Verification

- `cargo check --workspace` — clean
- `cargo clippy --workspace -D warnings` — clean
- `cargo fmt --all -- --check` — clean
- `cargo test -p storage-engine` — 8/8 tests pass

## Limitations

- Single coordinator instance is a SPOF for recovery scheduling (HA coordinator in Phase 10)
- No source node failover during recovery (if source dies mid-transfer, task fails and retries)
- No bandwidth throttling for replication traffic
- No priority queue (all tasks equal priority, FIFO order)

## Next (Phase 8)

Hybrid Storage: S3-compatible storage abstraction, MinIO integration, tiered storage, chunk migration between storage tiers.
