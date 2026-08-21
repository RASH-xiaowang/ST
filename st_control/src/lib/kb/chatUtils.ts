/* ============================================================
 * 知识库 — 对话展示纯函数
 * 自 KbChat.svelte 下沉：检索命中高亮分段、引用解析。
 * 不依赖组件状态，可独立单测。
 * ============================================================ */
import type { HighlightSegment } from './kbTypes';

/** 按查询词（空格分隔，≥2 字符）把内容切分为命中/未命中段 */
export function highlightSegments(content: string, q: string): HighlightSegment[] {
  const terms = q.split(/\s+/).filter((t) => t.length >= 2).map((t) => t.toLowerCase());
  if (terms.length === 0) return [{ text: content, hit: false }];
  const lower = content.toLowerCase();
  const segs: HighlightSegment[] = [];
  let i = 0;
  while (i < content.length) {
    let matched = false, mlen = 0;
    for (const t of terms) {
      const pos = lower.indexOf(t, i);
      if (pos === i) { matched = true; mlen = t.length; break; }
    }
    if (matched) {
      segs.push({ text: content.slice(i, i + mlen), hit: true });
      i += mlen;
    } else {
      const next = Math.min(...terms.map((t) => { const p = lower.indexOf(t, i); return p < 0 ? content.length : p; }));
      const end = next <= i ? content.length : next;
      segs.push({ text: content.slice(i, end), hit: false });
      i = end > i ? end : i + 1;
    }
  }
  return segs.length ? segs : [{ text: content, hit: false }];
}

/** 解析引用 JSON 字符串（非法/非数组返回空列表） */
export function parseCitations(c: string | null): Array<{ doc_id?: number; chunk_id?: number; kb_id?: number; doc_title?: string; section?: string | null; page_no?: number | null; score?: number; content?: string }> {
  if (!c) return [];
  try { const v = JSON.parse(c); return Array.isArray(v) ? v : []; } catch { return []; }
}
