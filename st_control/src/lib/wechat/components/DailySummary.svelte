<script lang="ts">
  import { errText } from '../../format';
  import { onMount } from 'svelte';
  import { onLlmConfigChanged } from '../../llm/store.svelte';
  import { llmApi } from '../../llm/services/ipc';
  import { fmtDate, fmtDuration, fmtTime, fmtTokens } from '../utils/summary';
  import { createMsg } from '../../services/msg.svelte';
  import { copyText } from '../../clipboard';
  import type { DailySummaryRecord, DailySummaryTask, ProviderOption, SessionEntry } from '../types';
  import { summarizeRecords } from '../utils/summary';
    import WechatHoverButton from './WechatHoverButton.svelte';
  import {
    deleteDailySummaryRecord,
    deleteDailySummaryTask,
    getDailySummaryFormats,
    getGroupMembers,
    getSessionList,
    listDailySummaryRecords,
    listDailySummaryTasks,
    runDailySummaryRange,
    runDailySummaryTask,
    saveDailySummaryTask,
    toggleDailySummaryTask,
  } from '../services/ipc';
  import DailySummaryForm from './DailySummaryForm.svelte';

  // ── 数据 ──
  let tasks = $state<DailySummaryTask[]>([]);
  let groups = $state<SessionEntry[]>([]);
  let members = $state<{ username: string; name: string }[]>([]);
  let formats = $state<{ key: string; label: string }[]>([]);
  let providers = $state<ProviderOption[]>([]);
  let records = $state<DailySummaryRecord[]>([]);

  let loading = $state(true);
  let loadingRecords = $state(false);
  let running = $state(false);
  const msg = createMsg(3500);
  let selectedTaskId = $state<number | null>(null);
  let expandedRecord = $state<number | null>(null);
  let recFilter = $state<'all' | 'done' | 'error'>('all');
  let view = $state<'empty' | 'new' | 'edit'>('empty');
  let tab = $state<'setup' | 'records'>('setup');
  let rangeStart = $state('');
  let rangeEnd = $state('');
  let rangeRunning = $state(false);
  let connTest = $state<{ running: boolean; result: string; ok: boolean }>({ running: false, result: '', ok: true });
  /** 后端连接异常信息（非空时顶部显示红色横幅） */
  let connError = $state('');
  /** 大模型配置变更订阅的取消函数 */
  let unsubLlmProviders: (() => void) | null = null;

  // ── 编辑表单 ──
  let form = $state({
    id: 0,
    group_username: '',
    group_name: '',
    target_users: [] as string[],
    target_all: true,
    provider_id: '',
    model: '',
    format: 'brief',
    custom_prompt: '',
    schedule_time: '08:00',
    enabled: true,
  });

  /** 记录筛选与统计 */
  let filteredRecords = $derived(recFilter === 'all' ? records : records.filter((r) => r.status === recFilter));
  let recordStats = $derived(summarizeRecords(records));

  function initRangeDates() {
    const end = new Date();
    end.setDate(end.getDate() - 1); // 昨天
    const start = new Date();
    start.setDate(start.getDate() - 7); // 7 天前
    rangeStart = fmtDate(start);
    rangeEnd = fmtDate(end);
  }

  function providerLabel(id: string | undefined): string {
    return providers.find((p) => p.id === id)?.name || id || '未配置';
  }

  function formatLabel(key: string | undefined): string {
    return formats.find((f) => f.key === key)?.label || key || '';
  }

  function targetLabel(task: DailySummaryTask): string {
    const users: string[] = task?.target_users ?? [];
    if (!users.length) return '全部成员';
    const names = members.filter((m) => users.includes(m.username)).map((m) => m.name);
    if (names.length <= 2) return names.join('、') || users.join('、');
    return `${names.slice(0, 2).join('、')} 等 ${users.length} 人`;
  }

  // ── 加载 ──
  async function loadAll() {
    loading = true;
    let anyOk = false;
    try {
      const [t, f, cfg] = await Promise.all([
        listDailySummaryTasks(),
        getDailySummaryFormats(),
        llmApi.getConfig().catch(() => null),
      ]);
      anyOk = true;
      tasks = Array.isArray(t) ? t : [];
      formats = Array.isArray(f?.formats) ? f.formats : [];
      providers = (cfg?.providers ?? []).map((p: { id: string; name: string; models?: string[]; default_model?: string }) => ({
        id: p.id,
        name: p.name,
        models: p.models ?? [],
        default_model: p.default_model ?? '',
      }));
      const sessions = await getSessionList().catch(() => []);
      groups = (sessions ?? []).filter((s) => (s.username ?? '').includes('@chatroom'));
    } catch (e: unknown) {
      const text = errText(e);
      // 常见原因：后端版本过旧，命令未注册
      connError = text.includes('not found') || text.includes('unknown') || text.includes('未找到')
        ? '后端未加载每日总结命令，请重新构建并重启应用'
        : text;
      msg.show(connError, false);
    } finally {
      loading = false;
      if (anyOk) connError = '';
    }
  }

  /** 大模型配置变更后仅刷新提供方/模型列表，避免整页重载 */
  async function reloadProviders() {
    try {
      const cfg = await llmApi.getConfig().catch(() => null);
      providers = (cfg?.providers ?? []).map((p: { id: string; name: string; models?: string[]; default_model?: string }) => ({
        id: p.id,
        name: p.name,
        models: p.models ?? [],
        default_model: p.default_model ?? '',
      }));
    } catch {
      providers = [];
    }
    // 当前表单所选提供方已被删除/失效时回退到第一个可用项
    if (form.provider_id && !providers.some((p) => p.id === form.provider_id)) {
      form.provider_id = providers[0]?.id ?? '';
      form.model = providers[0]?.default_model ?? '';
    }
  }

  async function loadMembers(groupUsername: string) {
    try {
      const r = await getGroupMembers(groupUsername);
      members = r?.members ?? [];
    } catch (e: unknown) {
      members = [];
      msg.show(`加载群成员失败：${errText(e)}`, false);
    }
  }

  async function loadRecords(taskId: number) {
    loadingRecords = true;
    try {
      const r = await listDailySummaryRecords(taskId);
      records = Array.isArray(r) ? r : [];
    } catch (e: unknown) {
      records = [];
      msg.show(`加载总结记录失败：${errText(e)}`, false);
    } finally {
      loadingRecords = false;
    }
  }

  // ── 表单操作 ──
  function resetForm() {
    form = {
      id: 0,
      group_username: '',
      group_name: '',
      target_users: [],
      target_all: true,
      provider_id: providers[0]?.id ?? '',
      model: providers[0]?.default_model ?? '',
      format: 'brief',
      custom_prompt: '',
      schedule_time: '08:00',
      enabled: true,
    };
    members = [];
    selectedTaskId = null;
    expandedRecord = null;
    tab = 'setup';
    view = 'new';
    scrollMainTop();
  }

  function editTask(task: DailySummaryTask) {
    const users: string[] = task?.target_users ?? [];
    form = {
      id: task?.id ?? 0,
      group_username: task?.group_username ?? '',
      group_name: task?.group_name ?? '',
      target_users: users,
      target_all: users.length === 0,
      provider_id: task?.provider_id ?? providers[0]?.id ?? '',
      model: task?.model ?? '',
      format: task?.format ?? 'brief',
      custom_prompt: task?.custom_prompt ?? '',
      schedule_time: task?.schedule_time ?? '08:00',
      enabled: !!task?.enabled,
    };
    selectedTaskId = task?.id ?? null;
    expandedRecord = null;
    tab = 'setup';
    view = 'edit';
    if (form.group_username) loadMembers(form.group_username);
    loadRecords(form.id);
    scrollMainTop();
  }

  function scrollMainTop() {
    setTimeout(() => {
      mainEl?.scrollTo?.({ top: 0, behavior: 'smooth' });
    }, 30);
  }

  let mainEl = $state<HTMLElement | null>(null);

  async function selectGroup(username: string) {
    const g = groups.find((x) => x.username === username);
    form.group_username = username;
    form.group_name = g?.name || username;
    form.target_users = [];
    form.target_all = true;
    await loadMembers(username);
  }

  function toggleTarget(username: string) {
    if (form.target_all) form.target_all = false;
    const idx = form.target_users.indexOf(username);
    if (idx >= 0) form.target_users.splice(idx, 1);
    else form.target_users.push(username);
    if (!form.target_users.length) form.target_all = true;
  }

  function onProviderChange() {
    const p = providers.find((x) => x.id === form.provider_id);
    form.model = p?.default_model ?? '';
  }

  async function saveTask() {
    if (!form.group_username) { msg.show('请先选择群聊', false); return; }
    if (!form.provider_id) { msg.show('请选择模型提供方（在「大模型」里配置）', false); return; }
    if (!form.model) { msg.show('请选择模型', false); return; }
    try {
      const saved = await saveDailySummaryTask({
        task: {
          id: form.id || undefined,
          group_username: form.group_username,
          group_name: form.group_name,
          target_users: form.target_all ? [] : form.target_users,
          provider_id: form.provider_id,
          model: form.model,
          format: form.format,
          custom_prompt: form.custom_prompt,
          schedule_time: form.schedule_time,
          enabled: form.enabled,
        },
      });
      msg.show(form.id ? '任务已更新' : '任务已创建', true);
      await loadAll();
      editTask(saved);
    } catch (e: unknown) {
      msg.show(errText(e), false);
    }
  }

  async function deleteTask(id: number) {
    if (!confirm('确认删除该任务及其全部总结记录？')) return;
    try {
    await deleteDailySummaryTask(id);
      tasks = tasks.filter((t) => t.id !== id);
      if (selectedTaskId === id) {
        selectedTaskId = null;
        records = [];
        view = 'empty';
      }
      msg.show('任务已删除', true);
    } catch (e: unknown) {
      msg.show(errText(e), false);
    }
  }

  async function toggleTask(t: DailySummaryTask) {
    if (t.id == null) return;
    const next = !t.enabled;
    try {
      await toggleDailySummaryTask(t.id, next);
      t.enabled = next;
      msg.show(next ? '已启用定时总结' : '已暂停定时总结', true);
    } catch (e: unknown) {
      msg.show(errText(e), false);
    }
  }

  async function runTask(id: number | undefined) {
    if (running || id == null) return;
    running = true;
    try {
      await runDailySummaryTask(id);
      msg.show('总结已生成', true);
      await loadRecords(id);
      const t = tasks.find((x) => x.id === id);
      if (t) { t.last_status = 'success'; t.last_error = ''; t.last_run_at = Date.now(); }
      if (records.length) expandedRecord = records[0]?.id ?? null;
      tab = 'records';
    } catch (e: unknown) {
      msg.show(errText(e), false);
      const t = tasks.find((x) => x.id === id);
      if (t) { t.last_status = 'error'; t.last_error = errText(e); }
    } finally {
      running = false;
    }
  }

  async function runRangeTask() {
    if (rangeRunning || !selectedTaskId) return;
    if (!rangeStart || !rangeEnd) { msg.show('请选择开始和结束日期', false); return; }
    if (rangeEnd < rangeStart) { msg.show('结束日期不能早于开始日期', false); return; }
    rangeRunning = true;
    try {
      await runDailySummaryRange({
        taskId: selectedTaskId,
        startDate: rangeStart,
        endDate: rangeEnd,
      });
      msg.show('总结已生成', true);
      await loadRecords(selectedTaskId);
      if (records.length) expandedRecord = records[0]?.id ?? null;
      tab = 'records';
    } catch (e: unknown) {
      msg.show(errText(e), false);
    } finally {
      rangeRunning = false;
    }
  }

  async function testConnection() {
    if (connTest.running) return;
    if (!form.provider_id) { msg.show('请先选择模型提供方', false); return; }
    connTest = { running: true, result: '', ok: true };
    try {
      const r = await llmApi.testConnection(form.provider_id);
      connTest = {
        running: false,
        ok: !!r?.ok,
        result: r?.ok
          ? `连接正常（${r.latency_ms} ms，模型：${r.model || form.model || '默认'}）`
          : (r?.error ?? '连接失败'),
      };
    } catch (e: unknown) {
      connTest = { running: false, ok: false, result: errText(e) };
    }
  }

  async function deleteRecord(id: number | undefined) {
    if (id == null) return;
    try {
      await deleteDailySummaryRecord(id);
      records = records.filter((r) => r.id !== id);
      if (expandedRecord === id) expandedRecord = null;
      msg.show('记录已删除', true);
    } catch (e: unknown) {
      msg.show(errText(e), false);
    }
  }

  /** 复制总结文本到剪贴板 */
  async function copyRecord(r: DailySummaryRecord) {
    const ok = await copyText(r?.summary || '');
    msg.show(ok ? '总结已复制' : '复制失败', ok);
  }

  /** 以当前任务为模板载入新建表单（保存后生成副本） */
  function duplicateTask() {
    const t = tasks.find((x) => x.id === selectedTaskId);
    if (!t) return;
    form = {
      id: 0,
      group_username: t.group_username ?? '',
      group_name: t.group_name ?? '',
      target_users: [...(t.target_users ?? [])],
      target_all: (t.target_users ?? []).length === 0,
      provider_id: t.provider_id ?? '',
      model: t.model ?? '',
      format: t.format ?? 'brief',
      custom_prompt: t.custom_prompt ?? '',
      schedule_time: t.schedule_time ?? '08:00',
      enabled: !!t.enabled,
    };
    selectedTaskId = null;
    expandedRecord = null;
    records = [];
    view = 'new';
    tab = 'setup';
    if (form.group_username) loadMembers(form.group_username);
    scrollMainTop();
    msg.show('已载入为新任务模板，保存后创建副本', true);
  }

  onMount(() => {
    initRangeDates();
    loadAll();
    // 大模型管理配置变化时实时同步提供方/模型选项（无需人工刷新）
    unsubLlmProviders = onLlmConfigChanged(() => { reloadProviders(); });
    return () => { unsubLlmProviders?.(); };
  });
</script>

<div class="ds-root">
  <header class="ds-hd">
    <div class="ds-brand">
      <h2 class="ds-title">每日总结</h2>
      <span class="ds-sub">定时把群聊中指定成员的聊天记录交给模型分析，结果汇总保存</span>
    </div>
    <div class="ds-hd-right">
      {#if msg}
        <div class="ds-toast" class:ds-toast-err={!msg.state.ok}>{msg.state.text}</div>
      {/if}
      <span class="ds-conn" class:ds-conn-bad={!!connError} title={connError || '后端已连接'}>
        <i></i>{connError ? '后端未连接' : '已连接'}
      </span>
    </div>
  </header>

  {#if connError}
    <div class="ds-conn-banner">
                  <span>⚠ {connError}。前端与后端版本不一致时会出现此提示。</span>
        <WechatHoverButton text="重试" onclick={loadAll} class="!px-3 !py-1 !text-xs" />
    </div>
  {/if}

  {#if loading}
    <div class="ds-loading"><span class="ds-spinner"></span> 加载中…</div>
  {:else}
    <div class="ds-body">
      <!-- 左侧：任务列表 -->
      <aside class="ds-side">
        <div class="ds-side-hd">
          <span>任务列表{tasks.length ? ` · ${tasks.length}` : ''}</span>
            <WechatHoverButton text="＋ 新建" onclick={resetForm} class="!px-3 !py-1 !text-xs" />
        </div>
        <div class="ds-task-list">
          {#if tasks.length === 0}
            <div class="ds-empty-side">还没有任务<br />点「新建」创建第一个每日总结</div>
          {:else}
            {#each tasks as t (t.id)}
              <div class="ds-task" class:ds-task-on={view === 'edit' && selectedTaskId === t.id}
                role="button" tabindex="0"
                onclick={() => editTask(t)}
                onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); editTask(t); } }}>
                <div class="ds-task-top">
                  <span class="ds-task-name" title={t.group_name}>{t.group_name || t.group_username}</span>
                  <span class="ds-task-status" class:ds-task-off={!t.enabled} title={t.enabled ? '定时总结已启用' : '定时总结已暂停'}>
                    {t.enabled ? '运行中' : '已暂停'}
                  </span>
                </div>
                <div class="ds-task-meta">
                  <span>{targetLabel(t)}</span>
                  <span>{t.schedule_time}</span>
                </div>
                <div class="ds-task-sub">
                  {providerLabel(t.provider_id)} · {t.model || '默认模型'} · {formatLabel(t.format)}
                </div>
                {#if t.last_status === 'error'}
                  <div class="ds-task-err" title={t.last_error}>最近一次失败：{t.last_error}</div>
                {:else if t.last_run_at}
                  <div class="ds-task-last">上次：{fmtTime(t.last_run_at)}</div>
                {/if}
                <div class="ds-task-actions">
                    <WechatHoverButton text="运行" onclick={(e) => { e.stopPropagation(); runTask(t.id); }} disabled={running} title="立即执行一次总结" class="!px-3 !py-1 !text-xs" />
                    <WechatHoverButton text={t.enabled ? '暂停' : '启用'} onclick={(e) => { e.stopPropagation(); toggleTask(t); }} title={t.enabled ? '暂停定时总结' : '启用定时总结'} class="!px-3 !py-1 !text-xs" />
                </div>
              </div>
            {/each}
          {/if}
        </div>
      </aside>

      <!-- 右侧：设置 / 记录 -->
      <main class="ds-main" bind:this={mainEl}>
        {#if view === 'new'}
          <div class="ds-main-hd">
            <h3 class="ds-card-title">新建每日总结任务</h3>
              <WechatHoverButton text="保存任务" onclick={saveTask} />
          </div>
          <DailySummaryForm
            bind:form {groups} {members} {formats}
            {selectGroup} {toggleTarget} providerChange={onProviderChange}
          />
        {:else if view === 'edit' && selectedTaskId}
          <div class="ds-main-hd">
            <div class="ds-tabs" role="tablist">
                <WechatHoverButton text="任务设置" onclick={() => tab = 'setup'} class={tab === 'setup' ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'} />
                <WechatHoverButton text="总结记录" onclick={() => tab = 'records'} class={tab === 'records' ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'} />
            </div>
            <div class="ds-hd-actions">
                <WechatHoverButton text={running ? '总结中…' : '立即运行'} onclick={() => selectedTaskId !== null && runTask(selectedTaskId)} disabled={running || !form.model || !form.provider_id} />
                <WechatHoverButton text="复制为新任务" onclick={duplicateTask} title="以当前任务设置载入新建表单" class="!px-3 !py-1 !text-xs" />
                <WechatHoverButton text="删除" onclick={() => selectedTaskId !== null && deleteTask(selectedTaskId)} class="!px-3 !py-1 !text-xs" />
            </div>
          </div>

          {#if tab === 'setup'}
            <div class="ds-card">
              <DailySummaryForm
            bind:form {groups} {members} {formats}
                {selectGroup} {toggleTarget} providerChange={onProviderChange}
              />
              <div class="ds-actions">
                  <WechatHoverButton text="保存修改" onclick={saveTask} />
              </div>
            </div>
            <div class="ds-card ds-range-card">
              <div class="ds-card-hd">
                <h3 class="ds-card-title">历史总结（自定义日期范围）</h3>
              </div>
              <p class="ds-range-tip">选择起止日期后立即总结该时段内关注成员的聊天记录，结果会保存到下方「总结记录」。</p>
              <div class="ds-range-row">
                <label class="ds-range-field">
                  <span class="ds-label">开始日期</span>
                  <input type="date" class="ds-select" bind:value={rangeStart} max={rangeEnd || undefined} />
                </label>
                <span class="ds-range-sep">至</span>
                <label class="ds-range-field">
                  <span class="ds-label">结束日期</span>
                  <input type="date" class="ds-select" bind:value={rangeEnd} min={rangeStart || undefined} />
                </label>
                  <WechatHoverButton text={rangeRunning ? '总结中…' : '立即总结'} onclick={runRangeTask} disabled={rangeRunning || !form.model || !form.provider_id} />
              </div>
            </div>
            <div class="ds-card ds-test-card">
              <div class="ds-card-hd">
                <h3 class="ds-card-title">模型连接测试</h3>
                  <WechatHoverButton text={connTest.running ? '测试中…' : '测试连接'} onclick={testConnection} disabled={connTest.running || !form.provider_id} class="!px-3 !py-1 !text-xs" />
              </div>
              {#if connTest.result}
                <div class="ds-test-result" class:ds-test-err={!connTest.ok}>{connTest.result}</div>
              {:else}
                <p class="ds-range-tip">用当前任务的提供方/模型向接口发一次极小请求，用于排查网络与配置问题。</p>
              {/if}
            </div>
          {:else}
            <div class="ds-card ds-records">
              <div class="ds-card-hd">
                <h3 class="ds-card-title">总结记录</h3>
                {#if loadingRecords}<span class="ds-records-loading">加载中…</span>{/if}
              </div>
              {#if records.length === 0}
                <div class="ds-empty-records">
                  还没有生成记录。点「立即运行」，或等定时时间自动生成。
                </div>
              {:else}
                <div class="ds-rec-stats">
                  <div class="ds-rec-stat"><span class="ds-rec-stat-val">{recordStats.total}</span><span class="ds-rec-stat-label">总记录</span></div>
                  <div class="ds-rec-stat"><span class="ds-rec-stat-val ds-rec-stat-ok">{recordStats.ok}</span><span class="ds-rec-stat-label">成功</span></div>
                  <div class="ds-rec-stat"><span class="ds-rec-stat-val ds-rec-stat-fail">{recordStats.fail}</span><span class="ds-rec-stat-label">失败</span></div>
                  <div class="ds-rec-stat"><span class="ds-rec-stat-val">{recordStats.avgChars}</span><span class="ds-rec-stat-label">平均字数</span></div>
                </div>
                <div class="ds-filter">
                    <WechatHoverButton text={`全部 (${records.length})`} onclick={() => recFilter = 'all'} class={recFilter === 'all' ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'} />
                    <WechatHoverButton text={`成功 (${recordStats.ok})`} onclick={() => recFilter = 'done'} class={recFilter === 'done' ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'} />
                    <WechatHoverButton text={`失败 (${recordStats.fail})`} onclick={() => recFilter = 'error'} class={recFilter === 'error' ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'} />
                </div>
                {#if filteredRecords.length === 0}
                  <div class="ds-empty-records">没有符合筛选条件的记录</div>
                {:else}
                  <div class="ds-record-list">
                    {#each filteredRecords as r (r.id)}
                      <div class="ds-record" class:ds-record-open={expandedRecord === r.id}>
                        <button class="ds-record-hd" onclick={() => expandedRecord = expandedRecord === (r.id ?? null) ? null : (r.id ?? null)}>
                          <span class="ds-record-date">{r.summary_date}</span>
                          <span class="ds-badge" class:ds-badge-err={r.status !== 'done'}>{r.status === 'done' ? '成功' : '失败'}</span>
                          <span class="ds-record-meta">
                            {r.message_count} 条消息 · {r.char_count} 字 · {providerLabel(r.provider_id)} / {r.model}
                          </span>
                          <span class="ds-record-toggle">{expandedRecord === r.id ? '收起' : '展开'}</span>
                        </button>
                        {#if expandedRecord === r.id}
                          <div class="ds-record-body">
                            {#if r.status === 'error'}
                              <div class="ds-record-err">{r.error || '生成失败'}</div>
                            {:else}
                              <p class="ds-record-text">{r.summary || '（空）'}</p>
                              {#if r.message_sample}
                                <details class="ds-sample">
                                  <summary>查看输入的聊天片段（前 {r.message_sample.split('\n').length} 条）</summary>
                                  <pre class="ds-sample-text">{r.message_sample}</pre>
                                </details>
                              {/if}
                            {/if}
                            {#if r.duration_ms || r.total_tokens}
                              <div class="ds-record-tele">
                                {#if r.duration_ms}<span>耗时 {fmtDuration(r.duration_ms)}</span>{/if}
                                {#if r.prompt_tokens}<span>输入 {fmtTokens(r.prompt_tokens)} tokens</span>{/if}
                                {#if r.completion_tokens}<span>输出 {fmtTokens(r.completion_tokens)} tokens</span>{/if}
                              </div>
                            {/if}
                            <div class="ds-record-actions">
                              {#if r.status === 'done'}
                                  <WechatHoverButton text="复制总结" onclick={() => copyRecord(r)} class="!px-3 !py-1 !text-xs" />
                              {/if}
                                <WechatHoverButton text="删除" onclick={() => deleteRecord(r.id)} class="!px-3 !py-1 !text-xs" />
                            </div>
                          </div>
                        {/if}
                      </div>
                    {/each}
                  </div>
                {/if}
              {/if}
            </div>
          {/if}
        {:else}
          <div class="ds-empty">
            <div class="ds-empty-icon">
              <svg viewBox="0 0 24 24" width="30" height="30" fill="none" stroke="currentColor" stroke-width="1.4" aria-hidden="true">
                <rect x="3" y="4" width="18" height="17" rx="2"/><path d="M8 2v4M16 2v4M3 9h18"/><path d="M12 13v4M10 15h4"/>
              </svg>
            </div>
            <p>还没有每日总结任务</p>
              <WechatHoverButton text="＋ 新建任务" onclick={resetForm} />
            <p class="ds-empty-sub">选择群聊与关注成员，指定模型和总结格式，到点自动分析前一天的聊天记录</p>
          </div>
        {/if}
      </main>
    </div>
  {/if}
</div>

<style>
  .ds-root { flex: 1; display: flex; flex-direction: column; min-width: 0; min-height: 0; background: var(--wc-bg); color: var(--wc-text); }
  .ds-hd { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 12px 18px; border-bottom: 1px solid var(--wc-border); flex-shrink: 0; }
  .ds-brand { display: flex; align-items: baseline; gap: 10px; min-width: 0; }
  .ds-title { font-size: 16px; font-weight: 700; margin: 0; }
  .ds-sub { font-size: 11.5px; color: var(--wc-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .ds-hd-right { display: flex; align-items: center; gap: 10px; flex-shrink: 0; }
  .ds-conn { display: inline-flex; align-items: center; gap: 6px; font-size: 11.5px; color: #16a34a; }
  .ds-conn i { width: 7px; height: 7px; border-radius: 50%; background: #16a34a; }
  .ds-conn-bad { color: #ef4444; }
  .ds-conn-bad i { background: #ef4444; }
  .ds-toast { font-size: 12px; color: #16a34a; background: color-mix(in srgb, #16a34a 12%, transparent); border: 1px solid color-mix(in srgb, #16a34a 28%, transparent); border-radius: 8px; padding: 6px 12px; max-width: 55%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ds-toast-err { color: #ef4444; background: color-mix(in srgb, #ef4444 10%, transparent); border-color: color-mix(in srgb, #ef4444 30%, transparent); }
  .ds-conn-banner { display: flex; align-items: center; justify-content: space-between; gap: 10px; padding: 8px 18px; background: color-mix(in srgb, #ef4444 10%, transparent); border-bottom: 1px solid color-mix(in srgb, #ef4444 28%, transparent); color: #ef4444; font-size: 12px; flex-shrink: 0; }
  .ds-loading { flex: 1; display: flex; align-items: center; justify-content: center; gap: 10px; color: var(--wc-muted); font-size: 13px; }
  .ds-spinner { width: 20px; height: 20px; border-radius: 50%; border: 2px solid var(--wc-border); border-top-color: var(--wc-theme,#576b95); animation: ds-spin .8s linear infinite; }
  @keyframes ds-spin { to { transform: rotate(360deg); } }
  .ds-body { flex: 1; display: flex; min-height: 0; }
  .ds-side { width: 240px; flex-shrink: 0; display: flex; flex-direction: column; border-right: 1px solid var(--wc-border); background: var(--wc-sidebar-bg); min-height: 0; }
  .ds-side-hd { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 12px 14px; border-bottom: 1px solid var(--wc-border); font-size: 13px; font-weight: 700; }
  .ds-task-list { flex: 1; overflow-y: auto; padding: 10px; display: flex; flex-direction: column; gap: 8px; }
  .ds-task { display: flex; flex-direction: column; gap: 5px; width: 100%; text-align: left; padding: 10px 12px; border: 1px solid var(--wc-border); border-radius: 10px; background: var(--wc-card); color: var(--wc-text); cursor: pointer; transition: all .15s ease; }
  .ds-task:hover { border-color: color-mix(in srgb, var(--wc-theme,#576b95) 45%, var(--wc-border)); }
  .ds-task-on { border-color: var(--wc-theme,#576b95); box-shadow: inset 3px 0 0 var(--wc-theme,#576b95); }
  .ds-task-top { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .ds-task-name { font-size: 12.5px; font-weight: 700; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ds-task-status { font-size: 11.5px; color: #16a34a; background: color-mix(in srgb, #16a34a 12%, transparent); border-radius: 4px; padding: 1px 7px; flex-shrink: 0; }
  .ds-task-off { color: var(--wc-muted); background: var(--wc-bg2); }
  .ds-task-meta { display: flex; align-items: center; justify-content: space-between; gap: 6px; font-size: 11.5px; color: var(--wc-text2); }
  .ds-task-sub { font-size: 11.5px; color: var(--wc-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ds-task-last { font-size: 11.5px; color: var(--wc-muted); }
  .ds-task-err { font-size: 11.5px; color: #ef4444; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ds-task-actions { display: flex; gap: 6px; margin-top: 2px; }
  .ds-empty-side { padding: 24px 12px; text-align: center; font-size: 12px; color: var(--wc-muted); line-height: 1.8; }
  .ds-main { flex: 1; min-width: 0; overflow-y: auto; padding: 16px 18px; display: flex; flex-direction: column; gap: 14px; }
  .ds-main-hd { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-shrink: 0; }
  .ds-card-title { font-size: 14px; font-weight: 700; margin: 0; }
  .ds-tabs { display: flex; gap: 2px; padding: 3px; background: var(--wc-bg2); border-radius: 10px; }
  .ds-hd-actions { display: flex; gap: 8px; flex-shrink: 0; }
  .ds-card { border: 1px solid var(--wc-border); border-radius: 14px; background: var(--wc-card); padding: 14px 16px; }
  .ds-card-hd { display: flex; align-items: center; justify-content: space-between; gap: 10px; margin-bottom: 12px; }
  .ds-actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 14px; }
  .ds-records { min-height: 200px; }
  .ds-records-loading { font-size: 11.5px; color: var(--wc-muted); }
  .ds-empty-records { padding: 22px 12px; text-align: center; font-size: 12px; color: var(--wc-muted); line-height: 1.7; }
  .ds-rec-stats { display: flex; gap: 8px; margin-bottom: 12px; }
  .ds-rec-stat {
    flex: 1; display: flex; flex-direction: column; align-items: center; gap: 2px;
    padding: 10px 8px; border-radius: 10px; background: var(--wc-bg2);
    border: 1px solid var(--wc-border-light);
  }
  .ds-rec-stat-val { font-size: 18px; font-weight: 800; color: var(--wc-text); font-variant-numeric: tabular-nums; letter-spacing: -0.01em; }
  .ds-rec-stat-ok { color: #16a34a; }
  .ds-rec-stat-fail { color: #ef4444; }
  .ds-rec-stat-label { font-size: 11.5px; color: var(--wc-muted); }
  .ds-filter { display: flex; gap: 6px; margin-bottom: 10px; }
  .ds-record-list { display: flex; flex-direction: column; gap: 8px; }
  .ds-record { border: 1px solid var(--wc-border); border-radius: 10px; overflow: hidden; }
  .ds-record-open { border-color: color-mix(in srgb, var(--wc-theme,#576b95) 40%, var(--wc-border)); }
  .ds-record-hd { display: flex; align-items: center; gap: 12px; width: 100%; text-align: left; padding: 10px 14px; background: var(--wc-bg2); border: none; color: var(--wc-text); cursor: pointer; }
  .ds-record-hd:hover { background: var(--wc-item-hover); }
  .ds-record-date { font-size: 13px; font-weight: 700; font-variant-numeric: tabular-nums; }
  .ds-badge {
    flex-shrink: 0; font-size: 11.5px; font-weight: 700; color: #16a34a;
    background: color-mix(in srgb, #16a34a 12%, transparent); border-radius: 4px; padding: 1px 7px;
  }
  .ds-badge-err { color: #ef4444; background: color-mix(in srgb, #ef4444 10%, transparent); }
  .ds-record-meta { flex: 1; font-size: 11.5px; color: var(--wc-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ds-record-toggle { font-size: 11.5px; color: var(--wc-theme,#576b95); }
  .ds-record-body { padding: 12px 14px; }
  .ds-record-text { margin: 0; font-size: 13px; line-height: 1.75; color: var(--wc-text); white-space: pre-wrap; word-break: break-word; }
  .ds-record-err { font-size: 12px; color: #ef4444; }
  .ds-record-actions { display: flex; justify-content: flex-end; margin-top: 10px; }
  .ds-sample { margin-top: 12px; border: 1px solid var(--wc-border-light); border-radius: 8px; background: var(--wc-bg2); }
  .ds-sample summary { cursor: pointer; font-size: 11.5px; color: var(--wc-text2); padding: 8px 11px; user-select: none; }
  .ds-sample summary:hover { color: var(--wc-text); }
  .ds-sample-text {
    margin: 0; padding: 4px 11px 10px; font-size: 11.5px; line-height: 1.7; color: var(--wc-text2);
    white-space: pre-wrap; word-break: break-word; font-family: inherit; max-height: 240px; overflow: auto;
  }
  .ds-record-tele { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 10px; }
  .ds-record-tele span {
    font-size: 11.5px; color: var(--wc-muted); padding: 2px 8px; border-radius: 6px;
    background: var(--wc-bg2); border: 1px solid var(--wc-border-light); font-variant-numeric: tabular-nums;
  }
  .ds-range-card { border-color: color-mix(in srgb, var(--wc-theme,#576b95) 30%, var(--wc-border)); }
  .ds-range-tip { font-size: 11.5px; color: var(--wc-muted); margin: 0 0 12px; line-height: 1.6; }
  .ds-range-row { display: flex; align-items: flex-end; gap: 10px; flex-wrap: wrap; }
  .ds-range-field { display: flex; flex-direction: column; gap: 6px; }
  .ds-range-field .ds-select { width: 150px; }
  .ds-range-sep { font-size: 12px; color: var(--wc-muted); padding-bottom: 8px; }
  .ds-test-card { border-color: color-mix(in srgb, var(--wc-theme,#576b95) 30%, var(--wc-border)); }
  .ds-test-result {
    font-size: 12px; line-height: 1.7; color: #16a34a; background: color-mix(in srgb, #16a34a 8%, transparent);
    border: 1px solid color-mix(in srgb, #16a34a 24%, transparent); border-radius: 10px;
    padding: 10px 12px; white-space: pre-wrap; word-break: break-all; max-height: 180px; overflow: auto;
  }
  .ds-test-err { color: #ef4444; background: color-mix(in srgb, #ef4444 7%, transparent); border-color: color-mix(in srgb, #ef4444 24%, transparent); }
  .ds-empty { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; color: var(--wc-muted); font-size: 13px; text-align: center; padding: 40px; }
  .ds-empty-icon { color: var(--wc-muted); opacity: .6; }
  .ds-empty p { margin: 0; }
  .ds-empty-sub { font-size: 12px; max-width: 360px; line-height: 1.7; }
  @media (max-width: 900px) {
    .ds-body { flex-direction: column; }
    .ds-side { width: 100%; border-right: none; border-bottom: 1px solid var(--wc-border); max-height: 220px; }
  }
</style>
