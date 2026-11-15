# Phase 18: Storage-Node gRPC Integration Tests

## Objective

The storage node's gRPC data path (streaming `StoreChunk` / `RetrieveChunk` /
`DeleteChunk`) was only exercised through mocked or direct storage-engine calls.
This phase adds **in-process gRPC tests** that bind a real `TcpListener`, serve
the generated `ChunkServiceServer`, and drive it with a real `ChunkServiceClient`.
No PostgreSQL or Docker is required, so the core durability path is now verified
on every `cargo test`.

## What Was Implemented

### In-process gRPC harness
- `start_node()` builds a `StorageNode` over a `tempfile::TempDir`, binds
  `127.0.0.1:0`, and serves `ChunkServiceServer` via
  `Server::serve_with_incoming(TcpListenerStream)`. The actual bound address is
  returned to the test, so there are no fixed ports or races.
- Clients connect with `ChunkServiceClient::connect("http://<addr>")`.

### Tests
- `grpc_store_retrieve_delete_roundtrip` — streams metadata + data frames to a
  node, streams them back, checks the metadata frame precedes data, verifies
  byte-for-byte equality, then deletes and confirms retrieval fails.
- `grpc_store_rejects_sha_mismatch` — a chunk whose declared SHA-256 does not
  match its bytes is rejected.
- `grpc_retrieve_missing_chunk_not_found` — an unknown chunk returns gRPC
  `NotFound`.

### Bug found and fixed
The test exposed an inconsistency in the filesystem backend:
`FsChunkStore::open_chunk_reader` returned `sha256_hex: String::new()`, whereas
the S3 backend returns the stored hash. `StorageNodeService::retrieve_chunk` now
fills an empty metadata hash from the caller-supplied `expected_sha256` (which
originates from the coordinator manifest). Data integrity is still enforced by
the caller, which re-hashes the streamed bytes.

## Crate Changes

- `crates/storage-node/src/service.rs` — `#[cfg(test)]` gRPC tests; metadata
  hash fallback in `retrieve_chunk`
- `crates/storage-node/Cargo.toml` — dev-dependencies: `tempfile`, `sha2`,
  `hex`, `tokio-stream` (`net`)

## Verification

- `cargo test -p storage-node` — 3 pass
- `cargo test --workspace` — 45 pass (was 42)
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean

## What Was Deferred (Future Work)

- The same in-process harness for the coordinator's `ChunkService` /
  `ClusterService` (needs a database).
- Concurrent transfer tests and backpressure assertions on the streaming paths.
- Propagating `x-request-id` into gRPC metadata (started, deferred because
  tonic 0.11 pins `tower` 0.4 / `http` 0.2 while the gateway uses `tower` 0.5 /
  `http` 1, so a shared tower layer needs a version-aware design).

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
