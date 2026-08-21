/* ============================================================
 * 系统指标 — SVG 图表路径纯函数
 * 自 DataDashboard.svelte 下沉：折线/面积/雷达图几何。
 * ============================================================ */

/** 折线 path（值归一化到画布；空数组返回空串） */
export function buildLine(vals: number[], w: number, h: number, pad = 2): string {
  if (vals.length === 0) return '';
  const max = Math.max(...vals, 1);
  const min = Math.min(...vals, 0);
  const range = max - min || 1;
  const step = (w - pad * 2) / Math.max(1, vals.length - 1);
  return vals
    .map((v, i) => {
      const x = pad + i * step;
      const y = pad + (h - pad * 2) * (1 - (v - min) / range);
      return (i === 0 ? 'M' : 'L') + x.toFixed(1) + ' ' + y.toFixed(1);
    })
    .join(' ');
}

/** 面积 path（折线闭合到底部） */
export function buildArea(vals: number[], w: number, h: number, pad = 2): string {
  const line = buildLine(vals, w, h, pad);
  if (!line) return '';
  const baseY = h - pad;
  return `${line} L ${(w - pad).toFixed(1)} ${baseY} L ${pad.toFixed(1)} ${baseY} Z`;
}

/** 雷达图几何：多边形 path + 轴线 HTML */
export function buildRadar(vals: number[], cx: number, cy: number, r: number) {
  const n = vals.length;
  const ang = (i: number) => -Math.PI / 2 + (i * 2 * Math.PI) / n;
  const pt = (i: number, rr: number): [number, number] => [cx + rr * Math.cos(ang(i)), cy + rr * Math.sin(ang(i))];
  const poly = vals
    .map((v, i) => {
      const rr = r * Math.max(0, Math.min(1, v));
      const [x, y] = pt(i, rr);
      return (i === 0 ? 'M' : 'L') + x.toFixed(1) + ' ' + y.toFixed(1);
    })
    .join(' ') + ' Z';
  let axes = '';
  for (let i = 0; i < n; i++) {
    const [x, y] = pt(i, r);
    axes += `<line x1="${cx}" y1="${cy}" x2="${x.toFixed(1)}" y2="${y.toFixed(1)}" class="dvr-radar-axis" />`;
  }
  return { poly, axes };
}
