/* ============================================================
 * 全局搜索 — 关键词高亮/摘要纯函数
 * 自 GlobalSearch.svelte 下沉：不依赖组件状态，可独立单测。
 * ============================================================ */

/** HTML 转义（防 XSS：搜索结果来自微信消息/KB 文档等不可信输入） */
function escapeHtml(raw: string): string {
  return raw
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

/** 关键词高亮：命中词包 <mark>（大小写不敏感，保留原文大小写）。
 *  安全：先整体转义再高亮，防止注入 HTML。 */
export function highlight(text: string, keyword: string): string {
  if (!text || !keyword) return escapeHtml(text ?? "");
  const escaped = escapeHtml(text);
  const esc = keyword.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const re = new RegExp(`(${esc})`, "gi");
  const parts = escaped.split(re);
  return parts.map((p) =>
    p.toLowerCase() === keyword.toLowerCase() ? `<mark>${p}</mark>` : p,
  ).join("");
}

/** 关键词摘要：命中位置前后截取，两侧加省略号。
 *  安全：返回已转义文本，可直接用于 {@html}。 */
export function excerpt(text: string, keyword: string, max = 140): string {
  if (!text) return "";
  const idx = keyword ? text.toLowerCase().indexOf(keyword.toLowerCase()) : -1;
  let raw: string;
  if (idx < 0) {
    raw = text.length > max ? text.slice(0, max) + "…" : text;
  } else {
    const start = Math.max(0, idx - Math.floor(max / 3));
    const end = Math.min(text.length, idx + keyword.length + Math.floor((max * 2) / 3));
    raw = (start > 0 ? "…" : "") + text.slice(start, end) + (end < text.length ? "…" : "");
  }
  return escapeHtml(raw);
}
