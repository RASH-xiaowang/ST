/* ============================================================
 * 通用图表几何纯函数
 * 自 ChartView.svelte 下沉：调色板、极坐标、饼图扇形路径。
 * ============================================================ */

/** 图表调色板（10 色循环） */
export const PALETTE = [
  "#6366f1", "#22c55e", "#f59e0b", "#ef4444", "#06b6d4",
  "#a855f7", "#ec4899", "#84cc16", "#f97316", "#14b8a6",
];

/** 调色板取色（循环） */
export function chartColor(i: number): string {
  return PALETTE[i % PALETTE.length];
}

/** 极坐标：角度（度，0=上方）→ 笛卡尔坐标 */
export function polar(cx: number, cy: number, r: number, angle: number) {
  const a = (angle - 90) * (Math.PI / 180);
  return { x: cx + r * Math.cos(a), y: cy + r * Math.sin(a) };
}

/** 饼图扇形 SVG path（start/end 为角度，0=上方顺时针） */
export function arcPath(cx: number, cy: number, r: number, start: number, end: number) {
  const s = polar(cx, cy, r, end);
  const e = polar(cx, cy, r, start);
  const large = end - start <= 180 ? 0 : 1;
  return `M ${cx} ${cy} L ${s.x} ${s.y} A ${r} ${r} 0 ${large} 0 ${e.x} ${e.y} Z`;
}

/** 饼图切片角度（start/end 累积，0-360 度；total 兜底 1 防除零；自 ChartView 下沉） */
export function pieSliceAngles<T extends { value: number }>(
  items: T[],
  color: (index: number) => string,
): (T & { start: number; end: number; color: string })[] {
  const total = items.reduce((a, b) => a + b.value, 0) || 1;
  let acc = 0;
  return items.map((p, i) => {
    const start = (acc / total) * 360;
    acc += p.value;
    const end = (acc / total) * 360;
    return { ...p, start, end, color: color(i) };
  });
}
