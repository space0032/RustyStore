// High-fidelity mock data for Demo Mode & offline testing

import { ClusterStats, FileInfo, FileSummary, NodeInfo } from '../types/api';

export const MOCK_NODES: NodeInfo[] = [
  {
    node_id: '9a31b672-10f8-4122-bc56-02a8bf8c9e01',
    addr: '127.0.0.1:50051',
    grpc_addr: 'http://storage-node-1:50051',
    healthy: true,
    available_bytes: 42949672960 - 157286400, // ~40 GiB
    capacity_bytes: 42949672960,
  },
  {
    node_id: '8b42c783-21a9-4233-cd67-13b9cf9d0f12',
    addr: '127.0.0.1:50052',
    grpc_addr: 'http://storage-node-2:50051',
    healthy: true,
    available_bytes: 42949672960 - 134217728, // ~40 GiB
    capacity_bytes: 42949672960,
  },
  {
    node_id: '7c53d894-32ba-4344-de78-24cad0ae1a23',
    addr: '127.0.0.1:50053',
    grpc_addr: 'http://storage-node-3:50051',
    healthy: true,
    available_bytes: 42949672960 - 150994944, // ~40 GiB
    capacity_bytes: 42949672960,
  },
];

export const MOCK_CLUSTER_STATS: ClusterStats = {
  total_nodes: 3,
  healthy_nodes: 3,
  total_capacity_bytes: 128849018880, // ~120 GiB
  total_available_bytes: 128406520320,
  total_files: 4,
  total_chunks: 11,
  under_replicated_chunks: 0,
};

export const MOCK_FILES: FileSummary[] = [
  {
    file_id: '1a2b3c4d-5e6f-7a8b-9c0d-1e2f3a4b5c6d',
    filename: 'linux-kernel-6.10-release.tar.gz',
    total_size: 14680064, // ~14 MiB -> 4 chunks (4MB default)
    chunk_count: 4,
    replication_factor: 3,
    status: 'committed',
    created_epoch_ms: Date.now() - 1000 * 60 * 18,
  },
  {
    file_id: '2b3c4d5e-6f7a-8b9c-0d1e-2f3a4b5c6d7e',
    filename: 'rust-deep-dive-diagram.png',
    total_size: 2457600, // ~2.3 MiB -> 1 chunk
    chunk_count: 1,
    replication_factor: 3,
    status: 'committed',
    created_epoch_ms: Date.now() - 1000 * 60 * 85,
  },
  {
    file_id: '3c4d5e6f-7a8b-9c0d-1e2f-3a4b5c6d7e8f',
    filename: 'cluster-telemetry-dataset.parquet',
    total_size: 20971520, // ~20 MiB -> 5 chunks
    chunk_count: 5,
    replication_factor: 3,
    status: 'committed',
    created_epoch_ms: Date.now() - 1000 * 60 * 360,
  },
  {
    file_id: '4d5e6f7a-8b9c-0d1e-2f3a-4b5c6d7e8f90',
    filename: 'rustystore-architecture.md',
    total_size: 18432, // 18 KB -> 1 chunk
    chunk_count: 1,
    replication_factor: 3,
    status: 'committed',
    created_epoch_ms: Date.now() - 1000 * 60 * 1440,
  },
];

export const MOCK_FILE_DETAILS: Record<string, FileInfo> = {
  '1a2b3c4d-5e6f-7a8b-9c0d-1e2f3a4b5c6d': {
    file_id: '1a2b3c4d-5e6f-7a8b-9c0d-1e2f3a4b5c6d',
    filename: 'linux-kernel-6.10-release.tar.gz',
    total_size: 14680064,
    chunk_size: 4194304,
    chunk_count: 4,
    replication_factor: 3,
    status: 'committed',
    chunks: [
      {
        chunk_id: 'f1a0e1b2-0001-4000-8000-000000000001',
        chunk_index: 0,
        size_bytes: 4194304,
        sha256_hex: 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
        node_ids: [
          '9a31b672-10f8-4122-bc56-02a8bf8c9e01',
          '8b42c783-21a9-4233-cd67-13b9cf9d0f12',
          '7c53d894-32ba-4344-de78-24cad0ae1a23',
        ],
      },
      {
        chunk_id: 'f1a0e1b2-0002-4000-8000-000000000002',
        chunk_index: 1,
        size_bytes: 4194304,
        sha256_hex: 'a591a6d40bf420404a011733cfb7b190d62c65bf0bcda32b57b277d9ad9f146e',
        node_ids: [
          '9a31b672-10f8-4122-bc56-02a8bf8c9e01',
          '8b42c783-21a9-4233-cd67-13b9cf9d0f12',
          '7c53d894-32ba-4344-de78-24cad0ae1a23',
        ],
      },
      {
        chunk_id: 'f1a0e1b2-0003-4000-8000-000000000003',
        chunk_index: 2,
        size_bytes: 4194304,
        sha256_hex: '8743b52063cd84097a65d1633f5c74f5',
        node_ids: [
          '9a31b672-10f8-4122-bc56-02a8bf8c9e01',
          '8b42c783-21a9-4233-cd67-13b9cf9d0f12',
          '7c53d894-32ba-4344-de78-24cad0ae1a23',
        ],
      },
      {
        chunk_id: 'f1a0e1b2-0004-4000-8000-000000000004',
        chunk_index: 3,
        size_bytes: 2097152,
        sha256_hex: 'b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9',
        node_ids: [
          '9a31b672-10f8-4122-bc56-02a8bf8c9e01',
          '8b42c783-21a9-4233-cd67-13b9cf9d0f12',
          '7c53d894-32ba-4344-de78-24cad0ae1a23',
        ],
      },
    ],
  },
};

export const MOCK_PROMETHEUS_RAW = `# HELP rustystore_uploaded_bytes_total Total bytes uploaded through gateway
# TYPE rustystore_uploaded_bytes_total counter
rustystore_uploaded_bytes_total 128450560
# HELP rustystore_downloaded_bytes_total Total bytes downloaded through gateway
# TYPE rustystore_downloaded_bytes_total counter
rustystore_downloaded_bytes_total 349175808
# HELP rustystore_upload_failures_total Total failed file uploads
# TYPE rustystore_upload_failures_total counter
rustystore_upload_failures_total 0
# HELP rustystore_active_storage_nodes Number of healthy registered storage nodes
# TYPE rustystore_active_storage_nodes gauge
rustystore_active_storage_nodes 3
# HELP rustystore_under_replicated_chunks Number of chunks with fewer than target replicas
# TYPE rustystore_under_replicated_chunks gauge
rustystore_under_replicated_chunks 0
`;
