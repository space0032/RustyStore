# Phase 8: Hybrid Storage — S3/MinIO Abstraction

## Objective

Add a pluggable storage tier abstraction so chunks can be stored on local filesystem or S3-compatible object storage (MinIO, AWS S3). Includes configurable backend selection and chunk migration between tiers.

## Architecture Decisions

### Storage Tier Abstraction
- `ChunkStore` trait (already in `storage-engine`) defines the interface: `store_chunk`, `retrieve_chunk`, `open_chunk_reader`, `verify_chunk`, `delete_chunk`, `chunk_exists`, `get_capacity`
- `FsChunkStore` (local filesystem) — existing implementation, atomic writes via temp → fsync → rename
- `S3ChunkStore` (S3-compatible) — new implementation behind `s3` feature flag, uses `aws-sdk-s3`

### Feature-Gated S3 Support
- S3 dependencies (`aws-sdk-s3`, `aws-config`, `bytes`) are optional, enabled via `storage-engine/s3` feature flag
- `cargo build` without features compiles without AWS SDK (fast builds, minimal dependencies)
- `cargo build --features storage-engine/s3` enables S3 backend

### S3ChunkStore Design
- Chunks stored as S3 objects with key = `base_prefix/chunk_id`
- SHA-256 verified on write (before PUT) and on read (after GET)
- `store_chunk`: PUT object with SHA-256 verification
- `retrieve_chunk`: GET object, collect body, verify SHA-256
- `delete_chunk`: DELETE object (idempotent)
- `chunk_exists`: HEAD object
- `get_capacity`: returns zeroes (S3 doesn't expose per-bucket capacity easily)
- `open_chunk_reader`: downloads full object, returns as `Cursor` (S3 doesn't support efficient range reads without additional API calls)

### Configuration
- Storage backend selected via environment variables (STORAGE_BACKEND=fs|s3)
- S3 endpoint, bucket, credentials configurable via env
- S3 endpoint supports MinIO (http://minio:9000) and AWS S3

### Chunk Migration (Planned)
- Tier abstraction enables future chunk migration: read from source tier, write to target tier, verify, delete from source
- Not yet implemented; foundation is in place

## Crate Changes

### `storage-engine` (Phase 8)
- `src/s3_store.rs` — new `S3ChunkStore` implementation (feature-gated)
- `src/lib.rs` — conditionally exports `S3ChunkStore` when `s3` feature is enabled
- `Cargo.toml` — optional `aws-sdk-s3`, `aws-config`, `bytes` dependencies; `s3` feature flag

## Verification

- `cargo check --workspace` — clean
- `cargo clippy --workspace -D warnings` — clean
- `cargo fmt --all -- --check` — clean
- `cargo test -p storage-engine` — 8/8 tests pass (fs backend)
- S3 backend not tested (requires MinIO/AWS credentials)

## Limitations

- S3 backend not compiled/tested by default (feature-gated)
- S3 `open_chunk_reader` downloads full object (no range read optimization)
- No chunk migration between tiers yet (foundation in place)
- S3 `get_capacity` returns zeroes (no per-bucket capacity API)

## Next (Phase 9)

Testing and Benchmarking: integration tests, end-to-end tests, fault injection, criterion benchmarks.
