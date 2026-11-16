# Phase 24: File Deletion End to End

## Objective

`DELETE /api/v1/files/{id}` was a stub: it returned `200 {"status":"deleted"}`
and did nothing. Clients believed data was gone while chunks stayed on disk and
metadata stayed in PostgreSQL. This phase implements deletion across the
coordinator, gateway, and storage nodes, and proves it with the end-to-end test.

## Protocol

`ClusterService` gains:

```proto
rpc DeleteFile(DeleteFileRequest) returns (DeleteFileResponse);

message DeleteFileRequest { string file_id = 1; }

message DeleteFileResponse {
  bool deleted = 1;
  repeated ChunkLocation chunks = 2;
}
```

The coordinator returns the chunk locations it removed so the caller can delete
the bytes.

## Ordering: Metadata First, Bytes Second

The coordinator deletes metadata before the gateway deletes bytes:

1. Look up the file (404 if absent).
2. Read `chunks_for_file` and each chunk's `replicas_for_chunk`, building the
   response locations.
3. `DELETE FROM files WHERE file_id=$1`. `ON DELETE CASCADE` removes `chunks`,
   `chunk_replicas`, `replication_tasks`, and `upload_sessions`.
4. Return the collected locations.

If step 4's byte deletion fails (node down, timeout), the chunks become
**unreferenced** — garbage, but never dangling metadata. The reverse order would
let a metadata-delete failure leave a live file pointing at deleted bytes.

## Gateway

- `CoordinatorClient::delete_file` returns `Vec<(chunk_id, node_ids)>`.
- The handler resolves node IDs to gRPC addresses via `ListNodes`, then calls
  `DeleteChunk` on every replica, concurrently per chunk is unnecessary here
  (deletes are small and idempotent), and counts successes/failures.
- Response: `{ status, file_id, chunks, chunk_replicas_deleted,
  chunk_replicas_failed }`. Partial byte failures still return 200 because the
  file is logically gone; the counts make the condition observable.
- `GatewayError::coordinator` maps gRPC `NotFound` to a 404 instead of the
  generic 502. `GET /files/{id}` and `GET /files/{id}/download` now return 404
  for missing files as well.

## Tests

The existing end-to-end test now continues past download:

1. Assert two chunk files exist in the storage directory.
2. `DELETE` the file; assert `chunks == 2`, `chunk_replicas_deleted == 2`,
   `chunk_replicas_failed == 0`.
3. Assert the storage directory is empty (bytes actually removed).
4. `GET /download` and `GET /files/{id}` both return 404.

The fake coordinator implements `DeleteFile` by removing its in-memory file and
returning the recorded chunk locations, so the test exercises the real gateway
handler and the real storage node's `DeleteChunk`.

Two unit tests cover the error mapping: `coordinator_not_found_maps_to_404` and
`coordinator_unavailable_maps_to_502`.

## Verification

- `cargo test --workspace` — 60 pass (was 58)
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean

## What Was Deferred (Future Work)

- Concurrent replica deletion per chunk (currently sequential).
- Reference counting / content-addressed dedup means a chunk could be shared;
  today each upload gets fresh chunk IDs, so deleting a file never removes data
  used by another file.
- A garbage-collection sweep for unreferenced chunks left by partial deletes.
- A PostgreSQL-backed deletion test (the fake validates the contract, not the
  cascade SQL).

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
