import React from 'react';
import { useQuery } from '@tanstack/react-query';
import { X, Copy, Layers, Download, AlertCircle, Hash, Server } from 'lucide-react';
import { FileSummary } from '../../types/api';
import { formatBytes, formatFullDate, truncateHash, truncateUuid } from '../../utils/formatters';
import { getFile } from '../../services/api';

interface FileDetailsModalProps {
  file: FileSummary | null;
  isOpen: boolean;
  onClose: () => void;
  onInspectChunks: (fileId: string) => void;
  onDownload: (file: FileSummary) => void;
  onNotify: (type: 'success' | 'error' | 'info' | 'warning', title: string, message?: string) => void;
}

export const FileDetailsModal: React.FC<FileDetailsModalProps> = ({
  file,
  isOpen,
  onClose,
  onInspectChunks,
  onDownload,
  onNotify,
}) => {
  const detailsQuery = useQuery({
    queryKey: ['file', file?.file_id],
    queryFn: () => getFile(file!.file_id),
    enabled: isOpen && !!file,
  });

  if (!isOpen || !file) return null;

  const handleCopy = async (value: string, label: string) => {
    try {
      await navigator.clipboard.writeText(value);
      onNotify('info', 'Copied', `${label} copied to clipboard.`);
    } catch {
      onNotify('error', 'Copy Failed', 'Clipboard access was denied.');
    }
  };

  const details = detailsQuery.data;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-slate-950/80 backdrop-blur-sm">
      <div className="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-3xl shadow-2xl overflow-hidden flex flex-col max-h-[85vh]">
        {/* Header */}
        <div className="flex items-start justify-between gap-4 px-6 py-4 border-b border-slate-800 shrink-0">
          <div className="min-w-0">
            <h3 className="font-semibold text-slate-100 truncate">{file.filename}</h3>
            <div className="flex items-center gap-2 mt-1">
              <span className="font-mono text-[11px] text-slate-500">{file.file_id}</span>
              <button
                onClick={() => void handleCopy(file.file_id, 'File UUID')}
                className="text-slate-500 hover:text-rust-400 transition-colors"
                title="Copy file UUID"
              >
                <Copy className="w-3.5 h-3.5" />
              </button>
            </div>
          </div>
          <button
            onClick={onClose}
            className="text-slate-400 hover:text-slate-200 p-1.5 rounded-lg hover:bg-slate-800 transition-colors shrink-0"
            aria-label="Close file details"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-auto p-6 space-y-5">
          {detailsQuery.isPending ? (
            <div className="flex flex-col items-center gap-2 py-12 text-slate-400">
              <div className="w-7 h-7 border-2 border-rust-500/30 border-t-rust-500 rounded-full animate-spin" />
              <span className="text-xs">Loading chunk manifest...</span>
            </div>
          ) : detailsQuery.error ? (
            <div className="p-4 rounded-xl bg-rose-500/10 border border-rose-500/30 flex items-center justify-between gap-3">
              <div className="flex items-center gap-3 text-xs text-rose-200">
                <AlertCircle className="w-4 h-4 shrink-0" />
                <span>{(detailsQuery.error as Error).message}</span>
              </div>
              <button
                onClick={() => void detailsQuery.refetch()}
                className="px-3 py-1.5 rounded-lg bg-rose-500/20 text-rose-200 hover:bg-rose-500/30 text-xs shrink-0"
              >
                Retry
              </button>
            </div>
          ) : details ? (
            <>
              {/* Metadata */}
              <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
                {[
                  ['Total size', formatBytes(details.total_size)],
                  ['Chunk size', formatBytes(details.chunk_size || 4194304)],
                  ['Chunks', String(details.chunk_count)],
                  ['Replication', `${details.replication_factor}x`],
                ].map(([label, value]) => (
                  <div key={label} className="p-3 rounded-xl bg-slate-950 border border-slate-800">
                    <div className="text-[10px] uppercase tracking-wider text-slate-500">{label}</div>
                    <div className="text-sm font-semibold text-slate-200 font-mono mt-1">{value}</div>
                  </div>
                ))}
              </div>

              <div className="flex flex-wrap items-center gap-x-6 gap-y-2 text-xs text-slate-400">
                <span>Status: <span className="text-emerald-400">{details.status}</span></span>
                <span>Uploaded: <span className="text-slate-300">{formatFullDate(file.created_epoch_ms)}</span></span>
              </div>

              {/* Chunk table */}
              <div className="rounded-xl border border-slate-800 overflow-hidden">
                <div className="px-4 py-3 bg-slate-950/60 border-b border-slate-800 flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-slate-400">
                  <Layers className="w-4 h-4 text-rust-400" />
                  Chunk Manifest ({details.chunks?.length ?? 0})
                </div>
                {details.chunks && details.chunks.length > 0 ? (
                  <table className="w-full text-left text-xs">
                    <thead className="bg-slate-900/60 text-slate-500 text-[10px] uppercase tracking-wider">
                      <tr>
                        <th className="px-4 py-2">#</th>
                        <th className="px-4 py-2">Size</th>
                        <th className="px-4 py-2">SHA-256</th>
                        <th className="px-4 py-2">Replicas</th>
                        <th className="px-4 py-2" />
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-slate-800/60 text-slate-300">
                      {details.chunks.map((chunk) => (
                        <tr key={chunk.chunk_id}>
                          <td className="px-4 py-2 font-mono">{chunk.chunk_index}</td>
                          <td className="px-4 py-2 font-mono">{formatBytes(chunk.size_bytes)}</td>
                          <td className="px-4 py-2">
                            <button
                              onClick={() => void handleCopy(chunk.sha256_hex, 'SHA-256 checksum')}
                              className="flex items-center gap-1.5 font-mono text-[11px] text-emerald-400 hover:text-emerald-300"
                              title="Copy checksum"
                            >
                              <Hash className="w-3 h-3" />
                              {truncateHash(chunk.sha256_hex, 10, 10)}
                            </button>
                          </td>
                          <td className="px-4 py-2">
                            <span className="inline-flex items-center gap-1 text-slate-300">
                              <Server className="w-3 h-3 text-rust-400" />
                              {chunk.node_ids?.length ?? details.replication_factor}
                            </span>
                          </td>
                          <td className="px-4 py-2 text-right font-mono text-[10px] text-slate-500">
                            {truncateUuid(chunk.chunk_id)}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                ) : (
                  <div className="p-6 text-center text-xs text-slate-500">
                    No chunk details are available for this object.
                  </div>
                )}
              </div>
            </>
          ) : null}
        </div>

        {/* Footer */}
        <div className="px-6 py-4 border-t border-slate-800 bg-slate-950/40 flex items-center justify-end gap-3 shrink-0">
          <button
            onClick={() => onDownload(file)}
            className="flex items-center gap-2 px-4 py-2 rounded-xl text-xs font-semibold bg-slate-800 text-slate-200 hover:bg-slate-700 transition-colors"
          >
            <Download className="w-3.5 h-3.5" />
            Download
          </button>
          <button
            onClick={() => onInspectChunks(file.file_id)}
            className="flex items-center gap-2 px-4 py-2 rounded-xl text-xs font-semibold bg-rust-500 text-white hover:bg-rust-600 transition-colors"
          >
            <Layers className="w-3.5 h-3.5" />
            Inspect Chunks
          </button>
        </div>
      </div>
    </div>
  );
};
