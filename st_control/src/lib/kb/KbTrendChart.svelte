<script lang="ts">
  import { kbApi } from './services/ipc';
  import { onMount } from 'svelte';
  import KbIcon from './KbIcon.svelte';
  import type { AnalyticsSetting } from './kbTypes';
  import { downloadBlob } from '../download';
  import { formatDate } from '../format';

  // 指标定义（配色经过精心搭配，确保在深浅背景下都清晰可辨）
  const METRICS: { key: string; label: string; unit: string; color: string; gradient: string }[] = [
    { key: 'messages', label: '消息量', unit: '', color: '#6366f1', gradient: 'rgba(99,102,241,0.15)' },
    { key: 'sessions', label: '会话量', unit: '', color: '#06b6d4', gradient: 'rgba(6,182,212,0.15)' },
    { key: 'recall', label: '召回率', unit: '%', color: '#f59e0b', gradient: 'rgba(245,158,11,0.15)' },
    { key: 'faq', label: '常用问答', unit: '', color: '#8b5cf6', gradient: 'rgba(139,92,246,0.15)' },
    { key: 'llm', label: 'LLM问答', unit: '', color: '#10b981', gradient: 'rgba(16,185,129,0.15)' },
    { key: 'recommend', label: '问题推荐', unit: '', color: '#f43f5e', gradient: 'rgba(244,63,94,0.15)' },
  ];

  let settings = $state<AnalyticsSetting[]>([]);
  let rangeOffset = $state(0);
  let hoverIdx = $state<number | null>(null);
  let chartReady = $state(false);

  const visibleMetrics = $derived.by(() => {
    const vis = settings.filter((s) => s.visible);
    if (vis.length === 0) return METRICS;
    return vis.map((s) => {
      const def = METRICS.find((m) => m.key === s.key);
      return { key: s.key, label: s.label, unit: def?.unit ?? '', color: def?.color ?? '#999', gradient: def?.gradient ?? 'rgba(153,153,153,0.15)' };
    });
  });

  let seriesMap = $state<Record<string, number[]>>(Object.fromEntries(
    METRICS.map((m) => [m.key, Array(7).fill(0)]),
  ));

  async function loadAnalytics() {
    try {
      const res = await kbApi.getAnalytics();
      for (const item of res?.metrics ?? []) {
        const arr = (item.series ?? []).map((s: { value: number }) => Number(s.value) || 0);
        if (arr.length === 7) seriesMap[item.key] = arr;
      }
      setTimeout(() => chartReady = true, 100);
    } catch { chartReady = true; }
  }

  async function loadSettings() {
    try { settings = await kbApi.getAnalyticsSettings(); } catch { settings = []; }
  }

  const fmtShort = (d: Date): string => `${d.getMonth() + 1}/${d.getDate()}`;
  const fmtWeekday = (d: Date): string => ['日', '一', '二', '三', '四', '五', '六'][d.getDay()];

  const range = $derived.by(() => {
    const end = new Date();
    end.setDate(end.getDate() - rangeOffset * 7);
    const start = new Date(end);
    start.setDate(start.getDate() - 6);
    return { start, end };
  });

  const allPoints = $derived.by(() => {
    const { start } = range;
    return visibleMetrics.map((m) => {
      const values = seriesMap[m.key] ?? Array(7).fill(0);
      return {
        ...m,
        points: values.map((v, i) => {
          const d = new Date(start);
          d.setDate(d.getDate() + i);
          return { date: formatDate(d, { dateOnly: true }), short: fmtShort(d), weekday: fmtWeekday(d), value: v };
        }),
      };
    });
  });

  const xLabels = $derived(allPoints[0]?.points.map((p) => p) ?? []);
  const maxY = $derived(Math.max(1, ...allPoints.flatMap((m) => m.points.map((p) => p.value))));

  // 图表几何（留出更多边距给装饰元素）
  const ML = 8, MR = 4, MT = 4, MB = 10;
  const PW = 100 - ML - MR;
  const PH = 100 - MT - MB;
  const xAt = (i: number) => ML + (xLabels.length <= 1 ? 0 : (i * PW) / (xLabels.length - 1));
  const yAt = (v: number) => MT + PH * (1 - v / maxY);

  function niceStep(raw: number): number {
    const pow = Math.pow(10, Math.floor(Math.log10(Math.max(raw, 1e-9))));
    for (const m of [1, 2, 5, 10]) { if (raw <= m * pow) return m * pow; }
    return 10 * pow;
  }

  const gridLevels = $derived.by(() => {
    const step = niceStep(maxY / 4);
    const levels: number[] = [];
    for (let v = 0; v <= maxY + step * 0.001; v += step) levels.push(v);
    if (levels.length < 2) levels.push(maxY);
    return levels.map((v) => ({ ratio: maxY > 0 ? v / maxY : 0, v }));
  });

  // Catmull-Rom 平滑曲线
  function smoothPath(pts: { x: number; y: number }[]): string {
    if (pts.length < 2) return '';
    if (pts.length === 2) return `M${pts[0].x},${pts[0].y}L${pts[1].x},${pts[1].y}`;
    let d = `M${pts[0].x.toFixed(2)},${pts[0].y.toFixed(2)}`;
    for (let i = 0; i < pts.length - 1; i++) {
      const p0 = pts[Math.max(0, i - 1)];
      const p1 = pts[i];
      const p2 = pts[i + 1];
      const p3 = pts[Math.min(pts.length - 1, i + 2)];
      const t = 0.3;
      d += `C${(p1.x + (p2.x - p0.x) * t).toFixed(2)},${(p1.y + (p2.y - p0.y) * t).toFixed(2)},${(p2.x - (p3.x - p1.x) * t).toFixed(2)},${(p2.y - (p3.y - p1.y) * t).toFixed(2)},${p2.x.toFixed(2)},${p2.y.toFixed(2)}`;
    }
    return d;
  }

  // 曲线 + 面积路径
  const chartPaths = $derived(allPoints.map((m) => {
    const pts = m.points.map((p, i) => ({ x: xAt(i), y: yAt(p.value) }));
    const line = smoothPath(pts);
    const area = `${line}L${xAt(pts.length - 1).toFixed(2)},${(MT + PH).toFixed(2)}L${ML},${(MT + PH).toFixed(2)}Z`;
    return { key: m.key, color: m.color, gradient: m.gradient, line, area };
  }));

  const hoverX = $derived(hoverIdx !== null ? xAt(hoverIdx) : xAt(xLabels.length - 1));

  function onChartMove(ev: PointerEvent) {
    const svg = (ev.currentTarget as SVGSVGElement);
    const rect = svg.getBoundingClientRect();
    if (rect.width <= 0) return;
    const px = ((ev.clientX - rect.left) / rect.width) * 100;
    let best = 0, bestDist = Infinity;
    for (let i = 0; i < xLabels.length; i++) {
      const d = Math.abs(xAt(i) - px);
      if (d < bestDist) { bestDist = d; best = i; }
    }
    hoverIdx = best;
  }

  function exportData() {
    const headers = ['日期', ...visibleMetrics.map((m) => `${m.label}${m.unit}`)];
    const rows = xLabels.map((x, i) => [x.date, ...visibleMetrics.map((m) => seriesMap[m.key]?.[i] ?? 0)]);
    const csv = [headers, ...rows].map((r) => r.join(',')).join('\r\n');
    downloadBlob(
      new Blob(['\ufeff' + csv], { type: 'text/csv;charset=utf-8' }),
      `趋势_${formatDate(range.start, { dateOnly: true })}-${formatDate(range.end, { dateOnly: true })}.csv`,
    );
  }

  // 汇总统计
  const summary = $derived(visibleMetrics.map((m) => {
    const vals = seriesMap[m.key] ?? [];
    const total = vals.reduce((a, b) => a + b, 0);
    const max = Math.max(0, ...vals);
    const trend = vals.length >= 2 && vals[vals.length - 2] > 0
      ? ((vals[vals.length - 1] - vals[vals.length - 2]) / vals[vals.length - 2] * 100).toFixed(1)
      : null;
    return { ...m, total, max, trend };
  }));

  onMount(() => { loadAnalytics(); loadSettings(); });
</script>

<div class="kb-trend-card">
  <!-- 头部：标题 + 时间选择 + 导出 -->
  <div class="kt-header">
    <div class="kt-header-left">
      <div class="kt-header-icon"><KbIcon name="chart" size={18} /></div>
      <div>
        <h3 class="kt-title">数据指标趋势</h3>
        <p class="kt-subtitle">{formatDate(range.start, { dateOnly: true })} — {formatDate(range.end, { dateOnly: true })}</p>
      </div>
    </div>
    <div class="kt-header-right">
      <div class="kt-range-picker">
        <button class="kt-range-btn" class:active={rangeOffset === 0} onclick={() => rangeOffset = 0}>本周</button>
        <button class="kt-range-btn" class:active={rangeOffset === 1} onclick={() => rangeOffset = 1}>上周</button>
        <button class="kt-range-btn" class:active={rangeOffset === 2} onclick={() => rangeOffset = 2}>上上周</button>
      </div>
      <button class="kt-export-btn" onclick={exportData} title="导出 CSV">
        <KbIcon name="download" size={14} />
      </button>
    </div>
  </div>

  <!-- 指标概览卡片行 -->
  <div class="kt-summary-row">
    {#each summary as m}
      <div class="kt-summary-card" style="--accent:{m.color}">
        <div class="kt-summary-dot" style="background:{m.color}"></div>
        <div class="kt-summary-info">
          <span class="kt-summary-label">{m.label}</span>
          <span class="kt-summary-value">{m.max}{m.unit}</span>
        </div>
        {#if m.trend !== null}
          <span class="kt-summary-trend" class:up={Number(m.trend) > 0} class:down={Number(m.trend) < 0}>
            {Number(m.trend) > 0 ? '↑' : Number(m.trend) < 0 ? '↓' : '—'}{Math.abs(Number(m.trend))}%
          </span>
        {/if}
      </div>
    {/each}
  </div>

  <!-- 图表区域 -->
  <div class="kt-chart-wrap">
    <!-- 图例 -->
    <div class="kt-legend">
      {#each visibleMetrics as m}
        <span class="kt-legend-item">
          <span class="kt-legend-line" style="background:{m.color}"></span>
          {m.label}
        </span>
      {/each}
    </div>

    <!-- 图表 -->
    <div class="kt-chart" class:ready={chartReady}>
      <svg viewBox="0 0 100 100" class="kt-svg"
        preserveAspectRatio="none"
        role="img" aria-label="趋势图"
        onpointermove={onChartMove} onpointerleave={() => hoverIdx = null}>

        <defs>
          {#each chartPaths as cp}
            <linearGradient id="grad-{cp.key}" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stop-color={cp.color} stop-opacity="0.25" />
              <stop offset="100%" stop-color={cp.color} stop-opacity="0.02" />
            </linearGradient>
          {/each}
        </defs>

        <!-- 网格线 -->
        {#each gridLevels as g}
          <line x1={ML} x2={100 - MR} y1={yAt(g.v)} y2={yAt(g.v)}
            stroke="var(--kb-border-subtle)" stroke-width="0.3" vector-effect="non-scaling-stroke" />
        {/each}

        <!-- 面积填充 -->
        {#each chartPaths as cp}
          <path d={cp.area} fill="url(#grad-{cp.key})" stroke="none" opacity="0.6" />
        {/each}

        <!-- 曲线 -->
        {#each chartPaths as cp}
          <path d={cp.line} fill="none" stroke={cp.color} stroke-width="1.8"
            vector-effect="non-scaling-stroke" stroke-linejoin="round" stroke-linecap="round"
            class:kt-line-animate={chartReady} />
        {/each}

        <!-- 悬停十字线 -->
        {#if hoverIdx !== null}
          <line x1={hoverX} x2={hoverX} y1={MT} y2={MT + PH}
            stroke="var(--kb-text-3)" stroke-width="0.5" vector-effect="non-scaling-stroke"
            stroke-dasharray="2 2" opacity="0.6" />
        {/if}
      </svg>

      <!-- 数据点（HTML 固定像素） -->
      {#if hoverIdx !== null}
        {#each allPoints as m}
          {@const val = m.points[hoverIdx]?.value ?? 0}
          <span class="kt-dot" style="left:{hoverX}%;top:{yAt(val)}%;--dot-color:{m.color}"></span>
        {/each}
      {/if}

      <!-- Y 轴刻度 -->
      {#each gridLevels as g}
        <span class="kt-axis-y" style="top:{yAt(g.v)}%">
          {Number.isInteger(g.v) ? g.v.toLocaleString() : g.v.toFixed(1)}
        </span>
      {/each}

      <!-- X 轴日期 -->
      {#each xLabels as x, i}
        <span class="kt-axis-x" style="left:{xAt(i)}%">
          <span class="kt-axis-date">{x.short}</span>
          <span class="kt-axis-weekday">周{x.weekday}</span>
        </span>
      {/each}

      <!-- 悬停提示框 -->
      {#if hoverIdx !== null}
        {@const pt = xLabels[hoverIdx]}
        <div class="kt-tooltip" style="left:{Math.min(92, Math.max(8, hoverX))}%;top:6%">
          <div class="kt-tooltip-header">
            <span class="kt-tooltip-date">{pt?.date}</span>
            <span class="kt-tooltip-weekday">周{pt?.weekday}</span>
          </div>
          <div class="kt-tooltip-body">
            {#each allPoints as m}
              <div class="kt-tooltip-row">
                <span class="kt-tooltip-dot" style="background:{m.color}"></span>
                <span class="kt-tooltip-label">{m.label}</span>
                <span class="kt-tooltip-val">{m.points[hoverIdx]?.value ?? 0}{m.unit}</span>
              </div>
            {/each}
          </div>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .kb-trend-card {
    background: var(--app-bg-color);
    border: 1px solid var(--kb-border);
    border-radius: var(--kb-radius, 10px);
    box-shadow: var(--kb-shadow-sm);
    overflow: hidden;
  }

  /* ── 头部 ── */
  .kt-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--kb-border-subtle);
    flex-wrap: wrap;
    gap: 12px;
  }
  .kt-header-left {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .kt-header-icon {
    width: 36px;
    height: 36px;
    border-radius: 10px;
    background: color-mix(in srgb, var(--kb-accent) 16%, var(--kb-surface));
    color: var(--kb-accent-bright);
    display: grid;
    place-items: center;
  }
  .kt-title {
    font-size: 15px;
    font-weight: 700;
    margin: 0;
    color: var(--kb-text);
  }
  .kt-subtitle {
    font-size: 12px;
    color: var(--kb-text-3);
    margin: 2px 0 0;
  }
  .kt-header-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .kt-range-picker {
    display: flex;
    background: var(--kb-surface);
    border-radius: 8px;
    padding: 3px;
    gap: 2px;
  }
  .kt-range-btn {
    padding: 5px 14px;
    border: none;
    border-radius: 6px;
    background: transparent;
    font-size: 12.5px;
    color: var(--kb-text-3);
    cursor: pointer;
    transition: all 0.15s;
  }
  .kt-range-btn:hover { color: var(--kb-text); }
  .kt-range-btn.active {
    background: var(--app-bg-color);
    color: var(--kb-text);
    font-weight: 600;
    box-shadow: 0 1px 3px rgba(0,0,0,0.08);
  }
  .kt-export-btn {
    width: 32px;
    height: 32px;
    border: 1px solid var(--kb-border);
    border-radius: 8px;
    background: var(--app-bg-color);
    color: var(--kb-text-3);
    display: grid;
    place-items: center;
    cursor: pointer;
    transition: all 0.15s;
  }
  .kt-export-btn:hover { background: var(--kb-surface); color: var(--kb-text); }

  /* ── 指标概览卡片行 ── */
  .kt-summary-row {
    display: grid;
    grid-template-columns: repeat(6, 1fr);
    gap: 1px;
    background: var(--kb-border);
    border-bottom: 1px solid var(--kb-border-subtle);
  }
  .kt-summary-card {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 14px;
    background: var(--app-bg-color);
    transition: background 0.12s;
  }
  .kt-summary-card:hover { background: var(--kb-surface); }
  .kt-summary-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
    box-shadow: 0 0 6px color-mix(in srgb, currentColor 30%, transparent);
  }
  .kt-summary-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .kt-summary-label {
    font-size: 11px;
    color: var(--kb-text-3);
    white-space: nowrap;
  }
  .kt-summary-value {
    font-size: 16px;
    font-weight: 700;
    color: var(--kb-text);
    font-variant-numeric: tabular-nums;
  }
  .kt-summary-trend {
    font-size: 11px;
    font-weight: 600;
    padding: 2px 6px;
    border-radius: 4px;
    flex-shrink: 0;
  }
  .kt-summary-trend.up {
    background: color-mix(in srgb, #16a34a 12%, transparent);
    color: #16a34a;
  }
  .kt-summary-trend.down {
    background: color-mix(in srgb, #dc2626 12%, transparent);
    color: #dc2626;
  }

  /* ── 图表区域 ── */
  .kt-chart-wrap {
    padding: 16px 20px 20px;
  }
  .kt-legend {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
    margin-bottom: 16px;
  }
  .kt-legend-item {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--kb-text-2);
  }
  .kt-legend-line {
    width: 16px;
    height: 3px;
    border-radius: 2px;
    flex-shrink: 0;
  }

  /* 图表容器 */
  .kt-chart {
    position: relative;
    height: 300px;
    border-radius: 8px;
    background: var(--kb-surface);
    padding: 8px 0 0 0;
  }
  .kt-svg {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    display: block;
  }

  /* 曲线入场动画 */
  .kt-line-animate {
    stroke-dasharray: 1000;
    stroke-dashoffset: 1000;
    animation: kt-draw 1.2s ease-out forwards;
  }
  @keyframes kt-draw {
    to { stroke-dashoffset: 0; }
  }

  /* 数据点 */
  .kt-dot {
    position: absolute;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    transform: translate(-50%, -50%);
    background: var(--dot-color);
    border: 2px solid var(--app-bg-color);
    box-shadow: 0 0 0 2px var(--dot-color), 0 2px 8px color-mix(in srgb, var(--dot-color) 30%, transparent);
    pointer-events: none;
    z-index: 2;
    animation: kt-dot-in 0.15s ease-out;
  }
  @keyframes kt-dot-in {
    from { transform: translate(-50%, -50%) scale(0); }
    to { transform: translate(-50%, -50%) scale(1); }
  }

  /* 坐标轴 */
  .kt-axis-y {
    position: absolute;
    left: 0;
    width: 8%;
    transform: translateY(-50%);
    text-align: right;
    padding-right: 10px;
    color: var(--kb-text-3);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    pointer-events: none;
  }
  .kt-axis-x {
    position: absolute;
    bottom: -4px;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    align-items: center;
    pointer-events: none;
  }
  .kt-axis-date {
    font-size: 12px;
    font-weight: 500;
    color: var(--kb-text);
    font-variant-numeric: tabular-nums;
  }
  .kt-axis-weekday {
    font-size: 10px;
    color: var(--kb-text-3);
    margin-top: 1px;
  }

  /* 提示框 */
  .kt-tooltip {
    position: absolute;
    transform: translateX(-50%);
    pointer-events: none;
    z-index: 3;
    background: var(--app-bg-color);
    border: 1px solid var(--kb-border);
    border-radius: 10px;
    box-shadow: 0 8px 24px rgba(0,0,0,0.12);
    padding: 0;
    white-space: nowrap;
    overflow: hidden;
    min-width: 160px;
    animation: kt-tip-in 0.15s ease-out;
  }
  @keyframes kt-tip-in {
    from { opacity: 0; transform: translateX(-50%) translateY(4px); }
    to { opacity: 1; transform: translateX(-50%) translateY(0); }
  }
  .kt-tooltip-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    background: var(--kb-surface);
    border-bottom: 1px solid var(--kb-border-subtle);
  }
  .kt-tooltip-date {
    font-size: 12px;
    font-weight: 600;
    color: var(--kb-text);
  }
  .kt-tooltip-weekday {
    font-size: 11px;
    color: var(--kb-text-3);
  }
  .kt-tooltip-body {
    padding: 8px 12px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .kt-tooltip-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }
  .kt-tooltip-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .kt-tooltip-label { color: var(--kb-text-2); flex: 1; }
  .kt-tooltip-val { font-weight: 600; color: var(--kb-text); font-variant-numeric: tabular-nums; }

  @media (max-width: 768px) {
    .kt-summary-row { grid-template-columns: repeat(3, 1fr); }
    .kt-chart { height: 220px; }
  }
</style>
