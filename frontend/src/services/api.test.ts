import { describe, expect, it } from 'vitest';
import { ApiError, toErrorMessage } from './api';

describe('api helpers', () => {
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
});
