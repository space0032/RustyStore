# Phase 3: Gateway — HTTP API Layer

## Objective

Build the client-facing HTTP Gateway using Axum that handles streaming file uploads, streaming downloads, file metadata, and cluster stats. The Gateway talks to the Coordinator over gRPC for placement decisions and manifest lookups.

## Architecture Decisions

### Streaming Upload (POST /api/v1/files)
- Request body is split into bounded chunks (4 MB default) via `ChunkGenerator`
- Each chunk's SHA-256 is computed incrementally during the split — never buffers more than one chunk in memory
- Each chunk is placed on N distinct replicas concurrently via `store_chunk_to_replicas`
- Replica storage uses streaming gRPC to storage nodes (`StoreChunk` with meta frame + data frames)
- Only after all replicas ack `durable=true` does the Gateway call `CompleteUpload`
- `CompleteUpload` runs inside a Coordinator transaction that flips `files.status='committed'` and `upload_sessions.status='committed'` atomically

### Streaming Download (GET /api/v1/files/{file_id}/download)
- Gateway fetches the manifest from the Coordinator (ordered chunks with locations + SHA-256)
- Chunks are fetched from replicas in order, with failover to the next replica on failure
- Each chunk's SHA-256 is verified on read; mismatch → try next replica
- Chunks are streamed to the client via an mpsc channel — never reassembles the full file in memory
- `ReceiverStream` adapts the channel into an Axum response body

### Architecture
- `AppState` holds the coordinator address and a lazily-connected `CoordinatorClient` (tonic channel connects on first use)
- `Mutex<Option<CoordinatorClient>>` allows shared access across Axum handlers
- All handlers return `GatewayError` which maps to appropriate HTTP status codes (404, 400, 500, 502)

## Proto Changes

Added to `ClusterService`:
- `CreateUploadSession` / `CreateUploadSessionRequest` / `CreateUploadSessionResponse`
- `GetChunkPlan` / `GetChunkPlanRequest` / `GetChunkPlanResponse`
- `CompleteUpload` / `CompleteUploadRequest` / `CompleteUploadResponse` / `ChunkManifest`
- `GetManifest` / `GetManifestRequest` / `GetManifestResponse` / `ChunkLocation`
- `ListNodes` / `ListNodesRequest` / `ListNodesResponse` / `NodeInfo`
- `GetStats` / `GetStatsRequest` / `GetStatsResponse`

## Crate Changes

### `gateway` (new implementation)
- `src/lib.rs` — `run()` binds TCP listener, serves Axum router
- `src/app.rs` — `AppState` with lazy coordinator client
- `src/routes.rs` — REST route registration
- `src/handlers/health.rs` — `GET /api/v1/health`
- `src/handlers/upload.rs` — `POST /api/v1/files` streaming upload
- `src/handlers/download.rs` — `GET /api/v1/files/{id}/download` streaming download
- `src/handlers/files.rs` — `GET`/`DELETE /api/v1/files/{id}`
- `src/handlers/cluster.rs` — `GET /api/v1/cluster/nodes`, `GET /api/v1/cluster/stats`
- `src/chunk_utils.rs` — `ChunkGenerator` (bounded chunking + incremental SHA-256)
- `src/error.rs` — `GatewayError` with `std::error::Error` impl

### `coordinator` (Phase 3 additions)
- `src/service.rs` — implemented gateway-facing RPCs: `create_upload_session`, `get_chunk_plan` (uses `PlacementStrategy::place_replicas`), `complete_upload` (transactional commit), `get_manifest`, `list_nodes`, `get_stats`
- `src/client.rs` — `CoordinatorClient` with methods matching the gateway-facing RPCs
- `src/types.rs` — `ChunkManifestEntry`, `ChunkLocationEntry`, `Manifest`, `NodeInfoEntry`, `ClusterStats`
- `src/lib.rs` — re-exports client types

### `storage-node`
- Updated to match new proto signatures (streaming RPCs, gateway-facing methods as stubs)

## Verification

- `cargo check --workspace` — clean
- `cargo clippy --workspace -D warnings` — clean
- `cargo fmt --all -- --check` — clean
- `cargo test -p storage-engine` — 8/8 tests pass

## Limitations

- Storage node gRPC services are stubs (unimplemented) — real storage node integration lands in Phase 5
- No authentication/authorization (Phase 10)
- No TLS/mTLS (Phase 10)
- Delete is a stub (reference counting for chunk cleanup not yet implemented)
- Concurrent chunk fetching in download is sequential per chunk (bounded concurrency for parallel chunks lands in Phase 5/6)

## Next (Phase 4)

Implement Coordinator: node registration, heartbeat/lease processing, placement strategy, under-replication detection, and recovery scheduling.
