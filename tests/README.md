# Integration tests

Phase 9 adds end-to-end tests:

1. Upload a small file
2. Upload a file larger than chunk size
3. Multi-gigabyte streaming upload
4. Download and compare SHA-256
5. Detect corrupted chunks
6. Duplicate chunks
7. Storage node disconnection
8. Restart a storage node
9. Expired node lease
10. Automatic re-replication
11. Unavailable replicas
12. Interrupted uploads
13. Concurrent uploads
14. Disk exhaustion
15. Database unavailability

Run with:

```bash
cargo test --workspace
```
