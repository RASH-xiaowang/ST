/* ============================================================
 * 数据库管理 — 列宽持久化格式
 * 自 DbManager.svelte 下沉：配置键的派生/解析/拼接纯函数。
 * 格式：`col_width:<dbKey>:<table>:<col>` → 像素值（字符串）。
 * ============================================================ */

const PREFIX = 'col_width:';

/** 列宽按数据源隔离的 key 前缀（内部库=internal，外部库=文件名） */
export function dbWidthKeyFromPath(path: string | null | undefined): string {
  return path ? (path.split(/[\\/]/).pop() || 'ext') : 'internal';
}

/** 拼接完整配置键：col_width:<dbKey>:<table>:<col> */
export function colWidthKey(dbKey: string, table: string, col: string): string {
  return `${PREFIX}${dbKey}:${table}:${col}`;
}

/** 解析配置项为列宽映射（key 形如 `<dbKey>:<table>:<col>` → 像素值） */
export function parseColWidths(items: { key: string; value: string }[]): Record<string, number> {
  const w: Record<string, number> = {};
  for (const item of items) {
    if (!item.key.startsWith(PREFIX)) continue;
    const inner = item.key.slice(PREFIX.length);
    const sep = inner.indexOf(':');
    if (sep <= 0) continue;
    const dbKey = inner.slice(0, sep);
    const rest = inner.slice(sep + 1); // "table:col"
    const v = parseInt(item.value, 10);
    if (v > 0) w[`${dbKey}:${rest}`] = v;
  }
  return w;
}
