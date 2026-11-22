import { render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { App } from './App';

vi.mock('./views/FilesView', () => ({ FilesView: (props: any) => <div data-testid="files-view">files:{props.searchTerm}</div> }));
vi.mock('./views/ChunkVisualizerView', () => ({ ChunkVisualizerView: () => <div>chunks-view</div> }));
vi.mock('./views/ClusterView', () => ({ ClusterView: () => <div>cluster-view</div> }));
vi.mock('./views/MetricsView', () => ({ MetricsView: () => <div>metrics-view</div> }));
vi.mock('./components/files/FileDetailsModal', () => ({ FileDetailsModal: ({ file, isOpen }: any) => isOpen ? <div role="dialog">details:{file?.file_id}</div> : null }));

vi.mock('./services/api', async () => {
  const actual = await vi.importActual<typeof import('./services/api')>('./services/api');
  return {
    ...actual,
    loadSettings: () => ({ baseUrl: 'http://localhost:8080', apiKey: '', demoMode: false, pollIntervalMs: 0 }),
    saveSettings: vi.fn(),
    checkReadiness: vi.fn().mockResolvedValue({ status: 'ready', metadata: true, coordinator: true }),
    getClusterStats: vi.fn().mockResolvedValue({ totalNodes: 1, aliveNodes: 1, deadNodes: 0, leaderId: 1, totalCapacity: 100, usedCapacity: 10, filesCount: 1, chunksCount: 2 }),
    getMetrics: vi.fn().mockResolvedValue({ available: true }),
    listFiles: vi.fn().mockResolvedValue({ files: [{ file_id: 'f1', filename: 'deep.txt', total_size: 10, chunk_count: 1, replication_factor: 3, status: 'committed', created_epoch_ms: 0 }], next_cursor: null }),
    listNodes: vi.fn().mockResolvedValue([]),
    downloadFile: vi.fn(),
    resetDemoData: vi.fn(),
  };
});

function renderApp() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <App />
    </QueryClientProvider>,
  );
}

describe('App deep links', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    window.history.replaceState({}, '', '/?tab=files&details=f1');
  });

  it('restores details modal from URL', async () => {
    renderApp();
    await waitFor(() => expect(screen.getByRole('dialog')).toHaveTextContent('details:f1'));
  });
});
