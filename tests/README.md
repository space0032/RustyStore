# Integration tests

## Default suite

`cargo test --workspace` runs the in-process integration tests in `src/lib.rs`
plus the gateway end-to-end test in `crates/gateway/tests/`. They need no
external services:

1. Full chunk lifecycle (store, retrieve, verify, delete)
2. Placement with healthy nodes
3. Placement excludes unhealthy/insufficient-disk nodes
4. Duplicate chunk deduplication
5. Corrupted chunk detection
6. Gateway upload/download/delete through a fake coordinator and real storage node

## Full-stack suite (`tests/tests/full_stack.rs`)

These tests start a real coordinator, three real storage nodes, and the gateway
router against PostgreSQL. They are skipped unless `TEST_DATABASE_URL` (or
`DATABASE_URL`) is set, and serialise on a shared database:

1. Multi-chunk upload, listing, byte-for-byte download, delete
2. Download survives a storage-node failure
3. Concurrent uploads all commit
4. Automatic re-replication after lease expiry and replica loss

```bash
scripts/test-postgres.sh        # or scripts/test-postgres.ps1 on Windows
cargo test -p rustystore_integration_tests --test full_stack
```

## S3/MinIO suite

Feature-gated and skipped unless `TEST_S3_ENDPOINT` is set:

```bash
cargo test -p storage-engine --features s3 --test s3_integration
```

Optional environment: `TEST_S3_BUCKET` (default `rustystore-test`),
`TEST_S3_ACCESS_KEY` / `TEST_S3_SECRET_KEY` (default `minioadmin`), and
`TEST_S3_REGION`.

## Still manual / fault-injection only

Multi-gigabyte streaming uploads, disk exhaustion, database unavailability,
interrupted uploads, and network partitions are covered by
`scripts/crash-node.sh`, `scripts/partition-node.sh`, and `scripts/e2e-smoke.sh`
rather than the automated suite.
