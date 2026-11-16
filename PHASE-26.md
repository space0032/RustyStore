# Phase 26: Request-ID Propagation into gRPC Metadata

## Objective

Phase 15 added `x-request-id` to HTTP requests and responses, and Phase 23's
end-to-end test showed the gateway making coordinator and storage-node gRPC
calls. But the id stopped at the gateway: every downstream call was untagged, so
a slow upload could not be traced from its HTTP request into coordinator
placement or storage-node writes.

Two obstacles made the naive fix wrong:

- The id was a local variable inside the middleware; handlers never saw it.
- The gateway fans out to storage nodes with `tokio::spawn`, and **spawned tasks
  do not inherit task-locals**. A task-local set in the middleware would read as
  empty inside the replica and download tasks.

## Design

### `common::request_id`

A task-local holds the current id:

```rust
tokio::task_local! { static CURRENT_REQUEST_ID: String; }

pub const HEADER: &str = "x-request-id";
pub fn current() -> Option<String>;
pub async fn scope<F: Future>(id: impl Into<String>, future: F) -> F::Output;
pub async fn scope_opt<F: Future>(id: Option<String>, future: F) -> F::Output;
```

`current()` is `None` outside a scope, so non-HTTP entry points (health checks,
background tasks) behave exactly as before. `scope_opt` is the spawn-friendly
form: capture `current()` before `tokio::spawn`, then wrap the spawned future.

### Gateway

- `request_id_middleware` now wraps `next.run(request)` in
  `common::request_id::scope(id, ...)`, so every handler runs with the id
  available.
- `gateway::grpc::request_with_id` wraps outgoing messages and inserts the
  header when a scope is active. Used by the upload streaming client and the
  download retrieve client.
- `store_chunk_to_replicas` captures the id before spawning and re-enters the
  scope inside each replica task.
- `download_file` captures the id before spawning `fetch_and_stream_chunks`, so
  the manifest/node-listing/retrieve calls made by the streaming task are
  tagged too.

### Coordinator client

`CoordinatorClient` routes every request through the same helper, so
`CreateUploadSession`, `GetChunkPlan`, `CompleteUpload`, `GetManifest`,
`DeleteFile`, `ListNodes`, and `GetStats` all carry the header.

Servers can read it with
`request.metadata().get(common::request_id::HEADER)`.

## Tests

Unit (`common`, 5 new): unset outside a scope, visible inside, nested scopes
restore the outer value, and both `scope_opt` cases.

Unit (`coordinator`, 2 new): the client helper attaches the scoped id and omits
the header when no id is scoped.

End-to-end (`upload_then_download_roundtrip`, extended):

- The test applies `gateway::request_id_middleware` to the router and sends
  `x-request-id: e2e-trace-1` on upload, download, and delete.
- The fake coordinator records the header on `CreateUploadSession`,
  `GetChunkPlan`, `CompleteUpload`, `GetManifest`, `ListNodes`, and
  `DeleteFile`.
- After the flow it asserts the recorded list is non-empty and **every** id is
  `e2e-trace-1` — including the `ListNodes` call issued from the spawned
  download task, which proves the re-scoping works.

## Verification

- `cargo test --workspace` — 67 pass (61 unit + 6 integration)
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean

## What Was Deferred (Future Work)

- Server-side spans/logs that read the header (today it is available but not
  emitted by coordinator/storage-node handlers).
- W3C `traceparent` propagation for OpenTelemetry interoperability.
- Returning the resolved id from the coordinator/storage-node so a client can
  confirm downstream correlation.
- Correlation on the coordinator's and storage node's HTTP endpoints (their
  `/health`/`/metrics` are not part of the request-id middleware).

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
