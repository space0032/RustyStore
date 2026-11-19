import React, { useState, useRef, useEffect } from 'react';
import { X, UploadCloud, File as FileIcon, CheckCircle2, Layers, AlertCircle, ArrowUpRight } from 'lucide-react';
import { formatBytes } from '../../utils/formatters';
import { uploadFileWithProgress, getDefaultChunkSize } from '../../services/api';
import { UploadProgress } from '../../types/api';
import { useFocusTrap, useEscapeClose } from '../../hooks/useFocusTrap';

interface UploadModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess: (filename: string) => void;
  onError: (err: string) => void;
}

export const UploadModal: React.FC<UploadModalProps> = ({
  isOpen,
  onClose,
  onSuccess,
  onError,
}) => {
  const [selectedFile, setSelectedFile] = useState<File | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [isUploading, setIsUploading] = useState(false);
  const [progress, setProgress] = useState<UploadProgress | null>(null);
  const [chunkSize, setChunkSize] = useState<number>(4194304);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const modalRef = useFocusTrap(isOpen && !isUploading);
  useEscapeClose(isOpen && !isUploading, onClose);

  if (!isOpen) return null;

  // Fetch the default chunk size from the cluster on mount
  useEffect(() => {
    getDefaultChunkSize().then(setChunkSize).catch(() => setChunkSize(4194304));
  }, []);

  const estimatedChunks = selectedFile
    ? Math.max(1, Math.ceil(selectedFile.size / chunkSize))
    : 0;

  const handleFileDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
    if (e.dataTransfer.files && e.dataTransfer.files[0]) {
      setSelectedFile(e.dataTransfer.files[0]);
    }
  };

  const handleStartUpload = async () => {
    if (!selectedFile) return;
    setIsUploading(true);
    setProgress({
      bytesUploaded: 0,
      totalBytes: selectedFile.size,
      percentage: 0,
      currentChunkEstimate: 1,
      totalChunksEstimate: estimatedChunks,
    });

    try {
      await uploadFileWithProgress(selectedFile, chunkSize, (p) => {
        setProgress(p);
      });
      onSuccess(selectedFile.name);
    } catch (err) {
      onError((err as Error).message || 'Upload failed');
    } finally {
      setIsUploading(false);
      onClose();
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-slate-950/80 backdrop-blur-sm animate-in fade-in duration-200" role="dialog" aria-modal="true" aria-labelledby="upload-modal-title">
      <div ref={modalRef} className="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-lg shadow-2xl overflow-hidden">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-slate-800">
          <div className="flex items-center gap-2.5">
            <div className="p-2 bg-rust-500/10 text-rust-400 rounded-lg">
              <UploadCloud className="w-5 h-5" />
            </div>
            <div>
              <h3 id="upload-modal-title" className="font-semibold text-slate-100">Upload to RustyStore</h3>
              <p className="text-xs text-slate-400">Streaming upload with chunked placement</p>
            </div>
          </div>
          {!isUploading && (
            <button
              onClick={onClose}
              className="text-slate-400 hover:text-slate-200 p-1.5 rounded-lg hover:bg-slate-800 transition-colors"
              aria-label="Close upload modal"
            >
              <X className="w-5 h-5" />
            </button>
          )}
        </div>

        {/* Content */}
        <div className="p-6 space-y-4">
          {!selectedFile ? (
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
              className={`border-2 border-dashed rounded-2xl p-8 flex flex-col items-center justify-center gap-3 cursor-pointer transition-all ${
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
                className="hidden"
                onChange={(e) => {
                  if (e.target.files && e.target.files[0]) {
                    setSelectedFile(e.target.files[0]);
                  }
                }}
              />
              <div className="p-4 rounded-full bg-slate-900 border border-slate-800 text-rust-400 shadow-inner">
                <UploadCloud className="w-8 h-8" />
              </div>
              <div className="text-center">
                <span className="text-sm font-semibold text-slate-200">
                  Click to browse or drop your file here
                </span>
                <p className="text-xs text-slate-400 mt-1">
                  Supports any file type. Uploads are streamed and chunked in {formatBytes(chunkSize)} blocks.
                </p>
              </div>
            </div>
          ) : (
            <div className="space-y-4">
              {/* Selected File Card */}
              <div className="p-4 rounded-xl bg-slate-950 border border-slate-800 flex items-center justify-between">
                <div className="flex items-center gap-3 min-w-0">
                  <div className="p-2.5 rounded-lg bg-rust-500/10 text-rust-400 shrink-0">
                    <FileIcon className="w-5 h-5" />
                  </div>
                  <div className="truncate">
                    <div className="text-sm font-semibold text-slate-200 truncate">
                      {selectedFile.name}
                    </div>
                    <div className="text-xs text-slate-400">
                      {formatBytes(selectedFile.size)}
                    </div>
                  </div>
                </div>
                {!isUploading && (
                  <button
                    onClick={() => {
                      setSelectedFile(null);
                      setProgress(null);
                    }}
                    className="text-xs text-slate-400 hover:text-rose-400 transition-colors px-2 py-1"
                  >
                    Change
                  </button>
                )}
              </div>

              {/* Chunk Breakdown Plan */}
              <div className="p-4 rounded-xl bg-slate-800/40 border border-slate-800 space-y-2">
                <div className="flex items-center gap-2 text-xs font-semibold text-slate-300">
                  <Layers className="w-4 h-4 text-rust-400" />
                  <span>Distributed Storage Breakdown</span>
                </div>
                <div className="grid grid-cols-2 gap-2 text-xs text-slate-400">
                  <div>
                    Estimated Chunks:{' '}
                    <span className="text-slate-200 font-mono font-medium">
                      {estimatedChunks}
                    </span>
                  </div>
                  <div>
                    Chunk Size:{' '}
                    <span className="text-slate-200 font-mono font-medium">{formatBytes(chunkSize)}</span>
                  </div>
                  <div>
                    Replication Factor:{' '}
                    <span className="text-slate-200 font-mono font-medium">3x</span>
                  </div>
                  <div>
                    Total Replicas:{' '}
                    <span className="text-slate-200 font-mono font-medium">
                      {estimatedChunks * 3}
                    </span>
                  </div>
                </div>
              </div>

              {/* Progress Bar */}
              {isUploading && progress && (
                <div className="space-y-2 pt-2" role="status" aria-live="polite" aria-label="Upload progress">
                  <div className="flex items-center justify-between text-xs">
                    <span className="text-slate-300 font-medium">
                      Chunk {progress.currentChunkEstimate} of {progress.totalChunksEstimate}
                    </span>
                    <span className="font-mono text-rust-400 font-bold">
                      {progress.percentage}%
                    </span>
                  </div>
                  <div className="w-full h-2 rounded-full bg-slate-800 overflow-hidden" role="progressbar" aria-valuenow={progress.percentage} aria-valuemin={0} aria-valuemax={100} aria-label="Upload progress">
                    <div
                      className="h-full bg-gradient-to-r from-rust-500 to-amber-500 transition-all duration-200"
                      style={{ width: `${progress.percentage}%` }}
                    />
                  </div>
                  <div className="flex justify-between text-[11px] text-slate-500">
                    <span>{formatBytes(progress.bytesUploaded)} uploaded</span>
                    <span>{formatBytes(progress.totalBytes)}</span>
                  </div>
                </div>
              )}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-end gap-3 px-6 py-4 border-t border-slate-800 bg-slate-950/40">
          <button
            type="button"
            disabled={isUploading}
            onClick={onClose}
            className="px-4 py-2 rounded-xl text-sm font-medium text-slate-400 hover:bg-slate-800 transition-colors disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            type="button"
            disabled={!selectedFile || isUploading}
            onClick={handleStartUpload}
            className="flex items-center gap-2 px-5 py-2 rounded-xl text-sm font-semibold bg-rust-500 hover:bg-rust-600 text-white shadow-lg shadow-rust-500/25 transition-all disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {isUploading ? (
              <>
                <div className="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin" aria-hidden="true" />
                <span>Uploading...</span>
              </>
            ) : (
              <>
                <span>Commit & Replicate</span>
                <ArrowUpRight className="w-4 h-4" aria-hidden="true" />
              </>
            )}
          </button>
        </div>
      </div>
    </div>
  );
};
