import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { ApiError, toErrorMessage, listFiles, saveSettings } from './api';

describe('api helpers', () => {
  beforeEach(() => {
    localStorage.clear();
    vi.restoreAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('preserves ApiError metadata', () => {
    const err = new ApiError('rate limited', 429, 'RATE_LIMITED', 'req-1', 30);
    expect(err.name).toBe('ApiError');
    expect(err.status).toBe(429);
    expect(err.code).toBe('RATE_LIMITED');
    expect(err.requestId).toBe('req-1');
    expect(err.retryAfter).toBe(30);
    expect(toErrorMessage(err)).toBe('rate limited');
  });

  it('stringifies unknown errors', () => {
    expect(toErrorMessage('boom')).toBe('boom');
  });

  it('includes search query when listing files', async () => {
    saveSettings({ baseUrl: 'http://example.test', apiKey: '', demoMode: false, pollIntervalMs: 0 });
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      ok: true,
      json: async () => ({ files: [], next_cursor: null }),
    } as Response);

    await listFiles(25, 'cursor-1', 'report');

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url] = fetchMock.mock.calls[0] as [string];
    expect(url).toContain('/api/v1/files?');
    expect(url).toContain('limit=25');
    expect(url).toContain('cursor=cursor-1');
    expect(url).toContain('search=report');
  });
});
