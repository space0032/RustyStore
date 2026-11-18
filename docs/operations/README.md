# Operations

## Running locally

```bash
cp .env.example .env
docker compose up -d postgres minio
cargo run -p coordinator
cargo run -p storage-node
cargo run -p gateway
```

The coordinator applies the embedded schema on startup; no separate migration
step is required. `docker compose up --build` starts the full stack.

## gRPC TLS/mTLS

Set `GRPC_TLS_CERT` and `GRPC_TLS_KEY` on the coordinator and storage nodes to
encrypt gRPC. Add `GRPC_TLS_CLIENT_CA` on servers to require client
certificates. Every gRPC client needs `GRPC_TLS_CA`; with mTLS it also needs
`GRPC_TLS_CLIENT_CERT` and `GRPC_TLS_CLIENT_KEY`.

```bash
./scripts/gen-certs.sh
docker compose -f docker-compose.yml -f docker-compose.tls.yml up --build
```

Certificates are read once at startup; rotate with a rolling restart. HTTP
health and metrics ports remain plaintext for probes and should be exposed only
through a TLS-terminating proxy.

## Health checks

- Gateway: `GET /api/v1/health` (liveness), `GET /api/v1/ready` (coordinator probe)
- Coordinator: `GET /health` (liveness), `GET /ready` (database probe) on port 8081
- Storage node: `GET /health` (liveness), `GET /ready` (storage volume probe) on port 8082

## Rate limiting

Gateway replicas share token buckets through the coordinator. Set
`RATE_LIMIT_RPS` above zero to enable limiting and `RATE_LIMIT_BURST` to bound
bursts (default `2 * RPS`). The coordinator stores buckets in PostgreSQL and
expires unused keys after one hour. If the coordinator is unavailable the
gateway falls back to its local bucket, so a coordinator outage degrades from
cluster-wide to per-process limiting rather than failing all requests.

## Recovery tuning and coordinator failover

`RECOVERY_BANDWIDTH_BYTES_PER_SEC` limits each recovery transfer;
`RECOVERY_AGGREGATE_BYTES_PER_SEC` caps total recovery traffic across concurrent
transfers. Both default to unlimited (`0`). `RECOVERY_PRIORITY_AGING_SECS`
(default `60`) adds one queue priority point per interval waited, preventing
low-priority repairs from starving behind newer work.

Only the coordinator holding the PostgreSQL lease runs recovery. Leadership
takeover increments a fencing epoch; a stale leader cannot claim pending tasks,
fail claimed tasks, or publish durable replicas after losing the lease. A
recovery write that finishes after fencing leaves unreferenced chunk bytes for
orphan GC rather than corrupting metadata.

## Coordinator discovery

Each coordinator registers `COORDINATOR_ADVERTISE_ADDR` in PostgreSQL and
renews a 15-second lease every 5 seconds. Gateways bootstrap from
`COORDINATOR_ADDRS` (or `COORDINATOR_ADDR`) and refresh their endpoint pool from
`ListCoordinators` every 15 seconds. Set the advertised address to a
peer-reachable host:port when the bind address is `0.0.0.0`; Compose uses
`coordinator:50051`. Expired registry rows are removed by the leader after one
hour.

## Simulating failure

```bash
./scripts/crash-node.sh storage-node-2
./scripts/partition-node.sh storage-node-3
```

## Metrics

Prometheus scrapes the HTTP port of each service:

- Gateway: `http://<gateway>:8080/metrics`
- Coordinator: `http://<coordinator>:8081/metrics`
- Storage node: `http://<storage-node>:8082/metrics`

All endpoints expose Prometheus text format. Gateway metrics cover upload and
download bytes/failures; coordinator metrics cover active nodes,
under-replicated chunks, leadership state, reclaimed orphan chunks/bytes, and
reaped stale uploads; storage nodes report chunks/bytes stored and deleted.
