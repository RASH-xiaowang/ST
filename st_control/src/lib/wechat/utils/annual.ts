/* ============================================================
 * 微信数据管理模块 — 年度总结展示纯函数
 * 自 AnnualSummary.svelte 下沉：热力色、数量缩写、千分位、占比。
 * ============================================================ */

/** 热力图背景色：v/max 比例映射绿色透明度 */
export function heatBg(v: number, max: number): string {
  if (max <= 0 || v <= 0) return 'rgba(7,193,96,0.05)';
  const t = Math.max(0.1, Math.min(1, v / max));
  return `rgba(7,193,96,${(0.08 + t * 0.85).toFixed(3)})`;
}

/** 数量缩写：≥1 万 → "x.x万"（去尾 0）；NaN/null → "0" */
export function fmtNum(n: number): string {
  if (n == null || Number.isNaN(n)) return '0';
  if (n >= 10000) return (n / 10000).toFixed(1).replace(/\.0$/, '') + '万';
  return String(n);
}

/** 整数千分位（zh-CN）；NaN/null → "0" */
export function fmtInt(n: number): string {
  if (n == null || Number.isNaN(n)) return '0';
  return Number(n).toLocaleString('zh-CN');
}

/** 占比（0.1% 精度）；whole ≤ 0 → 0 */
export function pct(part: number, whole: number): number {
  return whole > 0 ? Math.round((part / whole) * 1000) / 10 : 0;
}

/** 热力峰值：返回 (星期索引, 小时, 值)；空矩阵返回 null */
export function heatPeak(
  matrix: number[][],
): { w: number; h: number; value: number } | null {
  if (!matrix.length) return null;
  let bestW = 0;
  let bestH = 0;
  let bestV = 0;
  for (let w = 0; w < matrix.length; w++) {
    for (let h = 0; h < (matrix[w]?.length || 0); h++) {
      const v = Number(matrix[w][h]) || 0;
      if (v > bestV) {
        bestV = v;
        bestW = w;
        bestH = h;
      }
    }
  }
  return { w: bestW, h: bestH, value: bestV };
}

/** 热力峰值展示（星期标签/小时补零/值；自 AnnualSummary peakInfo 下沉） */
export function peakInfoOf(
  heatmap: { weekdayLabels?: string[] } | null | undefined,
  matrix: number[][],
): { weekday: string; hour: string; value: number } {
  const pk = heatPeak(matrix);
  if (!pk) return { weekday: '', hour: '', value: 0 };
  return {
    weekday: heatmap?.weekdayLabels?.[pk.w] ?? '',
    hour: String(pk.h).padStart(2, '0'),
    value: pk.value,
  };
}

/** 指定小时集合占总热力的百分比（复用 pct；空矩阵 → 0） */
export function hourShare(matrix: number[][], hours: number[]): number {
  if (!matrix.length) return 0;
  const set = new Set(hours);
  let s = 0;
  let heatTotal = 0;
  for (const row of matrix) {
    for (let h = 0; h < row.length; h++) {
      const v = Number(row[h]) || 0;
      heatTotal += v;
      if (set.has(h)) s += v;
    }
  }
  return pct(s, heatTotal);
}

/** 周末（星期 5、6 行）占总热力百分比；矩阵行数 < 7 时返回 0 */
export function weekendShareOf(matrix: number[][]): number {
  if (matrix.length < 7) return 0;
  const s =
    (Number(matrix[5]?.reduce((a, b) => a + (Number(b) || 0), 0)) || 0) +
    (Number(matrix[6]?.reduce((a, b) => a + (Number(b) || 0), 0)) || 0);
  let heatTotal = 0;
  for (const row of matrix) for (const v of row) heatTotal += Number(v) || 0;
  return pct(s, heatTotal);
}

/** 最活跃月份索引（最大正数值；空/全 0 → -1） */
export function bestIndex(
  values: (number | string | null | undefined)[],
): number {
  let idx = -1;
  let best = 0;
  values.forEach((v, i) => {
    const n = Number(v) || 0;
    if (n > best) {
      best = n;
      idx = i;
    }
  });
  return idx;
}

/** 最安静月份索引（最小正数值；无正数时取第一个非正值；空 → -1） */
export function calmIndex(
  values: (number | string | null | undefined)[],
): number {
  if (!values.length) return -1;
  let idx = 0;
  let calm = Number.MAX_SAFE_INTEGER;
  values.forEach((v, i) => {
    const n = Number(v) || 0;
    if (n > 0 && n < calm) {
      calm = n;
      idx = i;
    }
  });
  if (calm === Number.MAX_SAFE_INTEGER) {
    idx = values.findIndex((v) => !(Number(v) > 0));
  }
  return idx;
}

/** 人物画像标签（作息/周末/群聊/话痨维度） */
export function buildPersonaTags(opts: {
  nightShare: number;
  morningShare: number;
  weekendShare: number;
  groupShare: number;
  dayAvg: number;
}): string[] {
  const tags: string[] = [];
  if (opts.nightShare >= 35) tags.push('夜猫子');
  else if (opts.morningShare >= 25) tags.push('早起党');
  else tags.push('作息均衡');
  if (opts.weekendShare >= 40) tags.push('周末话痨');
  else if (opts.weekendShare <= 25) tags.push('周末也忙碌');
  else tags.push('节奏从容');
  if (opts.groupShare >= 50) tags.push('群聊担当');
  else tags.push('单聊派');
  if (opts.dayAvg >= 60) tags.push('重度话痨');
  else if (opts.dayAvg >= 20) tags.push('表达欲旺盛');
  else tags.push('惜字如金');
  return tags;
}
