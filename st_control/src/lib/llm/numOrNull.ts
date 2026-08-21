/* ============================================================
 * 大模型配置 — 数值输入解析
 * 自 ProviderConfigTab.svelte 下沉：空串/非法 → null。
 * ============================================================ */

/** 字符串 → 数字；空串或非法返回 null */
export function numOrNull(v: string): number | null {
  if (v.trim() === "") return null;
  const n = Number(v);
  return Number.isFinite(n) ? n : null;
}
