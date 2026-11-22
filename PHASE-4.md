# Phase 4: Coordinator — Node Registration, Health, Leases, Recovery

## Objective

Implement the Coordinator's core responsibilities: storage node registration, heartbeat/lease management, node health tracking, under-replication detection, and recovery task scheduling.

## Architecture Decisions

### Node Registration (Register RPC)
- Storage nodes stream a single `RegisterRequest` with addr, grpc_addr, capacity_bytes
- Coordinator generates a UUID `node_id`, stores the node in PostgreSQL, returns `node_id` + `lease_ttl_ms` (15s)
- Registration is idempotent via `ON CONFLICT (node_id) DO UPDATE`

### Heartbeat & Lease Management (Heartbeat RPC)
- Storage nodes stream `HeartbeatRequest` (node_id, available_bytes, used_bytes, chunk_count)
- Coordinator updates `lease_expiry` (now + 15s), `last_seen`, disk capacity on each heartbeat
- Returns `HeartbeatResponse` with `lease_ttl_ms` and `healthy=true`
- Heartbeat ≠ durability proof — a heartbeat only updates the lease; it does not verify data durability

### Lease Expiry Monitor (Background Task)
- Runs every 5 seconds
- Calls `MetadataStore::expire_leases()` which marks nodes `healthy=false` when `lease_expiry < now()`
- Returns the list of newly expired node IDs
- A node is NOT considered dead immediately on missed heartbeat — only after lease expiry

### Under-Replication Detector (Background Task)
- Runs every 10 seconds
- Calls `MetadataStore::under_replicated_chunks()` to find chunks where durable replica count < file's replication factor
- For each under-replicated chunk, finds a healthy source node with a durable replica and a healthy target node without the chunk
- Creates a `replication_tasks` row (status=pending) for the recovery scheduler

### Recovery Scheduler (Background Task)
- Runs every 5 seconds
- Calls `MetadataStore::claim_replication_task()` using `FOR UPDATE SKIP LOCKED` to atomically claim one pending task
- Executes the task: records replica intent (`insert_replica` with `durable=false`), marks task done
- TODO Phase 5/6: actual gRPC chunk transfer from source to target node with durable confirmation

### Placement Strategy (from Phase 3)
- `place_replicas()` selects N distinct healthy nodes with sufficient disk space
- Sorted by node_id for deterministic placement
- Excludes nodes that already hold the chunk

### Concurrency Safety
- `claim_replication_task()` uses `FOR UPDATE SKIP LOCKED` to prevent concurrent schedulers from claiming the same task
- `expire_leases()` and task completion run inside transactions
- `insert_replica` uses `ON CONFLICT (chunk_id, node_id) DO NOTHING` to prevent duplicate replicas

## Crate Changes

### `coordinator` (Phase 4)
- `src/service.rs` — `CoordinatorService` implements `NodeService` (Register, Heartbeat, ReportCapacity, Check) and `ClusterService` (gateway RPCs). `Coordinator` struct spawns 3 background tasks.
- `src/server.rs` — `CoordinatorServer::run()` starts the Tonic server; background tasks run for process lifetime.
- `src/lib.rs` — exports `Coordinator`, `CoordinatorService`, `CoordinatorServer`, `place_replicas`, `PlacementResult`, client types.

### `metadata` (Phase 4 additions)
- `expire_leases()` — mark nodes unhealthy when lease expires, returns expired node IDs
- `under_replicated_chunks()` — find chunks with durable replicas < replication factor
- `claim_replication_task()` — atomically claim a pending task (FOR UPDATE SKIP LOCKED)
- `mark_task_failed()` — mark task failed or reset to pending for retry
- `get_file_replication_factor()` — get RF for a file

## New Dependencies

- `coordinator`: `chrono`, `tracing`, `async-stream`

## Verification

- `cargo check --workspace` — clean
- `cargo clippy --workspace -D warnings` — clean
- `cargo fmt --all -- --check` — clean
- `cargo test -p storage-engine` — 8/8 tests pass

## Limitations

- Recovery tasks record replica intent but don't perform actual gRPC chunk transfer (Phase 5/6)
- No actual replica durability confirmation from target nodes (Phase 5/6)
- No concurrent recovery task execution (single task per cycle) — bounded concurrency in Phase 7
- No handling of source node failure during recovery (Phase 7)

## Next (Phase 5)

Cluster Communication: storage node gRPC service implementation, heartbeats from nodes to coordinator, node leases, timeouts and retries.
