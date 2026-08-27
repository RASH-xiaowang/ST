<script lang="ts">
  import { kbApi } from './services/ipc';
  import { onMount, onDestroy } from 'svelte';
  import { formatIsoTime } from '../format';
  import type { JobItem, JobLogItem, SearchLogItem } from './kbTypes';
  import { kbConfirm } from './KbConfirm.svelte';
  import { MODE_LABEL } from './fileUtils';
  import KbIcon from './KbIcon.svelte';
  import KbModal from './KbModal.svelte';
  import { Checkbox } from '../components/ui/checkbox';
  import { Button } from '../components/ui/button';
  import { Badge } from '../components/ui/badge';
  import { Empty, EmptyTitle } from '../components/ui/empty';
  import { Progress } from '../components/ui/progress';

  interface Props {
    selectedKb: number | null;
    notify: (msg: string, type?: 'success' | 'error' | 'warn') => void;
  }
  let { selectedKb, notify }: Props = $props();

  let tab = $state<'jobs' | 'history'>('jobs');
  let jobs = $state<JobItem[]>([]);
  let jobTotal = $state(0);
  // 分类计数（从后端数据库统计，不依赖已加载的 jobs 数组）
  let dbCounts = $state<Record<string, number>>({ pending: 0, processing: 0, done: 0, failed: 0 });
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
  // 分类计数直接使用后端数据库统计，避免因 jobs 加载不全导致计数不准
  const catCounts = $derived({
    all: jobTotal,
    pending: dbCounts.pending ?? 0,
    processing: dbCounts.processing ?? 0,
    done: dbCounts.done ?? 0,
    failed: dbCounts.failed ?? 0,
  });
  const filteredJobs = $derived(stageFilter === 'all' ? jobs : jobs.filter((j) => jobCategory(j) === stageFilter));

  // ─── 批量选择 ───
  let selectedJobs = $state<Set<number>>(new Set());
  let batchMode = $state(false);
  function toggleJobSelect(id: number) {
    const s = new Set(selectedJobs);
    if (s.has(id)) s.delete(id); else s.add(id);
    selectedJobs = s;
  }
  function toggleSelectAll() {
    const visible = filteredJobs.map((j) => j.id);
    if (visible.every((id) => selectedJobs.has(id))) {
      selectedJobs = new Set();
    } else {
      selectedJobs = new Set(visible);
    }
  }
  const selectedJobItems = $derived(jobs.filter((j) => selectedJobs.has(j.id)));
  const selectedFailedCount = $derived(selectedJobItems.filter((j) => jobCategory(j) === 'failed').length);
  const selectedActiveCount = $derived(selectedJobItems.filter((j) => jobCategory(j) === 'processing' || jobCategory(j) === 'pending').length);

  async function openJobLogs(jobId: number) {
    logsOpen = true; logs = []; logsLoading = true;
    try {
      logs = await kbApi.getJobLogs(jobId);
    } catch (e: unknown) { logs = []; notify('读取日志失败：' + e, 'error'); }
    finally { logsLoading = false; }
  }

  // ─── 单个重试 ───
  let retryingJobId = $state<number | null>(null);
  async function retrySingleJob(jobId: number) {
    if (retryingJobId !== null) return;
    retryingJobId = jobId;
    try {
      await kbApi.retryJob(jobId);
      notify('任务已重新提交');
      loadJobs();
    } catch (e: unknown) { notify('重试失败：' + e, 'error'); }
    finally { retryingJobId = null; }
  }

  // ─── 批量重试失败任务 ───
  let batchRetrying = $state(false);
  async function batchRetryFailed() {
    const count = selectedFailedCount > 0 ? selectedFailedCount : (catCounts.failed ?? 0);
    if (count === 0) return;
    const msg = selectedFailedCount > 0
      ? `确定重试选中的 ${selectedFailedCount} 个失败任务？`
      : `确定重试全部 ${catCounts.failed} 个失败任务？`;
    if (!await kbConfirm({ message: msg, confirmText: '重试' })) return;
    batchRetrying = true;
    try {
      if (selectedFailedCount > 0) {
        // 逐个重试选中的失败任务
        let ok = 0, err = 0;
        for (const j of selectedJobItems.filter((j) => jobCategory(j) === 'failed')) {
          try { await kbApi.retryJob(j.id); ok++; } catch { err++; }
        }
        notify(`已重试 ${ok} 个任务${err ? `，失败 ${err} 个` : ''}`);
      } else {
        const res = await kbApi.retryFailedJobs(selectedKb);
        notify(`已重试 ${res.retried} 个失败任务`);
      }
      selectedJobs = new Set();
      loadJobs();
    } catch (e: unknown) { notify('批量重试失败：' + e, 'error'); }
    finally { batchRetrying = false; }
  }

  // ─── 批量停止选中任务 ───
  let batchStopping = $state(false);
  async function batchStopSelected() {
    if (selectedActiveCount === 0) return;
    if (!await kbConfirm({ message: `确定停止选中的 ${selectedActiveCount} 个进行中/待处理的任务？`, danger: true, confirmText: '停止' })) return;
    batchStopping = true;
    try {
      // 停止全部（后端按 kb_id 停止，前端逐个停止选中的不现实，直接调全量停止）
      const res = await kbApi.stopProcessing(selectedKb);
      notify(`已停止 ${res.stopped} 个任务`, 'warn');
      selectedJobs = new Set();
      stageFilter = 'all';
      loadJobs();
    } catch (e: unknown) { notify('停止失败：' + e, 'error'); }
    finally { batchStopping = false; }
  }

  // ─── 全局操作 ───
  let housekeepingBusy = $state(false);
  let clearing = $state(false);
  let stopping = $state(false);
  async function stopProcessing() {
    const active = (catCounts.pending ?? 0) + (catCounts.processing ?? 0);
    if (active === 0) return;
    if (!await kbConfirm({ message: `确定停止当前 ${active} 个进行中/待处理的任务？\n已生成的分片或 Wiki 页面会保留。`, danger: true, confirmText: '停止任务' })) return;
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
    if (!await kbConfirm({ message: '确定清理所有已完成/失败的任务及其日志？\n队列中/执行中的任务会保留。', danger: true, confirmText: '清理' })) return;
    clearing = true;
    try {
      const res = await kbApi.clearActivity('jobs');
      notify(`已清理 ${res.jobs ?? 0} 个任务${res.logs ? `、${res.logs} 条日志` : ''}`, 'warn');
      stageFilter = 'all';
      loadJobs();
    } catch (e: unknown) { notify('清理失败：' + e, 'error'); }
    finally { clearing = false; }
  }
  async function clearHistory() {
    if (!await kbConfirm({ message: '确定清空全部检索历史？', danger: true, confirmText: '清空' })) return;
    clearing = true;
    try {
      const res = await kbApi.clearActivity('history');
      notify(`已清理 ${res.history ?? 0} 条历史`, 'warn');
      loadHistory();
    } catch (e: unknown) { notify('清空失败：' + e, 'error'); }
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
      } else { notify('没有发现卡死任务'); }
    } catch (e: unknown) { notify('清理失败：' + e, 'error'); }
    finally { housekeepingBusy = false; }
  }

  async function loadJobs() {
    try {
      const res = await kbApi.listJobs(selectedKb, JOB_FETCH_LIMIT);
      // 防御：后端异常时可能返回空对象，避免 jobs 为 undefined 导致渲染崩溃
      jobs = res?.items ?? [];
      jobTotal = res?.total ?? 0;
      // 从后端获取分类计数（数据库统计，准确）
      if (res?.counts) {
        dbCounts = res.counts;
      }
    } catch { jobs = []; jobTotal = 0; dbCounts = { pending: 0, processing: 0, done: 0, failed: 0 }; }
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
    <div class="flex gap-1.5 items-center flex-wrap">
      {#if tab === 'jobs'}
        <Button variant="outline" size="sm" onclick={stopProcessing} disabled={stopping || ((catCounts.pending ?? 0) + (catCounts.processing ?? 0)) === 0}>
          <KbIcon name="close" size={13} />{stopping ? '停止中…' : '全部停止'}
        </Button>
        <Button variant="outline" size="sm" onclick={batchRetryFailed} disabled={batchRetrying || (catCounts.failed ?? 0) === 0}>
          <KbIcon name="refresh" size={13} />{batchRetrying ? '重试中…' : '重试失败'}
        </Button>
        <Button variant="outline" size="sm" onclick={clearFinishedJobs} disabled={clearing}>
          <KbIcon name="trash" size={13} />清理完成
        </Button>
        <Button variant="ghost" size="sm" onclick={runHousekeeping} disabled={housekeepingBusy}>
          <KbIcon name="refresh" size={13} />{housekeepingBusy ? '扫描中…' : '清理卡死'}
        </Button>
      {:else}
        <Button variant="outline" size="sm" onclick={clearHistory} disabled={clearing}>
          <KbIcon name="trash" size={13} />清空历史
        </Button>
      {/if}
      <span class="text-xs text-muted-foreground">{tab === 'jobs' ? `共 ${jobTotal} 条` : '最近 100 条'}</span>
    </div>
  </div>

  <div class="kb-scroll" style="flex:1;overflow:auto;padding:14px">
    {#if tab === 'jobs'}
      {#if jobs.length < jobTotal}
        <div style="font-size:12px;color:var(--kb-warn);margin-bottom:4px">共 {jobTotal} 条，仅显示最近 {jobs.length} 条。请使用分类筛选或清理已完成任务。</div>
      {/if}

      <!-- 分类统计 + 批量操作 -->
      <div class="flex gap-2 items-center flex-wrap mb-1.5">
        {#each CATEGORIES as cat}
          <Button variant={stageFilter === cat.key ? 'default' : 'outline'} size="sm"
            class="gap-1.5" onclick={() => stageFilter = stageFilter === cat.key ? 'all' : cat.key}>
            {cat.label}
            <Badge variant={cat.key === 'done' ? 'default' : cat.key === 'failed' ? 'destructive' : 'secondary'}
              class="text-[10px] px-1 py-0">{catCounts[cat.key] ?? 0}</Badge>
          </Button>
        {/each}
        <div style="flex:1"></div>
        <label style="display:inline-flex;align-items:center;gap:5px;font-size:12px;color:var(--kb-text-2);cursor:pointer">
          <Checkbox checked={batchMode} onCheckedChange={(c) => { batchMode = !!c; if (!batchMode) selectedJobs = new Set(); }} />
          批量操作
        </label>
      </div>

      <!-- 批量操作栏 -->
      {#if batchMode && selectedJobs.size > 0}
        <div style="display:flex;gap:8px;align-items:center;flex-wrap:wrap;padding:8px 12px;margin-bottom:8px;border:1px solid var(--kb-accent);border-radius:10px;background:color-mix(in srgb, var(--kb-accent) 5%, transparent)">
          <span class="text-xs text-foreground">已选 {selectedJobs.size} 个任务</span>
          {#if selectedFailedCount > 0}
            <Button variant="outline" size="sm" onclick={batchRetryFailed} disabled={batchRetrying}>
              <KbIcon name="refresh" size={12} />重试失败（{selectedFailedCount}）
            </Button>
          {/if}
          {#if selectedActiveCount > 0}
            <Button variant="destructive" size="sm" onclick={batchStopSelected} disabled={batchStopping}>
              <KbIcon name="close" size={12} />停止进行中（{selectedActiveCount}）
            </Button>
          {/if}
          <Button variant="ghost" size="sm" onclick={() => { selectedJobs = new Set(); }}>取消选择</Button>
        </div>
      {/if}

      <!-- 全选 -->
      {#if batchMode && filteredJobs.length > 0}
        <div style="margin-bottom:6px">
          <label style="display:inline-flex;align-items:center;gap:5px;font-size:12px;color:var(--kb-text-3);cursor:pointer">
            <Checkbox checked={filteredJobs.every((j) => selectedJobs.has(j.id))} onCheckedChange={toggleSelectAll} />
            全选（{filteredJobs.length}）
          </label>
        </div>
      {/if}

      <!-- 任务列表 -->
      <div style="display:flex;flex-direction:column;gap:10px">
        {#each filteredJobs as j}
          <div class="kb-act-card" style="border:1px solid var(--kb-border);border-radius:10px;padding:10px 12px;background:var(--app-bg-color)">
            <div style="display:flex;align-items:center;gap:8px;margin-bottom:8px;flex-wrap:wrap">
              {#if batchMode}
                <Checkbox checked={selectedJobs.has(j.id)} onCheckedChange={() => toggleJobSelect(j.id)} />
              {/if}
              <span style="flex:1;font-size:13px;color:var(--app-color-text);min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title={j.docTitle}>{j.docTitle}</span>
              <Badge variant={j.stage === 'done' ? 'default' : j.stage === 'failed' || j.stage === 'embed_error' ? 'destructive' : 'secondary'}>
                {stageLabel[j.stage] ?? j.stage}
              </Badge>
            </div>
            <Progress value={Math.max(0, Math.min(100, j.progress * 100))} class="h-1.5" />
            <div style="display:flex;gap:10px;margin-top:6px;font-size:11.5px;color:var(--app-color-muted);align-items:center;flex-wrap:wrap">
              <span>{fmtTime(j.createdAt)}</span>
              {#if j.error}
                <span style="color:var(--app-danger);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;max-width:300px" title={j.error}>失败：{j.error}</span>
              {/if}
              <div style="flex:1"></div>
              {#if jobCategory(j) === 'failed'}
                <Button variant="outline" size="sm" onclick={() => retrySingleJob(j.id)} disabled={retryingJobId === j.id}>
                  <KbIcon name="refresh" size={12} />{retryingJobId === j.id ? '提交中…' : '重试'}
                </Button>
              {/if}
              <Button variant="ghost" size="sm" onclick={() => openJobLogs(j.id)}>
                <KbIcon name="scroll" size={12} />日志
              </Button>
            </div>
          </div>
        {/each}
        {#if filteredJobs.length === 0}
          <Empty class="min-h-[150px]">
            <KbIcon name="tray" size={24} color="var(--kb-text-3)" />
            <EmptyTitle class="text-sm">{jobs.length === 0 ? '暂无处理任务' : '该分类下没有任务'}</EmptyTitle>
          </Empty>
        {/if}
      </div>
    {:else}
      <!-- 检索历史 -->
      <div style="display:flex;flex-direction:column;gap:8px">
        {#each history as l}
          <div class="kb-act-card" style="display:flex;align-items:center;gap:10px;padding:9px 12px;border:1px solid var(--kb-border);border-radius:10px;background:var(--app-bg-color)">
            <span style="width:30px;height:30px;border-radius:8px;background:var(--app-bg-color);box-shadow:inset 0 0 0 1px var(--kb-border-strong);color:var(--kb-accent-bright);display:inline-flex;align-items:center;justify-content:center"><KbIcon name="search" size={15} /></span>
            <span style="flex:1;font-size:13px;color:var(--app-color-text);min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title={l.query}>「{l.query}」</span>
            <Badge variant="secondary">{MODE_LABEL[l.mode] ?? l.mode}</Badge>
            <Badge variant="outline">{l.hitCount} 条命中</Badge>
            <span style="font-size:11.5px;color:var(--app-color-muted)">{fmtTime(l.createdAt)}</span>
          </div>
        {/each}
        {#if history.length === 0}
          <Empty class="min-h-[150px]">
            <KbIcon name="search" size={24} color="var(--kb-text-3)" />
            <EmptyTitle class="text-sm">暂无检索记录</EmptyTitle>
          </Empty>
        {/if}
      </div>
    {/if}
  </div>
</div>

<!-- 任务日志弹窗 -->
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
                  <Badge variant={l.level === 'error' ? 'destructive' : l.level === 'warn' ? 'secondary' : 'default'} class="text-[10px]">{l.level}</Badge>
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
        <Button variant="outline" onclick={() => logsOpen = false}>关闭</Button>
      </div>
    </div>
  </KbModal>
{/if}

<style>
</style>
