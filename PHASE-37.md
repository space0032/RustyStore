# Phase 37: Coordinator HA-Lite, Gateway Failover & Image Publishing

## Objective

The coordinator was a single point of failure: every write needed that one
process, and a gateway connected to a single address had no recovery path if it
stopped. Compose also started services before dependencies were ready, storage
nodes registered unreachable bind addresses, and images were built but never
published. This phase adds database-backed coordinator leadership, gateway
failover across coordinators, readiness-gated Compose startup, reachable
advertised addresses, and GHCR publishing on version tags.

TLS/mTLS and distributed rate limiting were considered but are explicitly
deferred; see "What Was Deferred".

## Design

### 1. Coordinator leadership lease

A new singleton table serializes background work across coordinator processes:

```sql
CREATE TABLE coordinator_leader (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    leader_id UUID NOT NULL,
    lease_expiry TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

`MetadataStore::try_acquire_leadership(leader_id, ttl_ms)` attempts one atomic
upsert. It succeeds when the row is unowned, already owned by the caller, or its
lease has expired:

```sql
INSERT INTO coordinator_leader (singleton, leader_id, lease_expiry)
VALUES (TRUE, $1, now() + ($2 * INTERVAL '1 millisecond'))
ON CONFLICT (singleton) DO UPDATE
SET leader_id = EXCLUDED.leader_id,
    lease_expiry = EXCLUDED.lease_expiry,
    updated_at = now()
WHERE coordinator_leader.leader_id = EXCLUDED.leader_id
   OR coordinator_leader.lease_expiry <= now()
RETURNING leader_id
```

The caller is leader only when the returned id matches its own. The lease is
15 seconds and is renewed every 5 seconds, so a stopped leader loses ownership
quickly while transient database errors do not cause needless churn.

### 2. Only the leader runs background work

Every coordinator still serves the gRPC API, so any process can handle client
requests. A shared `Arc<AtomicBool>` gates the periodic work:

- lease monitoring (marking nodes unhealthy)
- under-replication detection and task creation
- recovery scheduling
- orphan-chunk GC

Each loop checks the flag at the start of a tick. Leadership transitions are
logged once, not every renewal, and a database error clears the flag until the
next successful renewal. GC's `spawn` now takes the flag so a standby cannot
reclaim chunks while the leader is still scheduling recovery.

### 3. Gateway failover across coordinators

`CoordinatorClient` now holds an ordered address list and a current index.
`connect_many` tries each address in order and fails only when all are
unreachable. `with_retry` runs an RPC against the current connection and, on a
transient gRPC status, reconnects to the next address and retries. Retryable
statuses are `Unavailable`, `DeadlineExceeded`, `Cancelled`, `Unknown`,
`Internal`, and `ResourceExhausted`; application errors such as `NotFound` and
`InvalidArgument` return immediately.

tonic requests are not `Clone`, so every call builds its request inside the
retry closure. The helper returns a boxed future to allow the closure to borrow
the mutable client:

```rust
type BoxFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, tonic::Status>> + Send + 'a>>;
```

The gateway reads `COORDINATOR_ADDRS` as a comma-separated list, falling back
to the single `COORDINATOR_ADDR`. Entries are trimmed, empty entries rejected,
and missing `http(s)` schemes normalized. `AppState::new` remains for tests that
need one address.

### 4. Reachable storage-node addresses

Registration now advertises `ADVERTISED_ADDR` as the node's gRPC address instead
of the bind address. Compose sets it to `storage-node-N:50051`, so the
coordinator and other nodes connect to the service name rather than a container
bind address. Local runs still default to `NODE_ADDR` when `ADVERTISED_ADDR` is
unset.

### 5. Readiness-gated Compose and GHCR publishing

Coordinator, storage-node, and gateway images install `curl` for healthchecks.
Compose probes coordinator `/health`, storage-node `/health`, and gateway
`/api/v1/health`; storage nodes and the gateway wait for the coordinator to be
healthy. The gateway receives `COORDINATOR_ADDRS=coordinator:50051`.

CI now has a tag-triggered `publish` job. It logs in to GHCR, lowercases the
repository name for the image path, and pushes
`ghcr.io/<owner>/<repo>/{coordinator,storage-node,gateway}` with the version tag
and `latest`, reusing the same buildx cache scopes as the build-only job.

## Verification

- `cargo fmt --all`
- `cargo check --workspace --all-targets`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`: 105 tests passed

New coverage:

- coordinator client retry classification and empty-address rejection
- gateway `COORDINATOR_ADDRS` parsing, fallback, and empty-list rejection
- PostgreSQL leadership lease: renew, standby blocked, expiry takeover, old
  leader rejected
- full-stack gateway failover: first request uses the active coordinator, the
  coordinator stops, and the next request succeeds through the standby

PostgreSQL-backed and full-stack tests return early locally when no database is
configured; CI runs them against service containers.

## What Was Deferred (Future Work)

- **TLS/mTLS** - services still expect a trusted network or a TLS-terminating
  reverse proxy. Adding transport security touches all gRPC clients, servers,
  Dockerfiles, and Compose configuration, and was not completed here.
- **Distributed rate limiting** - the token bucket remains per gateway process.
  A shared limiter would need a common store (for example Redis or PostgreSQL)
  and is left as future work.
- **Consensus leadership** - leadership is a PostgreSQL lease, not Raft or
  Paxos. There is no fencing token for recovery transfers already in flight when
  a lease is lost; replication is idempotent, so this is a brief-duplicate risk,
  not a correctness break.
- **Automatic coordinator discovery** - the gateway address list is static
  configuration rather than service discovery.

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