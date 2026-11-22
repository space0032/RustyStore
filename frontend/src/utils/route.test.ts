import { describe, expect, it } from 'vitest';
import { getRouteFromUrl } from './route';

describe('getRouteFromUrl', () => {
  it('defaults to files tab with no deep-link ids', () => {
    expect(getRouteFromUrl('')).toEqual({ tab: 'files', fileId: null, detailsId: null, previewId: null });
  });

  it('parses supported tab and deep-link ids', () => {
    expect(getRouteFromUrl('?tab=chunks&file=f1&details=d1&preview=p1')).toEqual({
      tab: 'chunks',
      fileId: 'f1',
      detailsId: 'd1',
      previewId: 'p1',
    });
  });

  it('falls back to files for unknown tab', () => {
    expect(getRouteFromUrl('?tab=unknown&file=f1').tab).toBe('files');
  });
});
