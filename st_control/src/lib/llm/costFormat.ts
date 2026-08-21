/* ============================================================
 * 大模型用量/成本 — 展示格式化纯函数
 * 自 UsageCostTab.svelte 下沉：不限额度、使用率。
 * ============================================================ */

/** 额度展示：null → "不限"，否则千分位 */
export function fmtLimit(v: number | null): string {
  return v == null ? "不限" : v.toLocaleString();
}

/** 使用率百分比（1 位小数） */
export function fmtRatio(r: number): string {
  return `${r.toFixed(1)}%`;
}
