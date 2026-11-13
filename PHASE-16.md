# Phase 16: Upload Size Limits

## Objective

The gateway accepted uploads of any size. Combined with unlimited chunking and
replication, a single client could fill storage and saturate the network. This
phase adds a configurable upload size ceiling enforced both up front and during
streaming.

## What Was Implemented

### Configuration
| Variable | Default | Meaning |
|---|---|---|
| `MAX_UPLOAD_BYTES` | `0` (unlimited) | Maximum accepted upload size |

Stored on `AppState`; `AppState::with_max_upload_bytes` allows explicit wiring
and deterministic tests.

### Two-stage enforcement
1. **Early check** — if `content-length` exceeds the limit, the request is
   rejected with `413 Payload Too Large` before creating an upload session or
   contacting storage nodes.
2. **Streaming check** — the running byte total is accumulated per chunk; the
   upload is aborted as soon as it would exceed the limit. This covers chunked
   requests without a trustworthy `content-length`.

### Error surface
New `GatewayError::PayloadTooLarge` variant → HTTP `413`, code
`payload_too_large`, with a message stating the limit and observed size.

## Behavior

```
$ MAX_UPLOAD_BYTES=10 ...
$ curl -X POST --data-binary @100-byte-file http://localhost:8080/api/v1/files
HTTP/1.1 413 Payload Too Large
x-request-id: b6019315-...

{"error":{"code":"payload_too_large","message":"payload too large: content-length 100 exceeds limit of 10 bytes"}}
```

## Crate Changes

- `crates/gateway/src/error.rs` — `PayloadTooLarge` (413 / `payload_too_large`)
- `crates/gateway/src/app.rs` — `max_upload_bytes` field, env read, builder
- `crates/gateway/src/handlers/upload.rs` — early + streaming checks, `exceeds_limit` helper, tests
- `crates/gateway/src/lib.rs` — startup log of the configured limit
- `docs/api/openapi.yaml` — `payload_too_large` code
- `README.md`, `.env.example` — configuration

## Verification

- `cargo clippy -p gateway --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean
- `cargo test -p gateway` — 16/16 pass (4 new: 3 limit helper + 1 error mapping)
- Live smoke test: 100-byte upload with a 10-byte limit returns `413` JSON

## What Was Deferred (Future Work)

- **Reclaim orphaned chunks** — chunks stored before a streaming limit is hit
  are not deleted by a sweeper.
- **Per-user quotas** rather than a single global ceiling.
- **Early `413` via `Content-Length` middleware** before routing (currently in
  the handler).
- **Bandwidth throttling** independent of total size.

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
