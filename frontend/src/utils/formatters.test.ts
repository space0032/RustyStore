import { describe, expect, it } from 'vitest';
import { formatBytes, formatFullDate, truncateHash, truncateUuid } from './formatters';

describe('formatters', () => {
  it('formats bytes', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(1024)).toBe('1 KiB');
    expect(formatBytes(1536)).toBe('1.5 KiB');
  });

  it('formats full date safely', () => {
    expect(formatFullDate(0)).toBe('-');
    expect(formatFullDate(1700000000000)).toContain('2023');
  });

  it('truncates hash and uuid', () => {
    expect(truncateHash('abcdef0123456789abcdef0123456789', 4, 4)).toBe('abcd...6789');
    expect(truncateUuid('12345678-aaaa-bbbb-cccc-ddddeeeeffff')).toBe('12345678');
  });
});
