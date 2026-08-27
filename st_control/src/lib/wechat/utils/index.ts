/* ============================================================
 * 微信数据管理模块 — 通用工具函数
 * ============================================================ */

/** 格式化日志错误 */
export function logError(context: string, err: unknown): void {
  console.error(`[WeChat] ${context}:`, err);
}

/** 格式化调试日志 */
export function logDebug(context: string, data: unknown): void {
  console.debug(`[WeChat] ${context}:`, data);
}

/** 数值钳制 */
export function clamp(n: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, n));
}

/** 安全解析整数 */
export function safeParseInt(
  v: string | number | undefined,
  fallback: number,
  min = Number.MIN_SAFE_INTEGER,
  max = Number.MAX_SAFE_INTEGER,
): number {
  if (typeof v === 'number') return clamp(Number.isFinite(v) ? v : fallback, min, max);
  if (typeof v !== 'string' || v.trim() === '') return fallback;
  const n = Number(v.trim());
  if (!Number.isFinite(n)) return fallback;
  return clamp(Math.trunc(n), min, max);
}

/** 格式化会话/消息时间分割线 */
export function formatDividerTime(ts: number | string | Date | undefined): string {
  if (!ts) return '';
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return String(ts);
  const now = new Date();
  const isToday = d.toDateString() === now.toDateString();
  const yesterday = new Date(now);
  yesterday.setDate(yesterday.getDate() - 1);
  const isYesterday = d.toDateString() === yesterday.toDateString();
  const pad = (n: number) => n.toString().padStart(2, '0');
  if (isToday) return `今天 ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  if (isYesterday) return `昨天 ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** 从用户名取头像首字母 */
export function avatarLetter(name: string): string {
  if (!name) return '?';
  const first = name.trim().charAt(0);
  if (/[\u4e00-\u9fff]/.test(first)) return first;
  return first.toUpperCase();
}

/** HTML 转义，防止 XSS */
export function escapeHtml(raw: string): string {
  return raw
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

/**
 * 将微信文本消息中的 `[表情名]` 代码替换为静态表情图片的 HTML。
 *
 * @param text  原始消息文本，如 "[微笑]你好[撇嘴]"
 * @param emojiMap 表情名→图片URL 映射 (如 "微笑" → "/emoticons/face/微笑.png")
 * @returns 替换后的 HTML 字符串，可用 {@html} 渲染
 *
 * @example
 * renderEmojiText("[微笑]你好", map)
 * // → '<img src="/emoticons/face/微笑.png" class="wc-emoji-inline" alt="[微笑]">你好'
 */
export function renderEmojiText(text: string, emojiMap: Map<string, string>): string {
  if (!text) return '';
  // 记忆化：聊天窗口滚动/追加时同一文本会反复渲染，避免每次重跑正则。
  // key 携带 emojiMap.size 做代际标记——静态表情清单晚于首屏加载完成时自动失效。
  const key = `${emojiMap.size}:${text}`;
  const hit = emojiMemo.get(key);
  if (hit !== undefined) return hit;
  const out = renderEmojiTextImpl(text, emojiMap);
  if (emojiMemo.size >= EMOJI_MEMO_MAX) emojiMemo.clear();
  emojiMemo.set(key, out);
  return out;
}

const emojiMemo = new Map<string, string>();
const EMOJI_MEMO_MAX = 800;

function renderEmojiTextImpl(text: string, emojiMap: Map<string, string>): string {
  // 安全修复：先整体转义，杜绝任何原文中的 HTML 注入（XSS 防护）。
  // escapeHtml 不影响 [xxx] 中的括号和中文，正则仍能正确匹配表情 token。
  const escaped = escapeHtml(text);
  // 匹配 [任意字符]（表情名中不含 ]）
  return escaped.replace(/\[([^\]]+)\]/g, (match, name: string) => {
    const path = emojiMap.get(name.trim());
    if (path) {
      // path 来自本地静态资源映射，已可信；alt 使用 match（已转义）
      return `<img src="${escapeHtml(path)}" class="wc-emoji-inline" alt="${match}">`;
    }
    // 未匹配到静态资源，保留已转义文本
    return match;
  });
}
