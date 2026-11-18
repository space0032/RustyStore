-- RustyStore initial schema
-- Phase 1: metadata foundation
--
-- Design decisions:
--   * files.status gates readability: only 'committed' files are served.
--   * chunk_replicas.durable is set true only after fsync + node ack.
--   * UNIQUE(chunk_id, node_id) prevents duplicate replica rows.
--   * replication_tasks is the recovery work queue (bounded concurrency).
--   * upload_sessions tracks partial uploads for abort/cleanup.

-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- storage nodes
CREATE TABLE IF NOT EXISTS storage_nodes (
  node_id        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  addr           TEXT NOT NULL,
  grpc_addr      TEXT NOT NULL,
  capacity_bytes BIGINT NOT NULL,
  available_bytes BIGINT NOT NULL,
  used_bytes     BIGINT NOT NULL DEFAULT 0,
  lease_expiry   TIMESTAMPTZ NOT NULL,
  last_seen      TIMESTAMPTZ NOT NULL DEFAULT now(),
  healthy        BOOLEAN NOT NULL DEFAULT true,
  version        TEXT,
  created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- files (manifest)
CREATE TABLE IF NOT EXISTS files (
  file_id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  filename          TEXT NOT NULL,
  total_size        BIGINT NOT NULL,
  chunk_size        BIGINT NOT NULL,
  chunk_count       INT NOT NULL,
  replication_factor INT NOT NULL DEFAULT 3,
  status            TEXT NOT NULL DEFAULT 'uploading', -- uploading, committed, aborted
  created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Newest-first listing of committed files (keyset pagination).
CREATE INDEX IF NOT EXISTS idx_files_status_created
  ON files(status, created_at DESC, file_id DESC);

-- chunks (content-addressed)
CREATE TABLE IF NOT EXISTS chunks (
  chunk_id    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  sha256_hex  CHAR(64) NOT NULL,
  size_bytes  BIGINT NOT NULL,
  chunk_index INT NOT NULL,
  file_id     UUID NOT NULL REFERENCES files(file_id) ON DELETE CASCADE,
  UNIQUE (file_id, chunk_index)
);

CREATE INDEX IF NOT EXISTS idx_chunks_sha256 ON chunks(sha256_hex);
CREATE INDEX IF NOT EXISTS idx_chunks_file ON chunks(file_id);

-- chunk replicas (placement)
CREATE TABLE IF NOT EXISTS chunk_replicas (
  replica_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  chunk_id   UUID NOT NULL REFERENCES chunks(chunk_id) ON DELETE CASCADE,
  node_id    UUID NOT NULL REFERENCES storage_nodes(node_id) ON DELETE CASCADE,
  durable    BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (chunk_id, node_id)
);

CREATE INDEX IF NOT EXISTS idx_chunk_replicas_node ON chunk_replicas(node_id);
CREATE INDEX IF NOT EXISTS idx_chunk_replicas_chunk ON chunk_replicas(chunk_id);
CREATE INDEX IF NOT EXISTS idx_chunk_replicas_durable ON chunk_replicas(durable);

-- upload sessions (for abort / orphan cleanup)
CREATE TABLE IF NOT EXISTS upload_sessions (
  upload_id    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  file_id      UUID REFERENCES files(file_id) ON DELETE CASCADE,
  status       TEXT NOT NULL DEFAULT 'pending', -- pending, committed, aborted
  created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
  completed_at TIMESTAMPTZ
);

-- replication tasks (recovery scheduling / work queue)
CREATE TABLE IF NOT EXISTS replication_tasks (
  task_id        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  chunk_id       UUID NOT NULL REFERENCES chunks(chunk_id) ON DELETE CASCADE,
  source_node_id UUID REFERENCES storage_nodes(node_id),
  target_node_id UUID NOT NULL REFERENCES storage_nodes(node_id),
  status         TEXT NOT NULL DEFAULT 'pending', -- pending, in_progress, done, failed
  priority       INT NOT NULL DEFAULT 0,          -- higher runs first
  attempts       INT NOT NULL DEFAULT 0,
  max_attempts   INT NOT NULL DEFAULT 5,
  created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Existing deployments created before the priority column was added.
ALTER TABLE replication_tasks ADD COLUMN IF NOT EXISTS priority INT NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_replication_tasks_status ON replication_tasks(status);
CREATE INDEX IF NOT EXISTS idx_replication_tasks_queue ON replication_tasks(status, priority DESC, created_at ASC);
CREATE INDEX IF NOT EXISTS idx_replication_tasks_target ON replication_tasks(target_node_id);
CREATE INDEX IF NOT EXISTS idx_replication_tasks_chunk ON replication_tasks(chunk_id);
