// TypeScript API definitions for RustyStore

export interface FileSummary {
  file_id: string;
  filename: string;
  total_size: number;
  chunk_count: number;
  replication_factor: number;
  status: string;
  created_epoch_ms: number;
}

export interface FileListResponse {
  files: FileSummary[];
  next_cursor?: string;
}

export interface ChunkInfo {
  chunk_id: string;
  chunk_index: number;
  size_bytes: number;
  sha256_hex: string;
  node_ids: string[];
}

export interface FileInfo {
  file_id: string;
  filename: string;
  total_size: number;
  chunk_size: number;
  chunk_count: number;
  replication_factor: number;
  status: string;
  chunks?: ChunkInfo[];
}

export interface NodeInfo {
  node_id: string;
  addr: string;
  grpc_addr: string;
  healthy: boolean;
  available_bytes: number;
  capacity_bytes: number;
}

export interface ClusterStats {
  total_nodes: number;
  healthy_nodes: number;
  total_capacity_bytes: number;
  total_available_bytes: number;
  total_files: number;
  total_chunks: number;
  under_replicated_chunks: number;
}

export interface ReadinessResponse {
  status: 'ready' | 'not_ready';
  reason?: string;
}

export interface HealthResponse {
  status: string;
  service: string;
}

export interface DeleteFileResponse {
  status: string;
  file_id: string;
  chunks: number;
  chunk_replicas_deleted: number;
  chunk_replicas_failed: number;
}

export interface UploadResponse {
  file_id: string;
  upload_id: string;
  chunk_size: number;
  chunk_count: number;
  replication_factor: number;
}

export interface UploadProgress {
  bytesUploaded: number;
  totalBytes: number;
  percentage: number;
  currentChunkEstimate: number;
  totalChunksEstimate: number;
}

export interface ParsedPrometheusMetrics {
  uploadedBytesTotal: number;
  downloadedBytesTotal: number;
  uploadFailuresTotal: number;
  activeStorageNodes: number;
  underReplicatedChunks: number;
  rawText: string;
  available: boolean;
}
