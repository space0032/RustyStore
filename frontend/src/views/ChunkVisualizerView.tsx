import React, { useEffect, useState } from 'react';
import {
  Layers,
  HardDrive,
  Copy,
  Check,
  ShieldCheck,
  Server,
  FileBox,
  Hash,
} from 'lucide-react';
import { FileInfo, FileSummary, NodeInfo } from '../types/api';
import { formatBytes, truncateUuid } from '../utils/formatters';
import { getFile } from '../services/api';

interface ChunkVisualizerViewProps {
  files: FileSummary[];
  nodes: NodeInfo[];
  selectedFileId: string | null;
  onSelectFileId: (id: string) => void;
  onNotify: (type: 'success' | 'error' | 'info' | 'warning', title: string, message?: string) => void;
}

export const ChunkVisualizerView: React.FC<ChunkVisualizerViewProps> = ({
  files,
  nodes,
  selectedFileId,
  onSelectFileId,
  onNotify,
}) => {
  const [fileDetails, setFileDetails] = useState<FileInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [copiedHash, setCopiedHash] = useState<string | null>(null);
  const [activeChunkIndex, setActiveChunkIndex] = useState<number>(0);
  const [reloadToken, setReloadToken] = useState(0);

  // Prefer explicit selection (including deep links not yet in loaded pages).
  const activeId = selectedFileId ?? files[0]?.file_id ?? null;
  const selectedInLoaded = activeId ? files.some((f) => f.file_id === activeId) : false;

  useEffect(() => {
    if (!activeId) {
      setFileDetails(null);
      setLoadError(null);
      return;
    }
    let active = true;
    setLoading(true);
    setLoadError(null);
    setFileDetails(null);
    setActiveChunkIndex(0);
    getFile(activeId)
      .then((data) => {
        if (active) setFileDetails(data);
      })
      .catch((err) => {
        if (!active) return;
        const msg = (err as Error).message;
        setFileDetails(null);
        setLoadError(msg);
        onNotify('error', 'Failed to fetch chunk manifest', msg);
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [activeId, reloadToken]); // eslint-disable-line react-hooks/exhaustive-deps -- onNotify is a stable-enough toast helper; avoid refetch loops if its identity changes.

  const handleCopyHash = (hash: string) => {
    navigator.clipboard.writeText(hash);
    setCopiedHash(hash);
    setTimeout(() => setCopiedHash(null), 2000);
    onNotify('info', 'Copied to clipboard', 'SHA-256 chunk hash copied.');
  };

  const nodeMap = new Map<string, NodeInfo>();
  nodes.forEach((n) => nodeMap.set(n.node_id, n));

  const chunks = fileDetails?.chunks || [];
  const selectedChunk = chunks[Math.min(activeChunkIndex, Math.max(0, chunks.length - 1))];

  return (
    <div className="flex-1 flex flex-col h-full overflow-hidden bg-slate-950 p-6 space-y-5">
      {/* Header */}
      <div className="flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-4">
        <div>
          <h2 className="text-xl font-bold tracking-tight text-slate-100">
            Distributed Chunk & Replica Visualizer
          </h2>
          <p className="text-xs text-slate-400 mt-0.5">
            Inspect fixed 4 MiB chunks, SHA-256 verification hashes, and node replication topology
          </p>
        </div>

        {/* File Selector Dropdown */}
        <div className="flex items-center gap-2">
          <label className="text-xs text-slate-400 font-medium">Select Object:</label>
          <select
            value={activeId || ''}
            onChange={(e) => onSelectFileId(e.target.value)}
            className="px-3 py-2 rounded-xl bg-slate-900 border border-slate-800 text-xs text-slate-100 focus:outline-none focus:border-rust-500 transition-colors max-w-xs"
            aria-label="Select file to visualize"
          >
            {activeId && !selectedInLoaded && (
              <option value={activeId}>
                {fileDetails?.file_id === activeId ? `${fileDetails.filename} (${formatBytes(fileDetails.total_size)})` : `Selected object (${truncateUuid(activeId)})`}
              </option>
            )}
            {files.map((f) => (
              <option key={f.file_id} value={f.file_id}>
                {f.filename} ({formatBytes(f.total_size)})
              </option>
            ))}
          </select>
        </div>
      </div>

      {loading ? (
        <div className="flex-1 flex flex-col items-center justify-center gap-3 text-slate-400">
          <div className="w-8 h-8 border-2 border-rust-500/30 border-t-rust-500 rounded-full animate-spin" />
          <span className="text-sm font-medium">Retrieving Coordinator Chunk Manifest...</span>
        </div>
      ) : fileDetails ? (
        <div className="flex-1 min-h-0 grid grid-cols-1 lg:grid-cols-3 gap-6 overflow-hidden">
          {/* Left Column: Chunk Pipeline & Overview */}
          <div className="lg:col-span-2 flex flex-col space-y-4 overflow-hidden">
            {/* Manifest Overview Card */}
            <div className="p-4 rounded-2xl bg-slate-900/60 border border-slate-800/80 flex flex-wrap items-center justify-between gap-4">
              <div className="flex items-center gap-3">
                <div className="p-3 rounded-xl bg-rust-500/10 text-rust-400">
                  <FileBox className="w-6 h-6" />
                </div>
                <div>
                  <h3 className="font-semibold text-slate-100">{fileDetails.filename}</h3>
                  <p className="text-xs text-slate-400">
                    Total: <span className="text-slate-200 font-mono">{formatBytes(fileDetails.total_size)}</span> • Chunks:{' '}
                    <span className="text-slate-200 font-mono">{fileDetails.chunk_count}</span> • Chunk Size:{' '}
                    <span className="text-slate-200 font-mono">{formatBytes(fileDetails.chunk_size || 4194304)}</span>
                  </p>
                </div>
              </div>

              <div className="flex items-center gap-2">
                <span className="px-3 py-1 rounded-full text-xs font-semibold bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 flex items-center gap-1.5">
                  <ShieldCheck className="w-3.5 h-3.5" />
                  <span>RF = {fileDetails.replication_factor}x</span>
                </span>
                <span className="px-3 py-1 rounded-full text-xs font-semibold bg-slate-800 text-slate-300">
                  Status: {fileDetails.status}
                </span>
              </div>
            </div>

            {/* Chunks Grid & Pipeline */}
            <div className="flex-1 min-h-0 bg-slate-900/60 border border-slate-800/80 rounded-2xl p-5 flex flex-col overflow-hidden">
              <div className="flex items-center justify-between mb-3 shrink-0">
                <div className="flex items-center gap-2 text-xs font-bold uppercase tracking-wider text-slate-400">
                  <Layers className="w-4 h-4 text-rust-400" />
                  <span>File Chunks Distribution ({chunks.length} total)</span>
                </div>
                <span className="text-xs text-slate-500">Click a chunk block to inspect replica placement</span>
              </div>

              <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-3 overflow-auto flex-1 p-1">
                {chunks.map((chunk, idx) => {
                  const isSelected = activeChunkIndex === idx;
                  return (
                    <button
                      key={chunk.chunk_id || idx}
                      onClick={() => setActiveChunkIndex(idx)}
                      className={`p-3.5 rounded-xl border text-left transition-all relative flex flex-col justify-between ${
                        isSelected
                          ? 'bg-rust-500/15 border-rust-500 shadow-lg shadow-rust-500/20'
                          : 'bg-slate-950/70 border-slate-800 hover:border-slate-700 hover:bg-slate-900/70'
                      }`}
                    >
                      <div className="flex items-center justify-between mb-2">
                        <span
                          className={`text-xs font-bold font-mono px-2 py-0.5 rounded-md ${
                            isSelected ? 'bg-rust-500 text-white' : 'bg-slate-800 text-slate-300'
                          }`}
                        >
                          Chunk #{chunk.chunk_index}
                        </span>
                        <span className="text-[11px] font-mono text-slate-400">
                          {formatBytes(chunk.size_bytes)}
                        </span>
                      </div>

                      <div className="space-y-1">
                        <div className="text-[10px] text-slate-500 font-mono truncate">
                          ID: {truncateUuid(chunk.chunk_id)}
                        </div>
                        <div className="flex items-center gap-1 text-[10px] text-emerald-400">
                          <Check className="w-3 h-3" />
                          <span>{chunk.node_ids?.length || fileDetails.replication_factor} Replicas</span>
                        </div>
                      </div>
                    </button>
                  );
                })}
              </div>
            </div>
          </div>

          {/* Right Column: Chunk Detail & Physical Replica Placement */}
          <div className="flex flex-col space-y-4 overflow-hidden">
            <div className="flex-1 bg-slate-900/60 border border-slate-800/80 rounded-2xl p-5 flex flex-col overflow-auto">
              <div className="flex items-center gap-2 text-xs font-bold uppercase tracking-wider text-slate-400 mb-4 pb-3 border-b border-slate-800">
                <HardDrive className="w-4 h-4 text-rust-400" />
                <span>Chunk #{selectedChunk?.chunk_index ?? 0} Details</span>
              </div>

              {selectedChunk ? (
                <div className="space-y-5 flex-1">
                  {/* Chunk Metadata */}
                  <div className="space-y-2 text-xs">
                    <div>
                      <span className="text-slate-500 block mb-0.5">Chunk UUID:</span>
                      <code className="text-slate-200 font-mono-code bg-slate-950 p-2 rounded-lg block border border-slate-800 break-all">
                        {selectedChunk.chunk_id}
                      </code>
                    </div>

                    <div>
                      <span className="text-slate-500 block mb-0.5">Payload Size:</span>
                      <span className="font-mono text-slate-200 font-semibold">
                        {formatBytes(selectedChunk.size_bytes)} ({selectedChunk.size_bytes.toLocaleString()} bytes)
                      </span>
                    </div>

                    <div>
                      <div className="flex items-center justify-between mb-0.5">
                        <span className="text-slate-500 flex items-center gap-1">
                          <Hash className="w-3 h-3" />
                          SHA-256 Checksum:
                        </span>
                        <button
                          onClick={() => handleCopyHash(selectedChunk.sha256_hex)}
                          className="text-[11px] text-rust-400 hover:text-rust-300 flex items-center gap-1"
                        >
                          {copiedHash === selectedChunk.sha256_hex ? (
                            <>
                              <Check className="w-3 h-3 text-emerald-400" />
                              <span className="text-emerald-400">Copied</span>
                            </>
                          ) : (
                            <>
                              <Copy className="w-3 h-3" />
                              <span>Copy</span>
                            </>
                          )}
                        </button>
                      </div>
                      <code className="text-emerald-400 font-mono-code text-[11px] bg-slate-950 p-2 rounded-lg block border border-slate-800 break-all select-all">
                        {selectedChunk.sha256_hex || 'Verified at Storage Node ingestion'}
                      </code>
                    </div>
                  </div>

                  {/* Physical Node Hosts */}
                  <div className="space-y-3 pt-2">
                    <span className="text-xs font-bold uppercase tracking-wider text-slate-400 block">
                      Target Replica Holders
                    </span>

                    <div className="space-y-2">
                      {(selectedChunk.node_ids && selectedChunk.node_ids.length > 0
                        ? selectedChunk.node_ids
                        : nodes.map((n) => n.node_id)
                      ).map((nodeId, idx) => {
                        const node = nodeMap.get(nodeId);
                        return (
                          <div
                            key={nodeId || idx}
                            className="p-3 rounded-xl bg-slate-950 border border-slate-800/80 flex items-center justify-between gap-3"
                          >
                            <div className="flex items-center gap-2.5 min-w-0">
                              <div className="p-2 rounded-lg bg-slate-800 text-slate-300">
                                <Server className="w-4 h-4" />
                              </div>
                              <div className="truncate">
                                <div className="text-xs font-semibold text-slate-200 truncate">
                                  {node ? `Node ${idx + 1} (${node.addr})` : `Storage Node ${idx + 1}`}
                                </div>
                                <div className="font-mono text-[10px] text-slate-500">
                                  UUID: {truncateUuid(nodeId)}
                                </div>
                              </div>
                            </div>
                            <span className="px-2 py-0.5 rounded-full text-[10px] font-semibold bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 shrink-0">
                              Durable Replica
                            </span>
                          </div>
                        );
                      })}
                    </div>
                  </div>
                </div>
              ) : (
                <div className="flex-1 flex items-center justify-center text-slate-500 text-xs">
                  Select a chunk to view replication details
                </div>
              )}
            </div>
          </div>
        </div>
      ) : (
        <div className="flex-1 flex flex-col items-center justify-center gap-3 text-slate-400">
          <div className="p-4 bg-slate-800/60 rounded-full text-slate-500">
            <Layers className="w-8 h-8" />
          </div>
          <span className="text-sm font-semibold text-slate-300">
            {activeId ? 'Unable to load chunk manifest' : files.length === 0 ? 'No objects to visualize' : 'No file selected'}
          </span>
          <p className="text-xs text-slate-500 max-w-sm text-center">
            {activeId
              ? loadError || 'The selected object could not be loaded. It may have been deleted or is not accessible.'
              : files.length === 0
                ? 'Upload a file to inspect its chunk manifest and replica placement.'
                : 'Select an object from the dropdown to view its distributed chunk layout.'}
          </p>
          {activeId && (
            <button
              onClick={() => setReloadToken((t) => t + 1)}
              className="mt-1 px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-medium transition-colors"
            >
              Retry
            </button>
          )}
        </div>
      )}
    </div>
  );
};
