<script lang="ts">
  import { onMount } from 'svelte';
  import { kbApi } from './services/ipc';
  import type { KbSummary, KbStats } from './kbTypes';
  import { formatBytes, formatDateOnly } from '../format';
  import { filterKbsByKeyword, kbMonogram, trendArrow, trendClass } from './fileUtils';
  import KbIcon from './KbIcon.svelte';
  import KbTrendChart from './KbTrendChart.svelte';
  import { Input } from '../components/ui/input';
  import LiveNumber from '../components/fancy/LiveNumber.svelte';

  interface Props {
    kbs: KbSummary[];
    selectedKb: number | null;
    refreshKbs: () => Promise<void>;
    onOpenKb: (id: number) => void;
    onNewKb: () => void;
    onEditKb: (kb: KbSummary) => void;
    onDeleteKb: (kb: KbSummary) => void;
    onTogglePin: (kb: KbSummary) => void;
    notify: (msg: string, type?: 'success' | 'error' | 'warn') => void;
    // full = 首页工作台（统计 + 状态 + 知识库 + 活动）；kbs = 知识库列表管理页
    mode?: 'full' | 'kbs';
  }
  let { kbs, onOpenKb, onNewKb, onEditKb, onDeleteKb, onTogglePin, mode = 'full' }: Props = $props();

  let stats = $state<KbStats | null>(null);
  let kbSearch = $state('');

  const totalDocs = $derived(kbs.reduce((s, k) => s + k.docCount, 0));
  const filteredKbs = $derived(filterKbsByKeyword(kbs, kbSearch));

  function fmtTime(t: string): string {
    return formatDateOnly(t, true);
  }
  /** 字节格式化：保持原实现（GB 两位小数；null 走 '0 B'） */
  function fmtBytes(n: number | null | undefined): string {
    return formatBytes(n, { gbPrecision: 2 });
  }
  // 首页数据指标（后端完整埋点口径：qa_messages/qa_sessions/kb_metric_events/
  // processing_jobs 实时聚合，含 7 天序列；未产生数据时显示 0/--）
  const METRIC_ICONS: Record<string, string> = {
    messages: 'chat', sessions: 'chatCircle', recall: 'search', handoff: 'send',
    faq: 'list', llm: 'sparkle', task: 'stack', recommend: 'idea',
  };
  const metrics = $state([
    { key: 'messages', label: '消息量', value: '0', daily: '--', yearly: '--', visible: true },
    { key: 'sessions', label: '会话量', value: '0', daily: '--', yearly: '--', visible: true },
    { key: 'recall', label: '整体召回率', value: '0%', daily: '--', yearly: '--', visible: true },
    { key: 'handoff', label: '转人工率', value: '0%', daily: '--', yearly: '--', visible: true },
    { key: 'faq', label: '常用问答', value: '0', daily: '--', yearly: '--', visible: true },
    { key: 'llm', label: 'LLM问答', value: '0', daily: '--', yearly: '--', visible: true },
    { key: 'task', label: '任务技能', value: '0', daily: '--', yearly: '--', visible: true },
    { key: 'recommend', label: '问题推荐', value: '0', daily: '--', yearly: '--', visible: true },
  ]);
  const visibleMetrics = $derived(metrics.filter((m) => m.visible !== false));
  // 拉取真实统计（8 项指标已由后端完整埋点计算；label/visible 由指标配置下发）
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
    stats = await kbApi.getStats().catch(() => null);
  }

  let searchInput = $state<HTMLInputElement | null>(null);
  onMount(() => {
    loadStats();
    loadAnalytics();
    // ⌘/Ctrl + K：聚焦知识库搜索
    const handler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
        // 面板不可见时让给全局搜索（App 层 Ctrl+K 打开全局搜索弹窗）
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

<div style="display:flex;flex-direction:column;gap:16px">
  {#if mode === 'full'}
  <!-- 统计总览 -->
  <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:12px;flex:none">
    <div class="kb-card kb-stat-card">
      <span class="kb-stat-ico"><KbIcon name="kb" size={20} /></span>
      <div><div class="kb-stat-value"><LiveNumber value={stats?.kb_count ?? kbs.length} duration={700} /></div><div class="kb-stat-label">知识库</div></div>
    </div>
    <div class="kb-card kb-stat-card">
      <span class="kb-stat-ico"><KbIcon name="docs" size={20} /></span>
      <div><div class="kb-stat-value"><LiveNumber value={stats?.doc_count ?? totalDocs} duration={700} /></div><div class="kb-stat-label">文档</div></div>
    </div>
    <div class="kb-card kb-stat-card">
      <span class="kb-stat-ico"><KbIcon name="stack" size={20} /></span>
      <div><div class="kb-stat-value">{#if stats?.chunk_count != null}<LiveNumber value={stats.chunk_count} duration={700} />{:else}-{/if}</div><div class="kb-stat-label">知识分片</div></div>
    </div>
    <div class="kb-card kb-stat-card">
      <span class="kb-stat-ico"><KbIcon name="wiki" size={20} /></span>
      <div><div class="kb-stat-value">{#if stats?.wiki_page_count != null}<LiveNumber value={stats.wiki_page_count} duration={700} />{:else}-{/if}</div><div class="kb-stat-label">Wiki 页</div></div>
    </div>
    <div class="kb-card kb-stat-card" style="grid-column:auto">
      <span class="kb-stat-ico"><KbIcon name="database" size={20} /></span>
      <div style="flex:1;min-width:0">
        <div class="kb-stat-value">{stats ? fmtBytes(stats.storage_bytes) : '-'}<span style="font-size:11.5px;color:var(--kb-text-3)"> / {stats ? fmtBytes(stats.storage_quota) : ''}</span></div>
        <div class="kb-stat-label">存储用量</div>
        {#if stats}
          <div style="height:5px;border-radius:3px;background:var(--kb-border);overflow:hidden;margin-top:6px">
            <div style="height:100%;width:{Math.min(100, (stats.storage_bytes / Math.max(1, stats.storage_quota)) * 100)}%;background:linear-gradient(90deg,var(--kb-btn-bg),var(--kb-accent));border-radius:3px"></div>
          </div>
        {/if}
      </div>
    </div>
  </div>

  <!-- 处理状态速览 -->
  {#if stats}
    <div style="display:flex;flex-wrap:wrap;gap:8px;align-items:center;padding:2px 2px 0;flex:none">
      <span style="font-size:12.5px;color:var(--app-color-muted)">文档状态：</span>
      <span class="kb-badge kb-badge-ok">就绪 {stats.doc_ready}</span>
      <span class="kb-badge kb-badge-warn">处理中 {stats.doc_processing}</span>
      <span class="kb-badge kb-badge-err">失败 {stats.doc_failed}</span>
      <span style="font-size:12.5px;color:var(--app-color-muted);margin-left:8px">任务：</span>
      <span class="kb-badge kb-badge-mute">排队/执行中 {stats.job_pending}</span>
      <span class="kb-badge kb-badge-ok">完成 {stats.job_done}</span>
      <span class="kb-badge kb-badge-err">失败 {stats.job_failed}</span>
    </div>
  {/if}

  <!-- 我的知识库（快捷入口：首页直达工作区） -->
  <div style="display:flex;flex-direction:column;gap:10px">
    <div class="kb-section-title">
      <KbIcon name="kb" size={16} color="var(--kb-accent-bright)" />我的知识库
      <span class="kb-section-sub">选择一个知识库，进入文档 / Wiki 工作区</span>
      <div style="flex:1"></div>
      <button class="kb-btn-sm" onclick={onNewKb} title="新建知识库"><KbIcon name="plus" size={13} weight="bold" />新建知识库</button>
    </div>
    {#if kbs.length === 0}
      <div class="kb-card"><div class="kb-empty" style="padding:26px 18px">
        <span class="kb-empty-ico"><KbIcon name="folderOpen" size={20} /></span>
        <span>还没有知识库，创建第一个开始搭建知识资产</span>
        <button class="kb-btn" onclick={onNewKb}><KbIcon name="plus" size={13} />新建第一个知识库</button>
      </div></div>
    {:else}
      <div style="display:grid;grid-template-columns:repeat(auto-fill,minmax(240px,1fr));gap:10px">
        {#each kbs as kb}
          <div class="kb-kb-card" role="button" tabindex="0"
            onclick={() => onOpenKb(kb.id)}
            onkeydown={(e) => e.key === 'Enter' && onOpenKb(kb.id)}>
            <div class="kb-kb-card-cover" style="height:54px">
              <div class="kb-kb-monogram" style="width:34px;height:34px;font-size:14px">{kbMonogram(kb.name)}</div>
              <div style="flex:1;min-width:0">
                <div style="display:flex;align-items:center;gap:6px">
                  <span style="font-size:13px;font-weight:600;color:var(--kb-text);overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title={kb.name}>{kb.name}</span>
                  {#if kb.pinned}<span class="kb-badge kb-badge-info"><KbIcon name="pin" size={11} /></span>{/if}
                  {#if kb.isSystem}<span class="kb-badge kb-badge-mute">系统</span>{/if}
                </div>
                <div style="font-size:11.5px;color:var(--kb-text-3)">{kb.docCount} 文档</div>
              </div>
              <KbIcon name="arrowRight" size={14} color="var(--kb-text-3)" />
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>

  <!-- 数据指标 -->
  <div style="display:flex;flex-direction:column;gap:12px">
    <div class="kb-section-title">
      <KbIcon name="chart" size={16} color="var(--kb-accent-bright)" />昨日关键指标
      <span class="kb-section-sub">昨日数据 · 接入统计管线后自动填充</span>
    </div>
    <div style="display:grid;grid-template-columns:repeat(4,1fr);gap:12px">
      {#each visibleMetrics as m}
        <div class="kb-card kb-metric-card">
          <div class="kb-metric-head">
            <span class="kb-metric-label">{m.label}</span>
            <span class="kb-metric-ico"><KbIcon name={METRIC_ICONS[m.key]} size={15} /></span>
          </div>
          <div class="kb-metric-value">{m.value}</div>
          <div class="kb-metric-rows">
            <span class="kb-metric-row"><i>日环比</i><b class={trendClass(m.daily)}>{trendArrow(m.daily)}{m.daily}</b></span>
            <span class="kb-metric-row"><i>日同比</i><b class={trendClass(m.yearly)}>{trendArrow(m.yearly)}{m.yearly}</b></span>
          </div>
        </div>
      {/each}
    </div>
  </div>

  <!-- 数据指标趋势图 -->
  <KbTrendChart />
  {:else if mode === 'kbs'}

  <!-- 知识库卡片 -->
  <div class="kb-card">
    <div class="kb-card-hd" style="justify-content:space-between">
      <span><KbIcon name="kb" size={15} color="var(--kb-accent-bright)" />我的知识库</span>
      <div style="display:flex;gap:8px;align-items:center">
        <div class="kb-searchbox">
          <span><KbIcon name="search" size={14} /></span>
          <Input class="kb-input" bind:ref={searchInput} style="width:220px" placeholder="搜索知识库…（Ctrl+K）" bind:value={kbSearch} />
        </div>
      </div>
    </div>
    <div class="kb-card-bd" style="display:grid;grid-template-columns:repeat(auto-fill,minmax(260px,1fr));gap:12px">
      {#each filteredKbs as kb}
        <div class="kb-kb-card"
          role="button" tabindex="0"
          onclick={() => onOpenKb(kb.id)}
          onkeydown={(e) => e.key === 'Enter' && onOpenKb(kb.id)}>
          <div class="kb-kb-card-cover">
            <div class="kb-kb-monogram">{kbMonogram(kb.name)}</div>
            <div style="flex:1;min-width:0">
              <div style="display:flex;align-items:center;gap:6px">
                <span style="font-size:14px;font-weight:600;color:var(--kb-text);overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title={kb.name}>{kb.name}</span>
                {#if kb.pinned}<span class="kb-badge kb-badge-info"><KbIcon name="pin" size={11} />置顶</span>{/if}
                {#if kb.isSystem}<span class="kb-badge kb-badge-mute">系统</span>{/if}
              </div>
              <div style="font-size:11.5px;color:var(--kb-text-3);margin-top:2px">
                {kb.docCount} 文档{ kb.created_at ? ' · ' + fmtTime(kb.created_at) : ''}
              </div>
            </div>
            <button class="kb-btn-sm kb-btn-ghost" style="padding:2px 6px" title={kb.pinned ? '取消置顶' : '置顶'}
              onclick={(e) => { e.stopPropagation(); onTogglePin(kb); }}>
              <KbIcon name="pin" size={14} color={kb.pinned ? 'var(--kb-accent-bright)' : 'var(--kb-text-3)'} />
            </button>
          </div>
          <div style="padding:12px 14px;display:flex;flex-direction:column;gap:10px;flex:1">
            <div style="font-size:12.5px;color:var(--kb-text-2);min-height:34px;display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical;overflow:hidden">
              {kb.description || '暂无描述'}
            </div>
            <div style="display:flex;gap:6px;margin-top:auto">
              {#if !kb.isSystem}
                <button class="kb-btn-sm" onclick={(e) => { e.stopPropagation(); onEditKb(kb); }}><KbIcon name="edit" size={13} />编辑</button>
                <button class="kb-btn-sm kb-dang" onclick={(e) => { e.stopPropagation(); onDeleteKb(kb); }}><KbIcon name="trash" size={13} />删除</button>
              {/if}
            </div>
          </div>
        </div>
      {/each}
      <button class="kb-kb-card-new" onclick={onNewKb}>
        <span class="kb-new-card-ico"><KbIcon name="plus" size={22} weight="bold" /></span>
        <span class="kb-new-card-txt">
          <span>新建知识库</span>
          <span>上传文档、搭建你的知识资产</span>
        </span>
      </button>
      {#if filteredKbs.length === 0}
        <div class="kb-empty" style="grid-column:1/-1">
          <span class="kb-empty-ico"><KbIcon name="folderOpen" size={22} /></span>
          <span>{kbs.length === 0 ? '还没有知识库，点击右侧「新建知识库」卡片开始' : '没有匹配的知识库'}</span>
        </div>
      {/if}
    </div>
  </div>
  {/if}
</div>
