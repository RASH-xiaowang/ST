<script lang="ts">
  import { kbApi } from './services/ipc';
  import { onMount, onDestroy } from 'svelte';
  import { formatIsoTime } from '../format';
  import type { JobItem, JobLogItem, SearchLogItem } from './kbTypes';
  import { MODE_LABEL } from './fileUtils';
  import KbIcon from './KbIcon.svelte';
  import KbModal from './KbModal.svelte';

  interface Props {
    selectedKb: number | null;
    notify: (msg: string, type?: 'success' | 'error' | 'warn') => void;
  }
  let { selectedKb, notify }: Props = $props();

  let tab = $state<'jobs' | 'history'>('jobs');
  let jobs = $state<JobItem[]>([]);
  // 任务总数（后端统计），用于展示「共 N 条」并判断列表是否被截断
  let jobTotal = $state(0);
  // 单次最多加载的任务数（覆盖大批量提炼场景，如一次提交 158 个文档）
  const JOB_FETCH_LIMIT = 1000;
  let history = $state<SearchLogItem[]>([]);
  let jobsTimer: ReturnType<typeof setInterval> | null = null;
  let logsOpen = $state(false);
  let logs = $state<JobLogItem[]>([]);
  let logsLoading = $state(false);

  const stageLabel: Record<string, string> = {
    pending: '排队中', parsing: '解析中', chunking: '分片中', embedding: '向量化',
    generating: 'Wiki 提炼', done: '已完成', failed: '失败', embed_error: '向量化失败',
  };
  // ─── 任务分类：待处理 / 处理中 / 已完成 / 失败 ───
  const ACTIVE_STAGES = ['parsing', 'chunking', 'embedding', 'generating'];
  const CATEGORIES = [
    { key: 'all', label: '全部' },
    { key: 'pending', label: '待处理' },
    { key: 'processing', label: '处理中' },
    { key: 'done', label: '已完成' },
    { key: 'failed', label: '失败' },
  ] as const;
  function jobCategory(j: JobItem): string {
    if (j.stage === 'pending') return 'pending';
    if (ACTIVE_STAGES.includes(j.stage)) return 'processing';
    if (j.stage === 'done') return 'done';
    return 'failed';
  }
  let stageFilter = $state('all');
  const catCounts = $derived.by(() => {
    const m: Record<string, number> = { all: jobTotal, pending: 0, processing: 0, done: 0, failed: 0 };
    for (const j of jobs) m[jobCategory(j)] = (m[jobCategory(j)] ?? 0) + 1;
    return m;
  });
  const filteredJobs = $derived(stageFilter === 'all' ? jobs : jobs.filter((j) => jobCategory(j) === stageFilter));

  async function openJobLogs(jobId: number) {
    logsOpen = true; logs = []; logsLoading = true;
    try {
    logs = await kbApi.getJobLogs(jobId);
    } catch (e: unknown) { logs = []; notify('读取日志失败：' + e, 'error'); }
    finally { logsLoading = false; }
  }

  let housekeepingBusy = $state(false);
  let clearing = $state(false);
  let stopping = $state(false);
  async function stopProcessing() {
    const active = (catCounts.pending ?? 0) + (catCounts.processing ?? 0);
    if (active === 0) return;
    if (!confirm(`确定停止当前 ${active} 个进行中/待处理的任务？\n已生成的分片或 Wiki 页面会保留，任务将被标记为「已手动停止」。`)) return;
    stopping = true;
    try {
      const res = await kbApi.stopProcessing(selectedKb);
      notify(`已停止 ${res.stopped} 个任务`, 'warn');
      stageFilter = 'all';
      loadJobs();
    } catch (e: unknown) { notify('停止任务失败：' + e, 'error'); }
    finally { stopping = false; }
  }
  async function clearFinishedJobs() {
    if (!confirm('确定清理所有已完成/失败的处理任务及其日志？\n队列中/执行中的任务会保留。')) return;
    clearing = true;
    try {
      const res = await kbApi.clearActivity('jobs');
      notify(`已清理 ${res.jobs ?? 0} 个已完成/失败任务${res.logs ? `、${res.logs} 条日志` : ''}`, 'warn');
      stageFilter = 'all';
      loadJobs();
    } catch (e: unknown) { notify('清理任务失败：' + e, 'error'); }
    finally { clearing = false; }
  }
  async function clearLogs() {
    if (!confirm('确定清空全部处理日志？')) return;
    clearing = true;
    try {
      const res = await kbApi.clearActivity('logs');
      notify(`已清理 ${res.logs ?? 0} 条处理日志`, 'warn');
    } catch (e: unknown) { notify('清理日志失败：' + e, 'error'); }
    finally { clearing = false; }
  }
  async function clearHistory() {
    if (!confirm('确定清空全部检索历史？')) return;
    clearing = true;
    try {
      const res = await kbApi.clearActivity('history');
      notify(`已清理 ${res.history ?? 0} 条检索历史`, 'warn');
      loadHistory();
    } catch (e: unknown) { notify('清空检索历史失败：' + e, 'error'); }
    finally { clearing = false; }
  }
  async function runHousekeeping() {
    if (housekeepingBusy) return;
    housekeepingBusy = true;
    try {
    const res = await kbApi.housekeeping();
      if (res.jobs > 0 || res.docs > 0) {
        notify(`已清理卡死任务 ${res.jobs} 个、恢复文档 ${res.docs} 个`, 'warn');
        loadJobs();
      } else {
        notify('没有发现卡死任务');
      }
    } catch (e: unknown) { notify('清理失败：' + e, 'error'); }
    finally { housekeepingBusy = false; }
  }

  async function loadJobs() {
    try {
      const res = await kbApi.listJobs(selectedKb, JOB_FETCH_LIMIT);
      jobs = res.items;
      jobTotal = res.total;
    } catch { jobs = []; jobTotal = 0; }
  }
  async function loadHistory() {
    try { history = await kbApi.searchHistory(100); }
    catch { history = []; }
  }
  function switchTab(t: 'jobs' | 'history') {
    tab = t;
    if (t === 'jobs') { loadJobs(); startPoll(); }
    if (t === 'history') { loadHistory(); stopPoll(); }
  }
  function startPoll() {
    if (jobsTimer) clearInterval(jobsTimer);
    jobsTimer = setInterval(() => { if (tab === 'jobs') loadJobs(); }, 3000);
  }
  function stopPoll() { if (jobsTimer) { clearInterval(jobsTimer); jobsTimer = null; } }
  function fmtTime(t: string): string {
    // processing_jobs / search_logs 存的是 UTC（datetime('now')），按 UTC 转本地展示
    return formatIsoTime(t, { showYear: true, utc: true });
  }
    onMount(() => { kbApi.housekeeping().catch(() => {}); loadJobs(); startPoll(); });
  onDestroy(stopPoll);
</script>

<div class="kb-card" style="height:100%;display:flex;flex-direction:column;min-height:0">
  <div class="kb-card-hd" style="justify-content:space-between">
    <div class="kb-seg">
      <button class="kb-seg-item" class:active={tab === 'jobs'} onclick={() => switchTab('jobs')}><KbIcon name="settings" size={14} />处理任务</button>
      <button class="kb-seg-item" class:active={tab === 'history'} onclick={() => switchTab('history')}><KbIcon name="activity" size={14} />检索历史</button>
    </div>
    <div style="display:flex;gap:6px;align-items:center">
      {#if tab === 'jobs'}
        <button class="kb-btn-sm" onclick={stopProcessing} disabled={stopping || ((catCounts.pending ?? 0) + (catCounts.processing ?? 0)) === 0} title="停止所有进行中/待处理的任务（已生成的分片/页面会保留）">
          <KbIcon name="close" size={13} />{stopping ? '停止中…' : '停止处理'}
        </button>
        <button class="kb-btn-sm" onclick={clearFinishedJobs} disabled={clearing} title="删除已完成/失败的任务及其日志（保留排队/执行中）">
          <KbIcon name="trash" size={13} />{clearing ? '清理中…' : '清理完成/失败任务'}
        </button>
        <button class="kb-btn-sm" onclick={clearLogs} disabled={clearing} title="清空全部处理日志">
          <KbIcon name="scroll" size={13} />清理日志
        </button>
        <button class="kb-btn-sm" onclick={runHousekeeping} disabled={housekeepingBusy} title="扫描并恢复超过 10 分钟无进展的任务">
          <KbIcon name="refresh" size={13} />{housekeepingBusy ? '清理中…' : '清理卡死任务'}
        </button>
      {:else}
        <button class="kb-btn-sm" onclick={clearHistory} disabled={clearing} title="清空检索历史">
          <KbIcon name="trash" size={13} />{clearing ? '清理中…' : '清空检索历史'}
        </button>
      {/if}
      <span style="font-size:12px;color:var(--app-color-muted)">{tab === 'jobs' ? `共 ${jobTotal} 条 · 3 秒自动刷新` : '最近 100 条'}</span>
    </div>
  </div>

  <div class="kb-scroll" style="flex:1;overflow:auto;padding:14px">
    {#if tab === 'jobs'}
      {#if jobs.length < jobTotal}
        <div style="font-size:12px;color:var(--kb-warn);margin-bottom:4px">共 {jobTotal} 条任务，当前仅显示最近 {jobs.length} 条（{JOB_FETCH_LIMIT} 条上限）。请使用上方分类筛选，或清理已完成/失败任务以查看更多。</div>
      {/if}
      <!-- 任务分类统计：点击可筛选 -->
      <div style="display:flex;gap:8px;align-items:center;flex-wrap:wrap;margin-bottom:6px">
        {#each CATEGORIES as cat}
          <button class="kb-btn-sm" class:kb-graph-cfg-on={stageFilter === cat.key}
            style="display:inline-flex;align-items:center;gap:6px"
            onclick={() => stageFilter = stageFilter === cat.key ? 'all' : cat.key}>
            {cat.label}
            <span class="kb-badge"
              class:kb-badge-ok={cat.key === 'done'}
              class:kb-badge-warn={cat.key === 'pending' || cat.key === 'processing'}
              class:kb-badge-err={cat.key === 'failed'}
              style="font-size:11px;padding:0 6px;line-height:16px">{catCounts[cat.key] ?? 0}</span>
          </button>
        {/each}
      </div>
      <div style="display:flex;flex-direction:column;gap:10px">
        {#each filteredJobs as j}
          <div class="kb-act-card" style="border:1px solid var(--kb-border);border-radius:10px;padding:10px 12px;background:var(--app-bg-color)">
            <div style="display:flex;align-items:center;gap:8px;margin-bottom:8px;flex-wrap:wrap">
              <span style="flex:1;font-size:13px;color:var(--app-color-text);min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title={j.docTitle}>{j.docTitle}</span>
              <span class="kb-badge" class:kb-badge-ok={j.stage === 'done'} class:kb-badge-err={j.stage === 'failed'} class:kb-badge-warn={j.stage !== 'done' && j.stage !== 'failed'}>{stageLabel[j.stage] ?? j.stage}</span>
            </div>
            <div style="height:6px;border-radius:3px;background:var(--kb-border);overflow:hidden">
              <div style="height:100%;width:{Math.max(0, Math.min(100, j.progress * 100))}%;background:linear-gradient(90deg,var(--kb-btn-bg),var(--kb-accent));border-radius:3px;transition:width .4s"></div>
            </div>
            <div style="display:flex;gap:10px;margin-top:6px;font-size:11.5px;color:var(--app-color-muted);align-items:center;flex-wrap:wrap">
              <span>{fmtTime(j.createdAt)}</span>
              {#if j.error}<span style="color:var(--app-danger)">失败：{j.error}</span>{/if}
              <button class="kb-btn-sm" style="margin-left:auto" onclick={() => openJobLogs(j.id)}><KbIcon name="scroll" size={12} />日志</button>
            </div>
          </div>
        {/each}
        {#if filteredJobs.length === 0}
          <div class="kb-empty"><span class="kb-empty-ico"><KbIcon name="tray" size={22} /></span><span>{jobs.length === 0 ? '暂无处理任务' : '该分类下没有任务'}</span></div>
        {/if}
      </div>
    {:else}
      <div style="display:flex;flex-direction:column;gap:8px">
        {#each history as l}
          <div class="kb-act-card" style="display:flex;align-items:center;gap:10px;padding:9px 12px;border:1px solid var(--kb-border);border-radius:10px;background:var(--app-bg-color)">
            <span style="width:30px;height:30px;border-radius:8px;background:var(--app-bg-color);box-shadow:inset 0 0 0 1px var(--kb-border-strong);color:var(--kb-accent-bright);display:inline-flex;align-items:center;justify-content:center"><KbIcon name="search" size={15} /></span>
            <span style="flex:1;font-size:13px;color:var(--app-color-text);min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title={l.query}>「{l.query}」</span>
  <span class="kb-badge kb-badge-info">{MODE_LABEL[l.mode] ?? l.mode}</span>
            <span class="kb-badge kb-badge-mute">{l.hitCount} 条命中</span>
            <span style="font-size:11.5px;color:var(--app-color-muted)">{fmtTime(l.createdAt)}</span>
          </div>
        {/each}
        {#if history.length === 0}
          <div class="kb-empty"><span class="kb-empty-ico"><KbIcon name="search" size={22} /></span><span>暂无检索记录</span></div>
        {/if}
      </div>
    {/if}
  </div>
</div>

<!-- 任务日志 -->
{#if logsOpen}
  <KbModal open={logsOpen} onClose={() => logsOpen = false} ariaLabel="关闭处理日志弹窗">
    <div class="kb-modal">
      <div class="kb-modal-hd"><KbIcon name="scroll" size={16} color="var(--kb-accent-bright)" />处理日志</div>
      <div class="kb-modal-bd" style="max-height:60vh;overflow:auto">
        {#if logsLoading}
          <div class="kb-empty" style="padding:20px">加载中…</div>
        {:else if logs.length === 0}
          <div class="kb-empty" style="padding:20px"><span class="kb-empty-ico"><KbIcon name="tray" size={22} /></span><span>暂无日志</span></div>
        {:else}
          <div style="display:flex;flex-direction:column;gap:8px">
            {#each logs as l}
              <div style="border:1px solid var(--kb-border);border-radius:8px;padding:8px 10px;font-size:12.5px;line-height:1.6;background:var(--app-bg-color)">
                <div style="display:flex;gap:8px;align-items:center;margin-bottom:3px">
                  <span class="kb-badge" class:kb-badge-err={l.level === 'error'} class:kb-badge-warn={l.level === 'warn'} class:kb-badge-info={l.level === 'info'}>{l.level}</span>
                  <span style="font-size:11.5px;color:var(--app-color-muted)">{fmtTime(l.createdAt)}</span>
                </div>
                <div style="color:var(--app-color-secondary);word-break:break-word;white-space:pre-wrap">{l.message}</div>
                {#if l.detail}<div style="color:var(--app-color-muted);font-size:12px;margin-top:4px;word-break:break-word;white-space:pre-wrap">{l.detail}</div>{/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>
      <div class="kb-modal-ft">
        <button class="kb-btn-md" onclick={() => logsOpen = false}>关闭</button>
      </div>
    </div>
  </KbModal>
{/if}

<style>
  .kb-btn-sm.kb-graph-cfg-on {
    background: var(--kb-hover-strong);
    border-color: var(--kb-accent);
    color: var(--kb-accent-bright);
  }
</style>
