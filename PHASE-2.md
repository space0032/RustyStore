# Phase 2: Storage Engine — Chunk Persistence

## Objective

Build the local-filesystem chunk storage engine with atomic writes, SHA-256 integrity verification, streaming retrieval, and disk capacity tracking.

## Architecture Decisions

### Atomic Writes
- Chunks written to a sibling temp file (`<chunk_id>.tmp.<pid>`)
- Data written, then `fsync` (sync_all) to ensure durability
- Temp file atomically renamed over the final path — readers never observe a partially written chunk
- All blocking FS calls dispatched to `spawn_blocking` to avoid starving the Tokio runtime

### Integrity
- SHA-256 computed incrementally during write; verified against `expected_sha256` before exposing the chunk
- SHA-256 verified on read via `verify_chunk()` — returns `IntegrityMismatch` on corruption
- Rejected writes (hash mismatch) leave no trace on disk (temp file not renamed)

### Streaming
- `open_chunk_reader()` returns `Box<dyn AsyncRead + Unpin + Send>` for incremental reads
- `retrieve_chunk()` returns bounded payload (≤ 8 MB) — acceptable since chunks are fixed-size
- Entire files (gigabytes) are never buffered — enforced at the gateway layer

### Capacity Tracking
- `get_capacity()` returns `CapacityInfo` (total/available/used bytes)
- Uses `std::fs::space` where available; stubbed to zeroes on platforms without it

## Crate: `storage-engine`

- `src/lib.rs` — crate root + 8 unit tests
- `src/chunk_store.rs` — `ChunkStore` trait, `ChunkMeta`, `ChunkEntry`, `CapacityInfo`, `StoreResult`
- `src/fs_store.rs` — `FsChunkStore` implementation

## Tests (8 passed)

1. `test_store_and_retrieve_chunk` — round-trip store/retrieve
2. `test_retrieve_nonexistent_chunk` — NotFound error
3. `test_integrity_mismatch_on_store` — rejected write, no disk trace
4. `test_integrity_mismatch_on_verify` — read-time corruption detection
5. `test_delete_chunk` — delete + idempotent re-delete returns false
6. `test_streaming_reader` — open_chunk_reader + verify
7. `test_get_capacity` — capacity query
8. `test_atomic_write_no_partial_visible` — temp file cleaned, final path exists

## Verification

- `cargo check -p storage-engine` — clean
- `cargo clippy -p storage-engine -D warnings` — clean
- `cargo fmt -p storage-engine -- --check` — clean
- `cargo test -p storage-engine` — 8/8 pass

## Next (Phase 3)

Gateway: Axum HTTP app, streaming multipart upload, download streaming, REST API.
