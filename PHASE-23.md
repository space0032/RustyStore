# Phase 23: End-to-End Upload/Download Test and Upload ID Fix

## Objective

Every piece of the upload path had unit or in-process tests, but nothing
exercised the gateway's real HTTP handler against a real coordinator and a real
storage node. Full end-to-end testing was blocked on PostgreSQL. This phase
builds that test with an in-memory coordinator fake, and in doing so exposes a
bug that would have made every real upload fail.

## The Bug

`CoordinatorClient::create_upload_session` generated `upload_id` and `file_id`
on the client and **discarded the IDs returned by the coordinator**:

```rust
let upload_id = Uuid::new_v4();
let file_id = Uuid::new_v4();
let _resp = self.inner.create_upload_session(req).await?;
Ok((upload_id, file_id))
```

The coordinator independently creates a `files` row under its own `file_id` and
returns it. The gateway then:

1. requests chunk plans with its own `upload_id` (only parsed, so harmless),
2. stores all chunks to storage nodes successfully,
3. calls `complete_upload` with its own `file_id`.

`complete_upload` inserts rows into `chunks`, which has
`file_id UUID NOT NULL REFERENCES files(file_id)`. The referenced row belongs to
the coordinator's `file_id`, so the insert violates the foreign key and the RPC
fails with `Status::internal`. The `UPDATE files SET status='committed'` also
matches zero rows. Net effect: an upload transfers and replicates every chunk,
then fails at the last step, leaving orphaned chunks and an `uploading` file
row. No upload could ever succeed against a real coordinator.

## The Fix

`create_upload_session` now parses and returns the coordinator-issued IDs, so
`complete_upload` and `get_manifest` refer to the same file row.

## The Test

`crates/gateway/tests/e2e_upload_download.rs`:

- Starts a real `storage_node::StorageNode` on an ephemeral `TcpListener`.
- Starts an in-memory `ClusterService` fake on another ephemeral listener. The
  fake:
  - issues upload/file IDs like the real coordinator,
  - returns the storage node as the sole replica from `GetChunkPlan`,
  - records the manifest from `CompleteUpload` and **rejects a `file_id` that
    does not match the session** (this is what catches the bug),
  - serves `GetManifest`, `ListNodes`, and `GetStats`.
- Builds the gateway router with `gateway::router` and drives it with
  `tower::ServiceExt::oneshot` — no TCP port, auth, or rate limiting needed.
- Uploads 5 MB + 123 bytes (two 4 MB chunks), asserts `chunk_count == 2`,
  downloads the file, and asserts the bytes are identical.

To support ephemeral ports, `storage_node::StorageNodeServer` gained a public
`serve_with_listener(listener, node, shutdown)` wrapper over the existing
internal `serve_with_listener_shutdown`.

## Verification

- `cargo test --workspace` — 58 pass (was 57)
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean

## What Was Deferred (Future Work)

- A PostgreSQL-backed end-to-end test (the fake validates ID/session semantics
  but not the SQL itself).
- Running the test through the full middleware stack (auth, rate limit, request
  IDs) instead of the bare router.
- Multi-node replication in the e2e test (currently one replica; the handler
  still fans out to the IDs the coordinator returns).
- Recovery after a node dies mid-download (the handler already tries replicas
  in order; only one replica exists in this test).

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
