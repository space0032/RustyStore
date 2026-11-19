import React, { useState } from 'react';
import {
  Search,
  Upload,
  RefreshCw,
  Download,
  Eye,
  Info,
  Layers,
  Trash2,
  File as FileIcon,
  ChevronRight,
  Database,
  ArrowUpDown,
  FileCode,
  FileImage,
  FileArchive,
  X,
} from 'lucide-react';
import { FileSummary } from '../types/api';
import { formatBytes, formatRelativeTime, formatFullDate, truncateUuid } from '../utils/formatters';
import { UploadModal } from '../components/files/UploadModal';
import { FilePreviewModal } from '../components/files/FilePreviewModal';
import { DeleteConfirmModal } from '../components/files/DeleteConfirmModal';

interface FilesViewProps {
  files: FileSummary[];
  loading: boolean;
  onRefresh: () => void;
  nextCursor?: string;
  onLoadMore?: () => void;
  onLoadAll?: () => Promise<void> | void;
  loadingMore?: boolean;
  onViewDetails: (file: FileSummary) => void;
  onInspectChunks: (fileId: string) => void;
  onDownloadFile: (file: FileSummary) => void;
  onNotify: (type: 'success' | 'error' | 'info' | 'warning', title: string, message?: string) => void;
  previewFile?: FileSummary | null;
  onPreviewFile?: (file: FileSummary | null) => void;
}

export const FilesView: React.FC<FilesViewProps> = ({
  files,
  loading,
  onRefresh,
  nextCursor,
  onLoadMore,
  onLoadAll,
  loadingMore = false,
  onViewDetails,
  onInspectChunks,
  onDownloadFile,
  onNotify,
  previewFile,
  onPreviewFile,
}) => {
  const [loadingAll, setLoadingAll] = useState(false);
  const [searchTerm, setSearchTerm] = useState('');
  const [sortField, setSortField] = useState<'filename' | 'total_size' | 'created_epoch_ms'>('created_epoch_ms');
  const [sortAsc, setSortAsc] = useState(false);
  const [uploadHadSuccess, setUploadHadSuccess] = useState(false);

  // Modals state
  const [isUploadOpen, setIsUploadOpen] = useState(false);
  const [localPreviewFile, setLocalPreviewFile] = useState<FileSummary | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<FileSummary | null>(null);

  const activePreviewFile = previewFile ?? localPreviewFile;
  const setPreview = (file: FileSummary | null) => {
    if (onPreviewFile) onPreviewFile(file);
    else setLocalPreviewFile(file);
  };

  const getFileIcon = (filename: string) => {
    const ext = filename.split('.').pop()?.toLowerCase();
    if (['png', 'jpg', 'jpeg', 'gif', 'svg', 'webp'].includes(ext || '')) {
      return <FileImage className="w-4 h-4 text-emerald-400" />;
    }
    if (['tar', 'gz', 'zip', '7z', 'bz2', 'xz'].includes(ext || '')) {
      return <FileArchive className="w-4 h-4 text-amber-400" />;
    }
    if (['rs', 'js', 'ts', 'py', 'json', 'yaml', 'toml', 'sql', 'sh'].includes(ext || '')) {
      return <FileCode className="w-4 h-4 text-sky-400" />;
    }
    return <FileIcon className="w-4 h-4 text-slate-400" />;
  };

  const filteredFiles = files
    .filter((f) => f.filename.toLowerCase().includes(searchTerm.toLowerCase()))
    .sort((a, b) => {
      let cmp = 0;
      if (sortField === 'filename') {
        cmp = a.filename.localeCompare(b.filename);
      } else if (sortField === 'total_size') {
        cmp = a.total_size - b.total_size;
      } else {
        cmp = a.created_epoch_ms - b.created_epoch_ms;
      }
      return sortAsc ? cmp : -cmp;
    });

  const toggleSort = (field: 'filename' | 'total_size' | 'created_epoch_ms') => {
    if (sortField === field) {
      setSortAsc(!sortAsc);
    } else {
      setSortField(field);
      setSortAsc(false);
    }
  };

  return (
    <div className="flex-1 flex flex-col h-full overflow-hidden bg-slate-950 p-6 space-y-5">
      {/* Top Action Header */}
      <div className="flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-4">
        <div>
          <h2 className="text-xl font-bold tracking-tight text-slate-100">Objects & Files</h2>
          <p className="text-xs text-slate-400 mt-0.5">
            Distributed files chunked into 4 MiB blocks and replicated across storage nodes
          </p>
        </div>

        <div className="flex items-center gap-3">
          <div className="relative flex-1 sm:w-64">
            <Search className="w-4 h-4 text-slate-400 absolute left-3 top-1/2 -translate-y-1/2" />
            <input
              type="text"
              placeholder="Search loaded files..."
              aria-label="Search loaded files"
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              className="w-full pl-9 pr-8 py-2 rounded-xl bg-slate-900 border border-slate-800 text-xs text-slate-100 placeholder-slate-500 focus:outline-none focus:border-rust-500 transition-colors"
            />
            {searchTerm && (
              <button
                type="button"
                onClick={() => setSearchTerm('')}
                className="absolute right-2 top-1/2 -translate-y-1/2 p-1 rounded text-slate-500 hover:text-slate-200"
                aria-label="Clear search"
              >
                <X className="w-3.5 h-3.5" />
              </button>
            )}
          </div>

          <button
            onClick={() => {
              setUploadHadSuccess(false);
              setIsUploadOpen(true);
            }}
            className="flex items-center gap-2 px-4 py-2 rounded-xl text-xs font-semibold bg-rust-500 hover:bg-rust-600 text-white shadow-lg shadow-rust-500/25 transition-all shrink-0"
          >
            <Upload className="w-3.5 h-3.5" />
            <span>Upload Object</span>
          </button>
        </div>
      </div>

      {/* Files Table Card */}
      <div className="flex-1 min-h-0 bg-slate-900/60 border border-slate-800/80 rounded-2xl flex flex-col overflow-hidden">
        <div className="overflow-auto flex-1">
          <table className="w-full text-left text-xs border-collapse">
            <thead className="sticky top-0 bg-slate-900 border-b border-slate-800 text-slate-400 uppercase tracking-wider text-[11px] font-semibold z-10">
              <tr>
                <th
                  scope="col"
                  aria-sort={sortField === 'filename' ? (sortAsc ? 'ascending' : 'descending') : 'none'}
                  className="py-3 px-4"
                >
                  <button type="button" onClick={() => toggleSort('filename')} className="flex items-center gap-1.5 hover:text-slate-200 transition-colors">
                    <span>Filename</span>
                    <ArrowUpDown className="w-3 h-3 text-slate-500" aria-hidden="true" />
                  </button>
                </th>
                <th
                  scope="col"
                  aria-sort={sortField === 'total_size' ? (sortAsc ? 'ascending' : 'descending') : 'none'}
                  className="py-3 px-4"
                >
                  <button type="button" onClick={() => toggleSort('total_size')} className="flex items-center gap-1.5 hover:text-slate-200 transition-colors">
                    <span>Size</span>
                    <ArrowUpDown className="w-3 h-3 text-slate-500" aria-hidden="true" />
                  </button>
                </th>
                <th scope="col" className="py-3 px-4">Chunks</th>
                <th scope="col" className="py-3 px-4">Replicas</th>
                <th
                  scope="col"
                  aria-sort={sortField === 'created_epoch_ms' ? (sortAsc ? 'ascending' : 'descending') : 'none'}
                  className="py-3 px-4"
                >
                  <button type="button" onClick={() => toggleSort('created_epoch_ms')} className="flex items-center gap-1.5 hover:text-slate-200 transition-colors">
                    <span>Committed</span>
                    <ArrowUpDown className="w-3 h-3 text-slate-500" aria-hidden="true" />
                  </button>
                </th>
                <th scope="col" className="py-3 px-4 text-right">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-800/50 text-slate-200">
              {filteredFiles.length > 0 ? (
                filteredFiles.map((file) => (
                  <tr
                    key={file.file_id}
                    className="hover:bg-slate-800/40 transition-colors group"
                  >
                    <td className="py-3 px-4">
                      <div className="flex items-center gap-2.5">
                        <div className="p-1.5 rounded-lg bg-slate-800 shrink-0">
                          {getFileIcon(file.filename)}
                        </div>
                        <div className="min-w-0">
                          <button
                            type="button"
                            className="font-medium text-slate-200 hover:text-rust-400 cursor-pointer truncate block max-w-full text-left focus:outline-none focus:underline"
                            onClick={() => setPreview(file)}
                            title={`Preview ${file.filename}`}
                          >
                            {file.filename}
                          </button>
                          <span className="font-mono text-[10px] text-slate-500 block">
                            ID: {truncateUuid(file.file_id)}
                          </span>
                        </div>
                      </div>
                    </td>
                    <td className="py-3 px-4 font-mono font-medium text-slate-300">
                      {formatBytes(file.total_size)}
                    </td>
                    <td className="py-3 px-4">
                      <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-slate-800 text-slate-300 font-mono text-[11px]">
                        <Layers className="w-3 h-3 text-rust-400" />
                        {file.chunk_count}
                      </span>
                    </td>
                    <td className="py-3 px-4">
                      <span className="inline-flex items-center px-2 py-0.5 rounded-md bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 font-mono text-[11px]">
                        {file.replication_factor}x
                      </span>
                    </td>
                    <td className="py-3 px-4 text-slate-400" title={formatFullDate(file.created_epoch_ms)}>
                      {formatRelativeTime(file.created_epoch_ms)}
                    </td>
                    <td className="py-3 px-4 text-right">
                      <div className="flex items-center justify-end gap-1.5">
                        <button
                          onClick={() => onViewDetails(file)}
                          className="p-1.5 rounded-lg text-slate-400 hover:text-sky-400 hover:bg-slate-800 transition-colors"
                          title="File Details"
                          aria-label={`Details for ${file.filename}`}
                        >
                          <Info className="w-3.5 h-3.5" />
                        </button>
                        <button
                          onClick={() => setPreview(file)}
                          className="p-1.5 rounded-lg text-slate-400 hover:text-slate-100 hover:bg-slate-800 transition-colors"
                          title="Preview Object"
                          aria-label={`Preview ${file.filename}`}
                        >
                          <Eye className="w-3.5 h-3.5" />
                        </button>
                        <button
                          onClick={() => onInspectChunks(file.file_id)}
                          className="p-1.5 rounded-lg text-slate-400 hover:text-rust-400 hover:bg-slate-800 transition-colors"
                          title="Inspect Chunks & Replicas"
                          aria-label={`Inspect chunks for ${file.filename}`}
                        >
                          <Layers className="w-3.5 h-3.5" />
                        </button>
                        <button
                          onClick={() => onDownloadFile(file)}
                          className="p-1.5 rounded-lg text-slate-400 hover:text-emerald-400 hover:bg-slate-800 transition-colors"
                          title="Stream Download"
                          aria-label={`Download ${file.filename}`}
                        >
                          <Download className="w-3.5 h-3.5" />
                        </button>
                        <button
                          onClick={() => setDeleteTarget(file)}
                          className="p-1.5 rounded-lg text-slate-400 hover:text-rose-400 hover:bg-slate-800 transition-colors"
                          title="Delete Object"
                          aria-label={`Delete ${file.filename}`}
                        >
                          <Trash2 className="w-3.5 h-3.5" />
                        </button>
                      </div>
                    </td>
                  </tr>
                ))
              ) : (
                <tr>
                  <td colSpan={6} className="py-12 text-center text-slate-400">
                    <div className="flex flex-col items-center gap-2">
                      <div className="p-3 bg-slate-800/60 rounded-full text-slate-500">
                        <Database className="w-6 h-6" />
                      </div>
                      <span className="text-sm font-semibold text-slate-300">
                        {loading
                          ? 'Loading files...'
                          : searchTerm
                            ? 'No matching objects'
                            : 'No objects found'}
                      </span>
                      <p className="text-xs text-slate-500 max-w-sm">
                        {loading
                          ? 'Communicating with RustyStore Coordinator...'
                          : searchTerm
                            ? `No files match "${searchTerm}". Try a different search term.`
                            : 'Upload a file to begin chunking and distributing data across nodes.'}
                      </p>
                    </div>
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>

        {/* Keyset Cursor Pagination Footer */}
        <div className="p-3 border-t border-slate-800 bg-slate-900/80 flex flex-col sm:flex-row sm:items-center justify-between gap-2 text-xs text-slate-400">
          <div className="space-y-0.5">
            <div>
              Showing <span className="font-semibold text-slate-200">{filteredFiles.length}</span>{' '}
              of <span className="font-semibold text-slate-200">{files.length}</span> loaded file(s)
              {searchTerm && <span> (filtered by "{searchTerm}")</span>}
            </div>
            {searchTerm && nextCursor && (onLoadMore || onLoadAll) && (
              <div className="text-[11px] text-amber-300/80">
                Search applies to loaded files only. Load more (or load all) to include older objects.
              </div>
            )}
          </div>
          <div className="flex items-center gap-2 flex-wrap justify-end">
            {searchTerm && nextCursor && onLoadAll && (
              <button
                onClick={async () => {
                  try {
                    setLoadingAll(true);
                    await onLoadAll();
                  } finally {
                    setLoadingAll(false);
                  }
                }}
                disabled={loadingMore || loadingAll}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-amber-500/10 hover:bg-amber-500/20 border border-amber-500/30 text-amber-200 font-medium transition-colors disabled:opacity-50"
              >
                <span>{loadingAll ? 'Loading all...' : 'Load all to search'}</span>
              </button>
            )}
            {nextCursor && onLoadMore && (
              <button
                onClick={onLoadMore}
                disabled={loadingMore || loadingAll}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 font-medium transition-colors disabled:opacity-50"
              >
                <span>{loadingMore ? 'Loading...' : 'Load More'}</span>
                <ChevronRight className="w-3.5 h-3.5" />
              </button>
            )}
          </div>
        </div>
      </div>

      {/* Modals */}
      <UploadModal
        isOpen={isUploadOpen}
        onClose={() => {
          setIsUploadOpen(false);
          if (uploadHadSuccess) {
            onRefresh();
            setUploadHadSuccess(false);
          }
        }}
        onSuccess={(name) => {
          setUploadHadSuccess(true);
          onNotify('success', 'File Uploaded', `${name} chunked and placed across healthy nodes.`);
        }}
        onError={(err) => onNotify('error', 'Upload Failed', err)}
      />

      <FilePreviewModal
        file={activePreviewFile}
        isOpen={!!activePreviewFile}
        onClose={() => setPreview(null)}
        onDownload={onDownloadFile}
      />

      <DeleteConfirmModal
        file={deleteTarget}
        isOpen={!!deleteTarget}
        onClose={() => setDeleteTarget(null)}
        onDeleted={(deleted) => {
          onNotify('info', 'File Deleted', `${deleted.filename} purged from metadata and nodes.`);
          onRefresh();
        }}
        onError={(err) => onNotify('error', 'Deletion Error', err)}
      />
    </div>
  );
};
