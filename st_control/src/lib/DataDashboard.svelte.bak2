<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { getRealtimeMetrics } from './system/services/ipc';
  import { formatBytes } from './format';
  import { colorFor, fmtLink, fmtPct, fmtRate, fmtUptime, pushHist } from './system/format';
  import { buildArea, buildLine, buildRadar } from './system/chartPaths';
  import { Badge } from './components/ui/badge';
  import { Card, CardContent, CardHeader, CardTitle } from './components/ui/card';
  import { Separator } from './components/ui/separator';
  import FancyCard from './components/fancy/FancyCard.svelte';

  // 面板是否可见（非当前 Tab 时暂停轮询与动画，降低后台开销）
  let { active = true }: { active?: boolean } = $props();

  // ─── 实时系统指标（全部来自后端 get_realtime_metrics，真实系统数据） ───
  type Snapshot = {
    now: string;
    now_str: string;
    uptime_secs: number;
    system_uptime_secs: number;
    os_name: string;
    cpu_usage: number;
    cpu_per_core: number[];
    mem_total_bytes: number;
    mem_used_bytes: number;
    mem_available_bytes: number;
    mem_usage_pct: number;
    swap_total_bytes: number;
    swap_used_bytes: number;
    disks: { name: string; mount: string; total_bytes: number; used_bytes: number; usage_pct: number }[];
    disk_activity_pct: number | null;
    disk_read_bytes_per_sec: number | null;
    disk_write_bytes_per_sec: number | null;
    gpu_name: string;
    gpu_usage_pct: number | null;
    net_throughput_bytes_per_sec: number | null;
    net_utilization_pct: number | null;
    net_link_speed_bps: number | null;
    net_latency_ms: number | null;
    net_ping_target: string;
  };

  let snap = $state<Snapshot | null>(null);
  let fps = $state(0);
  let lastUpdate = $state('');
  let refreshErr = $state<string | null>(null);
  let pollCount = $state(0);

const POLL_MS = 2500;
  let latencyHist = $state<number[]>([]);
  let fpsHist = $state<number[]>([]);
  let cpuHist = $state<number[]>([]);
  let memHist = $state<number[]>([]);
  let gpuHist = $state<number[]>([]);
  let diskActHist = $state<number[]>([]);
  let netThruHist = $state<number[]>([]);

  type LogLine = { t: string; level: 'info' | 'warn' | 'error'; msg: string };
  let logs = $state<LogLine[]>([]);

  let pollTimer: ReturnType<typeof setTimeout> | null = null;
  let rafId = 0;
  let frames = 0;
  let lastFpsT = performance.now();

  /** 字节格式化：保持原实现（含 PB 单位；0 → '0 B'） */
  function fmtBytes(n: number): string {
    return formatBytes(n, { units: ['B', 'KB', 'MB', 'GB', 'TB', 'PB'] });
  }

  function addLog(level: LogLine['level'], msg: string) {
    const t = snap?.now_str ?? new Date().toLocaleString('zh-CN');
    logs = [{ t, level, msg }, ...logs].slice(0, 40);
  }

  async function poll() {
    if (document.hidden || !active) return; // 页面/面板隐藏时暂停轮询，恢复后由 effect/visibilitychange 续跑
    try {
      const s = await getRealtimeMetrics<Snapshot>();
      snap = s;
      pollCount += 1;
      lastUpdate = new Date().toLocaleTimeString('zh-CN');
      refreshErr = null;

      if (s.net_latency_ms != null) latencyHist = pushHist(latencyHist, s.net_latency_ms);
      if (s.disk_activity_pct != null) diskActHist = pushHist(diskActHist, s.disk_activity_pct);
      if (s.net_throughput_bytes_per_sec != null) netThruHist = pushHist(netThruHist, s.net_throughput_bytes_per_sec);
      cpuHist = pushHist(cpuHist, s.cpu_usage);
      memHist = pushHist(memHist, s.mem_usage_pct);
      if (s.gpu_usage_pct != null) gpuHist = pushHist(gpuHist, s.gpu_usage_pct);

      const diskMax = s.disks.reduce((a, d) => Math.max(a, d.usage_pct), 0);
      const worst = Math.max(s.cpu_usage, s.mem_usage_pct, diskMax, s.disk_activity_pct ?? 0, s.gpu_usage_pct ?? 0);
      const level: LogLine['level'] = worst >= 90 ? 'error' : worst >= 75 ? 'warn' : 'info';
      const gpuTxt = s.gpu_usage_pct != null ? s.gpu_usage_pct.toFixed(1) + '%' : 'N/A';
      const diskActTxt = s.disk_activity_pct != null ? s.disk_activity_pct.toFixed(1) + '%' : 'N/A';
      const thruTxt = s.net_throughput_bytes_per_sec != null ? fmtRate(s.net_throughput_bytes_per_sec) : 'N/A';
      const latTxt = s.net_latency_ms != null ? s.net_latency_ms.toFixed(1) + 'ms' : 'N/A';
      addLog(level, `采样#${pollCount} CPU ${s.cpu_usage.toFixed(1)}% · 内存 ${s.mem_usage_pct.toFixed(1)}% · GPU ${gpuTxt} · 磁盘活动 ${diskActTxt} · 网络 ${thruTxt} · 延迟 ${latTxt} · FPS ${fps}`);
    } catch (e) {
      refreshErr = String(e);
    }
  }

  function rafLoop(t: number) {
    frames++;
    if (t - lastFpsT >= 1000) {
      fps = Math.round((frames * 1000) / (t - lastFpsT));
      frames = 0;
      lastFpsT = t;
      fpsHist = pushHist(fpsHist, fps);
    }
    rafId = document.hidden || !active ? 0 : requestAnimationFrame(rafLoop);
  }

  function onVisibility() {
    if (document.hidden || !active) {
      if (rafId) {
        cancelAnimationFrame(rafId);
        rafId = 0;
      }
    } else {
      poll();
      if (!rafId) rafId = requestAnimationFrame(rafLoop);
    }
  }

  // 切回本面板时立即恢复轮询
  $effect(() => {
    if (active && !document.hidden) {
      poll();
      if (!rafId) rafId = requestAnimationFrame(rafLoop);
    }
  });

  // ─── SVG 折线/面积生成 ───
  let radarVals = $derived(
    !snap
      ? [0, 0, 0, 0, 0, 0]
      : [
          (snap.cpu_usage ?? 0) / 100,
          (snap.mem_usage_pct ?? 0) / 100,
          snap.disks.reduce((a, d) => Math.max(a, d.usage_pct), 0) / 100,
          (snap.gpu_usage_pct ?? 0) / 100,
          Math.max(0, 1 - Math.min(snap.net_latency_ms ?? 0, 200) / 200),
          Math.min(1, fps / 120),
        ]
  );
  const radarLabels = ['CPU', '内存', '磁盘', 'GPU', '网络', '帧率'];
  let radarGeo = $derived(buildRadar(radarVals, 70, 70, 52));

  let diskMaxPct = $derived(snap ? snap.disks.reduce((a, d) => Math.max(a, d.usage_pct), 0) : 0);
  let memTotal = $derived(snap?.mem_total_bytes ?? 0);
  let memUsed = $derived(snap?.mem_used_bytes ?? 0);
  let memFree = $derived(snap?.mem_available_bytes ?? Math.max(0, memTotal - memUsed));
  let memCirc = $derived(memTotal > 0 ? (memUsed / memTotal) * 251.2 : 0);
  let gpuPct = $derived(snap?.gpu_usage_pct ?? 0);
  let diskRead = $derived(snap?.disk_read_bytes_per_sec ?? 0);
  let diskWrite = $derived(snap?.disk_write_bytes_per_sec ?? 0);
  let diskThru = $derived(diskRead + diskWrite);
  let diskAct = $derived(snap?.disk_activity_pct ?? 0);
  let netThru = $derived(snap?.net_throughput_bytes_per_sec ?? 0);
  let netUtil = $derived(snap?.net_utilization_pct ?? 0);

  onMount(() => {
    poll();
    pollTimer = setInterval(poll, POLL_MS);
    rafId = requestAnimationFrame(rafLoop);
    document.addEventListener('visibilitychange', onVisibility);
  });
  onDestroy(() => {
    if (pollTimer) clearInterval(pollTimer);
    if (rafId) cancelAnimationFrame(rafId);
    document.removeEventListener('visibilitychange', onVisibility);
  });
</script>

<div class="dvr-root">
  <header class="flex flex-wrap items-center justify-between gap-3">
    <div>
  <div class="text-base font-bold">ST 控制台 · 实时系统监控</div>
      <div class="mt-0.5 text-xs text-muted-foreground">{snap?.os_name ?? '正在连接系统…'}</div>
    </div>
    <div class="flex flex-wrap items-center gap-3">
      <Badge variant="default"><span class="mr-1 inline-block size-1.5 rounded-full bg-background align-middle"></span>LIVE · 2.5s 刷新</Badge>
      <div class="text-right">
        <div class="font-mono text-sm font-bold tabular-nums">{snap?.now_str ?? '--'}</div>
        <div class="text-[11px] text-muted-foreground">当前系统时间</div>
      </div>
      <div class="text-right">
        <div class="text-sm font-bold tabular-nums text-amber-400">{snap ? fmtUptime(snap.system_uptime_secs) : '--'}</div>
        <div class="text-[11px] text-muted-foreground">系统已运行</div>
      </div>
    </div>
  </header>

  <!-- KPI：大屏 4 列两行（4+4），避免 8 卡挤一行 -->
  <div class="grid grid-cols-2 gap-3 md:grid-cols-4 xl:grid-cols-4">
    {#each [
      { label: 'CPU 使用率', val: snap ? snap.cpu_usage.toFixed(1) + '%' : '--', pct: snap?.cpu_usage ?? 0 },
      { label: 'GPU 使用率', val: snap ? fmtPct(snap.gpu_usage_pct) : '--', pct: snap?.gpu_usage_pct ?? 0 },
      { label: '内存占用', val: snap ? snap.mem_usage_pct.toFixed(1) + '%' : '--', pct: snap?.mem_usage_pct ?? 0 },
      { label: '磁盘活动', val: snap ? fmtPct(snap.disk_activity_pct) : '--', pct: diskAct },
      { label: '磁盘读写', val: snap ? fmtRate(diskThru) : '--', pct: diskAct },
      { label: '网络吞吐', val: snap ? fmtRate(netThru) : '--', pct: netUtil },
      { label: '带宽占用', val: snap ? fmtPct(snap.net_utilization_pct) : '--', pct: netUtil },
      { label: '网络延迟', val: snap && snap.net_latency_ms != null ? Math.round(snap.net_latency_ms) + ' ms' : '--', pct: Math.min(100, (snap?.net_latency_ms ?? 0) * 0.5) },
    ] as k}
      <FancyCard slotClass="w-full">
        <div class="px-3 py-3">
          <div class="text-xs text-muted-foreground">{k.label}</div>
          <div class="mt-1 text-xl font-bold tabular-nums" style="color:{colorFor(k.pct)}">{k.val}</div>
          <div class="mt-2 h-1.5 overflow-hidden rounded-full bg-muted">
            <div class="h-full rounded-full" style="width:{Math.min(100, k.pct)}%;background:{colorFor(k.pct)}"></div>
          </div>
        </div>
      </FancyCard>
    {/each}
  </div>

  <div class="grid flex-1 auto-rows-fr grid-cols-1 gap-3 md:grid-cols-2">
    <Card class="flex min-h-0 flex-col">
      <CardHeader class="flex-row items-center justify-between space-y-0 pb-2">
        <CardTitle class="text-sm">网络状态</CardTitle>
        <span class="text-sm font-bold tabular-nums" style="color:{colorFor(netUtil)}">{fmtPct(snap?.net_utilization_pct)}</span>
      </CardHeader>
      <CardContent class="space-y-1">
        <div class="flex flex-wrap justify-between gap-1 text-[11px] text-muted-foreground">
          <span>延迟 <b class="tabular-nums text-foreground">{snap && snap.net_latency_ms != null ? snap.net_latency_ms.toFixed(1) + 'ms' : '--'}</b> · {snap?.net_ping_target || '--'}</span>
          <span>吞吐 <b class="tabular-nums text-foreground">{fmtRate(netThru)}</b></span>
          <span>链路 <b class="tabular-nums text-foreground">{snap?.net_link_speed_bps ? fmtLink(snap.net_link_speed_bps) : '--'}</b></span>
        </div>
        <div class="dvr-net-chart">
          <span class="dvr-net-chart-label">延迟</span>
          <svg viewBox="0 0 210 40" class="dvr-mini-chart">
            <path d={buildArea(latencyHist, 210, 40, 2)} fill="rgba(34,211,238,0.15)" />
            <path d={buildLine(latencyHist, 210, 40, 2)} fill="none" stroke="#22d3ee" stroke-width="1.4" />
          </svg>
        </div>
        <div class="dvr-net-chart">
          <span class="dvr-net-chart-label">吞吐</span>
          <svg viewBox="0 0 210 40" class="dvr-mini-chart">
            <path d={buildArea(netThruHist, 210, 40, 2)} fill="rgba(34,197,94,0.15)" />
            <path d={buildLine(netThruHist, 210, 40, 2)} fill="none" stroke="#22c55e" stroke-width="1.4" />
          </svg>
        </div>
      </CardContent>
    </Card>

    <Card class="flex min-h-0 flex-col">
      <CardHeader class="pb-2"><CardTitle class="text-sm">资源负载</CardTitle></CardHeader>
      <CardContent class="space-y-3">
        {#each [
          { n: 'CPU', v: snap?.cpu_usage ?? 0 },
          { n: '内存', v: snap?.mem_usage_pct ?? 0 },
          { n: '磁盘', v: diskMaxPct },
          { n: 'GPU', v: snap?.gpu_usage_pct ?? 0, na: snap?.gpu_usage_pct == null },
        ] as l}
          <div class="grid grid-cols-[40px_1fr_52px] items-center gap-2">
            <span class="text-xs text-muted-foreground">{l.n}</span>
            <div class="h-2 overflow-hidden rounded-full bg-muted">
              <div class="h-full rounded-full" style="width:{Math.min(100, l.v)}%;background:{colorFor(l.v)}"></div>
            </div>
            <span class="text-right text-xs tabular-nums" style="color:{colorFor(l.v)}">{l.na ? 'N/A' : l.v.toFixed(1) + '%'}</span>
          </div>
        {/each}
      </CardContent>
    </Card>

    <Card class="flex min-h-0 flex-col">
      <CardHeader class="flex-row items-center justify-between space-y-0 pb-2">
        <CardTitle class="text-sm">磁盘分区</CardTitle>
        <span class="text-xs text-muted-foreground">{snap?.disks.length ?? 0} 个</span>
      </CardHeader>
      <CardContent class="space-y-2">
        <div class="flex gap-3 text-[11px] text-muted-foreground">
          <span>活动 <b class="tabular-nums" style="color:{colorFor(diskAct)}">{fmtPct(snap?.disk_activity_pct)}</b></span>
          <span>读 <b class="tabular-nums text-foreground">{fmtRate(diskRead)}</b></span>
          <span>写 <b class="tabular-nums text-foreground">{fmtRate(diskWrite)}</b></span>
        </div>
        <svg viewBox="0 0 260 36" class="dvr-spark">
          <path d={buildArea(diskActHist, 260, 36, 2)} fill="rgba(251,191,36,0.15)" />
          <path d={buildLine(diskActHist, 260, 36, 2)} fill="none" stroke="#fbbf24" stroke-width="1.4" />
        </svg>
        <div class="dvr-disks">
          {#each (snap?.disks ?? []) as d}
            <div>
              <div class="flex items-center justify-between text-xs">
                <span class="font-medium">{d.name || d.mount}</span>
                <span class="tabular-nums" style="color:{colorFor(d.usage_pct)}">{d.usage_pct.toFixed(1)}%</span>
              </div>
              <div class="mt-1 h-1.5 overflow-hidden rounded-full bg-muted">
                <div class="h-full rounded-full" style="width:{Math.min(100, d.usage_pct)}%;background:{colorFor(d.usage_pct)}"></div>
              </div>
              <div class="text-[11px] text-muted-foreground">{fmtBytes(d.used_bytes)} / {fmtBytes(d.total_bytes)}</div>
            </div>
          {/each}
          {#if !snap}<div class="text-xs text-muted-foreground">加载中…</div>{/if}
        </div>
      </CardContent>
    </Card>

    <Card class="flex min-h-0 flex-col">
      <CardHeader class="pb-2"><CardTitle class="text-sm">内存构成</CardTitle></CardHeader>
      <CardContent class="flex flex-col items-center">
        <div class="relative flex size-32 items-center justify-center">
          <svg viewBox="0 0 100 100" class="size-32 -rotate-90">
            <circle cx="50" cy="50" r="40" fill="none" stroke="var(--muted)" stroke-width="12" />
            <circle cx="50" cy="50" r="40" fill="none" stroke={colorFor(snap?.mem_usage_pct ?? 0)} stroke-width="12"
              stroke-dasharray="{memCirc.toFixed(1)} 251.2" stroke-dashoffset="62.8" stroke-linecap="round" />
          </svg>
          <div class="absolute text-center">
            <div class="text-2xl font-extrabold tabular-nums" style="color:{colorFor(snap?.mem_usage_pct ?? 0)}">{(snap?.mem_usage_pct ?? 0).toFixed(1)}%</div>
            <div class="text-xs text-muted-foreground">已用</div>
          </div>
        </div>
        <Separator class="my-3" />
        <div class="flex w-full justify-center gap-4 text-xs text-muted-foreground">
          <span class="flex items-center gap-1.5"><i class="size-2 rounded-sm" style="background:{colorFor(snap?.mem_usage_pct ?? 0)}"></i>已用 {fmtBytes(memUsed)}</span>
          <span class="flex items-center gap-1.5"><i class="size-2 rounded-sm bg-muted-foreground/40"></i>空闲 {fmtBytes(memFree)}</span>
        </div>
      </CardContent>
    </Card>

    <Card class="flex min-h-0 flex-col">
      <CardHeader class="flex-row items-center justify-between space-y-0 pb-2">
        <CardTitle class="text-sm">CPU 各核心负载</CardTitle>
        <span class="text-xs text-muted-foreground">{snap?.cpu_per_core.length ?? 0} 核</span>
      </CardHeader>
      <CardContent>
        <div class="dvr-cores">
          {#each (snap?.cpu_per_core ?? []) as c, i}
            <div class="dvr-core" title="核心 {i}: {c.toFixed(1)}%">
              <div class="dvr-core-bar"><span style="height:{Math.min(100, c)}%;background:{colorFor(c)}"></span></div>
              <span class="dvr-core-label">{i}</span>
            </div>
          {/each}
          {#if !snap}<div class="text-xs text-muted-foreground">加载中…</div>{/if}
        </div>
      </CardContent>
    </Card>

    <Card class="flex min-h-0 flex-col">
      <CardHeader class="flex-row items-center justify-between space-y-0 pb-2">
        <CardTitle class="text-sm">GPU 占用</CardTitle>
        <span class="max-w-40 truncate text-xs text-muted-foreground" title={snap?.gpu_name}>{snap?.gpu_name ?? '--'}</span>
      </CardHeader>
      <CardContent class="flex items-center justify-center">
        <div class="relative flex items-center justify-center">
          <svg viewBox="0 0 120 120" class="size-32 -rotate-90">
            <circle cx="60" cy="60" r="50" fill="none" stroke="var(--muted)" stroke-width="10" />
            <circle cx="60" cy="60" r="50" fill="none" stroke={colorFor(gpuPct)} stroke-width="10"
              stroke-dasharray="{(gpuPct / 100 * 314).toFixed(1)} 314" stroke-linecap="round" />
          </svg>
          <div class="absolute text-center">
            <div class="text-2xl font-extrabold tabular-nums" style="color:{colorFor(gpuPct)}">{snap?.gpu_usage_pct != null ? gpuPct.toFixed(0) + '%' : 'N/A'}</div>
            <div class="text-xs text-muted-foreground">GPU</div>
          </div>
        </div>
      </CardContent>
    </Card>

    <Card class="flex min-h-0 flex-col">
      <CardHeader class="flex-row items-center justify-between space-y-0 pb-2">
        <CardTitle class="text-sm">实时帧率 (FPS)</CardTitle>
        <span class="text-sm font-bold tabular-nums" style="color:{fps < 30 ? '#f87171' : fps < 50 ? '#fbbf24' : '#22c55e'}">{fps}</span>
      </CardHeader>
      <CardContent>
        <svg viewBox="0 0 260 100" class="dvr-chart">
          <path d={buildArea(fpsHist, 260, 100, 4)} fill="rgba(34,197,94,0.15)" />
          <path d={buildLine(fpsHist, 260, 100, 4)} fill="none" stroke="#22c55e" stroke-width="1.6" />
        </svg>
      </CardContent>
    </Card>

    <Card class="flex min-h-0 flex-col">
      <CardHeader class="pb-2"><CardTitle class="text-sm">综合健康度雷达</CardTitle></CardHeader>
      <CardContent class="flex flex-col items-center">
        <svg viewBox="0 0 140 140" class="dvr-radar">
          <circle cx="70" cy="70" r="52" class="dvr-radar-ring" />
          <circle cx="70" cy="70" r="34" class="dvr-radar-ring" />
          <circle cx="70" cy="70" r="17" class="dvr-radar-ring" />
          {@html radarGeo.axes}
          <path d={radarGeo.poly} fill="rgba(34,211,238,0.3)" stroke="#22d3ee" stroke-width="1.5" />
          {#each radarLabels as lbl, i}
            {@const a = -Math.PI / 2 + (i * 2 * Math.PI) / radarLabels.length}
            <text x={(70 + 62 * Math.cos(a)).toFixed(1)} y={(70 + 62 * Math.sin(a) + 3).toFixed(1)} text-anchor="middle" class="dvr-radar-label">{lbl}</text>
          {/each}
        </svg>
        <div class="mt-1 text-[11px] text-muted-foreground">GPU / 网络获取失败时该轴按 0 计</div>
      </CardContent>
    </Card>
  </div>

  <Card class="flex max-h-72 min-h-40 shrink-0 flex-col">
    <CardHeader class="flex-row items-center justify-between space-y-0 pb-2">
      <CardTitle class="text-sm">系统事件日志</CardTitle>
      <span class="text-xs text-muted-foreground">实时采样 {pollCount} · 最近 40 条</span>
    </CardHeader>
    <CardContent class="flex min-h-0 flex-1 flex-col">
      <div class="dvr-log">
        {#each logs as l}
          <div class="dvr-log-line dvr-log-{l.level}"><span class="dvr-log-t">{l.t}</span><span class="dvr-log-msg">{l.msg}</span></div>
        {/each}
        {#if logs.length === 0}<div class="text-xs text-muted-foreground">等待采样数据…</div>{/if}
      </div>
    </CardContent>
  </Card>

  <footer class="flex flex-wrap items-center gap-x-5 gap-y-1 border-t pt-3 text-[11px] text-muted-foreground">
    <span>数据源：<b class="text-foreground">Tauri 后端 get_realtime_metrics</b></span>
    <span>GPU：PDH 最忙引擎口径（同任务管理器） · 磁盘：PhysicalDisk 活动率与读写吞吐</span>
    <span>网络：Network Interface 吞吐与带宽占用 · 延迟：ICMP ping</span>
    <span>内存：使用中（总量−可用，同任务管理器） · 运行时长：系统开机时长</span>
    <span class="ml-auto">刷新频率：<b class="text-foreground">1s</b> · 最近更新：{lastUpdate || '--'}</span>
    {#if refreshErr}<span class="text-destructive">指标获取异常：{refreshErr}</span>{/if}
  </footer>
</div>

<style>
  .dvr-root {
    display: flex;
    flex-direction: column;
    gap: 12px;
    height: 100%;
    overflow-y: auto;
    padding: 4px 4px 12px;
    scrollbar-gutter: stable;
  }
  .dvr-net-chart { display: flex; align-items: center; gap: 8px; height: 44px; }
  .dvr-net-chart-label { width: 32px; flex-shrink: 0; text-align: right; font-size: 11.5px; color: var(--muted-foreground); }
  .dvr-mini-chart { width: 100%; height: 40px; flex: 1; min-width: 0; }
  .dvr-spark { width: 100%; height: 36px; }
  .dvr-disks { display: flex; flex-direction: column; gap: 8px; max-height: 140px; overflow-y: auto; }
  .dvr-cores {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    align-items: flex-end;
    justify-content: center;
    align-content: flex-end;
    min-height: 110px;
    max-height: 180px;
    overflow-y: auto;
  }
  .dvr-core { display: flex; flex-direction: column; align-items: center; height: 104px; width: 20px; flex: 0 0 20px; }
  .dvr-core-bar { flex: 1; width: 10px; border-radius: 4px; background: var(--muted); display: flex; align-items: flex-end; overflow: hidden; }
  .dvr-core-bar span { width: 100%; border-radius: 4px; }
  .dvr-core-label { font-size: 11.5px; color: var(--muted-foreground); margin-top: 3px; }
  .dvr-chart { width: 100%; height: 120px; min-width: 0; }
  .dvr-radar { width: 160px; height: 160px; }
  .dvr-radar-ring { fill: none; stroke: var(--border); stroke-width: 1; }
  .dvr-radar-axis { stroke: color-mix(in oklab, var(--border) 130%, transparent); stroke-width: 1; }
  .dvr-radar-label { fill: var(--muted-foreground); font-size: 11.5px; }
  .dvr-log { flex: 1; min-height: 0; overflow-y: auto; font-family: var(--font-mono); font-size: 12px; line-height: 1.7; }
  .dvr-log-line { display: flex; gap: 10px; border-bottom: 1px solid var(--border); padding: 1px 0; }
  .dvr-log-t { color: var(--muted-foreground); flex-shrink: 0; }
  .dvr-log-info .dvr-log-msg { color: color-mix(in oklab, var(--foreground) 78%, var(--primary)); }
  .dvr-log-warn .dvr-log-msg { color: #fbbf24; }
  .dvr-log-error .dvr-log-msg { color: #f87171; }
</style>
