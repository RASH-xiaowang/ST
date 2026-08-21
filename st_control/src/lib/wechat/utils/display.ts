/* ============================================================
 * 微信数据管理模块 — 展示格式化纯函数
 * 自 RelationshipGraph.svelte 下沉：相对时间、榜单排名、数量缩写。
 * 不依赖组件状态，可独立单测。
 * ============================================================ */

/** 相对时间（后端 last_ts 为 Unix 秒） */
export function relTime(ts?: number): string {
  if (!ts || ts <= 0) return "—";
  const now = Date.now() / 1000;
  const d = now - ts;
  if (d < 60) return "刚刚";
  if (d < 3600) return `${Math.floor(d / 60)} 分钟前`;
  if (d < 86400) return `${Math.floor(d / 3600)} 小时前`;
  if (d < 172800) return "昨天";
  if (d < 604800) return `${Math.floor(d / 86400)} 天前`;
  const dt = new Date(ts * 1000);
  const mm = String(dt.getMonth() + 1).padStart(2, "0");
  const dd = String(dt.getDate()).padStart(2, "0");
  return `${dt.getFullYear()}-${mm}-${dd}`;
}

/** 在榜单中的排名（1 起；不在榜返回 0） */
export function rankOf<T>(list: T[], id: string, key: (n: T) => number): number {
  const idx = [...list]
    .sort((a, b) => key(b) - key(a))
    .findIndex((n) => (n as { id?: unknown }).id === id);
  return idx >= 0 ? idx + 1 : 0;
}

/** 数量缩写：万 → "x.xw"，千 → "x.xk"，否则原样 */
export function fmtCount(n: number): string {
  if (n >= 10000) return (n / 10000).toFixed(1) + "w";
  if (n >= 1000) return (n / 1000).toFixed(1) + "k";
  return String(n);
}
