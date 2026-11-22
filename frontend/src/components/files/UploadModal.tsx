import React, { useState, useRef, useEffect, useCallback } from 'react';
import { X, UploadCloud, File as FileIcon, Layers, ArrowUpRight, RotateCcw, XCircle, CheckCircle2, AlertCircle } from 'lucide-react';
import { formatBytes } from '../../utils/formatters';
import { uploadFileWithProgress, getDefaultChunkSize } from '../../services/api';
import { UploadProgress } from '../../types/api';
import { useFocusTrap, useEscapeClose } from '../../hooks/useFocusTrap';

type UploadStatus = 'queued' | 'uploading' | 'done' | 'error' | 'cancelled';

interface UploadItem {
  id: string;
  file: File;
  status: UploadStatus;
  progress: UploadProgress | null;
  error?: string;
  abort?: () => void;
}

interface UploadModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess: (filename: string) => void;
  onError: (err: string) => void;
}

function makeId() {
  return `u-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

export const UploadModal: React.FC<UploadModalProps> = ({
  isOpen,
  onClose,
  onSuccess,
  onError,
}) => {
  const [items, setItems] = useState<UploadItem[]>([]);
  const [isDragging, setIsDragging] = useState(false);
  const [chunkSize, setChunkSize] = useState<number>(4194304);
  const [queueRunning, setQueueRunning] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const runIdRef = useRef(0);
  const itemsRef = useRef<UploadItem[]>([]);

  useEffect(() => {
    itemsRef.current = items;
  }, [items]);

  const isBusy = items.some((i) => i.status === 'uploading');
  const resetQueue = useCallback(() => {
    setItems([]);
    setQueueRunning(false);
  }, []);

  const handleClose = useCallback(() => {
    if (isBusy) return;
    resetQueue();
    onClose();
  }, [isBusy, onClose, resetQueue]);

  const modalRef = useFocusTrap(isOpen && !isBusy);
  useEscapeClose(isOpen && !isBusy, handleClose);

  useEffect(() => {
    if (isOpen) {
      getDefaultChunkSize().then(setChunkSize).catch(() => setChunkSize(4194304));
    }
  }, [isOpen]);

  const addFiles = useCallback((fileList: FileList | File[]) => {
    const arr = Array.from(fileList);
    if (arr.length === 0) return;
    setItems((prev) => [
      ...prev,
      ...arr.map((file) => ({
        id: makeId(),
        file,
        status: 'queued' as UploadStatus,
        progress: null,
      })),
    ]);
  }, []);

  const updateItem = useCallback((id: string, patch: Partial<UploadItem>) => {
    setItems((prev) => prev.map((it) => (it.id === id ? { ...it, ...patch } : it)));
  }, []);

  const runQueue = useCallback(async () => {
    if (queueRunning) return;
    setQueueRunning(true);
    const myRun = ++runIdRef.current;
    try {
      // Process sequentially to avoid overwhelming gateway and to keep UX predictable.
      // Read from itemsRef so newly queued/retried items are visible during the loop.
      while (runIdRef.current === myRun) {
        const next = itemsRef.current.find((i) => i.status === 'queued');
        if (!next) break;
        const totalChunks = Math.max(1, Math.ceil(next.file.size / chunkSize));
        updateItem(next.id, {
          status: 'uploading',
          error: undefined,
          progress: {
            bytesUploaded: 0,
            totalBytes: next.file.size,
            percentage: 0,
            currentChunkEstimate: 1,
            totalChunksEstimate: totalChunks,
          },
        });
        const handle = uploadFileWithProgress(next.file, chunkSize, (p) => {
          updateItem(next.id, { progress: p });
        });
        updateItem(next.id, { abort: handle.abort });
        try {
          await handle.promise;
          if (runIdRef.current !== myRun) return;
          updateItem(next.id, {
            status: 'done',
            abort: undefined,
            progress: {
              bytesUploaded: next.file.size,
              totalBytes: next.file.size,
              percentage: 100,
              currentChunkEstimate: totalChunks,
              totalChunksEstimate: totalChunks,
            },
          });
          onSuccess(next.file.name);
        } catch (err) {
          if (runIdRef.current !== myRun) return;
          const e = err as Error & { retryAfter?: number; code?: string };
          const cancelled = e.code === 'cancelled' || e.message?.toLowerCase().includes('cancel');
          const retryMsg = e.retryAfter ? ` Please retry after ${e.retryAfter}s.` : '';
          const msg = cancelled ? 'Upload cancelled' : `${e.message || 'Upload failed'}${retryMsg}`;
          updateItem(next.id, { status: cancelled ? 'cancelled' : 'error', error: msg, abort: undefined });
          if (!cancelled) onError(msg);
        }
        // Yield to let React commit state updates so itemsRef stays current.
        await Promise.resolve();
      }
    } finally {
      if (runIdRef.current === myRun) setQueueRunning(false);
    }
  }, [chunkSize, onError, onSuccess, queueRunning, updateItem]);

  // Kick queue when items change and there is queued work
  useEffect(() => {
    if (!isOpen) return;
    const hasQueued = items.some((i) => i.status === 'queued');
    const hasUploading = items.some((i) => i.status === 'uploading');
    if (hasQueued && !hasUploading && !queueRunning) {
      // eslint-disable-next-line react-hooks/set-state-in-effect -- intentional queue pump
      void runQueue();
    }
  }, [isOpen, items, queueRunning, runQueue]);

  if (!isOpen) return null;

  const handleFileDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
    if (e.dataTransfer.files?.length) addFiles(e.dataTransfer.files);
  };

  const retryItem = (id: string) => {
    setItems((prev) => prev.map((it) => (it.id === id ? { ...it, status: 'queued', error: undefined, progress: null } : it)));
  };

  const cancelItem = (id: string) => {
    const item = items.find((i) => i.id === id);
    if (item?.abort) item.abort();
    if (item?.status === 'queued') updateItem(id, { status: 'cancelled', error: 'Upload cancelled' });
  };

  const removeItem = (id: string) => {
    setItems((prev) => prev.filter((i) => i.id !== id));
  };

  const pendingCount = items.filter((i) => i.status === 'queued' || i.status === 'uploading').length;
  const doneCount = items.filter((i) => i.status === 'done').length;
  const errorCount = items.filter((i) => i.status === 'error' || i.status === 'cancelled').length;
  const allFinished = items.length > 0 && pendingCount === 0;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-slate-950/80 backdrop-blur-sm animate-in fade-in duration-200" role="dialog" aria-modal="true" aria-labelledby="upload-modal-title">
      <div ref={modalRef} className="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-2xl shadow-2xl overflow-hidden max-h-[90vh] flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-slate-800">
          <div className="flex items-center gap-2.5">
            <div className="p-2 bg-rust-500/10 text-rust-400 rounded-lg">
              <UploadCloud className="w-5 h-5" />
            </div>
            <div>
              <h3 id="upload-modal-title" className="font-semibold text-slate-100">Upload to RustyStore</h3>
              <p className="text-xs text-slate-400">Multi-file queue • sequential streaming uploads • chunked placement</p>
            </div>
          </div>
          {!isBusy && (
            <button
              onClick={handleClose}
              className="text-slate-400 hover:text-slate-200 p-1.5 rounded-lg hover:bg-slate-800 transition-colors"
              aria-label="Close upload modal"
            >
              <X className="w-5 h-5" />
            </button>
          )}
        </div>

        {/* Content */}
        <div className="p-6 space-y-4 overflow-y-auto flex-1">
          <div
            onDragOver={(e) => {
              e.preventDefault();
              setIsDragging(true);
            }}
            onDragLeave={() => setIsDragging(false)}
            onDrop={handleFileDrop}
            onClick={() => fileInputRef.current?.click()}
            role="button"
            tabIndex={0}
            aria-label="Drop zone for file upload"
            className={`border-2 border-dashed rounded-2xl p-6 flex flex-col items-center justify-center gap-2 cursor-pointer transition-all ${
              isDragging
                ? 'border-rust-500 bg-rust-500/10'
                : 'border-slate-700 hover:border-rust-500/60 bg-slate-950/50 hover:bg-slate-950'
            }`}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                fileInputRef.current?.click();
              }
            }}
          >
            <input
              ref={fileInputRef}
              type="file"
              multiple
              className="hidden"
              onChange={(e) => {
                if (e.target.files?.length) addFiles(e.target.files);
                e.target.value = '';
              }}
            />
            <div className="p-3 rounded-full bg-slate-900 border border-slate-800 text-rust-400 shadow-inner">
              <UploadCloud className="w-7 h-7" />
            </div>
            <div className="text-center">
              <span className="text-sm font-semibold text-slate-200">Click to browse or drop files here</span>
              <p className="text-xs text-slate-400 mt-1">
                Add multiple files. Uploads run sequentially in {formatBytes(chunkSize)} chunks.
              </p>
            </div>
          </div>

          {items.length > 0 && (
            <div className="space-y-3">
              <div className="flex items-center justify-between text-xs text-slate-400">
                <div className="flex items-center gap-3">
                  <span className="inline-flex items-center gap-1"><Layers className="w-3.5 h-3.5 text-rust-400" /> {items.length} file(s)</span>
                  {doneCount > 0 && <span className="text-emerald-400">{doneCount} done</span>}
                  {errorCount > 0 && <span className="text-rose-400">{errorCount} failed/cancelled</span>}
                </div>
                <span className="font-mono text-slate-500">Chunk size {formatBytes(chunkSize)}</span>
              </div>

              <div className="space-y-2 max-h-80 overflow-y-auto pr-1">
                {items.map((item) => {
                  const totalChunks = Math.max(1, Math.ceil(item.file.size / chunkSize));
                  const pct = item.progress?.percentage ?? (item.status === 'done' ? 100 : 0);
                  return (
                    <div key={item.id} className="p-3 rounded-xl bg-slate-950 border border-slate-800 space-y-2">
                      <div className="flex items-start justify-between gap-3">
                        <div className="flex items-center gap-2.5 min-w-0">
                          <div className="p-2 rounded-lg bg-slate-800 text-slate-300 shrink-0">
                            <FileIcon className="w-4 h-4" />
                          </div>
                          <div className="min-w-0">
                            <div className="text-sm font-medium text-slate-200 truncate">{item.file.name}</div>
                            <div className="text-[11px] text-slate-500 font-mono">
                              {formatBytes(item.file.size)} • ~{totalChunks} chunk(s) • 3x replicas
                            </div>
                          </div>
                        </div>
                        <div className="flex items-center gap-1 shrink-0">
                          {item.status === 'done' && <span className="inline-flex items-center gap-1 text-[11px] text-emerald-400"><CheckCircle2 className="w-3.5 h-3.5" />Done</span>}
                          {item.status === 'uploading' && <span className="text-[11px] text-rust-400 font-medium">Uploading</span>}
                          {item.status === 'queued' && <span className="text-[11px] text-slate-400">Queued</span>}
                          {item.status === 'error' && <span className="inline-flex items-center gap-1 text-[11px] text-rose-400"><AlertCircle className="w-3.5 h-3.5" />Failed</span>}
                          {item.status === 'cancelled' && <span className="text-[11px] text-amber-400">Cancelled</span>}

                          {item.status === 'uploading' && (
                            <button onClick={() => cancelItem(item.id)} className="p-1.5 rounded-lg text-slate-400 hover:text-rose-400 hover:bg-slate-800" title="Cancel upload" aria-label={`Cancel upload ${item.file.name}`}>
                              <XCircle className="w-4 h-4" />
                            </button>
                          )}
                          {item.status === 'queued' && (
                            <button onClick={() => cancelItem(item.id)} className="p-1.5 rounded-lg text-slate-400 hover:text-amber-400 hover:bg-slate-800" title="Remove from queue" aria-label={`Remove ${item.file.name} from queue`}>
                              <X className="w-4 h-4" />
                            </button>
                          )}
                          {(item.status === 'error' || item.status === 'cancelled') && (
                            <>
                              <button onClick={() => retryItem(item.id)} className="p-1.5 rounded-lg text-slate-400 hover:text-rust-400 hover:bg-slate-800" title="Retry upload" aria-label={`Retry upload ${item.file.name}`}>
                                <RotateCcw className="w-4 h-4" />
                              </button>
                              <button onClick={() => removeItem(item.id)} className="p-1.5 rounded-lg text-slate-400 hover:text-slate-200 hover:bg-slate-800" title="Remove" aria-label={`Remove ${item.file.name}`}>
                                <X className="w-4 h-4" />
                              </button>
                            </>
                          )}
                          {item.status === 'done' && (
                            <button onClick={() => removeItem(item.id)} className="p-1.5 rounded-lg text-slate-400 hover:text-slate-200 hover:bg-slate-800" title="Remove from list" aria-label={`Remove ${item.file.name} from list`}>
                              <X className="w-4 h-4" />
                            </button>
                          )}
                        </div>
                      </div>

                      {(item.status === 'uploading' || item.status === 'queued' || item.status === 'done') && (
                        <div className="space-y-1.5">
                          <div className="w-full h-1.5 rounded-full bg-slate-800 overflow-hidden" role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100} aria-label={`Upload progress for ${item.file.name}`}>
                            <div className="h-full bg-gradient-to-r from-rust-500 to-amber-500 transition-all duration-200" style={{ width: `${pct}%` }} />
                          </div>
                          <div className="flex justify-between text-[11px] text-slate-500">
                            <span>
                              {item.progress
                                ? `Chunk ${item.progress.currentChunkEstimate}/${item.progress.totalChunksEstimate} • ${formatBytes(item.progress.bytesUploaded)} / ${formatBytes(item.progress.totalBytes)}`
                                : item.status === 'queued'
                                  ? 'Waiting in queue...'
                                  : 'Completed'}
                            </span>
                            <span className="font-mono text-slate-400">{pct}%</span>
                          </div>
                        </div>
                      )}

                      {item.error && (item.status === 'error' || item.status === 'cancelled') && (
                        <div className={`text-[11px] px-2.5 py-1.5 rounded-lg border ${item.status === 'error' ? 'text-rose-300 bg-rose-500/10 border-rose-500/20' : 'text-amber-300 bg-amber-500/10 border-amber-500/20'}`}>
                          {item.error}
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-between gap-3 px-6 py-4 border-t border-slate-800 bg-slate-950/40">
          <div className="text-xs text-slate-500">
            {items.length === 0
              ? 'No files selected'
              : isBusy
                ? `Uploading queue... (${doneCount}/${items.length} completed)`
                : allFinished
                  ? `Queue finished: ${doneCount} succeeded, ${errorCount} failed/cancelled`
                  : `${pendingCount} pending`}
          </div>
          <div className="flex items-center gap-2">
            {!isBusy && items.some((i) => i.status === 'error' || i.status === 'cancelled') && (
              <button
                type="button"
                onClick={() => setItems((prev) => prev.map((it) => (it.status === 'error' || it.status === 'cancelled') ? { ...it, status: 'queued', error: undefined, progress: null } : it))}
                className="flex items-center gap-1.5 px-3 py-2 rounded-xl text-xs font-medium text-slate-300 bg-slate-800 hover:bg-slate-700 transition-colors"
              >
                <RotateCcw className="w-3.5 h-3.5" /> Retry failed
              </button>
            )}
            <button
              type="button"
              disabled={isBusy}
              onClick={handleClose}
              className="px-4 py-2 rounded-xl text-sm font-medium text-slate-400 hover:bg-slate-800 transition-colors disabled:opacity-50"
            >
              {allFinished ? 'Close' : 'Cancel'}
            </button>
            {items.length === 0 && (
              <button
                type="button"
                onClick={() => fileInputRef.current?.click()}
                className="flex items-center gap-2 px-5 py-2 rounded-xl text-sm font-semibold bg-rust-500 hover:bg-rust-600 text-white shadow-lg shadow-rust-500/25 transition-all"
              >
                <span>Select Files</span>
                <ArrowUpRight className="w-4 h-4" aria-hidden="true" />
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
