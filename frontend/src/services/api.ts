// API Client for RustyStore Gateway

import {
  ClusterStats,
  DeleteFileResponse,
  FileInfo,
  FileListResponse,
  FileSummary,
  HealthResponse,
  NodeInfo,
  ParsedPrometheusMetrics,
  ReadinessResponse,
  UploadProgress,
  UploadResponse,
} from '../types/api';
import {
  MOCK_CLUSTER_STATS,
  MOCK_FILE_DETAILS,
  MOCK_FILES,
  MOCK_NODES,
  MOCK_PROMETHEUS_RAW,
} from './mockData';
import { nextId } from '../utils/id';

const SETTINGS_KEY = 'rustystore_settings';
const FETCH_TIMEOUT_MS = 10000;

/** Structured gateway error with correlation info for the UI. */
export class ApiError extends Error {
  readonly status: number;
  readonly code?: string;
  readonly requestId?: string;

  constructor(message: string, status: number, code?: string, requestId?: string) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
    this.requestId = requestId;
  }
}

async function throwApiError(res: Response): Promise<never> {
  let message = `${res.status} ${res.statusText}`;
  let code: string | undefined;
  try {
    const body = await res.json();
    code = body?.error?.code;
    message = body?.error?.message ?? body?.reason ?? message;
  } catch {
    // Non-JSON error body; keep the HTTP status message.
  }
  throw new ApiError(message, res.status, code, res.headers.get('x-request-id') ?? undefined);
}

export function toErrorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export interface AppSettings {
  baseUrl: string;
  apiKey: string;
  demoMode: boolean;
  pollIntervalMs: number;
}

export function loadSettings(): AppSettings {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    if (raw) return JSON.parse(raw);
  } catch (e) {
    console.error('Failed to load settings', e);
  }
  return {
    baseUrl: '',
    apiKey: '',
    demoMode: false,
    pollIntervalMs: 5000,
  };
}

export function saveSettings(settings: AppSettings): void {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
  } catch (e) {
    console.error('Failed to save settings', e);
  }
}

function getHeaders(extra: Record<string, string> = {}): Record<string, string> {
  const settings = loadSettings();
  const headers: Record<string, string> = { ...extra };
  if (settings.apiKey.trim()) {
    headers['Authorization'] = `Bearer ${settings.apiKey.trim()}`;
  }
  return headers;
}

function buildUrl(path: string): string {
  const settings = loadSettings();
  const base = settings.baseUrl.trim().replace(/\/+$/, '');
  return `${base}${path}`;
}

/** Save a blob using an anchor, releasing the object URL only after the click. */
function saveBlob(blob: Blob, filename: string): void {
  const url = window.URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  // Revoking synchronously can cancel the download in some browsers.
  setTimeout(() => window.URL.revokeObjectURL(url), 1000);
}

/** Download URL honoring the configured gateway base URL (auth applied by callers). */
export function getDownloadUrl(fileId: string): string {
  return buildUrl(`/api/v1/files/${encodeURIComponent(fileId)}/download`);
}

export async function checkReadiness(): Promise<ReadinessResponse> {
  const settings = loadSettings();
  if (settings.demoMode) {
    return { status: 'ready' };
  }
  try {
    const res = await fetch(buildUrl('/api/v1/ready'), {
      headers: getHeaders(),
      signal: AbortSignal.timeout(3000),
    });
    if (res.ok) {
      return (await res.json()) as ReadinessResponse;
    }
    let reason = `HTTP ${res.status}`;
    try {
      const body = await res.json();
      reason = body?.reason ?? body?.error?.message ?? reason;
    } catch {
      // Keep the HTTP status as the reason.
    }
    return { status: 'not_ready', reason };
  } catch (err) {
    return { status: 'not_ready', reason: toErrorMessage(err) };
  }
}

export async function checkHealth(): Promise<HealthResponse> {
  const settings = loadSettings();
  if (settings.demoMode) {
    return { status: 'ok', service: 'gateway (demo)' };
  }
  const res = await fetch(buildUrl('/api/v1/health'), {
    headers: getHeaders(),
    signal: AbortSignal.timeout(FETCH_TIMEOUT_MS),
  });
  if (!res.ok) await throwApiError(res);
  return res.json();
}

export async function listFiles(
  limit = 50,
  cursor?: string
): Promise<FileListResponse> {
  const settings = loadSettings();
  if (settings.demoMode) {
    return { files: MOCK_FILES };
  }
  const params = new URLSearchParams();
  params.set('limit', limit.toString());
  if (cursor) params.set('cursor', cursor);

  const res = await fetch(buildUrl(`/api/v1/files?${params.toString()}`), {
    headers: getHeaders(),
    signal: AbortSignal.timeout(FETCH_TIMEOUT_MS),
  });
  if (!res.ok) await throwApiError(res);
  return res.json();
}

export async function getFile(fileId: string): Promise<FileInfo> {
  const settings = loadSettings();
  if (settings.demoMode) {
    const mock = MOCK_FILE_DETAILS[fileId];
    if (mock) return mock;
    const summary = MOCK_FILES.find((f) => f.file_id === fileId);
    return {
      file_id: fileId,
      filename: summary?.filename || 'unknown.dat',
      total_size: summary?.total_size || 4194304,
      chunk_size: 4194304,
      chunk_count: summary?.chunk_count || 1,
      replication_factor: 3,
      status: 'committed',
      chunks: [],
    };
  }
  const res = await fetch(buildUrl(`/api/v1/files/${fileId}`), {
    headers: getHeaders(),
    signal: AbortSignal.timeout(FETCH_TIMEOUT_MS),
  });
  if (!res.ok) await throwApiError(res);
  return res.json();
}

export async function deleteFile(fileId: string): Promise<DeleteFileResponse> {
  const settings = loadSettings();
  if (settings.demoMode) {
    const idx = MOCK_FILES.findIndex((f) => f.file_id === fileId);
    if (idx !== -1) MOCK_FILES.splice(idx, 1);
    return {
      status: 'deleted',
      file_id: fileId,
      chunks: 3,
      chunk_replicas_deleted: 9,
      chunk_replicas_failed: 0,
    };
  }
  const res = await fetch(buildUrl(`/api/v1/files/${fileId}`), {
    method: 'DELETE',
    headers: getHeaders(),
    signal: AbortSignal.timeout(FETCH_TIMEOUT_MS),
  });
  if (!res.ok) await throwApiError(res);
  return res.json();
}

export function downloadFile(fileId: string, filename: string): Promise<void> {
  const settings = loadSettings();
  if (settings.demoMode) {
    // Generate synthetic dummy download in demo mode
    const blob = new Blob([`RustyStore Demo Data for ${filename}`], {
      type: 'application/octet-stream',
    });
    saveBlob(blob, filename);
    return Promise.resolve();
  }

  return new Promise((resolve, reject) => {
    const url = getDownloadUrl(fileId);
    const xhr = new XMLHttpRequest();
    xhr.open('GET', url, true);
    xhr.timeout = 30000;
    xhr.responseType = 'blob';

    const headers = getHeaders();
    Object.entries(headers).forEach(([k, v]) => xhr.setRequestHeader(k, v));

    xhr.onload = () => {
      if (xhr.status >= 200 && xhr.status < 300) {
        saveBlob(xhr.response, filename);
        resolve();
      } else {
        reject(new ApiError(`Download failed with status ${xhr.status}`, xhr.status));
      }
    };
    xhr.onerror = () => reject(new ApiError('Network error during download', 0));
    xhr.ontimeout = () => reject(new ApiError('Download timed out', 0));
    xhr.send();
  });
}

export function uploadFileWithProgress(
  file: File,
  chunkSize = 4194304,
  onProgress?: (progress: UploadProgress) => void
): Promise<UploadResponse> {
  const settings = loadSettings();
  if (settings.demoMode) {
    return new Promise((resolve) => {
      let uploaded = 0;
      const total = file.size;
      const totalChunks = Math.max(1, Math.ceil(total / chunkSize));
      const interval = setInterval(() => {
        if (total > 0) uploaded += Math.min(total / 10, total - uploaded);
        const pct = total === 0 ? 100 : Math.min(100, Math.round((uploaded / total) * 100));
        const currentChunk = Math.min(
          totalChunks,
          Math.max(1, Math.ceil(uploaded / chunkSize))
        );
        onProgress?.({
          bytesUploaded: uploaded,
          totalBytes: total,
          percentage: pct,
          currentChunkEstimate: currentChunk,
          totalChunksEstimate: totalChunks,
        });

        if (uploaded >= total) {
          clearInterval(interval);
          const newFile: FileSummary = {
            file_id: nextId('demo-file'),
            filename: file.name,
            total_size: file.size,
            chunk_count: totalChunks,
            replication_factor: 3,
            status: 'committed',
            created_epoch_ms: Date.now(),
          };
          MOCK_FILES.unshift(newFile);
          resolve({
            file_id: newFile.file_id,
            upload_id: nextId('demo-upload'),
            chunk_size: chunkSize,
            chunk_count: totalChunks,
            replication_factor: newFile.replication_factor,
          });
        }
      }, 150);
    });
  }

  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    const url = buildUrl('/api/v1/files');
    xhr.open('POST', url, true);
    xhr.timeout = 30000;

    const headers = getHeaders({
      'x-filename': file.name,
      'content-type': 'application/octet-stream',
    });
    Object.entries(headers).forEach(([k, v]) => xhr.setRequestHeader(k, v));

    const totalChunks = Math.max(1, Math.ceil(file.size / chunkSize));

    xhr.upload.onprogress = (evt) => {
      if (evt.lengthComputable) {
        const pct = Math.round((evt.loaded / evt.total) * 100);
        const currentChunk = Math.min(
          totalChunks,
          Math.max(1, Math.ceil(evt.loaded / chunkSize))
        );
        onProgress?.({
          bytesUploaded: evt.loaded,
          totalBytes: evt.total,
          percentage: pct,
          currentChunkEstimate: currentChunk,
          totalChunksEstimate: totalChunks,
        });
      }
    };

    xhr.onload = () => {
      if (xhr.status >= 200 && xhr.status < 300) {
        try {
          const res = JSON.parse(xhr.responseText) as UploadResponse;
          resolve(res);
        } catch (e) {
          reject(new ApiError('Invalid response from server', xhr.status));
        }
      } else {
        reject(new ApiError(`Upload failed (${xhr.status}): ${xhr.responseText}`, xhr.status));
      }
    };

    xhr.onerror = () => reject(new ApiError('Network error during upload', 0));
    xhr.ontimeout = () => reject(new ApiError('Upload timed out', 0));
    xhr.send(file);
  });
}

export async function listNodes(): Promise<NodeInfo[]> {
  const settings = loadSettings();
  if (settings.demoMode) {
    return MOCK_NODES;
  }
  const res = await fetch(buildUrl('/api/v1/cluster/nodes'), {
    headers: getHeaders(),
    signal: AbortSignal.timeout(FETCH_TIMEOUT_MS),
  });
  if (!res.ok) await throwApiError(res);
  return res.json();
}

export async function getClusterStats(): Promise<ClusterStats> {
  const settings = loadSettings();
  if (settings.demoMode) {
    return {
      ...MOCK_CLUSTER_STATS,
      total_files: MOCK_FILES.length,
    };
  }
  const res = await fetch(buildUrl('/api/v1/cluster/stats'), {
    headers: getHeaders(),
    signal: AbortSignal.timeout(FETCH_TIMEOUT_MS),
  });
  if (!res.ok) await throwApiError(res);
  return res.json();
}

export async function getMetrics(): Promise<ParsedPrometheusMetrics> {
  const settings = loadSettings();
  let text: string;
  let available = true;

  if (settings.demoMode) {
    text = MOCK_PROMETHEUS_RAW;
  } else {
    try {
      const res = await fetch(buildUrl('/metrics'), {
        headers: getHeaders(),
        signal: AbortSignal.timeout(FETCH_TIMEOUT_MS),
      });
      if (!res.ok) await throwApiError(res);
      text = await res.text();
    } catch (e) {
      // Never show mock counters as if they were live telemetry.
      console.warn('Metrics endpoint unreachable', e);
      text = `# Metrics unavailable: ${(e as Error).message}`;
      available = false;
    }
  }

  const parseCounter = (pattern: RegExp): number => {
    const match = text.match(pattern);
    return match ? parseFloat(match[1]) : 0;
  };

  return {
    uploadedBytesTotal: parseCounter(/rustystore_uploaded_bytes_total\s+([0-9.eE+-]+)/),
    downloadedBytesTotal: parseCounter(/rustystore_downloaded_bytes_total\s+([0-9.eE+-]+)/),
    uploadFailuresTotal: parseCounter(/rustystore_upload_failures_total\s+([0-9.eE+-]+)/),
    activeStorageNodes: parseCounter(/rustystore_active_storage_nodes\s+([0-9.eE+-]+)/),
    underReplicatedChunks: parseCounter(/rustystore_under_replicated_chunks\s+([0-9.eE+-]+)/),
    rawText: text,
    available,
  };
}

/** Fetch file bytes as text, honoring gateway base URL and API key settings. */
export async function fetchFileText(fileId: string): Promise<string> {
  const settings = loadSettings();
  if (settings.demoMode) {
    return `RustyStore demo preview for ${fileId}\n\nThis is simulated text content. Disable Demo Mode to stream the real object from the gateway.`;
  }
  const res = await fetch(getDownloadUrl(fileId), {
    headers: getHeaders(),
    signal: AbortSignal.timeout(FETCH_TIMEOUT_MS),
  });
  if (!res.ok) await throwApiError(res);
  return res.text();
}

/** Fetch file bytes as a blob object URL. Caller must revoke the returned URL. */
export async function fetchFileBlobUrl(fileId: string): Promise<string> {
  const settings = loadSettings();
  let blob: Blob;
  if (settings.demoMode) {
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="640" height="360"><rect width="640" height="360" fill="#0f172a"/><text x="320" y="175" fill="#e05a47" font-family="sans-serif" font-size="24" text-anchor="middle">RustyStore Demo Preview</text><text x="320" y="210" fill="#94a3b8" font-family="sans-serif" font-size="14" text-anchor="middle">${fileId}</text></svg>`;
    blob = new Blob([svg], { type: 'image/svg+xml' });
  } else {
    const res = await fetch(getDownloadUrl(fileId), {
      headers: getHeaders(),
      signal: AbortSignal.timeout(FETCH_TIMEOUT_MS),
    });
    if (!res.ok) await throwApiError(res);
    blob = await res.blob();
  }
  return window.URL.createObjectURL(blob);
}

export interface ConnectionTestResult {
  ok: boolean;
  message: string;
  requestId?: string;
}

/** Validate gateway health, readiness, and the configured credentials. */
export async function testConnection(): Promise<ConnectionTestResult> {
  if (loadSettings().demoMode) {
    return { ok: true, message: 'Demo Mode is active; no gateway connection is required.' };
  }
  try {
    const health = await checkHealth();
    const ready = await checkReadiness();
    if (ready.status !== 'ready') {
      return { ok: false, message: ready.reason ?? 'Coordinator is not ready.' };
    }
    return { ok: true, message: `${health.service} is healthy and the coordinator is reachable.` };
  } catch (err) {
    const apiError = err instanceof ApiError ? err : undefined;
    return {
      ok: false,
      message: toErrorMessage(err),
      requestId: apiError?.requestId,
    };
  }
}
