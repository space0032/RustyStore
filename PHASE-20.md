# Phase 20: Validated Startup Configuration

## Objective

Every binary had its own copy of `env_or` / `env_parse`, and the numeric helpers
silently ignored unparsable values:

```rust
std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
```

A typo such as `REPLICATION_FACTOR=three`, `MAX_UPLOAD_BYTES=10MB`, or
`REQUIRE_AUTH=maybe` therefore started the process with the default — for
`REQUIRE_AUTH` that meant **authentication silently disabled**. This phase makes
configuration failures loud, typed, and test-covered.

## What Was Implemented

### `common::config`
- `ConfigError` (`thiserror`) with variants `InvalidValue`, `InvalidAddr`,
  `NotPositive`, and `Empty`. Messages name the offending key and value.
- `env_string` — read with a default.
- `parse_env` / `parse_value` — parse with a default; invalid input is an error,
  not a fallback.
- `parse_addr` / `parse_addr_value` — same for `SocketAddr`.
- `require_positive` and `require_non_empty` — range checks.
- Six unit tests cover valid/invalid parsing, address parsing, and range checks
  without touching process-global environment variables.

### Binary wiring
- **Coordinator** — `COORDINATOR_ADDR` / `COORDINATOR_HTTP_ADDR` via
  `parse_addr`; `REPLICATION_FACTOR` and `CHUNK_SIZE_BYTES` parsed and required
  to be positive.
- **Storage node** — `NODE_ADDR` / `STORAGE_NODE_HTTP_ADDR` via
  `parse_addr_value` / `parse_addr`; `STORAGE_PATH` required non-empty;
  `NODE_CAPACITY_BYTES` required positive.
- **Gateway** — `GATEWAY_ADDR` via `parse_addr`; `COORDINATOR_ADDR` required
  non-empty; `MAX_UPLOAD_BYTES`, `REQUIRE_AUTH`, `RATE_LIMIT_RPS`, and
  `RATE_LIMIT_BURST` now go through `parse_env` / `parse_value` at startup.
- `AuthConfig::from_env` and `RateLimitConfig::from_env` became fallible
  `try_from_env` constructors; `gateway::run` propagates the error.
- `AppState::new` no longer reads the environment; `run` parses the limit and
  applies it with `with_max_upload_bytes`, keeping tests deterministic.

## Examples

```
$ GATEWAY_ADDR=not-an-address ./gateway
Error: InvalidAddr { key: "GATEWAY_ADDR", value: "not-an-address" }

$ REQUIRE_AUTH=maybe ./gateway
Error: InvalidValue { key: "REQUIRE_AUTH", value: "maybe" }

$ MAX_UPLOAD_BYTES=10MB ./gateway
Error: InvalidValue { key: "MAX_UPLOAD_BYTES", value: "10MB" }
```

All three exit with status 1 before binding a socket or contacting a dependency.

## Crate Changes

- `crates/common/src/config.rs` — `ConfigError`, parsers, validators, tests
- `crates/coordinator/src/main.rs` — validated config, helpers removed
- `crates/storage-node/src/main.rs` — validated config, helpers removed
- `crates/gateway/src/main.rs` — validated address/endpoint
- `crates/gateway/src/lib.rs` — parse limits/auth/rate config, propagate errors
- `crates/gateway/src/app.rs` — environment-free `AppState::new`
- `crates/gateway/src/auth.rs`, `rate_limit.rs` — fallible `try_from_env`
- `README.md`

## Verification

- `cargo test --workspace` — 56 pass (was 50)
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean
- Live fail-fast check against the gateway binary for a bad address, a bad
  boolean, and a bad size; each exits 1 with the offending key

## What Was Deferred (Future Work)

- Surfacing `ConfigError` through a `Display`-style top-level message instead of
  Rust's default `Debug` rendering of the boxed error.
- Validating cross-field constraints (for example `REPLICATION_FACTOR` versus
  the number of registered nodes) at startup.
- A `--check-config` mode that validates and exits without starting services.

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
