# Phase 5: Cluster Communication — Storage Node gRPC

## Objective

Implement the storage node gRPC service with full NodeService (Register, Heartbeat, ReportCapacity, Check) and ChunkService (StoreChunk streaming, RetrieveChunk streaming, DeleteChunk, ReplicateChunk streaming) implementations, plus heartbeat client, node leases, timeouts, and retries.

## Architecture Decisions

### StorageNode struct
- Wraps `FsChunkStore` from storage-engine with node identity (node_id, addr, grpc_addr)
- Shared via `Arc` across gRPC service instances
- `register()` generates a UUID node_id and initializes the store directory
- All chunk operations delegate to `FsChunkStore` (atomic writes + SHA-256 verification)

### gRPC Services

**NodeService** (coordinator-facing):
- `Register`: accepts streaming `RegisterRequest`, returns `RegisterResponse` with node_id and lease_ttl_ms
- `Heartbeat`: streaming heartbeats update lease_expiry on the coordinator side
- `ReportCapacity`: updates node capacity/disk info
- `Check`: health check, always returns serving=true

**ChunkService** (coordinator/gateway-facing):
- `StoreChunk`: streaming transfer — first frame is metadata (ChunkReference), subsequent frames are 64KB data chunks. Stores atomically via `FsChunkStore`, returns `durable=true` only after fsync + atomic rename
- `RetrieveChunk`: streaming response — metadata frame first, then 64KB data frames. Verifies SHA-256 on read
- `DeleteChunk`: removes chunk file, returns false if not found
- `ReplicateChunk`: streaming transfer for inter-node replication (stub in Phase 5)

**ClusterService** (coordinator-to-node):
- `AssignReplica`: coordinator instructs node to accept a replica (stub)
- `NodeHeartbeat`: node-to-coordinator heartbeat stream (stub)
- Gateway-facing RPCs: stubs (create_upload_session, get_chunk_plan, etc.)

### Concurrency
- `StorageNode` is `Clone` (wraps `Arc<StorageNodeInner>`)
- `StorageNodeService` holds `Arc<StorageNode>` and clones it per request
- Interior mutability via `std::sync::RwLock` for node_id, addr, grpc_addr

### Heartbeat/Lease Design
- Coordinator lease TTL: 15 seconds
- Nodes send heartbeats to refresh lease (Phase 5 implements the server side)
- Lease expiry monitored by coordinator background task (Phase 4)
- Heartbeat ≠ durability proof — only updates lease, doesn't verify data

### Timeouts and Retries
- gRPC calls use tonic's default timeouts
- Recovery tasks use `FOR UPDATE SKIP LOCKED` for atomic claiming (Phase 4)
- Replication tasks have `max_attempts=5` with automatic retry (Phase 4)

## Crate Changes

### `storage-node` (Phase 5)
- `src/lib.rs` — exports `StorageNode` and `StorageNodeServer`
- `src/node.rs` — `StorageNode` struct with register, store_chunk, open_chunk_reader, delete_chunk
- `src/service.rs` — full `NodeService`, `ChunkService`, `ClusterService` implementations; `StorageNodeServer::run()`
- `src/server.rs` — `StorageNodeServer::run()` wires Tonic services to TCP listener

### Dependencies added
- `storage-node`: `async-stream`, `tracing`

## Verification

- `cargo check --workspace` — clean
- `cargo clippy --workspace -D warnings` — clean
- `cargo fmt --all -- --check` — clean
- `cargo test -p storage-engine` — 8/8 tests pass

## Limitations

- Storage node doesn't yet send heartbeats to coordinator (client-side heartbeat loop not implemented)
- `ReplicateChunk` and `AssignReplica` are stubs (Phase 6)
- Node restart recovery: store directory is re-initialized, node must re-register
- No TLS/mTLS (Phase 10)

## Next (Phase 6)

Replication: initial replica creation, replica acknowledgements, replication factor enforcement, replica placement.
