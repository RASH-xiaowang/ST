/* ============================================================
 * 通用关键词过滤纯函数（跨 feature 共享）
 * 自 wechat/utils/panel.ts 上移：单/多字段、字符串数组分段匹配。
 * ============================================================ */

/** keyFn 提取的匹配字段：单段字符串，或字符串数组（任一元素命中即命中） */
export type FilterText = string | readonly string[];

function textIncludes(haystack: FilterText, kw: string): boolean {
  if (typeof haystack === 'string') return haystack.toLowerCase().includes(kw);
  return haystack.some((s) => s.toLowerCase().includes(kw));
}

/** 按关键词（去首尾空格、大小写不敏感）过滤列表：keyFn 提取匹配字段。
 * 空白关键词返回原数组引用。 */
export function filterByKeyword<T>(
  items: T[],
  q: string,
  keyFn: (item: T) => FilterText,
): T[] {
  if (!q.trim()) return items;
  const kw = q.trim().toLowerCase();
  return items.filter((it) => textIncludes(keyFn(it), kw));
}

/** 按关键词（去首尾空格、大小写不敏感）过滤列表：任一 keyFn 提取的字段命中即保留。
 * 空白关键词返回原数组引用。 */
export function filterByAnyKeyword<T>(
  items: T[],
  q: string,
  ...keyFns: Array<(item: T) => FilterText>
): T[] {
  const kw = q.trim().toLowerCase();
  if (!kw) return items;
  return items.filter((it) => keyFns.some((fn) => textIncludes(fn(it), kw)));
}
