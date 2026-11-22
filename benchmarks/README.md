# Benchmarks

Criterion benchmarks live here. Phase 9 implements:

- Small-file uploads
- Large-file uploads (streaming, > chunk size)
- Concurrent uploads / downloads
- Chunk hashing throughput (SHA-256)
- Storage-node throughput
- Replication overhead
- Recovery duration
- Memory usage under load

Run with:

```bash
cargo bench
```

Results are compared against theoretical expectations; no results are invented.
