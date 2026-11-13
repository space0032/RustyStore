# Phase 15: Structured Errors & Request Correlation

## Objective

Gateway errors were plain-text and dropped all detail, and there was no way to
correlate a client complaint with a specific request in the logs. This phase
introduces structured JSON error responses and a request-id middleware.

## What Was Implemented

### Structured JSON errors (`gateway::error`)
Errors render as:

```json
{ "error": { "code": "invalid_argument", "message": "invalid argument: ..." } }
```

| Variant | HTTP | `code` |
|---|---|---|
| `NotFound` | 404 | `not_found` |
| `InvalidArgument` | 400 | `invalid_argument` |
| `Internal` | 500 | `internal` |
| `Grpc` | 502 | `upstream_error` |

`status()` and `code()` are exposed so callers can map errors without parsing
the body.

### Request correlation (`gateway::request_id`)
- Reads an incoming `x-request-id`; if present and non-empty it is preserved,
  otherwise a UUID v4 is generated.
- The id is written onto the request headers, the response headers, and the
  request tracing span (`request{request_id=...}`).
- Applied as the **outermost** layer so every response — including 401, 429,
  timeouts, and errors — carries the header.

## Behavior

```
$ curl -sD - -H 'x-request-id: trace-abc' \
    http://localhost:8080/api/v1/files/not-a-uuid
HTTP/1.1 400 Bad Request
x-request-id: trace-abc

{"error":{"code":"invalid_argument","message":"invalid argument: invalid character: found `n` at 0"}}

$ curl -sD - http://localhost:8080/api/v1/files/not-a-uuid
x-request-id: dd605451-af1a-4dad-af0a-9e504d804ed2
```

## Crate Changes

- `crates/gateway/src/error.rs` — JSON `IntoResponse`, `status()`/`code()`, tests
- `crates/gateway/src/request_id.rs` — new module (resolve + middleware + tests)
- `crates/gateway/src/lib.rs` — module export and outermost layer
- `crates/gateway/Cargo.toml` — `tower` `util` feature (tests use `ServiceExt`)
- `docs/api/openapi.yaml` — `Error` schema + `x-request-id` note

## Verification

- `cargo clippy -p gateway --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean
- `cargo test -p gateway` — 12/12 pass (7 new tests for errors + request ids)
- Live smoke test: client id preserved, UUID generated when absent, JSON error
  body with stable code.

## What Was Deferred (Future Work)

- Include the request id inside the JSON error body (currently in the header).
- Propagate the request id into outgoing gRPC metadata so coordinator and
  storage-node logs join the same trace.
- OpenTelemetry trace-context propagation.

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
