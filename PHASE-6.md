# Phase 6: Replication — Replica Creation, Acknowledgements, RF Enforcement

## Objective

Implement the full replication lifecycle: initial replica creation during upload, replica acknowledgements (durable confirmation), replication factor enforcement, replica placement, and automatic recovery via gRPC data transfer between nodes.

## Architecture Decisions

### Initial Replica Creation (Upload Path)
- Gateway splits file into bounded chunks via `ChunkGenerator` (Phase 3)
- For each chunk, gateway calls `get_chunk_plan` from Coordinator to get N distinct target nodes
- Gateway streams chunk to all N replicas concurrently via `StoreChunk` gRPC
- Each storage node stores atomically (temp → fsync → rename) and verifies SHA-256
- Each replica replies `durable=true` only after confirming durable storage
- Gateway collects all N acks, then calls `CompleteUpload` on Coordinator
- Coordinator flips `files.status='committed'` and `upload_sessions.status='committed'` in one transaction

### Replica Acknowledgements
- A replica is durable only after: SHA-256 verified + fsync + atomic rename + gRPC ack
- `StoreChunkResponse.durable=true` confirms durable storage
- Gateway never considers a chunk replicated until all N replicas ack durable

### Replication Factor Enforcement
- Coordinator's `place_replicas()` selects N distinct healthy nodes with sufficient disk
- `UNIQUE(chunk_id, node_id)` constraint prevents duplicate replica rows
- `durable` flag on `chunk_replicas` tracks confirmation status
- Only durable replicas count toward the replication factor

### Replica Placement
- Deterministic: nodes sorted by node_id for stable placement
- Excludes nodes that already hold the chunk
- Requires sufficient available disk space on target

### Automatic Recovery (Coordinator Background Tasks)
Three background tasks (Phase 4 + Phase 6):

1. **Lease Expiry Monitor** (every 5s): Marks nodes unhealthy when `lease_expiry < now()`
2. **Under-Replication Detector** (every 10s): Finds chunks where durable replicas < RF, creates `replication_tasks` rows
3. **Recovery Scheduler** (every 5s): Claims pending tasks via `FOR UPDATE SKIP LOCKED`, performs gRPC data transfer:
   - Claims task atomically (prevents concurrent schedulers from double-processing)
   - Looks up source and target node gRPC addresses from metadata
   - Connects to source node's `RetrieveChunk`, streams chunk data
   - Verifies SHA-256 of retrieved data
   - Connects to target node's `StoreChunk`, pushes chunk data
   - Target stores atomically, replies `durable=true`
   - Coordinator marks replica `durable=true` in metadata
   - Marks task as done (or failed with retry up to 5 attempts)

### Concurrency Safety
- `claim_replication_task()` uses `FOR UPDATE SKIP LOCKED` for atomic task claiming
- Recovery tasks are processed one at a time per scheduler cycle (bounded concurrency)
- Failed tasks are retried up to 5 times before being marked permanently failed

## Crate Changes

### `coordinator` (Phase 6)
- `src/service.rs` — `run_recovery_cycle()` now performs actual gRPC replication:
  - Retrieves chunk data from source node via `RetrieveChunk` streaming
  - Verifies SHA-256 integrity
  - Pushes data to target node via `StoreChunk` streaming
  - Confirms durable storage before marking replica durable
  - Handles task failures with retry logic
- `src/Cargo.toml` — added `sha2`, `hex` dependencies

### `metadata` (Phase 6)
- `src/queries.rs` — added `get_chunk(chunk_id)` to look up chunk SHA-256 and size

### `storage-node` (Phase 6)
- `src/service.rs` — `replicate_chunk()` remains a stub (control-plane RPC; data transfer coordinated by Coordinator)
- `src/node.rs` — `StorageNode::store_chunk()` used by recovery for target-side storage

## Proto Changes
None (Phase 6 uses existing proto contracts: StoreChunk, RetrieveChunk, ReplicateChunkRequest)

## Verification

- `cargo check --workspace` — clean
- `cargo clippy --workspace -D warnings` — clean
- `cargo fmt --all -- --check` — clean
- `cargo test -p storage-engine` — 8/8 tests pass

## Limitations

- Recovery processes one task per 5-second cycle (no parallel task execution yet — Phase 7)
- No source node failover during recovery (if source dies mid-transfer, task fails and retries)
- No bandwidth throttling for replication traffic
- No chunk deduplication during recovery (always transfers full chunk)

## Next (Phase 7)

Fault Tolerance: node failure detection, automatic re-replication, recovery verification, concurrent task execution, handling of concurrent node failures.
