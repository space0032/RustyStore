import React, { useState, useCallback, useMemo, lazy, Suspense } from 'react';
import { useInfiniteQuery, useQuery, useQueryClient } from '@tanstack/react-query';
import { Navbar } from './components/layout/Navbar';
import { Sidebar, ActiveTab } from './components/layout/Sidebar';
import { ToastContainer, ToastMessage, ToastType } from './components/common/Toast';
import { SettingsModal } from './components/settings/SettingsModal';
import { FilesView } from './views/FilesView';
import { ChunkVisualizerView } from './views/ChunkVisualizerView';
import { ClusterView } from './views/ClusterView';
import { FileDetailsModal } from './components/files/FileDetailsModal';

// Recharts is only needed on the Metrics tab; keep it out of the main bundle.
const MetricsView = lazy(() =>
  import('./views/MetricsView').then((module) => ({ default: module.MetricsView }))
);
import { FileSummary } from './types/api';
import {
  checkReadiness,
  downloadFile,
  getClusterStats,
  getMetrics,
  listFiles,
  listNodes,
  loadSettings,
  saveSettings,
  AppSettings,
} from './services/api';
import { nextId } from './utils/id';

interface EndpointError {
  label: string;
  message: string;
  status?: number;
  requestId?: string;
  retryAfter?: number;
}

function toEndpointError(label: string, error: unknown): EndpointError {
  const err = error as { message?: string; status?: number; requestId?: string; retryAfter?: number } | null;
  return {
    label,
    message: err?.message ?? 'Unknown error',
    status: err?.status,
    requestId: err?.requestId,
    retryAfter: err?.retryAfter,
  };
}

function getRouteFromUrl(): { tab: ActiveTab; fileId: string | null; detailsId: string | null } {
  try {
    const params = new URLSearchParams(window.location.search);
    const tabParam = params.get('tab');
    const tab: ActiveTab =
      tabParam === 'chunks' || tabParam === 'cluster' || tabParam === 'metrics' ? tabParam : 'files';
    const fileId = params.get('file');
    const detailsId = params.get('details');
    return { tab, fileId: fileId || null, detailsId: detailsId || null };
  } catch {
    // ignore
  }
  return { tab: 'files', fileId: null, detailsId: null };
}

export function App() {
  const queryClient = useQueryClient();
  const [settings, setSettings] = useState<AppSettings>(loadSettings());
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const initialRoute = getRouteFromUrl();
  const [activeTab, setActiveTab] = useState<ActiveTab>(initialRoute.tab);
  const [selectedFileId, setSelectedFileId] = useState<string | null>(initialRoute.fileId);
  const [detailsFileId, setDetailsFileId] = useState<string | null>(initialRoute.detailsId);
  const [toasts, setToasts] = useState<ToastMessage[]>([]);

  React.useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    if (activeTab === 'files') {
      params.delete('tab');
    } else {
      params.set('tab', activeTab);
    }
    if (activeTab === 'chunks' && selectedFileId) {
      params.set('file', selectedFileId);
    } else {
      params.delete('file');
    }
    if (detailsFileId) {
      params.set('details', detailsFileId);
    } else {
      params.delete('details');
    }
    const qs = params.toString();
    const url = qs ? `${window.location.pathname}?${qs}` : window.location.pathname;
    window.history.replaceState(null, '', url);
  }, [activeTab, selectedFileId, detailsFileId]);

  React.useEffect(() => {
    const onPopState = () => {
      const route = getRouteFromUrl();
      setActiveTab(route.tab);
      if (route.tab === 'chunks') {
        setSelectedFileId(route.fileId);
      } else {
        setSelectedFileId(null);
      }
      setDetailsFileId(route.detailsId);
    };
    window.addEventListener('popstate', onPopState);
    return () => window.removeEventListener('popstate', onPopState);
  }, []);

  const pollInterval = settings.pollIntervalMs > 0 ? settings.pollIntervalMs : false;

  const readinessQuery = useQuery({
    queryKey: ['readiness'],
    queryFn: checkReadiness,
    refetchInterval: pollInterval,
  });
  const filesQuery = useInfiniteQuery({
    queryKey: ['files'],
    queryFn: ({ pageParam }) => listFiles(50, pageParam),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (lastPage) => lastPage.next_cursor,
    refetchInterval: pollInterval,
  });
  const nodesQuery = useQuery({
    queryKey: ['nodes'],
    queryFn: listNodes,
    refetchInterval: pollInterval,
  });
  const statsQuery = useQuery({
    queryKey: ['stats'],
    queryFn: getClusterStats,
    refetchInterval: pollInterval,
  });
  const metricsQuery = useQuery({
    queryKey: ['metrics'],
    queryFn: getMetrics,
    refetchInterval: pollInterval,
  });

  const files = useMemo(
    () => filesQuery.data?.pages.flatMap((page) => page.files) ?? [],
    [filesQuery.data]
  );

  const detailsFile = useMemo(() => {
    if (!detailsFileId) return null;
    const found = files.find((f) => f.file_id === detailsFileId);
    if (found) return found;
    // Fallback so deep links still open even if the file isn't in loaded pages yet.
    return {
      file_id: detailsFileId,
      filename: 'Loading...',
      total_size: 0,
      chunk_count: 0,
      replication_factor: 3,
      status: 'unknown',
      created_epoch_ms: 0,
    } as FileSummary;
  }, [detailsFileId, files]);

  const loading = filesQuery.isPending || nodesQuery.isPending || statsQuery.isPending;
  const isFetching =
    filesQuery.isFetching ||
    nodesQuery.isFetching ||
    statsQuery.isFetching ||
    metricsQuery.isFetching ||
    readinessQuery.isFetching;

  const endpointErrors: EndpointError[] = [
    filesQuery.error && toEndpointError('Object list', filesQuery.error),
    nodesQuery.error && toEndpointError('Node list', nodesQuery.error),
    statsQuery.error && toEndpointError('Cluster stats', statsQuery.error),
    metricsQuery.error && toEndpointError('Metrics', metricsQuery.error),
    readinessQuery.data?.status === 'not_ready' && {
      label: 'Readiness',
      message: readinessQuery.data.reason ?? 'Coordinator is not ready.',
    },
  ].filter((e): e is EndpointError => Boolean(e));

  const authError = endpointErrors.find((e) => e.status === 401 || e.status === 403);

  const handleRefresh = useCallback(() => {
    // Manual refresh: refetch active queries without discarding loaded file pages.
    void queryClient.refetchQueries({ type: 'active' });
  }, [queryClient]);

  const handleFilesChanged = useCallback(() => {
    // After upload/delete, reset file list so changes (and new ordering/cursors) are consistent.
    void queryClient.resetQueries({ queryKey: ['files'] });
    void queryClient.invalidateQueries({ queryKey: ['stats'] });
    void queryClient.invalidateQueries({ queryKey: ['nodes'] });
  }, [queryClient]);

  const handleLoadAllFiles = useCallback(async () => {
    // Best-effort: keep fetching while more pages are available.
    for (let i = 0; i < 200; i++) {
      const res = await filesQuery.fetchNextPage();
      if (!res.hasNextPage) break;
    }
  }, [filesQuery]);

  const notify = useCallback((type: ToastType, title: string, message?: string) => {
    const id = nextId('toast');
    setToasts((prev) => [...prev, { id, type, title, message }]);
    setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== id));
    }, 4500);
  }, []);

  const dismissToast = (id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  };

  const handleSaveSettings = (newSettings: AppSettings) => {
    setSettings(newSettings);
    saveSettings(newSettings);
    // Settings changes (URL, auth, demo mode) can fundamentally change data; reset pagination.
    void queryClient.resetQueries();
    notify('success', 'Settings Saved', 'Connection parameters have been updated.');
  };

  const handleToggleDemo = () => {
    const updated = { ...settings, demoMode: !settings.demoMode };
    setSettings(updated);
    saveSettings(updated);
    void queryClient.resetQueries();
    notify(
      'info',
      updated.demoMode ? 'Demo Mode Enabled' : 'Demo Mode Disabled',
      updated.demoMode
        ? 'Using simulated cluster metadata and mock objects.'
        : 'Connecting to live RustyStore Gateway.'
    );
  };

  const handleInspectChunks = (fileId: string) => {
    setSelectedFileId(fileId);
    setDetailsFileId(null);
    setActiveTab('chunks');
  };

  const handleDownload = async (file: FileSummary) => {
    try {
      notify('info', 'Starting Download', `Streaming ${file.filename} from healthy replicas...`);
      await downloadFile(file.file_id, file.filename);
      notify('success', 'Download Complete', `${file.filename} verified with SHA-256.`);
    } catch (err) {
      const e = err as Error & { retryAfter?: number };
      const retryMsg = e.retryAfter ? ` Retry after ${e.retryAfter}s.` : '';
      notify('error', 'Download Failed', `${e.message}${retryMsg}`);
    }
  };

  return (
    <div className="min-h-screen bg-slate-950 text-slate-100 flex flex-col antialiased">
      <Navbar
        readiness={readinessQuery.data}
        isFetching={isFetching}
        onRefresh={handleRefresh}
        settings={settings}
        onOpenSettings={() => setIsSettingsOpen(true)}
        onToggleDemo={handleToggleDemo}
      />

      <div className="flex-1 flex overflow-hidden">
        <Sidebar
          activeTab={activeTab}
          onTabChange={setActiveTab}
          fileCount={files.length}
          nodeCount={nodesQuery.data?.length ?? 0}
        />

        <main className="flex-1 flex flex-col overflow-hidden" aria-label="Main content">
          {(endpointErrors.length > 0 || authError) && (
            <div className="px-6 pt-4 space-y-2" role="alert">
              {authError && (
                <div className="p-3 rounded-xl bg-rose-500/10 border border-rose-500/30 flex items-center justify-between gap-3 text-xs">
                  <div>
                    <span className="font-semibold text-rose-300">Authentication failed. </span>
                    <span className="text-rose-200/80">Check the API key in settings and retry.</span>
                  </div>
                  <button
                    onClick={() => setIsSettingsOpen(true)}
                    className="shrink-0 px-3 py-1.5 rounded-lg bg-rose-500/20 text-rose-200 hover:bg-rose-500/30 transition-colors"
                  >
                    Open Settings
                  </button>
                </div>
              )}
              {endpointErrors.map((err) => (
                <div
                  key={err.label}
                  className="p-3 rounded-xl bg-amber-500/10 border border-amber-500/30 flex items-center justify-between gap-3 text-xs"
                >
                  <div className="min-w-0">
                    <span className="font-semibold text-amber-300">{err.label}: </span>
                    <span className="text-amber-100/80">{err.message}</span>
                    {err.retryAfter != null && (
                      <span className="ml-2 text-amber-200/80">Retry after {err.retryAfter}s.</span>
                    )}
                    {err.requestId && (
                      <span className="ml-2 font-mono text-[10px] text-amber-200/60">
                        request {err.requestId}
                      </span>
                    )}
                  </div>
                  <button
                    onClick={handleRefresh}
                    className="shrink-0 px-3 py-1.5 rounded-lg bg-amber-500/20 text-amber-200 hover:bg-amber-500/30 transition-colors"
                  >
                    Retry
                  </button>
                </div>
              ))}
            </div>
          )}

          {activeTab === 'files' && (
            <FilesView
              files={files}
              loading={filesQuery.isPending}
              onRefresh={handleFilesChanged}
              nextCursor={filesQuery.data?.pages[filesQuery.data.pages.length - 1]?.next_cursor}
              onLoadMore={
                filesQuery.hasNextPage ? () => void filesQuery.fetchNextPage() : undefined
              }
              onLoadAll={filesQuery.hasNextPage ? handleLoadAllFiles : undefined}
              loadingMore={filesQuery.isFetchingNextPage}
              onViewDetails={(file) => setDetailsFileId(file.file_id)}
              onInspectChunks={handleInspectChunks}
              onDownloadFile={handleDownload}
              onNotify={notify}
            />
          )}

          {activeTab === 'chunks' && (
            <ChunkVisualizerView
              files={files}
              nodes={nodesQuery.data ?? []}
              selectedFileId={selectedFileId}
              onSelectFileId={setSelectedFileId}
              onNotify={notify}
            />
          )}

          {activeTab === 'cluster' && (
            <ClusterView
              stats={statsQuery.data ?? null}
              nodes={nodesQuery.data ?? []}
              loading={loading}
              onRefresh={handleRefresh}
            />
          )}

          {activeTab === 'metrics' && (
            <Suspense
              fallback={
                <div className="flex-1 flex items-center justify-center text-xs text-slate-400">
                  Loading telemetry view...
                </div>
              }
            >
              <MetricsView
                metrics={metricsQuery.data ?? null}
                loading={metricsQuery.isPending}
                onRefresh={handleRefresh}
                onNotify={notify}
              />
            </Suspense>
          )}
        </main>
      </div>

      <SettingsModal
        isOpen={isSettingsOpen}
        onClose={() => setIsSettingsOpen(false)}
        settings={settings}
        onSave={handleSaveSettings}
      />

      <FileDetailsModal
        file={detailsFile}
        isOpen={!!detailsFileId}
        onClose={() => setDetailsFileId(null)}
        onInspectChunks={handleInspectChunks}
        onDownload={handleDownload}
        onNotify={notify}
      />

      <ToastContainer toasts={toasts} onDismiss={dismissToast} />
    </div>
  );
}

export default App;
