<script lang="ts">
  import { onMount } from 'svelte';
  import { kbApi } from './services/ipc';
  import type { KbSummary, KbStats } from './kbTypes';
  import { formatBytes, formatDateOnly } from '../format';
  import { filterKbsByKeyword, kbMonogram, trendArrow, trendClass } from './fileUtils';
  import KbIcon from './KbIcon.svelte';
  import KbTrendChart from './KbTrendChart.svelte';
  import FancyStat from '../components/fancy/FancyStat.svelte';
  import { Input } from '../components/ui/input';
  import { Button } from '../components/ui/button';
  import { Badge } from '../components/ui/badge';
  import { Card } from '../components/ui/card';
  import { Skeleton } from '../components/ui/skeleton';
  import { Alert, AlertDescription, AlertTitle } from '../components/ui/alert';
  import { Empty } from '../components/ui/empty';
  import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from '../components/ui/dropdown-menu';

  interface Props {
    kbs: KbSummary[];
    selectedKb: number | null;
    refreshKbs: () => Promise<void>;
    onOpenKb: (id: number) => void;
    onNewKb?: () => void;
    onImportKb?: (e: Event) => void;
    onEditKb: (kb: KbSummary) => void;
    onDeleteKb: (kb: KbSummary) => void;
    onTogglePin: (kb: KbSummary) => void;
    onExportKb?: (kb: KbSummary) => void;
    notify: (msg: string, type?: 'success' | 'error' | 'warn') => void;
    mode?: 'full' | 'kbs';
    isAdmin?: boolean;
  }
  let { kbs, onOpenKb, onNewKb, onImportKb, onEditKb, onDeleteKb, onTogglePin, onExportKb, mode = 'full', isAdmin = false }: Props = $props();

  let stats = $state<KbStats | null>(null);
  let kbSearch = $state('');
  let loading = $state(true);
  let error = $state('');

  const totalDocs = $derived(kbs.reduce((s, k) => s + k.docCount, 0));
  // 过滤后的知识库列表：渲染期实时读取 kbs/kbSearch。
  // 不能用 $derived 捕获 props 数组：props 在首帧后才异步就绪时，derived 会持有
  // 初始空数组引用导致「已有知识库却显示空态」（首页首帧复现）。普通函数每次渲染
  // 重新读取最新 props，与模板条件/列表保持同源。
  function filteredKbs() {
    return filterKbsByKeyword(kbs, kbSearch);
  }

  function fmtTime(t: string): string {
    return formatDateOnly(t, true);
  }
  function fmtBytes(n: number | null | undefined): string {
    return formatBytes(n, { gbPrecision: 2 });
  }

  // 首页数据指标
  const METRIC_ICONS: Record<string, string> = {
    messages: 'chat', sessions: 'chatCircle', recall: 'search',
    faq: 'list', llm: 'sparkle', recommend: 'idea',
  };
  const metrics = $state([
    { key: 'messages', label: '消息量', value: '0', daily: '--', yearly: '--', visible: true },
    { key: 'sessions', label: '会话量', value: '0', daily: '--', yearly: '--', visible: true },
    { key: 'recall', label: '整体召回率', value: '0%', daily: '--', yearly: '--', visible: true },
    { key: 'faq', label: '常用问答', value: '0', daily: '--', yearly: '--', visible: true },
    { key: 'llm', label: 'LLM问答', value: '0', daily: '--', yearly: '--', visible: true },
    { key: 'recommend', label: '问题推荐', value: '0', daily: '--', yearly: '--', visible: true },
  ]);
  const visibleMetrics = $derived(metrics.filter((m) => m.visible !== false));

  async function loadAnalytics() {
    try {
      const res = await kbApi.getAnalytics();
      for (const item of res?.metrics ?? []) {
        const target = metrics.find((x) => x.key === item.key);
        if (!target) continue;
        target.value = String(item.value ?? item.today ?? 0);
        target.daily = item.daily ?? '--';
        target.yearly = item.yearly ?? '--';
        if (item.label) target.label = item.label;
        if (typeof item.visible === 'boolean') target.visible = item.visible;
      }
    } catch { /* 统计不可用时保持占位 */ }
  }

  async function loadStats() {
    try {
      stats = await kbApi.getStats();
      error = '';
    } catch (e) {
      error = '加载统计数据失败，请稍后重试';
    }
  }

  let searchInput = $state<HTMLInputElement | null>(null);
  onMount(() => {
    loading = true;
    Promise.all([loadStats(), loadAnalytics()]).finally(() => { loading = false; });
    // ⌘/Ctrl + K：聚焦知识库搜索
    const handler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
        if (!searchInput || searchInput.offsetParent === null) return;
        e.preventDefault();
        searchInput?.focus();
        searchInput?.select();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  });
</script>

<!-- ============================================ -->
<!-- 首页工作台 / 知识库管理                          -->
<!-- ============================================ -->
<div class="kb-dashboard">
  {#if error}
    <Alert variant="destructive">
      <KbIcon name="warning" size={16} />
      <AlertTitle>加载失败</AlertTitle>
      <AlertDescription class="flex items-center gap-2">
        {error}
        <Button variant="outline" size="sm" onclick={() => { loadStats(); loadAnalytics(); }}>重试</Button>
      </AlertDescription>
    </Alert>
  {/if}

  {#if mode === 'full'}
  <!-- ─── 统计总览（仅首页显示） ─── -->
  <section class="kb-stats-grid">
    {#if loading}
      {#each Array(5) as _}
        <Skeleton class="h-[88px] rounded-xl" />
      {/each}
    {:else}
      <FancyStat value={stats?.kb_count ?? kbs.length} label="知识库" />
      <FancyStat value={stats?.doc_count ?? totalDocs} label="文档" />
      <FancyStat value={stats?.chunk_count ?? 0} label="知识分片" />
      <FancyStat value={stats?.wiki_page_count ?? 0} label="Wiki 页" />
      <Card class="p-4 flex flex-col gap-1">
        <div class="flex items-center justify-between">
          <span class="text-xs text-muted-foreground">存储用量</span>
          <span class="text-sm font-semibold">{stats ? fmtBytes(stats.storage_bytes) : '-'}</span>
        </div>
        {#if stats}
          <div class="h-1.5 rounded-full bg-muted overflow-hidden mt-1">
            <div
              class="h-full rounded-full bg-primary transition-all duration-500"
              style="width:{Math.min(100, (stats.storage_bytes / Math.max(1, stats.storage_quota)) * 100)}%"
            ></div>
          </div>
          <span class="text-xs text-muted-foreground">/ {fmtBytes(stats.storage_quota)}</span>
        {/if}
      </Card>
    {/if}
  </section>

  <!-- ─── 处理状态速览 ─── -->
  {#if stats && !loading}
    <div class="kb-status-bar">
      <span class="kb-status-label">文档状态</span>
      <Badge variant="default">就绪 {stats.doc_ready}</Badge>
      <Badge variant="secondary">处理中 {stats.doc_processing}</Badge>
      <Badge variant="destructive">失败 {stats.doc_failed}</Badge>
      <span class="kb-status-label" style="margin-left:12px">任务</span>
      <Badge variant="outline">排队/执行中 {stats.job_pending}</Badge>
      <Badge variant="default">完成 {stats.job_done}</Badge>
      <Badge variant="destructive">失败 {stats.job_failed}</Badge>
    </div>
  {/if}
  {/if}

  <!-- ─── 知识库管理（搜索 + 卡片网格） ─── -->
  <section class="kb-section">
    <div class="kb-section-header">
      <div class="kb-section-title">
        <KbIcon name="kb" size={16} color="var(--kb-accent-bright)" />
        <span>我的知识库</span>
      </div>
      <div class="flex items-center gap-2">
        <div class="kb-searchbox">
          <KbIcon name="search" size={14} />
          <Input
            bind:ref={searchInput}
            class="w-[200px]"
            placeholder="搜索知识库…（Ctrl+K）"
            bind:value={kbSearch}
          />
        </div>
        {#if mode === 'kbs'}
          {#if onNewKb}
            <Button size="sm" onclick={onNewKb} title="新建知识库">
              <KbIcon name="plus" size={14} weight="bold" />新建知识库
            </Button>
          {/if}
          {#if onImportKb}
            <label class="inline-flex">
              <Button variant="outline" size="sm" title="从导出文件导入知识库">
                <KbIcon name="upload" size={14} />导入
              </Button>
              <input type="file" hidden accept=".json" onchange={onImportKb} />
            </label>
          {/if}
        {/if}
      </div>
    </div>

      {#if loading}
        <div class="kb-grid-manage">
          {#each Array(6) as _}
            <Skeleton class="h-[160px] rounded-xl" />
          {/each}
        </div>
      {:else if kbs.length === 0}
        <Empty>
          <KbIcon name="folderOpen" size={32} color="var(--kb-text-3)" />
          {#if mode === 'full'}
            <div class="text-sm text-muted-foreground">还没有知识库，请到「知识库」面板创建第一个</div>
          {:else}
            <div class="text-sm text-muted-foreground">还没有知识库，创建第一个开始搭建知识资产</div>
            {#if onNewKb}
              <Button onclick={onNewKb}>
                <KbIcon name="plus" size={14} weight="bold" />新建第一个知识库
              </Button>
            {/if}
          {/if}
        </Empty>
      {:else}
        <div class="kb-grid-manage">
          {#each filteredKbs() as kb}
            <Card
              class="kb-kb-card-full"
              role="button"
              tabindex={0}
              onclick={() => onOpenKb(kb.id)}
              onkeydown={(e: KeyboardEvent) => e.key === 'Enter' && onOpenKb(kb.id)}
            >
              <div class="kb-kb-card-header">
                <div class="kb-kb-monogram">{kbMonogram(kb.name)}</div>
                <div class="kb-kb-card-info">
                  <div class="kb-kb-card-name">
                    <span title={kb.name}>{kb.name}</span>
                    {#if kb.pinned}<Badge variant="secondary" class="text-[10px] px-1.5 py-0">置顶</Badge>{/if}
                    {#if kb.isSystem}<Badge variant="outline" class="text-[10px] px-1.5 py-0">系统</Badge>{/if}
                  </div>
                  <span class="text-xs text-muted-foreground">
                    {kb.docCount} 文档{ kb.created_at ? ' · ' + fmtTime(kb.created_at) : ''}
                  </span>
                </div>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  title={kb.pinned ? '取消置顶' : '置顶'}
                  onclick={(e: MouseEvent) => { e.stopPropagation(); onTogglePin(kb); }}
                >
                  <KbIcon name="pin" size={14} color={kb.pinned ? 'var(--kb-accent-bright)' : 'var(--kb-text-3)'} />
                </Button>
              </div>

              <div class="kb-kb-card-desc">
                {kb.description || '暂无描述'}
              </div>

              <div class="kb-kb-card-actions">
                {#if !kb.isSystem && isAdmin}
                  <DropdownMenu>
                    <DropdownMenuTrigger>
                      <Button variant="ghost" size="icon-sm" onclick={(e: MouseEvent) => e.stopPropagation()}>
                        <KbIcon name="more" size={14} />
                      </Button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end">
                      <DropdownMenuItem onclick={() => onEditKb(kb)}>
                        <KbIcon name="edit" size={13} />编辑
                      </DropdownMenuItem>
                      {#if onExportKb}
                        <DropdownMenuItem onclick={() => onExportKb(kb)}>
                          <KbIcon name="download" size={13} />导出
                        </DropdownMenuItem>
                      {/if}
                      <DropdownMenuSeparator />
                      <DropdownMenuItem class="text-destructive" onclick={() => onDeleteKb(kb)}>
                        <KbIcon name="trash" size={13} />删除
                      </DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                {:else if !isAdmin}
                  <span class="text-xs text-muted-foreground">仅管理员可编辑</span>
                {/if}
              </div>
            </Card>
          {/each}

          <!-- 新建知识库卡片 -->
          {#if onNewKb}
          <Card class="kb-kb-card-new" role="button" tabindex={0} onclick={onNewKb}
            onkeydown={(e: KeyboardEvent) => e.key === 'Enter' && onNewKb()}>
            <div class="flex items-center gap-3">
              <div class="kb-new-icon">
                <KbIcon name="plus" size={22} weight="bold" />
              </div>
              <div class="kb-new-text">
                <span class="font-semibold text-sm">新建知识库</span>
                <span class="text-xs text-muted-foreground">上传文档、搭建你的知识资产</span>
              </div>
            </div>
          </Card>
          {/if}

          {#if filteredKbs().length === 0 && !loading}
            <div class="col-span-full">
              <Empty>
                <KbIcon name="folderOpen" size={28} color="var(--kb-text-3)" />
                <span class="text-sm text-muted-foreground">
                  {kbs.length === 0 ? '还没有知识库，点击上方卡片开始' : '没有匹配的知识库'}
                </span>
              </Empty>
            </div>
          {/if}
        </div>
      {/if}
    </section>

    {#if mode === 'full'}
    <!-- ─── 昨日关键指标 ─── -->
    <section class="kb-section">
      <div class="kb-section-header">
        <div class="kb-section-title">
          <KbIcon name="chart" size={16} color="var(--kb-accent-bright)" />
          <span>昨日关键指标</span>
          <span class="kb-section-sub">昨日数据 · 接入统计管线后自动填充</span>
        </div>
      </div>

      {#if loading}
        <div class="kb-metrics-grid">
          {#each Array(6) as _}
            <Skeleton class="h-[100px] rounded-xl" />
          {/each}
        </div>
      {:else}
        <div class="kb-metrics-grid">
          {#each visibleMetrics as m}
            <Card class="kb-metric-card p-4">
              <div class="flex items-center justify-between mb-2">
                <span class="text-xs text-muted-foreground">{m.label}</span>
                <KbIcon name={METRIC_ICONS[m.key]} size={15} color="var(--kb-text-3)" />
              </div>
              <div class="text-xl font-bold mb-2">{m.value}</div>
              <div class="flex gap-4 text-xs">
                <span class="text-muted-foreground">日环比 <b class={trendClass(m.daily)}>{trendArrow(m.daily)}{m.daily}</b></span>
                <span class="text-muted-foreground">日同比 <b class={trendClass(m.yearly)}>{trendArrow(m.yearly)}{m.yearly}</b></span>
              </div>
            </Card>
          {/each}
        </div>
      {/if}
    </section>

    <!-- ─── 数据趋势图 ─── -->
    <KbTrendChart />
    {/if}
</div>

<style>
  /* ── Dashboard 容器 ── */
  .kb-dashboard {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  /* ── 统计指标网格：5 列 → 3 列 → 2 列 ── */
  .kb-stats-grid {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: 12px;
  }
  @media (max-width: 1279px) {
    .kb-stats-grid { grid-template-columns: repeat(3, 1fr); }
  }
  @media (max-width: 767px) {
    .kb-stats-grid { grid-template-columns: repeat(2, 1fr); }
  }

  /* ── 处理状态栏 ── */
  .kb-status-bar {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    padding: 2px 0;
  }
  .kb-status-label {
    font-size: 12px;
    color: var(--kb-text-3);
  }

  /* ── 通用区块 ── */
  .kb-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .kb-section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .kb-section-title {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 14px;
    font-weight: 600;
  }
  .kb-section-sub {
    font-size: 12px;
    font-weight: 400;
    color: var(--kb-text-3);
    margin-left: 4px;
  }

  /* ── 知识库卡片网格：3 列 → 2 列 → 1 列 ── */
  .kb-grid-3 {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
  }
  @media (max-width: 1279px) {
    .kb-grid-3 { grid-template-columns: repeat(2, 1fr); }
  }
  @media (max-width: 767px) {
    .kb-grid-3 { grid-template-columns: 1fr; }
  }

  /* ── 知识库管理页网格 ── */
  .kb-grid-manage {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 12px;
  }
  @media (max-width: 767px) {
    .kb-grid-manage { grid-template-columns: 1fr; }
  }

  /* ── 指标卡片网格：6 列 → 3 列 → 2 列 ── */
  .kb-metrics-grid {
    display: grid;
    grid-template-columns: repeat(6, 1fr);
    gap: 12px;
  }
  @media (max-width: 1279px) {
    .kb-metrics-grid { grid-template-columns: repeat(3, 1fr); }
  }
  @media (max-width: 767px) {
    .kb-metrics-grid { grid-template-columns: repeat(2, 1fr); }
  }

  /* ── 知识库卡片（简洁入口） ─── */
  :global(.kb-kb-card) {
    cursor: pointer;
    padding: 14px !important;
    transition: border-color 0.15s, box-shadow 0.15s;
  }
  :global(.kb-kb-card:hover) {
    border-color: var(--kb-accent);
    box-shadow: 0 0 0 1px var(--kb-accent);
  }
  .kb-kb-card-header {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .kb-kb-monogram {
    width: 34px;
    height: 34px;
    border-radius: 8px;
    background: var(--kb-accent);
    color: #fff;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 14px;
    font-weight: 700;
    flex-shrink: 0;
  }
  .kb-kb-card-info {
    flex: 1;
    min-width: 0;
  }
  .kb-kb-card-name {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* ── 知识库卡片（管理页完整版） ─── */
  :global(.kb-kb-card-full) {
    cursor: pointer;
    padding: 14px !important;
    display: flex;
    flex-direction: column;
    gap: 10px;
    transition: border-color 0.15s, box-shadow 0.15s;
  }
  :global(.kb-kb-card-full:hover) {
    border-color: var(--kb-accent);
    box-shadow: 0 0 0 1px var(--kb-accent);
  }
  .kb-kb-card-desc {
    font-size: 12.5px;
    color: var(--kb-text-2);
    min-height: 34px;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .kb-kb-card-actions {
    display: flex;
    gap: 6px;
    margin-top: auto;
    align-items: center;
    justify-content: flex-end;
  }

  /* ── 新建知识库卡片 ─── */
  :global(.kb-kb-card-new) {
    cursor: pointer;
    border-style: dashed !important;
    padding: 18px !important;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: border-color 0.15s, background 0.15s;
  }
  :global(.kb-kb-card-new:hover) {
    border-color: var(--kb-accent);
    background: color-mix(in srgb, var(--kb-accent) 5%, transparent);
  }
  .kb-new-icon {
    width: 40px;
    height: 40px;
    border-radius: 10px;
    background: var(--kb-surface-2);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--kb-text-3);
  }
  .kb-new-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  /* ── 搜索框 ─── */
  .kb-searchbox {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--kb-text-3);
  }
  .kb-manage-card {
    padding: 0 !important;
  }
  .kb-manage-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--kb-border-subtle);
  }
  .kb-manage-body {
    padding: 16px 20px;
  }

  /* ── 趋势箭头色 ── */
  :global(.kb-trend-up) { color: var(--kb-ok); }
  :global(.kb-trend-down) { color: var(--kb-err); }
</style>
