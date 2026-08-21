/* ============================================================
 * 全局搜索 — 关键词高亮/摘要纯函数
 * 自 GlobalSearch.svelte 下沉：不依赖组件状态，可独立单测。
 * ============================================================ */

/** 关键词高亮：命中词包 <mark>（大小写不敏感，保留原文大小写） */
export function highlight(text: string, keyword: string): string {
  if (!text || !keyword) return text;
  const esc = keyword.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const re = new RegExp(`(${esc})`, "gi");
  const parts = text.split(re);
  return parts.map((p) =>
    p.toLowerCase() === keyword.toLowerCase() ? `<mark>${p}</mark>` : p,
  ).join("");
}

/** 关键词摘要：命中位置前后截取，两侧加省略号 */
export function excerpt(text: string, keyword: string, max = 140): string {
  if (!text) return "";
  const idx = keyword ? text.toLowerCase().indexOf(keyword.toLowerCase()) : -1;
  if (idx < 0) return text.length > max ? text.slice(0, max) + "…" : text;
  const start = Math.max(0, idx - Math.floor(max / 3));
  const end = Math.min(text.length, idx + keyword.length + Math.floor((max * 2) / 3));
  return (start > 0 ? "…" : "") + text.slice(start, end) + (end < text.length ? "…" : "");
}
