import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { DeleteConfirmModal } from './DeleteConfirmModal';

const deleteFileMock = vi.fn();

vi.mock('../../services/api', () => ({
  deleteFile: (...args: unknown[]) => deleteFileMock(...args),
}));

vi.mock('../../utils/formatters', () => ({
  formatBytes: (n: number) => `${n} B`,
}));

vi.mock('../../hooks/useFocusTrap', () => ({
  useFocusTrap: () => ({ current: null }),
  useEscapeClose: (_isOpen: boolean, onClose: () => void) => {
    // expose for manual invocation not needed; escape handled by document in real hook
    void onClose;
  },
}));

describe('DeleteConfirmModal', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('deletes file and calls callbacks', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const onDeleted = vi.fn();
    const onError = vi.fn();
    const file = { file_id: 'f1', filename: 'a.bin', total_size: 10, chunk_count: 1, replication_factor: 3, status: 'committed', created_epoch_ms: 0 } as any;
    deleteFileMock.mockResolvedValue({ ok: true });

    render(<DeleteConfirmModal file={file} isOpen onClose={onClose} onDeleted={onDeleted} onError={onError} />);

    await user.click(screen.getByRole('button', { name: /Confirm Delete/i }));
    await waitFor(() => expect(deleteFileMock).toHaveBeenCalledWith('f1'));
    expect(onDeleted).toHaveBeenCalledWith(file);
    expect(onClose).toHaveBeenCalled();
    expect(onError).not.toHaveBeenCalled();
  });

  it('surfaces delete errors', async () => {
    const user = userEvent.setup();
    const onError = vi.fn();
    const file = { file_id: 'f1', filename: 'a.bin', total_size: 10, chunk_count: 1, replication_factor: 3, status: 'committed', created_epoch_ms: 0 } as any;
    deleteFileMock.mockRejectedValue(new Error('boom'));

    render(<DeleteConfirmModal file={file} isOpen onClose={vi.fn()} onDeleted={vi.fn()} onError={onError} />);

    await user.click(screen.getByRole('button', { name: /Confirm Delete/i }));
    await waitFor(() => expect(onError).toHaveBeenCalledWith('boom'));
  });
});
