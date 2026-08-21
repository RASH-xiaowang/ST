/* ============================================================
 * 系统指标 — 展示格式化纯函数
 * 自 DataDashboard.svelte 下沉：历史窗口、速率/带宽/在线时长/颜色/百分比。
 * 不依赖组件状态，可独立单测。
 * ============================================================ */

/** 历史窗口上限（与组件 HIST 一致） */
export const HIST = 48;

/** 追加数值并保持窗口上限（超限移除最旧） */
export function pushHist(arr: number[], v: number): number[] {
  const next = arr.concat(v);
  if (next.length > HIST) next.shift();
  return next;
}

/** 速率格式化：0/NaN → "0 B/s"，B/s..GB/s 1 位小数 */
export function fmtRate(n: number): string {
  if (!n || !isFinite(n)) return '0 B/s';
  const u = ['B/s', 'KB/s', 'MB/s', 'GB/s'];
  let i = Math.floor(Math.log(n) / Math.log(1024));
  i = Math.min(i, u.length - 1);
  return (n / Math.pow(1024, i)).toFixed(i === 0 ? 0 : 1) + ' ' + u[i];
}

/** 带宽格式化：≥1Gbps 一位小数，否则取整 Mbps；0/NaN → "--" */
export function fmtLink(bps: number): string {
  if (!bps || !isFinite(bps)) return '--';
  if (bps >= 1e9) return (bps / 1e9).toFixed(1) + ' Gbps';
  return Math.round(bps / 1e6) + ' Mbps';
}

/** 在线时长格式化：天/时/分/秒 */
export function fmtUptime(s: number): string {
  s = Math.floor(s);
  const d = Math.floor(s / 86400); s %= 86400;
  const h = Math.floor(s / 3600); s %= 3600;
  const mi = Math.floor(s / 60);
  const se = s % 60;
  if (d > 0) return `${d}天 ${h}时 ${mi}分`;
  if (h > 0) return `${h}时 ${mi}分 ${se}秒`;
  return `${mi}分 ${se}秒`;
}

/** 使用率 → 状态颜色（青 → 黄 → 橙 → 红） */
export function colorFor(v: number): string {
  if (v < 50) return '#22d3ee';
  if (v < 75) return '#fbbf24';
  if (v < 90) return '#fb923c';
  return '#f87171';
}

/** 百分比格式化：null/undefined → "N/A"，否则 1 位小数 + % */
export function fmtPct(v: number | null | undefined): string {
  return v == null ? 'N/A' : v.toFixed(1) + '%';
}
