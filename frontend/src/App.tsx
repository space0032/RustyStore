import React, { useState, useEffect, useCallback, useRef } from 'react';
import { Navbar } from './components/layout/Navbar';
import { Sidebar, ActiveTab } from './components/layout/Sidebar';
import { ToastContainer, ToastMessage, ToastType } from './components/common/Toast';
import { SettingsModal } from './components/settings/SettingsModal';
import { FilesView } from './views/FilesView';
import { ChunkVisualizerView } from './views/ChunkVisualizerView';
import { ClusterView } from './views/ClusterView';
import { MetricsView } from './views/MetricsView';
import {
  ClusterStats,
  FileSummary,
  NodeInfo,
  ParsedPrometheusMetrics,
  ReadinessResponse,
} from './types/api';
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

export function App() {
  const [settings, setSettings] = useState<AppSettings>(loadSettings());
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [activeTab, setActiveTab] = useState<ActiveTab>('files');
  const [selectedFileId, setSelectedFileId] = useState<string | null>(null);

  // Cluster data state
  const [files, setFiles] = useState<FileSummary[]>([]);
  const [nextCursor, setNextCursor] = useState<string | undefined>(undefined);
  const cursorRef = useRef<string | undefined>(undefined);
  const [loadingMore, setLoadingMore] = useState(false);
  const [nodes, setNodes] = useState<NodeInfo[]>([]);
  const [stats, setStats] = useState<ClusterStats | null>(null);
  const [readiness, setReadiness] = useState<ReadinessResponse | undefined>(undefined);
  const [metrics, setMetrics] = useState<ParsedPrometheusMetrics | null>(null);

  const [loading, setLoading] = useState(false);
  const [toasts, setToasts] = useState<ToastMessage[]>([]);

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

  const fetchData = useCallback(async (append = false) => {
    if (append) setLoadingMore(true);
    else setLoading(true);
    try {
      const [readyRes, filesRes, nodesRes, statsRes, metricsRes] = await Promise.allSettled([
        checkReadiness(),
        listFiles(50, append ? cursorRef.current : undefined),
        listNodes(),
        getClusterStats(),
        getMetrics(),
      ]);

      if (readyRes.status === 'fulfilled') setReadiness(readyRes.value);
      if (filesRes.status === 'fulfilled') {
        const page = filesRes.value.files;
        setFiles((prev) => {
          if (!append) return page;
          const seen = new Set(prev.map((f) => f.file_id));
          return [...prev, ...page.filter((f) => !seen.has(f.file_id))];
        });
        cursorRef.current = filesRes.value.next_cursor;
        setNextCursor(filesRes.value.next_cursor);
      }
      if (nodesRes.status === 'fulfilled') setNodes(nodesRes.value);
      if (statsRes.status === 'fulfilled') setStats(statsRes.value);
      if (metricsRes.status === 'fulfilled') setMetrics(metricsRes.value);
    } catch (err) {
      console.error('Error fetching cluster data', err);
    } finally {
      setLoading(false);
      setLoadingMore(false);
    }
  }, []);

  const handleLoadMore = useCallback(() => {
    if (nextCursor && !loading && !loadingMore) void fetchData(true);
  }, [nextCursor, loading, loadingMore, fetchData]);

  useEffect(() => {
    fetchData();
  }, [fetchData, settings.demoMode]);

  // Polling loop
  useEffect(() => {
    if (!settings.pollIntervalMs || settings.pollIntervalMs <= 0) return;
    const interval = setInterval(() => {
      fetchData();
    }, settings.pollIntervalMs);
    return () => clearInterval(interval);
  }, [fetchData, settings.pollIntervalMs]);

  const handleSaveSettings = (newSettings: AppSettings) => {
    setSettings(newSettings);
    saveSettings(newSettings);
    notify('success', 'Settings Saved', 'Connection parameters have been updated.');
  };

  const handleToggleDemo = () => {
    const updated = { ...settings, demoMode: !settings.demoMode };
    setSettings(updated);
    saveSettings(updated);
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
    setActiveTab('chunks');
  };

  const handleDownload = async (file: FileSummary) => {
    try {
      notify('info', 'Starting Download', `Streaming ${file.filename} from healthy replicas...`);
      await downloadFile(file.file_id, file.filename);
      notify('success', 'Download Complete', `${file.filename} verified with SHA-256.`);
    } catch (err) {
      notify('error', 'Download Failed', (err as Error).message);
    }
  };

  return (
    <div className="min-h-screen bg-slate-950 text-slate-100 flex flex-col antialiased">
      <Navbar
        readiness={readiness}
        isFetching={loading}
        onRefresh={fetchData}
        settings={settings}
        onOpenSettings={() => setIsSettingsOpen(true)}
        onToggleDemo={handleToggleDemo}
      />

      <div className="flex-1 flex overflow-hidden">
        <Sidebar
          activeTab={activeTab}
          onTabChange={setActiveTab}
          fileCount={files.length}
          nodeCount={nodes.length}
        />

        <main className="flex-1 flex flex-col overflow-hidden">
          {activeTab === 'files' && (
            <FilesView
              files={files}
              loading={loading}
              onRefresh={fetchData}
              nextCursor={nextCursor}
              onLoadMore={handleLoadMore}
              loadingMore={loadingMore}
              onInspectChunks={handleInspectChunks}
              onDownloadFile={handleDownload}
              onNotify={notify}
            />
          )}

          {activeTab === 'chunks' && (
            <ChunkVisualizerView
              files={files}
              nodes={nodes}
              selectedFileId={selectedFileId}
              onSelectFileId={setSelectedFileId}
              onNotify={notify}
            />
          )}

          {activeTab === 'cluster' && (
            <ClusterView
              stats={stats}
              nodes={nodes}
              loading={loading}
              onRefresh={fetchData}
            />
          )}

          {activeTab === 'metrics' && (
            <MetricsView
              metrics={metrics}
              loading={loading}
              onRefresh={fetchData}
              onNotify={notify}
            />
          )}
        </main>
      </div>

      <SettingsModal
        isOpen={isSettingsOpen}
        onClose={() => setIsSettingsOpen(false)}
        settings={settings}
        onSave={handleSaveSettings}
      />

      <ToastContainer toasts={toasts} onDismiss={dismissToast} />
    </div>
  );
}

export default App;
