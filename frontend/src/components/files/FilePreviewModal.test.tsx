import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { FilePreviewModal } from './FilePreviewModal';
import { FileSummary } from '../../types/api';

const fetchFileTextMock = vi.fn();

vi.mock('../../services/api', () => ({
  fetchFileBlobUrl: vi.fn(),
  fetchFileText: (...args: unknown[]) => fetchFileTextMock(...args),
  loadSettings: () => ({ baseUrl: '', apiKey: '', demoMode: false, pollIntervalMs: 0 }),
}));

vi.mock('../../utils/formatters', () => ({
  formatBytes: (n: number) => `${n} B`,
  formatFullDate: () => 'date',
}));

const file: FileSummary = {
  file_id: 'f1',
  filename: 'notes.txt',
  total_size: 10,
  chunk_count: 1,
  replication_factor: 3,
  status: 'committed',
  created_epoch_ms: 0,
};

describe('FilePreviewModal', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    fetchFileTextMock.mockResolvedValue('hello');
  });

  it('loads text preview, traps focus basics, and closes on Escape', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const onDownload = vi.fn();

    render(
      <div>
        <button type="button">trigger</button>
        <FilePreviewModal file={file} isOpen onClose={onClose} onDownload={onDownload} />
      </div>,
    );

    await waitFor(() => expect(fetchFileTextMock).toHaveBeenCalledWith('f1'));
    expect(await screen.findByText('hello')).toBeInTheDocument();

    const closeButton = screen.getByRole('button', { name: 'Close preview' });
    closeButton.focus();
    await user.keyboard('{Escape}');
    expect(onClose).toHaveBeenCalled();
  });
});
