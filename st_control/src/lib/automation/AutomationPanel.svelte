<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { automationApi } from './services/ipc';
  import type { AutomationRule, AutomationStats, AutomationTask, RuleCondition, AnalyzeField } from './services/ipc';
  import { agentApi } from '../agents/services/ipc';
  import { llmApi } from '../llm/services/ipc';
  import BotPanel from '../bot/BotPanel.svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { toast } from 'svelte-sonner';
  import { Button } from '../components/ui/button';
  import LiveNumber from '../components/fancy/LiveNumber.svelte';
  import { RippleButton } from 'fancy-ui-svelte';
  import { Input } from '../components/ui/input';
  import { Textarea } from '../components/ui/textarea';
  import { Badge } from '../components/ui/badge';
  import { Switch } from '../components/ui/switch';
  import { Label } from '../components/ui/label';
  import { Card, CardContent, CardHeader, CardTitle } from '../components/ui/card';
  import { Tabs, TabsList, TabsTrigger } from '../components/ui/tabs';
  import { Root as SelectRoot } from '../components/ui/select';
  import { SelectContent, SelectItem, SelectTrigger } from '../components/ui/select';
  import {
    Dialog as DialogRoot, DialogContent, DialogHeader, DialogTitle, DialogFooter,
  } from '../components/ui/dialog';
  import {
    Sheet as SheetRoot, SheetContent, SheetHeader, SheetTitle, SheetDescription,
  } from '../components/ui/sheet';
  import {
    RadioGroup, RadioGroupItem,
  } from '../components/ui/radio-group';
  import ActivityIcon from '@lucide/svelte/icons/activity';
  import RadioIcon from '@lucide/svelte/icons/radio';
  import SlidersHorizontalIcon from '@lucide/svelte/icons/sliders-horizontal';
  import ListChecksIcon from '@lucide/svelte/icons/list-checks';
  import BotIcon from '@lucide/svelte/icons/bot';
  import MessageSquareTextIcon from '@lucide/svelte/icons/message-square-text';
  import PlusIcon from '@lucide/svelte/icons/plus';
  import Trash2Icon from '@lucide/svelte/icons/trash-2';
  import PencilIcon from '@lucide/svelte/icons/pencil';
  import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
  import SearchIcon from '@lucide/svelte/icons/search';
  import XIcon from '@lucide/svelte/icons/x';
  import SendIcon from '@lucide/svelte/icons/send';
  import ClockIcon from '@lucide/svelte/icons/clock';
  import CheckCircle2Icon from '@lucide/svelte/icons/check-circle-2';
  import RotateCcwIcon from '@lucide/svelte/icons/rotate-ccw';
  import {
    classifyMessageType,
    kindColor,
    kindLabel,
    mediaLabel,
    statusBadge,
    STATUS_META,
    type LiveMessage,
  } from './display';
  import { formatIsoTime, formatTs } from '../format';

  const FIELD_LABELS: Record<string, string> = {
    content: '消息内容', sender: '发送人', session: '会话/群', media_type: '媒体类型', is_send: '是否自己发送',
  };
  const OP_LABELS: Record<string, string> = {
    contains: '包含', not_contains: '不包含', equals: '等于', regex: '正则匹配',
  };
  const FIELD_OPTIONS = ['content', 'sender', 'session', 'media_type', 'is_send'];
  const OP_OPTIONS = ['contains', 'not_contains', 'equals', 'regex'];

  // ─── 视图状态 ───
  type View = 'overview' | 'rules' | 'tasks' | 'robot' | 'channels';
  let view = $state<View>('overview');

  // ─── 概览 ───
  let stats = $state<AutomationStats | null>(null);
  let liveMsgs = $state<LiveMessage[]>([]);
  let liveUnlisten: UnlistenFn | null = null;
  let connStatus = $state<{ connected: boolean; received: number; lastAt: string | null }>({
    connected: false, received: 0, lastAt: null,
  });
  type PushType = 'text' | 'image' | 'video' | 'file' | 'all';
  let pushType = $state<PushType>('text');
  const PUSH_TYPE_LABELS: Record<PushType, string> = {
    text: '文本', image: '图片', video: '视频', file: '文件', all: '全部',
  };

  // ─── 规则 ───
  let rules = $state<AutomationRule[]>([]);
  let ruleDialogOpen = $state(false);
  let editingRule = $state<AutomationRule | null>(null);
  let form = $state({
    name: '', enabled: true, priority: 0,
    conditions: [{ field: 'content', op: 'contains', value: '' }] as RuleCondition[],
    analyzeFields: [{ name: '', desc: '' }] as AnalyzeField[],
    promptOverride: '', providerId: '', model: '',
    dispatchMode: 'fixed', targetType: 'agent', targetId: '',
    roleId: '',
  });

  // ─── 消息与任务 ───
  let tasks = $state<AutomationTask[]>([]);
  let taskTotal = $state(0);
  let taskPage = $state(1);
  const TASK_PAGE_SIZE = 50;
  let taskStatusFilter = $state('');
  let taskKeyword = $state('');
  let taskDetail = $state<AutomationTask | null>(null);
  let taskLoading = $state(false);

  // ─── 回复机器人 ───
  let toReplyTasks = $state<AutomationTask[]>([]);

  // ─── 智能体/Agent 列表（派发目标） ───
  let agentOptions = $state<{ id: number; name: string }[]>([]);
  // ─── AI 角色列表（规则可绑定，内置 Worker 执行时注入提示词） ───
  let roleOptions = $state<{ id: string; name: string }[]>([]);

  async function loadStats() {
    try { stats = await automationApi.stats(); } catch { stats = null; }
  }
  async function loadRules() {
    try { rules = await automationApi.listRules(); } catch { rules = []; }
  }
  async function loadTasks() {
    taskLoading = true;
    try {
      const res = await automationApi.listTasks({
        status: taskStatusFilter || null,
        keyword: taskKeyword || null,
        page: taskPage,
        pageSize: TASK_PAGE_SIZE,
      });
      tasks = res.items ?? [];
      taskTotal = res.total ?? 0;
    } catch { tasks = []; }
    finally { taskLoading = false; }
  }
  async function loadAgents() {
    try {
      const list = await agentApi.list();
      agentOptions = (list ?? []).map((a) => ({ id: a.id, name: a.name }));
    } catch { agentOptions = []; }
  }
  async function loadRoles() {
    try {
      const list = await llmApi.getAiRoles();
      roleOptions = (list ?? []).filter((r) => r.enabled).map((r) => ({ id: r.id, name: r.name }));
    } catch { roleOptions = []; }
  }
  async function loadToReply() {
    try {
      const res = await automationApi.listTasks({ status: 'to_reply', keyword: null, page: 1, pageSize: 50 });
      toReplyTasks = res.items ?? [];
    } catch { toReplyTasks = []; }
  }
  async function loadConnStatus() {
    try {
      connStatus = await automationApi.connStatus();
    } catch { /* 保持上次状态 */ }
  }
  async function reconnectSse() {
    try {
      await automationApi.reconnect();
      toast.success('已请求重连 SSE');
      setTimeout(loadConnStatus, 1200);
    } catch (e: unknown) { toast.error('重连失败：' + e); }
  }
  async function simulatePush() {
    try {
      const id = await automationApi.simulatePush({
        content: null, senderUsername: null, username: null,
      });
      toast.success(id > 0 ? `已模拟推送并入库（任务 #${id}）` : '已模拟推送（未命中规则）');
      loadStats(); loadTasks(); loadToReply();
    } catch (e: unknown) { toast.error('模拟推送失败：' + e); }
  }

  function refreshAll() {
    loadStats(); loadRules(); loadTasks(); loadToReply(); loadConnStatus();
  }

  let connStatusTimer: ReturnType<typeof setInterval> | null = null;
  onMount(async () => {
    refreshAll();
    loadAgents();
    loadRoles();
    liveUnlisten = await listen<LiveMessage>('automation://message', (e) => {
      liveMsgs = [e.payload, ...liveMsgs].slice(0, 200);
      connStatus = { ...connStatus, received: connStatus.received + 1, lastAt: new Date().toLocaleString('zh-CN') };
    });
    connStatusTimer = setInterval(loadConnStatus, 5000);
  });
  onDestroy(() => {
    liveUnlisten?.();
    if (connStatusTimer) {
      clearInterval(connStatusTimer);
      connStatusTimer = null;
    }
  });

  // ─── 规则表单 ───
  function openNewRule() {
    editingRule = null;
    form = {
      name: '', enabled: true, priority: rules.length,
      conditions: [{ field: 'content', op: 'contains', value: '' }],
      analyzeFields: [{ name: '', desc: '' }],
      promptOverride: '', providerId: '', model: '',
      dispatchMode: 'fixed', targetType: 'agent', targetId: '',
      roleId: '',
    };
    ruleDialogOpen = true;
  }
  function openEditRule(r: AutomationRule) {
    editingRule = r;
    form = {
      name: r.name, enabled: r.enabled, priority: r.priority,
      conditions: r.conditions.length ? r.conditions : [{ field: 'content', op: 'contains', value: '' }],
      analyzeFields: r.analyzeFields.length ? r.analyzeFields : [{ name: '', desc: '' }],
      promptOverride: r.promptOverride || '', providerId: r.providerId || '', model: r.model || '',
      dispatchMode: r.dispatchMode || 'fixed', targetType: r.targetType || 'agent', targetId: r.targetId || '',
      roleId: r.roleId || '',
    };
    ruleDialogOpen = true;
  }
  async function saveRule() {
    if (!form.name.trim()) { toast.error('请输入规则名称'); return; }
    const validConditions = form.conditions.filter((c) => c.value.trim());
    if (validConditions.length === 0) { toast.error('至少需要一个条件'); return; }
    if (form.dispatchMode === 'fixed' && !form.targetId) { toast.error('固定派发需选择目标'); return; }
    try {
      await automationApi.saveRule({
          id: editingRule?.id ?? null,
          name: form.name,
          enabled: form.enabled,
          priority: Number(form.priority) || 0,
          conditions: validConditions,
          analyzeFields: form.analyzeFields.filter((f) => f.name.trim()),
          promptOverride: form.promptOverride,
          providerId: form.providerId,
          model: form.model,
          dispatchMode: form.dispatchMode,
          targetType: form.targetType,
          targetId: form.targetId,
          roleId: form.roleId,
      });
      toast.success(editingRule ? '规则已更新' : '规则已创建');
      ruleDialogOpen = false;
      loadRules(); loadStats();
    } catch (e: unknown) { toast.error('保存失败：' + e); }
  }
  async function toggleRule(r: AutomationRule) {
    try {
      await automationApi.toggleRule(r.id, !r.enabled);
      loadRules(); loadStats();
    } catch (e: unknown) { toast.error(String(e)); }
  }
  async function removeRule(r: AutomationRule) {
    if (!confirm(`确认删除规则「${r.name}」？`)) return;
    try { await automationApi.deleteRule(r.id); loadRules(); loadStats(); }
    catch (e: unknown) { toast.error(String(e)); }
  }

  // ─── 任务操作 ───
  async function setTaskStatus(t: AutomationTask, status: string) {
    try { await automationApi.setTaskStatus(t.id, status); loadTasks(); loadStats(); loadToReply(); }
    catch (e: unknown) { toast.error(String(e)); }
  }
  async function dispatchTask(t: AutomationTask, targetType: string, targetId: string) {
    try {
      await automationApi.setTaskTarget(t.id, targetType, targetId);
      toast.success('已派发');
      loadTasks(); loadStats();
    } catch (e: unknown) { toast.error(String(e)); }
  }
  async function saveReply(t: AutomationTask, reply: string, status: string) {
    try {
      await automationApi.editTaskReply(t.id, reply, status);
      toast.success('已保存');
      loadTasks(); loadToReply(); loadStats();
    } catch (e: unknown) { toast.error(String(e)); }
  }
  async function saveAiExtract(t: AutomationTask, extract: string) {
    try { await automationApi.editAiExtract(t.id, extract); toast.success('AI 结果已更新'); loadTasks(); }
    catch (e: unknown) { toast.error(String(e)); }
  }
  async function removeTask(t: AutomationTask) {
    if (!confirm(`确认删除任务 #${t.id}？`)) return;
    try { await automationApi.deleteTask(t.id); loadTasks(); loadStats(); }
    catch (e: unknown) { toast.error(String(e)); }
  }
  function openTaskDetail(t: AutomationTask) {
    taskDetail = t;
  }

  // ─── 帮助函数 ───
  function fmtTime(iso: string): string {
    return formatIsoTime(iso, { showYear: false, useLocale: true });
  }
  function fmtTs(ts: number): string {
    return formatTs(ts, { showYear: false, useLocale: true });
  }
  /** 分类推送消息（本地 PushType 含 'all' 过滤项，分类结果映射为 MessageKind） */
  function msgType(m: LiveMessage): PushType {
    return classifyMessageType(m) as PushType;
  }
  const filteredLiveMsgs = $derived(
    liveMsgs.filter((m) => m.automationHit || pushType === 'all' || msgType(m) === pushType)
  );
  function avatarText(m: LiveMessage): string {
    const s = m?.sender_username || m?.username || '?';
    return (String(s).charAt(0) || '?').toUpperCase();
  }
  /** 消息类型徽章配色 */
  function typeColor(m: LiveMessage): string {
    return kindColor(classifyMessageType(m));
  }
  /** 消息类型标签 */
  function typeLabel(m: LiveMessage): string {
    return kindLabel(classifyMessageType(m));
  }
</script>

<div class="flex h-full flex-col gap-3 p-1">
<Tabs bind:value={view} class="ap-tabs shrink-0">
    <TabsList class="h-9 w-fit">
      <TabsTrigger value="overview"><ActivityIcon class="size-3.5" />概览</TabsTrigger>
      <TabsTrigger value="rules"><SlidersHorizontalIcon class="size-3.5" />规则管理</TabsTrigger>
      <TabsTrigger value="tasks"><ListChecksIcon class="size-3.5" />消息与任务</TabsTrigger>
      <TabsTrigger value="robot"><BotIcon class="size-3.5" />回复机器人</TabsTrigger>
      <TabsTrigger value="channels"><MessageSquareTextIcon class="size-3.5" />消息通道</TabsTrigger>
    </TabsList>
  </Tabs>

  {#if view === 'overview'}
<div class="ap-view flex min-h-0 flex-1 flex-col gap-3">
    <!-- ═══════ 概览 ═══════ -->
    <div class="flex items-center justify-between">
      <div>
        <div class="text-base font-bold">自动化概览</div>
        <div class="mt-0.5 text-xs text-muted-foreground">SSE 实时消息 → 规则匹配 → 智能体派发 → 回复机器人 → 消息通道</div>
      </div>
      <div class="flex items-center gap-4">
        <span class="inline-flex items-center gap-1.5 rounded-md border px-2 py-1 text-[11px] {connStatus.connected ? 'border-[color-mix(in_srgb,var(--app-success)_35%,transparent)] bg-[color-mix(in_srgb,var(--app-success)_10%,transparent)] text-[var(--app-success)]' : 'border-[color-mix(in_srgb,var(--app-danger)_35%,transparent)] bg-[color-mix(in_srgb,var(--app-danger)_10%,transparent)] text-[var(--app-danger)]'}">
          <span class="size-1.5 rounded-full {connStatus.connected ? 'bg-[var(--app-success)]' : 'bg-[var(--app-danger)]'}"></span>
          SSE {connStatus.connected ? '已连接' : '未连接'} · 收到 {connStatus.received} 条{connStatus.lastAt ? ` · ${connStatus.lastAt}` : ''}
        </span>
        <div class="flex items-center gap-1.5">
        <RippleButton
          onclick={reconnectSse}
          title="手动重连 SSE"
          rippleColor="#22d3ee"
          class="h-8 rounded-md border border-[var(--border)] bg-[var(--card)] px-3 text-xs font-medium text-[var(--foreground)] hover:bg-[var(--muted)]"
        >
          <RotateCcwIcon class="size-3.5" />重连
        </RippleButton>
        <RippleButton
          onclick={simulatePush}
          title="构造一条测试消息，走完整推送链路（含规则引擎）"
          rippleColor="#22d3ee"
          class="h-8 rounded-md border border-[var(--border)] bg-[var(--card)] px-3 text-xs font-medium text-[var(--foreground)] hover:bg-[var(--muted)]"
        >
          <SendIcon class="size-3.5" />模拟推送
        </RippleButton>
        <RippleButton
          onclick={refreshAll}
          rippleColor="#22d3ee"
          class="h-8 rounded-md border border-[var(--border)] bg-[var(--card)] px-3 text-xs font-medium text-[var(--foreground)] hover:bg-[var(--muted)]"
        ><RefreshCwIcon class="size-3.5" />刷新</RippleButton>
        </div>
      </div>
    </div>

    <!-- 统计卡：大屏 7 列单行，指标等宽对齐（消除 4+3 第二行空位）；小屏 2/4 列回退 -->
    <div class="grid grid-cols-2 gap-3 md:grid-cols-4 xl:grid-cols-7">
      {#each [
        { label: '今日推送', val: stats?.todayPushed ?? 0, color: 'var(--brand-strong)', icon: SendIcon },
        { label: '规则命中', val: stats?.rulesTotal ?? 0, color: 'var(--foreground)', icon: SlidersHorizontalIcon },
        { label: '待处理', val: stats?.pending ?? 0, color: 'var(--app-warning)', icon: ClockIcon },
        { label: '已派发', val: stats?.claimed ?? 0, color: 'var(--foreground)', icon: ListChecksIcon },
        { label: '处理中', val: stats?.processing ?? 0, color: 'var(--foreground)', icon: ActivityIcon },
        { label: '待回复', val: stats?.toReply ?? 0, color: 'var(--app-warning)', icon: MessageSquareTextIcon },
        { label: '已回复', val: stats?.replied ?? 0, color: 'var(--app-success)', icon: CheckCircle2Icon },
      ] as m}
        {@const Icon = m.icon}
        <div class="flex items-center gap-2.5 rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--card)] px-3 py-2.5">
          <span class="flex size-9 shrink-0 items-center justify-center rounded-lg" style="background:color-mix(in srgb, {m.color} 14%, transparent)">
            <Icon class="size-4.5" style="color:{m.color}" />
          </span>
          <div class="min-w-0">
            <div class="truncate text-[11px] font-semibold uppercase tracking-[0.08em] text-[var(--muted-foreground)]">{m.label}</div>
            <div class="mt-0.5 text-xl font-bold leading-none tabular-nums" style="color:{m.color}">
              <LiveNumber value={m.val} duration={650} />
            </div>
          </div>
        </div>
      {/each}
    </div>

    <Card class="flex min-h-0 flex-1 flex-col">
      <CardHeader class="flex-row items-center justify-between space-y-0 py-3">
        <CardTitle class="text-sm">实时消息流</CardTitle>
        <div class="flex items-center gap-2">
          <SelectRoot type="single" value={pushType} onValueChange={(v) => (pushType = v as PushType)}>
            <SelectTrigger class="h-7 w-24 text-xs">
              <span>{PUSH_TYPE_LABELS[pushType]}</span>
            </SelectTrigger>
            <SelectContent>
              {#each Object.entries(PUSH_TYPE_LABELS) as [k, v]}<SelectItem value={k}>{v}</SelectItem>{/each}
            </SelectContent>
          </SelectRoot>
          <span class="flex items-center gap-1.5 text-xs text-muted-foreground">
            <span class="size-1.5 animate-pulse rounded-full bg-emerald-400"></span>
            {filteredLiveMsgs.length} 条
          </span>
        </div>
      </CardHeader>
      <CardContent class="min-h-0 flex-1 overflow-auto p-0">
        {#if filteredLiveMsgs.length === 0}
          <div class="flex h-full min-h-40 flex-col items-center justify-center gap-2.5 text-center">
            <RadioIcon class="size-8 text-[var(--muted-foreground)]/50" />
            <div class="text-sm font-medium text-[var(--foreground)]">
              {liveMsgs.length === 0 ? '等待消息推送' : `暂无${PUSH_TYPE_LABELS[pushType]}类型的消息`}
            </div>
            <div class="text-xs text-[var(--muted-foreground)]">
              {liveMsgs.length === 0 ? '微信 / QQ 消息到达后实时显示；也可点击右上角「模拟推送」测试链路' : '切换上方消息类型筛选查看其它消息'}
            </div>
          </div>
        {:else}
          <div class="space-y-2 p-2">
            {#each filteredLiveMsgs as m}
              <div
                class="flex items-start gap-3 rounded-lg border px-3 py-2.5 transition-colors
                  {m.automationHit
                    ? 'border-primary/70 bg-primary/10 shadow-[0_0_14px_rgba(34,211,238,0.22)]'
                    : 'border-transparent hover:bg-muted/40'}"
              >
                <span class="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-lg text-xs font-bold {typeColor(m)}">
                  {avatarText(m)}
                </span>
                <div class="min-w-0 flex-1">
                  <div class="flex flex-wrap items-center gap-x-2 gap-y-0.5">
                    <span class="truncate text-xs font-medium text-foreground">{m.sender_username || m.username || '未知'}</span>
                {#if m.is_group}<span class="rounded bg-muted px-1 text-[11px] text-muted-foreground">群聊</span>{/if}
                    <span class="rounded bg-muted px-1 text-[10px] text-muted-foreground">{typeLabel(m)}</span>
                    {#if m.automationHit}
                      <span class="inline-flex items-center gap-1 rounded-full border border-primary/50 bg-primary/20 px-2 py-0.5 text-[10px] font-semibold text-primary">
                        ⚡ 命中规则{m.ruleName ? ` · ${m.ruleName}` : ''}
                      </span>
                    {/if}
                <span class="ml-auto shrink-0 text-[11px] text-muted-foreground">{fmtTs(m.timestamp ?? 0)}</span>
                  </div>
                  <div class="mt-1 break-words text-xs leading-relaxed {m.automationHit ? 'text-primary-foreground' : 'text-foreground/90'}">
                    {m.content || '[媒体消息]'}
                  </div>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </CardContent>
    </Card>

  </div>
  {:else if view === 'rules'}
<div class="ap-view flex min-h-0 flex-1 flex-col gap-3">
    <!-- ═══════ 规则管理 ═══════ -->
    <div class="flex items-center justify-between">
      <div>
        <div class="text-base font-bold">规则管理</div>
        <div class="mt-0.5 text-xs text-muted-foreground">{rules.filter((r) => r.enabled).length} 条启用 · {rules.length} 条规则</div>
      </div>
      <RippleButton
        onclick={openNewRule}
        rippleColor="#a5f3fc"
        class="h-8 rounded-md border-0 bg-[var(--primary)] px-3.5 text-xs font-medium text-[var(--primary-foreground)] hover:opacity-90"
      ><PlusIcon class="size-3.5" />新建规则</RippleButton>
    </div>

    {#if rules.length === 0}
      <Card class="flex-1 flex-col items-center justify-center">
        <CardContent class="py-12 text-center text-sm text-muted-foreground">
          还没有规则，点击「新建规则」配置消息匹配与派发
        </CardContent>
      </Card>
    {:else}
      <div class="grid grid-cols-1 gap-3 xl:grid-cols-2">
        {#each rules as r}
          <Card class={r.enabled ? '' : 'opacity-60'}>
            <CardHeader class="flex-row items-start justify-between space-y-0 py-3">
              <div class="min-w-0">
                <div class="flex items-center gap-2">
                  <span class="truncate text-sm font-semibold">{r.name}</span>
                  <Badge variant="secondary" class="text-[11px]">P{r.priority}</Badge>
                  {#if r.dispatchMode === 'ai'}<Badge class="bg-primary/15 text-primary text-[11px]">AI 决策</Badge>{/if}
                </div>
                <div class="mt-1 text-[11px] text-muted-foreground">
                  命中 {r.hitCount} 次 · {r.targetId ? `→ ${r.targetType === 'agent' ? '智能体' : 'Agent'} #${r.targetId}` : '待定'}
                </div>
              </div>
              <Switch checked={r.enabled} onCheckedChange={() => toggleRule(r)} />
            </CardHeader>
            <CardContent class="py-2">
              <div class="flex flex-wrap gap-1.5">
                {#each r.conditions as c}
                  <span class="rounded-md border bg-muted/40 px-1.5 py-0.5 text-[11px] text-muted-foreground">
                    {FIELD_LABELS[c.field] ?? c.field} {OP_LABELS[c.op] ?? c.op} <b class="text-foreground">{c.value}</b>
                  </span>
                {/each}
              </div>
              <div class="mt-2 flex items-center justify-end gap-1.5">
                <Button size="sm" variant="ghost" class="h-7 px-2" onclick={() => openEditRule(r)}><PencilIcon class="size-3.5" />编辑</Button>
                <Button size="sm" variant="ghost" class="h-7 px-2 text-destructive hover:text-destructive" onclick={() => removeRule(r)}><Trash2Icon class="size-3.5" />删除</Button>
              </div>
            </CardContent>
          </Card>
        {/each}
      </div>
    {/if}

  </div>
  {:else if view === 'tasks'}
<div class="ap-view flex min-h-0 flex-1 flex-col gap-3">
    <div class="flex items-center justify-between">
      <div>
        <div class="text-base font-bold">消息与任务</div>
        <div class="mt-0.5 text-xs text-muted-foreground">规则命中后生成的任务 · 状态跟踪与详情</div>
      </div>
    </div>
    <!-- ═══════ 消息与任务 ═══════ -->
    <div class="flex flex-wrap items-center gap-2">
      <SelectRoot type="single" value={taskStatusFilter} onValueChange={(v) => { taskStatusFilter = v; taskPage = 1; loadTasks(); }}>
        <SelectTrigger class="h-8 w-32"><span>{taskStatusFilter ? STATUS_META[taskStatusFilter]?.label ?? taskStatusFilter : '全部状态'}</span></SelectTrigger>
        <SelectContent>
          <SelectItem value="">全部状态</SelectItem>
          {#each Object.entries(STATUS_META) as [k, m]}<SelectItem value={k}>{m.label}</SelectItem>{/each}
        </SelectContent>
      </SelectRoot>
      <div class="relative flex-1 max-w-sm">
        <SearchIcon class="absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
        <Input class="h-8 pl-8" placeholder="搜索内容 / 发送人 / 会话 / 规则…" bind:value={taskKeyword}
          onkeydown={(e) => e.key === 'Enter' && (taskPage = 1, loadTasks())} />
      </div>
      <Button size="sm" variant="outline" class="h-8" onclick={() => { taskPage = 1; loadTasks(); }}>查询</Button>
      <Button size="sm" variant="ghost" class="h-8" onclick={refreshAll}><RefreshCwIcon class="size-3.5" /></Button>
      <span class="ml-auto text-xs text-muted-foreground">共 {taskTotal} 条</span>
    </div>

    <Card class="min-h-0 flex-1 overflow-hidden">
      <CardContent class="h-full overflow-auto p-0">
        <table class="w-full text-left text-xs">
          <thead class="sticky top-0 z-10 bg-card">
            <tr class="border-b text-muted-foreground">
              <th class="px-3 py-2.5 font-medium">ID</th>
              <th class="px-3 py-2.5 font-medium">内容</th>
              <th class="px-3 py-2.5 font-medium">发送人</th>
              <th class="px-3 py-2.5 font-medium">会话</th>
              <th class="px-3 py-2.5 font-medium">命中规则</th>
              <th class="px-3 py-2.5 font-medium">目标</th>
              <th class="px-3 py-2.5 font-medium">状态</th>
              <th class="px-3 py-2.5 font-medium">时间</th>
              <th class="px-3 py-2.5 text-right font-medium">操作</th>
            </tr>
          </thead>
          <tbody>
            {#each tasks as t}
              <tr class="border-b hover:bg-muted/30">
                <td class="px-3 py-2 font-mono text-muted-foreground">#{t.id}</td>
                <td class="max-w-52 truncate px-3 py-2" title={t.content}>{t.content || '[媒体消息]'}</td>
                <td class="px-3 py-2 font-mono text-muted-foreground">{t.senderUsername}</td>
                <td class="max-w-36 truncate px-3 py-2 font-mono text-muted-foreground" title={t.username}>{t.username}</td>
                <td class="px-3 py-2">{t.ruleName || '—'}</td>
                <td class="px-3 py-2">{t.targetId ? `${t.targetType === 'agent' ? '智能体' : 'Agent'} #${t.targetId}` : '—'}</td>
                <td class="px-3 py-2">{@html statusBadge(t.status)}{#if t.error}<span class="ml-1 text-[11px] text-destructive" title={t.error}>!</span>{/if}</td>
                <td class="px-3 py-2 text-muted-foreground">{fmtTime(t.createdAt)}</td>
                <td class="px-3 py-2 text-right">
                  <Button size="sm" variant="ghost" class="h-7 px-2" onclick={() => openTaskDetail(t)}>详情</Button>
                </td>
              </tr>
            {:else}
              <tr><td colspan={9} class="py-12 text-center text-muted-foreground">{taskLoading ? '加载中…' : '暂无任务记录'}</td></tr>
            {/each}
          </tbody>
        </table>
      </CardContent>
    </Card>

    <div class="flex items-center justify-end gap-2 text-xs text-muted-foreground">
      <Button size="sm" variant="outline" class="h-7" disabled={taskPage <= 1} onclick={() => { taskPage--; loadTasks(); }}>上一页</Button>
      <span class="tabular-nums">{taskPage} / {Math.max(1, Math.ceil(taskTotal / TASK_PAGE_SIZE))}</span>
      <Button size="sm" variant="outline" class="h-7" disabled={taskPage * TASK_PAGE_SIZE >= taskTotal} onclick={() => { taskPage++; loadTasks(); }}>下一页</Button>
    </div>

  </div>
  {:else if view === 'robot'}
<div class="ap-view flex min-h-0 flex-1 flex-col gap-3">
    <!-- ═══════ 回复机器人 ═══════ -->
    <Card>
      <CardHeader>
        <CardTitle class="text-sm">回复机器人机制</CardTitle>
      </CardHeader>
      <CardContent class="space-y-2 text-xs text-muted-foreground">
        <p>1. 智能体/Agent 处理完成后将结果写入 <code class="rounded bg-muted px-1">task_wechat_info.reply_text</code>，状态变为「待回复」。</p>
        <p>2. 回复机器人定时读取状态为「待回复」的记录，取 <code class="rounded bg-muted px-1">reply_text</code> 发送到对应会话（username）。</p>
        <p>3. 发送成功后机器人将状态更新为「已回复」；发送失败保持「待回复」并在下方提示重试。</p>
      </CardContent>
    </Card>

    <Card class="min-h-0 flex-1">
      <CardHeader class="flex-row items-center justify-between space-y-0 py-3">
        <CardTitle class="text-sm">待回复队列（{toReplyTasks.length}）</CardTitle>
        <Button size="sm" variant="ghost" class="h-7 px-2" onclick={loadToReply}><RefreshCwIcon class="size-3.5" /></Button>
      </CardHeader>
      <CardContent class="overflow-auto">
        {#if toReplyTasks.length === 0}
          <div class="py-10 text-center text-xs text-muted-foreground">暂无待回复消息</div>
        {:else}
          <div class="space-y-2">
            {#each toReplyTasks as t}
              <div class="flex items-center gap-3 rounded-lg border p-3">
                <div class="min-w-0 flex-1">
                  <div class="truncate text-xs">{t.content || '[媒体消息]'}</div>
                  <div class="mt-0.5 truncate text-[11px] text-muted-foreground">→ {t.username} · {fmtTime(t.createdAt)}</div>
                  {#if t.replyText}
                    <div class="mt-1.5 rounded bg-muted/50 px-2 py-1 text-[11px] text-foreground">回复：{t.replyText}</div>
                  {/if}
                </div>
                <Button size="sm" variant="outline" class="h-7" onclick={() => openTaskDetail(t)}><SendIcon class="size-3" />查看</Button>
              </div>
            {/each}
          </div>
        {/if}
      </CardContent>
    </Card>
  </div>
  {:else if view === 'channels'}
<div class="ap-view flex min-h-0 flex-1 flex-col gap-3">
    <!-- ═══════ 消息通道（微信 iLink / QQ 官方机器人，并入本面板） ═══════ -->
    <BotPanel />
  </div>
  {/if}
</div>

<!-- ═══════ 规则编辑弹窗 ═══════ -->
<DialogRoot open={ruleDialogOpen} onOpenChange={(o) => !o && (ruleDialogOpen = false)}>
  <DialogContent class="max-w-2xl">
    <DialogHeader>
      <DialogTitle>{editingRule ? '编辑规则' : '新建规则'}</DialogTitle>
    </DialogHeader>
    <div class="max-h-[62vh] space-y-4 overflow-auto pr-1">
      <div class="grid grid-cols-2 gap-3">
        <div class="space-y-1.5">
          <Label>规则名称</Label>
          <Input placeholder="例如：新丰田预审" bind:value={form.name} />
        </div>
        <div class="space-y-1.5">
          <Label>优先级（越小越优先）</Label>
          <Input type="number" bind:value={form.priority} />
        </div>
      </div>

      <div class="space-y-2">
        <Label>触发条件（全部满足 AND）</Label>
        {#each form.conditions as c, i}
          <div class="flex items-center gap-2">
            <SelectRoot type="single" value={c.field} onValueChange={(v) => (c.field = v)}>
              <SelectTrigger class="h-8 w-32"><span>{FIELD_LABELS[c.field] ?? c.field}</span></SelectTrigger>
              <SelectContent>
                {#each FIELD_OPTIONS as f}<SelectItem value={f}>{FIELD_LABELS[f]}</SelectItem>{/each}
              </SelectContent>
            </SelectRoot>
            <SelectRoot type="single" value={c.op} onValueChange={(v) => (c.op = v)}>
              <SelectTrigger class="h-8 w-24"><span>{OP_LABELS[c.op] ?? c.op}</span></SelectTrigger>
              <SelectContent>
                {#each OP_OPTIONS as o}<SelectItem value={o}>{OP_LABELS[o]}</SelectItem>{/each}
              </SelectContent>
            </SelectRoot>
            <Input class="h-8 flex-1" placeholder="值…" bind:value={c.value} />
            <Button size="icon" variant="ghost" class="h-8 w-8" disabled={form.conditions.length <= 1}
              onclick={() => form.conditions.splice(i, 1)}><XIcon class="size-3.5" /></Button>
          </div>
        {/each}
        <Button size="sm" variant="outline" class="h-7"
          onclick={() => form.conditions = [...form.conditions, { field: 'content', op: 'contains', value: '' }]}>
          <PlusIcon class="size-3.5" />添加条件
        </Button>
      </div>

      <div class="space-y-2">
        <Label>AI 分析（提取业务字段）</Label>
        {#each form.analyzeFields as f, i}
          <div class="flex items-center gap-2">
            <Input class="h-8 w-40" placeholder="字段名，如 购车价格" bind:value={f.name} />
            <Input class="h-8 flex-1" placeholder="说明（可选），如 万元" bind:value={f.desc} />
            <Button size="icon" variant="ghost" class="h-8 w-8" disabled={form.analyzeFields.length <= 1}
              onclick={() => form.analyzeFields.splice(i, 1)}><XIcon class="size-3.5" /></Button>
          </div>
        {/each}
        <Button size="sm" variant="outline" class="h-7"
          onclick={() => form.analyzeFields = [...form.analyzeFields, { name: '', desc: '' }]}>
          <PlusIcon class="size-3.5" />添加字段
        </Button>
      </div>

      <div class="space-y-1.5">
        <Label>分析提示词（可选，覆盖自动生成）</Label>
        <Textarea rows={3} placeholder="可手动编写完整分析指令…" bind:value={form.promptOverride} />
      </div>

      <div class="grid grid-cols-2 gap-3">
        <div class="space-y-1.5">
          <Label>分析模型（提供方）</Label>
          <Input placeholder="提供方 ID（留空则不做 AI 分析）" bind:value={form.providerId} />
        </div>
        <div class="space-y-1.5">
          <Label>模型</Label>
          <Input placeholder="模型名（留空用默认）" bind:value={form.model} />
        </div>
      </div>

      <div class="space-y-1.5">
        <Label>绑定 AI 角色（可选，内置 Worker 执行时注入角色提示词）</Label>
        <SelectRoot type="single" value={form.roleId} onValueChange={(v) => (form.roleId = v ?? '')}>
          <SelectTrigger class="h-8 w-full">
            <span>{roleOptions.find((r) => r.id === form.roleId)?.name ?? '不绑定（使用默认执行提示词）'}</span>
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="">不绑定（默认提示词）</SelectItem>
            {#each roleOptions as r}<SelectItem value={r.id}>{r.name}</SelectItem>{/each}
          </SelectContent>
        </SelectRoot>
      </div>

      <div class="space-y-2">
        <Label>派发方式</Label>
        <RadioGroup bind:value={form.dispatchMode} class="flex gap-4">
          <label class="flex items-center gap-2 text-sm"><RadioGroupItem value="fixed" />固定派发</label>
          <label class="flex items-center gap-2 text-sm"><RadioGroupItem value="ai" />AI 决策派发</label>
        </RadioGroup>
        {#if form.dispatchMode === 'fixed'}
          <div class="flex items-center gap-2">
            <SelectRoot type="single" value={form.targetType} onValueChange={(v) => (form.targetType = v)}>
              <SelectTrigger class="h-8 w-32"><span>{form.targetType === 'agent' ? '智能体' : '已接入 Agent'}</span></SelectTrigger>
              <SelectContent>
                <SelectItem value="agent">智能体</SelectItem>
                <SelectItem value="agent_instance">已接入 Agent</SelectItem>
              </SelectContent>
            </SelectRoot>
            {#if form.targetType === 'agent'}
              <SelectRoot type="single" value={form.targetId} onValueChange={(v) => (form.targetId = v)}>
                <SelectTrigger class="h-8 flex-1"><span>{agentOptions.find((a) => String(a.id) === form.targetId)?.name ?? '选择智能体…'}</span></SelectTrigger>
                <SelectContent>
                  {#each agentOptions as a}<SelectItem value={String(a.id)}>{a.name}</SelectItem>{/each}
                </SelectContent>
              </SelectRoot>
            {:else}
              <Input class="h-8 flex-1" placeholder="Agent ID" bind:value={form.targetId} />
            {/if}
          </div>
        {:else}
          <p class="text-xs text-muted-foreground">命中后由大模型分析消息并决定是否派发、派发给哪个智能体/Agent。</p>
        {/if}
      </div>
    </div>
    <DialogFooter>
      <Button variant="outline" onclick={() => (ruleDialogOpen = false)}>取消</Button>
      <Button onclick={saveRule}>保存规则</Button>
    </DialogFooter>
  </DialogContent>
</DialogRoot>

<!-- ═══════ 任务详情抽屉 ═══════ -->
<SheetRoot open={taskDetail !== null} onOpenChange={(o) => !o && (taskDetail = null)}>
  <SheetContent side="right" class="flex w-[560px] max-w-[94vw] flex-col gap-0 p-0 sm:max-w-[560px]">
    <SheetHeader class="border-b px-5 py-4">
      <SheetTitle class="text-sm">任务 #{taskDetail?.id}</SheetTitle>
      <SheetDescription>{taskDetail?.ruleName || '未命中规则'} · {@html statusBadge(taskDetail?.status ?? '')}</SheetDescription>
    </SheetHeader>
    {#if taskDetail}
      {@const t = taskDetail}
      <div class="flex-1 space-y-4 overflow-y-auto p-5 text-xs">
        <div class="space-y-1.5 rounded-lg border p-3">
          <div class="font-semibold text-foreground">原始消息</div>
          <div class="break-words">{t.content || '[媒体消息]'}</div>
          <div class="flex flex-wrap gap-x-4 gap-y-1 text-[11px] text-muted-foreground">
            <span>发送人 <b class="font-mono text-foreground">{t.senderUsername}</b></span>
            <span>会话 <b class="font-mono text-foreground">{t.username}</b></span>
            <span>类型 {mediaLabel(t.mediaType)}</span>
            <span>时间 {fmtTs(t.timestamp)}</span>
            {#if t.error}
              <span class="text-destructive">错误：{t.error}{t.retryCount > 0 ? `（已自动重试 ${t.retryCount} 次）` : ''}</span>
            {/if}
          </div>
        </div>

        <div class="space-y-1.5 rounded-lg border p-3">
          <div class="flex items-center justify-between">
            <span class="font-semibold text-foreground">AI 提取结果</span>
            <Button size="sm" variant="ghost" class="h-6 px-2" onclick={() => {
              const v = prompt('编辑 AI 提取 JSON：', JSON.stringify(t.aiExtract ?? {}, null, 2));
              if (v != null) saveAiExtract(t, v);
            }}><PencilIcon class="size-3" />编辑</Button>
          </div>
          <pre class="max-h-56 overflow-auto rounded bg-muted/40 p-2 font-mono text-[11px]">{JSON.stringify(t.aiExtract ?? {}, null, 2)}</pre>
        </div>

        <div class="space-y-1.5 rounded-lg border p-3">
          <div class="font-semibold text-foreground">派发与执行</div>
          <div class="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
            当前目标：
            <span class="rounded bg-muted px-1.5 py-0.5">{t.targetId ? `${t.targetType === 'agent' ? '智能体' : 'Agent'} #${t.targetId}` : '未派发'}</span>
          </div>
          <div class="flex flex-wrap gap-2 pt-1">
            <SelectRoot type="single" value={t.targetType} onValueChange={(v) => (t.targetType = v)}>
              <SelectTrigger class="h-8 w-32"><span>{t.targetType === 'agent' ? '智能体' : 'Agent'}</span></SelectTrigger>
              <SelectContent>
                <SelectItem value="agent">智能体</SelectItem>
                <SelectItem value="agent_instance">Agent</SelectItem>
              </SelectContent>
            </SelectRoot>
            <SelectRoot type="single" value={t.targetId} onValueChange={(v) => (t.targetId = v)}>
              <SelectTrigger class="h-8 flex-1"><span>{agentOptions.find((a) => String(a.id) === t.targetId)?.name ?? '选择目标…'}</span></SelectTrigger>
              <SelectContent>
                {#each agentOptions as a}<SelectItem value={String(a.id)}>{a.name}</SelectItem>{/each}
              </SelectContent>
            </SelectRoot>
            <Button size="sm" class="h-8" onclick={() => dispatchTask(t, t.targetType, t.targetId)}>派发</Button>
          </div>
        </div>

        <div class="space-y-1.5 rounded-lg border p-3">
          <div class="font-semibold text-foreground">AI 回复文本</div>
          <Textarea rows={3} bind:value={t.replyText} placeholder="智能体处理结果 / 人工编辑回复内容" />
          <div class="flex gap-2 pt-1">
            <Button size="sm" variant="outline" class="h-8" onclick={() => saveReply(t, t.replyText, 'to_reply')}>保存为待回复</Button>
            <Button size="sm" variant="outline" class="h-8" onclick={() => saveReply(t, t.replyText, 'replied')}>标记已回复</Button>
            <Button size="sm" variant="outline" class="h-8" onclick={() => saveReply(t, t.replyText, 'done')}>标记完成</Button>
          </div>
        </div>

        <div class="flex flex-wrap gap-2">
          <Button size="sm" variant="outline" class="h-8" onclick={() => setTaskStatus(t, 'pending')}>重置待处理</Button>
          <Button size="sm" variant="outline" class="h-8" onclick={() => setTaskStatus(t, 'ignored')}>标记忽略</Button>
          <Button size="sm" variant="destructive" class="h-8" onclick={() => { removeTask(t); taskDetail = null; }}>删除任务</Button>
          <div class="flex-1"></div>
          <Button size="sm" class="h-8" onclick={() => (taskDetail = null)}>关闭</Button>
        </div>
      </div>
    {/if}
  </SheetContent>
</SheetRoot>
