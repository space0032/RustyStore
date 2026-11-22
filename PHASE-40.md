# Phase 40: gRPC TLS/mTLS

## Objective

All service-to-service gRPC traffic was plaintext: gateways sent credentials and
file data to coordinators, coordinators moved chunk bytes between storage nodes,
and storage nodes registered over unencrypted connections. Phase 40 adds
transport encryption and optional mutual authentication without breaking local
plaintext development.

## Design

### 1. Shared TLS configuration

`common::tls` is the single place that reads certificate files and builds tonic
configs. TLS is enabled by the presence of certificate variables, so existing
deployments need no new boolean flag.

Server variables:

- `GRPC_TLS_CERT` / `GRPC_TLS_KEY` — server identity in PEM.
- `GRPC_TLS_CLIENT_CA` — require and trust client certificates (mTLS).

Client variables:

- `GRPC_TLS_CA` — trust root for the server certificate.
- `GRPC_TLS_CLIENT_CERT` / `GRPC_TLS_CLIENT_KEY` — client identity for mTLS.
- `GRPC_TLS_DOMAIN` — expected server name when it differs from the endpoint
  host.

Incomplete certificate/key pairs fail startup with the missing variable name
instead of silently downgrading to plaintext. Certificates are read once at
startup; rotation is a rolling restart.

### 2. Server wiring

The coordinator and storage-node runners call
`common::tls::server_config_from_env()` and apply the result to tonic's
`Server::builder()` before adding services. With `GRPC_TLS_CLIENT_CA` set, a
client certificate signed by that CA is required before any RPC is served.

### 3. Client wiring

Every gRPC client uses `common::tls::connect(endpoint)`: the gateway's
coordinator client and chunk clients, storage-node registration and heartbeat
clients, coordinator recovery source/target clients, and orphan-GC listing
clients. The helper applies the client TLS config when configured and otherwise
connects exactly as before.

`https://` endpoints and the existing `http://` normalization remain supported.
No application RPC or protobuf message changed.

### 4. Development deployment

`scripts/gen-certs.sh` generates a local CA, a coordinator certificate
(`coordinator`, `localhost`), a shared storage-node certificate covering
`storage-node-1..3`, and a client identity. `docker-compose.tls.yml` is an
optional overlay that mounts the certificates and sets the mTLS variables on
every gRPC service. HTTP health/metrics ports remain plaintext for probes and
belong behind a TLS-terminating proxy.

## Verification

- `cargo fmt --all`
- `cargo check --workspace --all-targets`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`: 108 tests passed

Plaintext behavior is covered by the existing gRPC, end-to-end, and full-stack
tests, which run without TLS variables. mTLS itself is exercised by the Compose
overlay and generated certificates; automated TLS handshake tests require
checked-in or generated fixtures and are not part of the unit suite.

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
