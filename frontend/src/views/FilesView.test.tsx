import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { FilesView } from './FilesView';
import { FileSummary } from '../types/api';

vi.mock('../components/files/UploadModal', () => ({ UploadModal: () => null }));
vi.mock('../components/files/FilePreviewModal', () => ({ FilePreviewModal: () => null }));
vi.mock('../components/files/DeleteConfirmModal', () => ({ DeleteConfirmModal: () => null }));

function makeFile(i: number): FileSummary {
  return {
    file_id: `id-${i}`,
    filename: `file-${i}.txt`,
    total_size: i * 10,
    chunk_count: 1,
    replication_factor: 3,
    status: 'committed',
    created_epoch_ms: 1_700_000_000_000 + i,
  } as FileSummary;
}

const baseProps = () => ({
  files: [makeFile(1), makeFile(2), makeFile(3)],
  loading: false,
  onRefresh: vi.fn(),
  onViewDetails: vi.fn(),
  onInspectChunks: vi.fn(),
  onDownloadFile: vi.fn(),
  onNotify: vi.fn(),
});

describe('FilesView', () => {
  it('renders files and supports keyboard row navigation', () => {
    const props = baseProps();
    render(<FilesView {...props} />);

    const firstRow = screen.getByRole('row', { name: /file-1\.txt/i });
    firstRow.focus();
    fireEvent.keyDown(firstRow, { key: 'ArrowDown' });
    const secondRow = screen.getByRole('row', { name: /file-2\.txt/i });
    fireEvent.keyDown(secondRow, { key: 'Enter' });
    expect(props.onViewDetails).toHaveBeenCalledWith(expect.objectContaining({ filename: 'file-2.txt' }));

    props.onViewDetails.mockClear();
    const thirdRow = screen.getByRole('row', { name: /file-3\.txt/i });
    thirdRow.focus();
    fireEvent.keyDown(thirdRow, { key: 'Home' });
    const homeRow = screen.getByRole('row', { name: /file-1\.txt/i });
    fireEvent.keyDown(homeRow, { key: 'Enter' });
    expect(props.onViewDetails).toHaveBeenCalledWith(expect.objectContaining({ filename: 'file-1.txt' }));
  });

  it('uses controlled search callback when provided', async () => {
    const user = userEvent.setup();
    const onSearchTermChange = vi.fn();
    render(<FilesView {...baseProps()} searchTerm="" onSearchTermChange={onSearchTermChange} />);

    await user.type(screen.getByLabelText('Search all files by name'), 'abc');
    expect(onSearchTermChange).toHaveBeenCalled();
    expect(onSearchTermChange.mock.calls.map((c) => c[0]).join('')).toContain('abc');
  });

  it('hides local-search load-all hint when server search is controlled', () => {
    render(
      <FilesView
        {...baseProps()}
        searchTerm="report"
        onSearchTermChange={vi.fn()}
        nextCursor="cursor-1"
        onLoadMore={vi.fn()}
        onLoadAll={vi.fn()}
      />,
    );

    expect(screen.queryByText(/Search applies to loaded files only/i)).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Load all to search/i })).not.toBeInTheDocument();
  });
});
