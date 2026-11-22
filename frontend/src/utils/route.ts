import type { ActiveTab } from '../components/layout/Sidebar';

export interface RouteState {
  tab: ActiveTab;
  fileId: string | null;
  detailsId: string | null;
  previewId: string | null;
}

export function getRouteFromUrl(search: string = window.location.search): RouteState {
  try {
    const params = new URLSearchParams(search);
    const tabParam = params.get('tab');
    const tab: ActiveTab =
      tabParam === 'chunks' || tabParam === 'cluster' || tabParam === 'metrics' ? tabParam : 'files';
    const fileId = params.get('file');
    const detailsId = params.get('details');
    const previewId = params.get('preview');
    return { tab, fileId: fileId || null, detailsId: detailsId || null, previewId: previewId || null };
  } catch {
    // ignore
  }
  return { tab: 'files', fileId: null, detailsId: null, previewId: null };
}
