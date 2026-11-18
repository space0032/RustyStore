# Phase 43: Coordinator Registry and Gateway Discovery

## Objective

Gateways previously learned coordinator endpoints only from the static
`COORDINATOR_ADDRS` environment variable. Adding or replacing a coordinator
required reconfiguring and restarting every gateway. Phase 43 adds a
PostgreSQL-backed live registry so gateways can discover coordinators
dynamically while keeping the static list as a bootstrap fallback.

## Design

### 1. Coordinator registry

`coordinator_registry` stores one row per coordinator:

- `coordinator_id` — process instance id (the same id used for leadership);
- `grpc_addr` — advertised endpoint reachable by gateways and peers;
- `lease_expiry` / `last_seen` — discovery liveness.

Each coordinator renews its row every 5 seconds with a 15-second lease,
independent of leadership, because every coordinator serves gateway RPCs. The
advertised endpoint comes from `COORDINATOR_ADVERTISE_ADDR`. If unset, the
listener address is used and an unspecified bind address is mapped to
`127.0.0.1` for local development. Endpoints are normalized to include a scheme.

The leader periodically deletes rows expired for more than one hour; discovery
already filters expired rows, so cleanup only bounds table growth.

### 2. `ListCoordinators` RPC

`ClusterService` gains:

```proto
rpc ListCoordinators(ListCoordinatorsRequest) returns (ListCoordinatorsResponse);
```

The response contains `(coordinator_id, grpc_addr)` for every live registry row.
Any coordinator can answer, so a gateway only needs one reachable bootstrap
endpoint.

### 3. Gateway discovery

The gateway keeps its existing `COORDINATOR_ADDRS`/`COORDINATOR_ADDR` bootstrap
pool. A background task calls `ListCoordinators` immediately and every 15
seconds, normalizes returned endpoints, and appends previously unknown entries
to the active `CoordinatorClient`. Existing retry/failover behavior then rotates
across the expanded pool.

Discovery is best-effort: RPC failures or a 5-second timeout leave the current
pool untouched, and the bootstrap list continues to work. The refresh clones the
client before the network call so ordinary requests are not blocked behind a
slow discovery attempt.

### 4. Compatibility

The protobuf addition is additive. Gateways using an older coordinator receive
`Unimplemented` from `ListCoordinators`, log the failure, and continue with
their configured addresses. Coordinators that do not set
`COORDINATOR_ADVERTISE_ADDR` still register their listener address, which is
correct for single-host deployments.

## Verification

- `cargo fmt --all`
- `cargo check --workspace --all-targets`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`: 112 tests passed

Added `coordinator_registry_reports_only_live_rows`, covering registration,
address renewal, live filtering, and stale-row cleanup. PostgreSQL-backed tests
skip locally without `TEST_DATABASE_URL`; CI runs them against PostgreSQL.

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
| 28 | Replication Task Deduplication | Complete |
| 29 | Orphan-Chunk GC & Stale Upload Reaper | Complete |
| 30 | PostgreSQL-Backed Integration Tests | Complete |
| 31 | Paginated File Listing | Complete |
| 32 | Graceful HTTP Shutdown, CI, Container Builds | Complete |
| 33 | Recovery Source Failover | Complete |
| 34 | Recovery Target Failover & Shutdown-Aware Registration | Complete |
| 35 | Full-Stack & S3 Integration Tests | Complete |
| 36 | Recovery Race, Provenance, Priority & Throttle | Complete |
| 37 | Coordinator HA-Lite, Gateway Failover & Image Publishing | Complete |
| 38 | Legacy RPC & Workspace Cleanup | Complete |
| 39 | Dead RPC, Task Recovery & Docs Cleanup | Complete |
| 40 | gRPC TLS/mTLS | Complete |
| 41 | Shared Rate Limiting | Complete |
| 42 | Leader Fencing & Recovery QoS | Complete |
| 43 | Coordinator Registry & Gateway Discovery | Complete |
