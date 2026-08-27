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

/** 中文整句查询预处理：提取 2-4 字关键词用于 FTS 回退。
 * 混合检索对中文整句返回 0 命中时，KbChat 用提取的关键词重试。 */
export function extractChineseTerms(q: string): string[] {
  // 移除常见停用词和标点
  const stopWords = new Set([
    '的', '了', '在', '是', '我', '有', '和', '就', '不', '人', '都', '一', '一个',
    '上', '也', '很', '到', '说', '要', '去', '你', '会', '着', '没有', '看', '好',
    '自己', '这', '他', '她', '它', '们', '那', '里', '为', '什么', '怎么', '如何',
    '哪些', '哪些', '关于', '中', '与', '及', '等', '之', '把', '被', '让', '给',
    '从', '向', '对', '以', '因', '但', '而', '如', '所', '可以', '能', '应该',
    '请', '问', '想', '知道', '告诉', '下', '吗', '呢', '吧', '啊',
  ]);
  // 提取 2-4 字的中文词组
  const terms: string[] = [];
  const clean = q.replace(/[^一-龥a-zA-Z0-9]/g, ' ');
  const segments = clean.split(/\s+/).filter(Boolean);
  for (const seg of segments) {
    if (/[\u4e00-\u9fa5]/.test(seg)) {
      // 中文段：提取 2-4 字组合
      for (let len = 4; len >= 2; len--) {
        for (let i = 0; i <= seg.length - len; i++) {
          const term = seg.slice(i, i + len);
          if (!stopWords.has(term)) terms.push(term);
        }
      }
    } else if (seg.length >= 2) {
      terms.push(seg);
    }
  }
  // 去重并返回
  return [...new Set(terms)];
}
