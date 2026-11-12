# Phase 9: Testing and Benchmarking

## Objective

Add comprehensive testing (unit, integration, fault injection) and benchmarking (criterion) to validate correctness, performance, and fault tolerance.

## Test Coverage

### Storage Engine Tests (8 tests, all passing)
1. `test_store_and_retrieve_chunk` - Round-trip store/retrieve
2. `test_retrieve_nonexistent_chunk` - NotFound error for missing chunks
3. `test_integrity_mismatch_on_store` - Reject mismatched SHA-256 on write
4. `test_integrity_mismatch_on_verify` - Detect corruption on read
5. `test_delete_chunk` - Delete + idempotent re-delete
6. `test_streaming_reader` - Open chunk reader + verify hash
7. `test_get_capacity` - Disk capacity query
8. `test_atomic_write_no_partial_visible` - Atomic write pattern (temp → fsync → rename)

### Coordinator Placement Tests (5 tests, all passing)
1. `test_place_replicas_selects_distinct_nodes` - N distinct nodes
2. `test_place_replicas_insufficient_nodes` - Error when not enough nodes
3. `test_place_replicas_excludes_unhealthy_nodes` - Skip unhealthy nodes
4. `test_place_replicas_respects_disk_space` - Skip nodes with insufficient disk
5. `test_place_replicas_excludes_specified_nodes` - Respect exclude list

## Benchmarks (Criterion)

### Chunk Hashing Throughput
- `sha256_1kb` - 1 KB chunk hashing
- `sha256_4mb` - 4 MB chunk hashing (default chunk size)
- `sha256_8mb` - 8 MB chunk hashing (alternative chunk size)
- `sha256_random_1mb` - 1 MB random data hashing

### Storage Throughput
- `store_write_4kb` / `store_write_1mb` / `store_write_4mb` - Write latency
- `store_read_4mb` - Read latency
- `store_delete_4mb` - Delete + rewrite cycle

## Fault Injection Scripts

### `scripts/crash-node.sh`
- Stops a storage node via Docker Compose
- Waits 20s for lease expiry / failure detection
- Verifies coordinator health
- Restarts the node
- Verifies automatic re-replication

### `scripts/partition-node.sh`
- Isolates a node from the Docker network
- Waits 30s for lease expiry
- Reconnects the node

### `scripts/e2e-smoke.sh`
- Starts full stack via Docker Compose
- Runs database migrations
- Builds all services
- Provides manual test instructions

## Verification

- `cargo check --workspace` — clean
- `cargo clippy --workspace -D warnings` — clean
- `cargo fmt --all -- --check` — clean
- `cargo test -p storage-engine` — 8/8 tests pass
- `cargo test -p coordinator` — 5/5 tests pass

## Limitations

- Integration tests requiring running services (coordinator, storage nodes, PostgreSQL) are not automated in CI
- End-to-end tests require manual Docker Compose setup
- Fault injection scripts require Docker and running services
- Criterion benchmarks not yet executed (require `cargo bench`)

## Next (Phase 10)

Production Hardening: Authentication, Authorization, TLS/mTLS, Rate limiting, Observability, Structured logging, Graceful shutdown, Configuration management, Deployment documentation.
