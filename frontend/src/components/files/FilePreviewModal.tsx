import React, { useEffect, useState, useRef } from 'react';
import { X, FileText, Image as ImageIcon, Download, AlertCircle, FileCode } from 'lucide-react';
import { FileSummary } from '../../types/api';
import { formatBytes, formatFullDate } from '../../utils/formatters';
import { fetchFileBlobUrl, fetchFileText, loadSettings } from '../../services/api';
import { useFocusTrap, useEscapeClose } from '../../hooks/useFocusTrap';

interface FilePreviewModalProps {
  file: FileSummary | null;
  isOpen: boolean;
  onClose: () => void;
  onDownload: (file: FileSummary) => void;
}

export const FilePreviewModal: React.FC<FilePreviewModalProps> = ({
  file,
  isOpen,
  onClose,
  onDownload,
}) => {
  const [content, setContent] = useState<string | null>(null);
  const [mediaUrl, setMediaUrl] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const objectUrlRef = useRef<string | null>(null);
  const modalRef = useFocusTrap(isOpen);
  useEscapeClose(isOpen, onClose);

  const ext = file?.filename.split('.').pop()?.toLowerCase() || '';
  const isImage = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg'].includes(ext);
  const isText = ['txt', 'md', 'json', 'rs', 'toml', 'yaml', 'yml', 'csv', 'log', 'sh', 'sql', 'js', 'ts'].includes(ext);
  const isVideo = ['mp4', 'webm', 'ogg'].includes(ext);
  const isAudio = ['mp3', 'wav', 'ogg', 'm4a'].includes(ext);
  const settings = loadSettings();
  const demoMediaBlocked = !!(isOpen && file && settings.demoMode && (isVideo || isAudio));
  const canPreviewText = !!(file && isText && file.total_size < 2 * 1024 * 1024);
  const canPreviewMedia = !!(file && (isImage || isVideo || isAudio) && !demoMediaBlocked);
  const shouldLoad = !!(isOpen && file && (canPreviewText || canPreviewMedia));
  const effectiveError = demoMediaBlocked ? 'Media playback is unavailable in Demo Mode.' : error;
  const effectiveLoading = shouldLoad ? loading : false;
  const effectiveContent = shouldLoad && isText ? content : null;
  const effectiveMediaUrl = shouldLoad && (isImage || isVideo || isAudio) ? mediaUrl : null;

  const revokeUrl = () => {
    if (objectUrlRef.current) {
      window.URL.revokeObjectURL(objectUrlRef.current);
      objectUrlRef.current = null;
    }
  };

  useEffect(() => {
    revokeUrl();
    if (!isOpen || !file || demoMediaBlocked) return undefined;

    let active = true;
    const startLoad = () => {
      setLoading(true);
      setError(null);
      setContent(null);
      setMediaUrl(null);
    };

    if (isText && file.total_size < 2 * 1024 * 1024) {
      startLoad();
      fetchFileText(file.file_id)
        .then((txt) => {
          if (active) setContent(txt);
        })
        .catch((err) => {
          if (active) setError((err as Error).message);
        })
        .finally(() => {
          if (active) setLoading(false);
        });
    } else if (isImage || isVideo || isAudio) {
      startLoad();
      fetchFileBlobUrl(file.file_id)
        .then((url) => {
          if (!active) {
            window.URL.revokeObjectURL(url);
            return;
          }
          objectUrlRef.current = url;
          setMediaUrl(url);
        })
        .catch((err) => {
          if (active) setError((err as Error).message);
        })
        .finally(() => {
          if (active) setLoading(false);
        });
    } else {
      return undefined;
    }

    return () => {
      active = false;
      revokeUrl();
    };
  }, [isOpen, file, isText, isImage, isVideo, isAudio, demoMediaBlocked]);

  if (!isOpen || !file) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-slate-950/80 backdrop-blur-sm animate-in fade-in duration-200" role="dialog" aria-modal="true" aria-labelledby="preview-modal-title">
      <div ref={modalRef} className="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-3xl shadow-2xl overflow-hidden flex flex-col max-h-[85vh]">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-slate-800 shrink-0">
          <div className="flex items-center gap-3 min-w-0">
            <div className="p-2 bg-rust-500/10 text-rust-400 rounded-lg shrink-0" aria-hidden="true">
              {isImage ? <ImageIcon className="w-5 h-5" /> : isText ? <FileCode className="w-5 h-5" /> : <FileText className="w-5 h-5" />}
            </div>
            <div className="truncate">
              <h3 id="preview-modal-title" className="font-semibold text-slate-100 truncate">{file.filename}</h3>
              <p className="text-xs text-slate-400">
                {formatBytes(file.total_size)} • {file.chunk_count} chunk(s) • RF={file.replication_factor}
              </p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={() => onDownload(file)}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-semibold bg-slate-800 hover:bg-slate-700 text-slate-200 transition-colors"
            >
              <Download className="w-3.5 h-3.5" aria-hidden="true" />
              <span>Download</span>
            </button>
            <button
              onClick={onClose}
              className="text-slate-400 hover:text-slate-200 p-1.5 rounded-lg hover:bg-slate-800 transition-colors"
              aria-label="Close preview"
            >
              <X className="w-5 h-5" />
            </button>
          </div>
        </div>

        {/* Preview Area */}
        <div className="flex-1 overflow-auto p-6 flex flex-col items-center justify-center bg-slate-950/60 min-h-[300px]">
          {effectiveLoading ? (
            <div className="flex flex-col items-center gap-2 text-slate-400" role="status" aria-live="polite" aria-label="Loading preview">
              <div className="w-6 h-6 border-2 border-rust-500/30 border-t-rust-500 rounded-full animate-spin" aria-hidden="true" />
              <span className="text-xs">Streaming object chunks...</span>
            </div>
          ) : isImage ? (
            effectiveMediaUrl ? (
              <img
                src={effectiveMediaUrl}
                alt={file.filename}
                className="max-h-[500px] w-auto max-w-full rounded-xl object-contain border border-slate-800 shadow-xl"
                onError={() => setError('Failed to render image stream')}
              />
            ) : (
              <div className="text-center text-slate-400 text-sm">No image preview available.</div>
            )
          ) : isVideo ? (
            effectiveMediaUrl ? (
              <video controls className="max-h-[500px] w-full rounded-xl border border-slate-800 shadow-xl">
                <source src={effectiveMediaUrl} />
                Your browser does not support video playback.
              </video>
            ) : (
              <div className="text-center text-slate-400 text-sm">No video preview available.</div>
            )
          ) : isAudio ? (
            effectiveMediaUrl ? (
              <audio controls className="w-full max-w-md">
                <source src={effectiveMediaUrl} />
                Your browser does not support audio playback.
              </audio>
            ) : (
              <div className="text-center text-slate-400 text-sm">No audio preview available.</div>
            )
          ) : isText ? (
            effectiveContent !== null ? (
              <pre className="w-full h-full p-4 rounded-xl bg-slate-950 border border-slate-800 text-xs font-mono-code text-slate-200 overflow-auto whitespace-pre-wrap">
                {effectiveContent}
              </pre>
            ) : (
              <div className="text-center text-slate-400 text-sm">
                No text preview available for objects larger than 2 MiB.
              </div>
            )
          ) : (
            <div className="flex flex-col items-center gap-3 text-center max-w-md p-6">
              <div className="p-4 rounded-full bg-slate-900 border border-slate-800 text-slate-500">
                <FileText className="w-10 h-10" />
              </div>
              <h4 className="text-sm font-semibold text-slate-200">Binary Object</h4>
              <p className="text-xs text-slate-400">
                Direct in-browser preview is not supported for this file format (<code>.{ext || 'bin'}</code>). You can download the file to inspect it locally.
              </p>
            </div>
          )}

          {effectiveError && (
            <div className="flex items-center gap-2 text-xs text-amber-400 bg-amber-500/10 border border-amber-500/20 px-3 py-2 rounded-xl mt-4" role="alert" aria-live="assertive">
              <AlertCircle className="w-4 h-4 shrink-0" aria-hidden="true" />
              <span>{effectiveError}</span>
            </div>
          )}
        </div>

        {/* Footer Meta */}
        <div className="px-6 py-3 border-t border-slate-800 bg-slate-950/80 text-xs text-slate-400 flex items-center justify-between shrink-0">
          <span>Uploaded: {formatFullDate(file.created_epoch_ms)}</span>
          <span className="font-mono text-[11px] text-slate-500">UUID: {file.file_id}</span>
        </div>
      </div>
    </div>
  );
};
