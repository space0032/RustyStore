import React, { useState } from 'react';
import { Trash2, AlertTriangle, X, Check } from 'lucide-react';
import { FileSummary } from '../../types/api';
import { formatBytes } from '../../utils/formatters';
import { deleteFile } from '../../services/api';
import { useFocusTrap, useEscapeClose } from '../../hooks/useFocusTrap';

interface DeleteConfirmModalProps {
  file: FileSummary | null;
  isOpen: boolean;
  onClose: () => void;
  onDeleted: (file: FileSummary) => void;
  onError: (err: string) => void;
}

export const DeleteConfirmModal: React.FC<DeleteConfirmModalProps> = ({
  file,
  isOpen,
  onClose,
  onDeleted,
  onError,
}) => {
  const [isDeleting, setIsDeleting] = useState(false);
  const modalRef = useFocusTrap(isOpen);
  useEscapeClose(isOpen, onClose);

  if (!isOpen || !file) return null;

  const handleDelete = async () => {
    setIsDeleting(true);
    try {
      await deleteFile(file.file_id);
      onDeleted(file);
    } catch (err) {
      onError((err as Error).message || 'Failed to delete file');
    } finally {
      setIsDeleting(false);
      onClose();
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-slate-950/80 backdrop-blur-sm animate-in fade-in duration-200" role="dialog" aria-modal="true" aria-labelledby="delete-modal-title">
      <div ref={modalRef} className="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-md shadow-2xl overflow-hidden">
        <div className="p-6 space-y-4">
          <div className="flex items-center gap-3">
            <div className="p-3 bg-rose-500/10 text-rose-400 rounded-xl" aria-hidden="true">
              <Trash2 className="w-6 h-6" />
            </div>
            <div>
              <h3 id="delete-modal-title" className="font-semibold text-slate-100">Delete Object?</h3>
              <p className="text-xs text-slate-400">This action will permanently purge file replicas</p>
            </div>
          </div>

          <div className="p-4 rounded-xl bg-slate-950 border border-slate-800 space-y-1.5 text-xs text-slate-300">
            <div className="font-semibold text-slate-100 truncate">{file.filename}</div>
            <div className="text-slate-400">
              Size: <span className="text-slate-200 font-mono">{formatBytes(file.total_size)}</span>
            </div>
            <div className="text-slate-400">
              Chunks:{' '}
              <span className="text-slate-200 font-mono">
                {file.chunk_count} chunk(s) × {file.replication_factor} replicas
              </span>
            </div>
          </div>

          <div className="flex items-start gap-2 p-3 rounded-xl bg-rose-500/10 border border-rose-500/20 text-xs text-rose-300" role="alert" aria-live="polite">
            <AlertTriangle className="w-4 h-4 shrink-0 mt-0.5" aria-hidden="true" />
            <span>
              Metadata will be deleted immediately, followed by best-effort chunk byte deletion from all storage nodes.
            </span>
          </div>

          <div className="flex justify-end gap-3 pt-2">
            <button
              type="button"
              disabled={isDeleting}
              onClick={onClose}
              className="px-4 py-2 rounded-xl text-sm font-medium text-slate-400 hover:bg-slate-800 transition-colors"
            >
              Cancel
            </button>
            <button
              type="button"
              disabled={isDeleting}
              onClick={handleDelete}
              className="flex items-center gap-2 px-5 py-2 rounded-xl text-sm font-semibold bg-rose-500 hover:bg-rose-600 text-white shadow-lg shadow-rose-500/25 transition-all disabled:opacity-50"
            >
              {isDeleting ? (
                <>
                  <div className="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin" aria-hidden="true" />
                  <span>Deleting...</span>
                </>
              ) : (
                <span>Confirm Delete</span>
              )}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
