<script lang="ts">
  // 通用可视化图表组件（纯 SVG，无第三方依赖）
  // 支持类型：bar（柱状）/ line（折线）/ pie（饼图）
  // 兼容两种数据描述：
  //   1) { type, title?, labels: string[], series: [{ name, data: number[] }] }
  //   2) { type:'pie', title?, data: [{ label, value }] }
  //   3) 退化：{ title?, data: [{ label, value }] } 默认按饼图渲染

  import { arcPath, chartColor, pieSliceAngles } from "../../components/chartGeometry";
  import type { ChartSpec } from "../types";
  import { normalizeChart, type NormalizedChart } from "../chartSpec";

  let { spec }: { spec: ChartSpec } = $props();

  const W = 520;
  const H = 280;
  const PAD = { top: 28, right: 16, bottom: 40, left: 44 };

  // 绘图区尺寸（常量）
  const plotW = W - PAD.left - PAD.right;
  const plotH = H - PAD.top - PAD.bottom;

  const n = $derived<NormalizedChart>(normalizeChart(spec));

  // 调色板取色（保持组件内 color 名称，模板/派生零改动）
  function color(i: number) {
    return chartColor(i);
  }
  // ── 数值范围 ──
  const maxVal = $derived(
    Math.max(1, ...n.series.flatMap((s) => s.data), ...n.pie.map((p) => p.value)),
  );

  // 轴步进（类别间距）
  const step = $derived(n.labels.length > 1 ? plotW / (n.labels.length - 1) : plotW);

  // ── 饼图角度 ──
  const pieSlices = $derived(n.kind === "pie" ? pieSliceAngles(n.pie, color) : []);
</script>

<div class="llm-chart">
  {#if n.title}<div class="llm-chart-title">{n.title}</div>{/if}

  {#if n.kind === "pie"}
    <div class="llm-chart-pie">
      <svg viewBox="0 0 200 200" width="200" height="200" role="img" aria-label={n.title ?? "饼图"}>
        {#if pieSlices.length === 1}
          <circle cx="100" cy="100" r="80" fill={pieSlices[0].color} />
        {:else}
          {#each pieSlices as s (s.label + s.start)}
            <path d={arcPath(100, 100, 80, s.start, s.end)} fill={s.color} stroke="#fff" stroke-width="1" />
          {/each}
        {/if}
      </svg>
      <ul class="llm-chart-legend">
        {#each pieSlices as s (s.label)}
          <li><span class="dot" style:background={s.color}></span>{s.label} · {s.value}</li>
        {/each}
      </ul>
    </div>
  {:else}
    <svg viewBox="0 0 {W} {H}" width="100%" role="img" aria-label={n.title ?? "图表"}>
      {#each Array(5) as _, gi (gi)}
        {@const gy = PAD.top + ((H - PAD.top - PAD.bottom) * gi) / 4}
        <line x1={PAD.left} y1={gy} x2={W - PAD.right} y2={gy} class="grid" />
        <text x={PAD.left - 6} y={gy + 3} class="axis" text-anchor="end">
          {Math.round((maxVal * (4 - gi)) / 4)}
        </text>
      {/each}

      {#each n.labels as lb, li (li)}
        <text x={PAD.left + step * li} y={H - PAD.bottom + 16} class="axis" text-anchor="middle">
          {lb}
        </text>
      {/each}

      {#each n.series as s, si (si)}
        {@const col = color(si)}
        {#if n.kind === "bar"}
          {@const bw = Math.min(40, (step * 0.6) / n.series.length)}
          {#each s.data as v, di (di)}
            {@const x = PAD.left + step * di - (n.series.length * bw) / 2 + si * bw}
            {@const h = (v / maxVal) * plotH}
            <rect x={x} y={PAD.top + plotH - h} width={bw - 2} height={h} fill={col} rx="2" />
          {/each}
        {:else}
          {@const pts = s.data.map((v, di) => `${PAD.left + step * di},${PAD.top + plotH - (v / maxVal) * plotH}`).join(" ")}
          <polyline points={pts} fill="none" stroke={col} stroke-width="2" />
          {#each s.data as v, di (di)}
            {@const cx = PAD.left + step * di}
            {@const cy = PAD.top + plotH - (v / maxVal) * plotH}
            <circle cx={cx} cy={cy} r="3" fill={col} />
          {/each}
        {/if}
      {/each}
    </svg>
    {#if n.series.length > 1}
      <div class="llm-chart-legend">
        {#each n.series as s, si (si)}
          <span><span class="dot" style:background={color(si)}></span>{s.name ?? `系列${si + 1}`}</span>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<style>
  .llm-chart { background: var(--app-color-surface-alt); border: 1px solid var(--app-color-border); border-radius: 8px; padding: 8px 10px; margin: 8px 0; }
  .llm-chart-title { font-size: 12px; font-weight: 600; color: var(--app-color-text); margin-bottom: 6px; }
  .llm-chart-pie { display: flex; gap: 14px; align-items: center; flex-wrap: wrap; }
  .llm-chart-legend { list-style: none; margin: 6px 0 0; padding: 0; display: flex; flex-wrap: wrap; gap: 10px; font-size: 11.5px; color: var(--app-color-muted); }
  .llm-chart-legend span { display: inline-flex; align-items: center; gap: 4px; }
  .dot { width: 9px; height: 9px; border-radius: 2px; display: inline-block; }
  :global(.llm-chart svg .grid) { stroke: var(--app-color-border); stroke-width: 1; }
  :global(.llm-chart svg .axis) { fill: var(--app-color-muted); font-size: 11.5px; }
</style>
