import { fireEvent, render, screen, waitFor, act } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { UploadModal } from './UploadModal';

const uploadFileWithProgressMock = vi.fn();
const getDefaultChunkSizeMock = vi.fn();

vi.mock('../../services/api', () => ({
  uploadFileWithProgress: (...args: unknown[]) => uploadFileWithProgressMock(...args),
  getDefaultChunkSize: (...args: unknown[]) => getDefaultChunkSizeMock(...args),
}));

vi.mock('../../utils/formatters', () => ({
  formatBytes: (n: number) => `${n} B`,
}));

function deferred<T = void>() {
  let resolve!: (v: T | PromiseLike<T>) => void;
  let reject!: (e?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe('UploadModal', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getDefaultChunkSizeMock.mockResolvedValue(4);
  });

  it('queues files and cancels a queued item while first upload is in flight', async () => {
    const user = userEvent.setup();
    const onSuccess = vi.fn();
    const d1 = deferred<void>();

    uploadFileWithProgressMock.mockImplementationOnce(() => ({ promise: d1.promise, abort: vi.fn() }));

    render(<UploadModal isOpen onClose={vi.fn()} onSuccess={onSuccess} onError={vi.fn()} />);

    const input = document.querySelector('input[type="file"]') as HTMLInputElement;
    fireEvent.change(input, {
      target: {
        files: [
          new File(['aaaa'], 'a.txt', { type: 'text/plain' }),
          new File(['bbbb'], 'b.txt', { type: 'text/plain' }),
        ],
      },
    });

    await waitFor(() => expect(uploadFileWithProgressMock).toHaveBeenCalledTimes(1));
    expect(screen.getByText('a.txt')).toBeInTheDocument();
    expect(screen.getByText('b.txt')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Remove b.txt from queue' }));
    expect(screen.getByText('Cancelled')).toBeInTheDocument();

    await act(async () => {
      d1.resolve();
      await Promise.resolve();
    });
    await waitFor(() => expect(onSuccess).toHaveBeenCalledWith('a.txt'));
    expect(uploadFileWithProgressMock).toHaveBeenCalledTimes(1);
  });
});
