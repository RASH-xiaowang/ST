// 知识库埋点辅助（fire-and-forget 调 kb_track_event，失败不影响业务）
import { kbApi } from './services/ipc';

export interface TrackEventOptions {
  kbId?: number | null;
  docId?: number | null;
  pageId?: number | null;
  sessionId?: number | null;
  detail?: string | null;
}

export function track(eventType: string, opts: TrackEventOptions = {}) {
  try {
    kbApi.trackEvent({
      eventType,
      kbId: opts.kbId ?? null,
      docId: opts.docId ?? null,
      pageId: opts.pageId ?? null,
      sessionId: opts.sessionId ?? null,
      detail: opts.detail ?? null,
    }).catch(() => {
      /* 埋点失败静默忽略 */
    });
  } catch {
    /* 忽略 */
  }
}
