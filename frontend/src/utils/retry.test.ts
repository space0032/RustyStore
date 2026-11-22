import { describe, expect, it } from 'vitest';
import { ApiError } from '../services/api';
import { getErrorStatus, getRetryDelay, shouldRetryQuery } from './retry';

describe('retry helpers', () => {
  it('extracts status and respects Retry-After', () => {
    const err = new ApiError('rate limited', 429, 'RATE_LIMITED', 'req', 12);
    expect(getErrorStatus(err)).toBe(429);
    expect(getRetryDelay(0, err)).toBe(12000);
    expect(getRetryDelay(0, new ApiError('rate limited', 429, 'RATE_LIMITED', 'req', 120))).toBe(30000);
  });

  it('applies retry policy by status', () => {
    expect(shouldRetryQuery(0, new ApiError('auth', 401))).toBe(false);
    expect(shouldRetryQuery(0, new ApiError('not found', 404))).toBe(false);
    expect(shouldRetryQuery(3, new ApiError('rate', 429))).toBe(true);
    expect(shouldRetryQuery(4, new ApiError('rate', 429))).toBe(false);
    expect(shouldRetryQuery(0, new ApiError('bad request', 400))).toBe(true);
    expect(shouldRetryQuery(1, new ApiError('bad request', 400))).toBe(false);
    expect(shouldRetryQuery(1, new ApiError('server', 500))).toBe(true);
    expect(shouldRetryQuery(2, new ApiError('server', 500))).toBe(false);
  });
});
