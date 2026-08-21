<script lang="ts">
  import { kbApi } from './services/ipc';
  import { onMount } from 'svelte';
  import KbIcon from './KbIcon.svelte';
  import type { AnalyticsSetting } from './kbTypes';
  import { downloadBlob } from '../download';
  import { formatDate } from '../format';

  // 指标定义（与首页指标卡一致）
  const METRICS: { key: string; label: string; unit: string }[] = [
    { key: 'messages', label: '消息量', unit: '' },
    { key: 'sessions', label: '会话量', unit: '' },
    { key: 'handoff', label: '转人工率', unit: '%' },
    { key: 'recommend', label: '问题推荐', unit: '' },
    { key: 'recall', label: '整体召回率', unit: '%' },
    { key: 'faq', label: '常用问答', unit: '' },
    { key: 'task', label: '任务技能', unit: '' },
    { key: 'llm', label: 'LLM问答', unit: '' },
  ];
  const DEFAULT_UNITS: Record<string, string> = {
    messages: '', sessions: '', handoff: '%', recommend: '',
    recall: '%', faq: '', task: '', llm: '',
  };

  let metric = $state('messages');
  let settings = $state<AnalyticsSetting[]>([]);
  // 时间范围：0 = 本周（最近 7 天），1 = 上周，2 = 上上周
  let rangeOffset = $state(0);
  let hoverIdx = $state<number | null>(null);

  // 可见指标（由指标配置决定；未配置时显示全部）
  const visibleMetrics = $derived.by(() => {
    const vis = settings.filter((s) => s.visible);
    if (vis.length === 0) return METRICS;
    return vis.map((s) => ({ key: s.key, label: s.label, unit: DEFAULT_UNITS[s.key] ?? '' }));
  });

  // 数据序列来自 kb_get_analytics（完整埋点）
  let seriesMap = $state<Record<string, number[]>>(Object.fromEntries(
    METRICS.map((m) => [m.key, Array(7).fill(0)]),
  ));
  async function loadAnalytics() {
    try {
    const res = await kbApi.getAnalytics();
      for (const item of res?.metrics ?? []) {
        const arr = (item.series ?? []).map((s) => Number(s.value) || 0);
        if (arr.length === 7) seriesMap[item.key] = arr;
      }
    } catch { /* 未配置统计时保持占位 */ }
  }
  async function loadSettings() {
    try {
    settings = await kbApi.getAnalyticsSettings();
      if (settings.length > 0 && !settings.some((s) => s.key === metric && s.visible)) {
        metric = settings.find((s) => s.visible)?.key ?? metric;
      }
    } catch {
      settings = [];
    }
  }

  const fmtShort = (d: Date): string => `${d.getMonth() + 1}/${d.getDate()}`;

  // 时间段：以今天为基准，向前取 7 天（含今天）
  const range = $derived.by(() => {
    const end = new Date();
    end.setDate(end.getDate() - rangeOffset * 7);
    const start = new Date(end);
    start.setDate(start.getDate() - 6);
    return { start, end };
  });

  const points = $derived.by(() => {
    const { start } = range;
    const values = seriesMap[metric] ?? Array(7).fill(0);
    return values.map((v, i) => {
      const d = new Date(start);
      d.setDate(d.getDate() + i);
      return { date: formatDate(d, { dateOnly: true }), short: fmtShort(d), value: v };
    });
  });

  const curLabel = $derived(visibleMetrics.find((m) => m.key === metric)?.label ?? '');
  const maxY = $derived(Math.max(1, ...points.map((p) => p.value)));

  // 图表几何：全部用百分比坐标（0-100），SVG 与 HTML 坐标轴标签共用，
  // 文字不随 SVG 拉伸缩放，始终以固定 CSS 字号渲染
  const pctL = 6, pctR = 3, pctT = 4, pctB = 9;
  const plotW = 100 - pctL - pctR;
  const plotH = 100 - pctT - pctB;
  const xAt = (i: number) => pctL + (points.length <= 1 ? 0 : (i * plotW) / (points.length - 1));
  const yAt = (v: number) => pctT + plotH * (1 - v / maxY);
  // 整数友好的 Y 轴刻度：消息量等整数指标不再出现 0.25/0.75 之类小数刻度
  function niceStep(raw: number): number {
    const pow = Math.pow(10, Math.floor(Math.log10(Math.max(raw, 1e-9))));
    for (const m of [1, 2, 5, 10]) {
      if (raw <= m * pow) return m * pow;
    }
    return 10 * pow;
  }
  const gridLevels = $derived.by(() => {
    const step = niceStep(maxY / 4);
    const levels: number[] = [];
    for (let v = 0; v <= maxY + step * 0.001; v += step) levels.push(v);
    if (levels.length < 2) levels.push(maxY);
    return levels.map((v) => ({ ratio: maxY > 0 ? v / maxY : 0, v }));
  });
  const linePath = $derived(points.map((p, i) => `${i === 0 ? 'M' : 'L'}${xAt(i).toFixed(2)},${yAt(p.value).toFixed(2)}`).join(' '));
  const areaPath = $derived(`${linePath}L${xAt(points.length - 1).toFixed(2)},${(pctT + plotH).toFixed(2)}L${pctL},${(pctT + plotH).toFixed(2)}Z`);
  const hover = $derived(hoverIdx === null ? points[points.length - 1] : (points[hoverIdx] ?? points[points.length - 1]));
  const hoverX = $derived(hoverIdx === null ? xAt(points.length - 1) : xAt(hoverIdx));
  const hoverY = $derived(yAt(hover?.value ?? 0));
  const curUnit = $derived(visibleMetrics.find((m) => m.key === metric)?.unit ?? '');

  // 悬停定位：屏幕像素 → 最近数据点
  function onChartMove(ev: PointerEvent) {
    const svg = (ev.currentTarget as SVGSVGElement);
    const rect = svg.getBoundingClientRect();
    if (rect.width <= 0) return;
    const px = ((ev.clientX - rect.left) / rect.width) * 100;
    let best = 0, bestDist = Infinity;
    for (let i = 0; i < points.length; i++) {
      const d = Math.abs(xAt(i) - px);
      if (d < bestDist) { bestDist = d; best = i; }
    }
    hoverIdx = best;
  }

  // 导出 CSV（UTF-8 BOM，Excel 可直接打开）
  function exportData() {
    const unit = METRICS.find((m) => m.key === metric)?.unit ?? '';
    const rows = [
      ['日期', `${curLabel}${unit}`],
      ...points.map((p) => [p.date, p.value]),
    ];
    const csv = rows.map((r) => r.join(',')).join('\r\n');
    downloadBlob(
      new Blob(['\ufeff' + csv], { type: 'text/csv;charset=utf-8' }),
      `${curLabel}_趋势_${range.start.getFullYear()}${String(range.start.getMonth() + 1).padStart(2, '0')}${String(range.start.getDate()).padStart(2, '0')}-${formatDate(range.end, { dateOnly: true })}.csv`,
    );
  }

  onMount(() => { loadAnalytics(); loadSettings(); });
</script>

<div class="kb-card kb-trend">
  <div class="kb-card-hd" style="justify-content:space-between;flex-wrap:wrap;gap:8px">
    <span><KbIcon name="chart" size={15} color="var(--kb-accent-bright)" />数据指标趋势图</span>
    <div style="display:flex;gap:6px;align-items:center;flex-wrap:wrap">
      <button class="kb-btn-sm" onclick={exportData} title="导出当前指标与时间段的 CSV 数据">
        <KbIcon name="download" size={13} />导出数据
      </button>
    </div>
  </div>
  <div class="kb-card-bd" style="display:flex;flex-direction:column;gap:12px">
    <!-- 指标选项卡 -->
    <div class="kb-trend-tabs">
      {#each visibleMetrics as m}
        <button class="kb-trend-tab" class:active={metric === m.key} onclick={() => metric = m.key}>{m.label}</button>
      {/each}
    </div>
    <!-- 时间范围 -->
    <div style="display:flex;align-items:center;gap:8px;flex-wrap:wrap">
      <div class="kb-seg">
        <button class="kb-seg-item" class:active={rangeOffset === 0} onclick={() => rangeOffset = 0}>本周</button>
        <button class="kb-seg-item" class:active={rangeOffset === 1} onclick={() => rangeOffset = 1}>上周</button>
        <button class="kb-seg-item" class:active={rangeOffset === 2} onclick={() => rangeOffset = 2}>上上周</button>
      </div>
      <span class="kb-trend-range">{formatDate(range.start, { dateOnly: true })} 至 {formatDate(range.end, { dateOnly: true })}</span>
      <div style="flex:1"></div>
      <span class="kb-trend-hint">移动鼠标查看每日数据</span>
    </div>
    <!-- 折线图 -->
    <div style="position:relative;height:260px">
      <svg viewBox="0 0 100 100" style="position:absolute;inset:0;width:100%;height:100%;display:block" preserveAspectRatio="none"
        role="img" aria-label="趋势图"
        onpointermove={onChartMove} onpointerleave={() => hoverIdx = null}>
        <!-- 网格 -->
        {#each gridLevels as g}
          <line x1={pctL} x2={100 - pctR} y1={yAt(g.v)} y2={yAt(g.v)} stroke="var(--kb-border-subtle)" stroke-width="1" vector-effect="non-scaling-stroke" />
        {/each}
        <!-- 面积 + 折线 -->
        <path d={areaPath} fill="color-mix(in srgb, var(--app-accent) 12%, transparent)" stroke="none" vector-effect="non-scaling-stroke" />
        <path d={linePath} fill="none" stroke="var(--kb-accent-bright)" stroke-width="2" vector-effect="non-scaling-stroke" stroke-linejoin="round" stroke-linecap="round" />
        <!-- 绿色参考线 + 数据点 -->
        <line x1={hoverX} x2={hoverX} y1={pctT} y2={pctT + plotH} stroke="var(--kb-ok)" stroke-width="1.5" vector-effect="non-scaling-stroke" stroke-dasharray="4 3" />
      </svg>
      <!-- 数据点（HTML，固定像素大小，不随 SVG 拉伸） -->
      <span class="kb-trend-dot" style="left:{hoverX}%;top:{hoverY}%"></span>
      <!-- Y 轴刻度（HTML，固定字号不随图表拉伸） -->
      {#each gridLevels as g}
        <span class="kb-trend-axis-y" style="top:{yAt(g.v)}%">{Number.isInteger(g.v) ? g.v.toLocaleString() : (+g.v.toFixed(2)).toLocaleString()}</span>
      {/each}
      <!-- X 轴日期（HTML） -->
      {#each points as p, i}
        <span class="kb-trend-axis-x" style="left:{xAt(i)}%">{p.short}</span>
      {/each}
      <!-- 提示框 -->
      <div class="kb-trend-tip" style="left:{Math.min(96, Math.max(5, hoverX))}%;top:{Math.max(0, hoverY - 16)}%">
        <div class="kb-trend-tip-date">{hover.date}</div>
        <div class="kb-trend-tip-val">{curLabel}：{hover.value}{curUnit}</div>
      </div>
    </div>
  </div>
</div>

<style>
  .kb-trend-tabs {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }
  .kb-trend-tab {
    height: 28px;
    padding: 0 12px;
    border: 1px solid transparent;
    border-radius: 6px;
    background: transparent;
    color: var(--kb-text-2);
    font-size: 12.5px;
    font-family: inherit;
    cursor: pointer;
    transition: background .12s, color .12s, border-color .12s;
  }
  .kb-trend-tab:hover { background: var(--kb-hover); color: var(--kb-text); }
  .kb-trend-tab.active {
    background: var(--kb-hover-strong);
    border-color: var(--kb-border-strong);
    color: var(--kb-accent-bright);
    font-weight: 600;
  }
  .kb-trend-range {
    font-size: 12.5px;
    color: var(--kb-text-2);
    font-variant-numeric: tabular-nums;
  }
  .kb-trend-hint { font-size: 11.5px; color: var(--kb-text-3); }
  .kb-trend-axis-y {
    position: absolute;
    left: 0;
    width: 6%;
    transform: translateY(-50%);
    text-align: right;
    padding-right: 8px;
    color: var(--kb-text-3);
    font-size: 11.5px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    pointer-events: none;
  }
  .kb-trend-axis-x {
    position: absolute;
    bottom: 2px;
    transform: translateX(-50%);
    color: var(--kb-text-3);
    font-size: 11.5px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    pointer-events: none;
  }
  .kb-trend-dot {
    position: absolute;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    transform: translate(-50%, -50%);
    background: var(--kb-accent-bright);
    border: 2px solid var(--kb-ok);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--app-bg-color) 72%, transparent);
    pointer-events: none;
    z-index: 2;
  }
  .kb-trend-tip {
    position: absolute;
    transform: translateX(-50%);
    pointer-events: none;
    z-index: 3;
    background: var(--kb-surface-2);
    border: 1px solid var(--kb-border-strong);
    /* impeccable-disable-next-line side-tab -- 趋势提示条状态色刻线 */
    border-left: 3px solid var(--kb-ok);
    border-radius: 6px;
    box-shadow: var(--kb-shadow);
    padding: 5px 9px;
    white-space: nowrap;
  }
  .kb-trend-tip-date { font-size: 11.5px; color: var(--kb-text-3); }
  .kb-trend-tip-val { font-size: 12px; font-weight: 600; color: var(--kb-text); margin-top: 1px; }
</style>
