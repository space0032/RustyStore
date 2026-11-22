# Phase 27: Record Durable Replicas at Upload Completion

## Objective

Phases 23 and 24 made the gateway exercise real uploads, downloads, and
deletions end to end. Reading `complete_upload` while planning orphan-chunk
garbage collection exposed a correctness bug: **initial uploads never recorded
their replicas**.

`CompleteUpload` inserted `chunks` rows, but `chunk_replicas` was only written
by the recovery path (`execute_recovery_task`). A normal upload therefore
committed chunks with zero replicas in metadata. Consequences:

- `GetManifest` filtered replicas by `durable` and returned an **empty
  `node_ids`** for every chunk, so a real download would fail with "chunk
  unavailable on all replicas" even though the bytes were on disk.
- Under-replication detection counted every chunk as under-replicated.
- `DeleteFile` returned no chunk locations, so chunk bytes were never removed
  (the gateway deletes what the coordinator reports).
- The end-to-end test did not catch this because its fake coordinator invented
  `node_ids: vec![node_id]` in `complete_upload` instead of using what the
  gateway sent.

## The Fix

The gateway already knows the placement it used. `CompleteUpload` now carries it:

```proto
message ChunkManifest {
  string chunk_id = 1;
  int64 chunk_index = 2;
  int64 size_bytes = 3;
  string sha256_hex = 4;
  // Nodes where the gateway durably stored this chunk.
  repeated string node_ids = 5;
}
```

- `ChunkManifestEntry` gains `node_ids`; the upload handler copies the plan's
  node list into each entry.
- `CoordinatorClient::complete_upload` forwards them.
- The coordinator **rejects an empty replica list** with `InvalidArgument`
  rather than committing an undownloadable file.
- `MetadataStore::insert_chunk_with_replicas` inserts the chunk row and one
  `chunk_replicas` row per node (`durable = true`) inside the same transaction
  that commits the file and upload session. Previously `insert_chunk` used the
  pool while a transaction was open, so the "transaction" did not cover chunks.
- The e2e fake now uses `ChunkManifest.node_ids` and fails if it is empty,
  which makes the test able to catch this regression.

## Why It Was Not Caught Earlier

- The fake coordinator fabricated replica IDs, so the gateway never had to send
  them.
- The real coordinator path requires PostgreSQL, and there is no Postgres-backed
  integration test yet (blocked on Docker in this environment).
- Recovery code writes replicas, so the table was not obviously unused.

## Verification

- `cargo test --workspace` — 67 pass (61 unit + 6 integration)
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean
- The e2e test now fails if the gateway omits `node_ids` or the coordinator
  client drops them.

## What Was Deferred (Future Work)

- A PostgreSQL-backed integration test that uploads through the real
  coordinator and asserts `chunk_replicas` rows and manifest node IDs.
- Orphan-chunk garbage collection (failed uploads and crashed deletions still
  leave bytes on nodes; with replicas now recorded, GC can distinguish them).
- Cleaning up files stuck in `uploading` after a crash.

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
