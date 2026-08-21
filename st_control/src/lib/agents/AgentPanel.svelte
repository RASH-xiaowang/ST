<script lang="ts">
  import { onMount } from 'svelte';
  import { toast } from 'svelte-sonner';
  import { agentToForm, createBlankAgentForm, type AgentInput } from './agentForm';
  import { onLlmConfigChanged } from '../llm/store.svelte';
  import type { AiRole } from '../llm/types';
  import { llmApi } from '../llm/services/ipc';
  import { agentApi, type AgentItem } from './services/ipc';
  import { kbApi } from '../kb/services/ipc';
  import type { KbSummary, ModelInfo } from '../kb/kbTypes';
  import { agents } from '../communication';
  import type { AgentInfo } from '../communication/types';
  import BotIcon from '@lucide/svelte/icons/bot';
  import { Input } from '../components/ui/input';
  import { Label } from '../components/ui/label';
  import { Textarea } from '../components/ui/textarea';
  import { Badge } from '../components/ui/badge';
  import { Tabs, TabsList, TabsTrigger } from '../components/ui/tabs';
  import { Root as SelectRoot } from '../components/ui/select';
  import {
    SelectContent,
    SelectItem,
    SelectTrigger,
  } from '../components/ui/select';
  import { Root as AlertDialogRoot } from '../components/ui/alert-dialog';
  import {
    AlertDialogAction,
    AlertDialogCancel,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
  } from '../components/ui/alert-dialog';
  import { RippleButton } from 'fancy-ui-svelte';

  interface Props {
    onOpenAgent?: (agent: AgentInfo) => void;
  }
  let { onOpenAgent }: Props = $props();

  interface Msg { role: 'user' | 'assistant'; content: string; }

  let agentList = $state<AgentItem[]>([]);
  let roles = $state<AiRole[]>([]);
  let models = $state<ModelInfo[]>([]);
  let kbs = $state<KbSummary[]>([]);

  let editingId = $state<number | null>(null);
  let form = $state<AgentInput>(createBlankAgentForm());
  let formBusy = $state(false);
  let formErr = $state('');

  let messages = $state<Msg[]>([]);
  let chatInput = $state('');
  let chatBusy = $state(false);
  let streamText = $state<string | null>(null);
  let msgEl = $state<HTMLDivElement | null>(null);
  let delAgent = $state<AgentItem | null>(null);

  const selectedAgent = $derived(agentList.find((a) => a.id === editingId) ?? null);
  let view = $state<'studio' | 'connected'>('studio');
  const connectedCount = $derived($agents.length);

  const roleItems = $derived([
    { value: '', label: '不绑定角色' },
    ...roles.map((r) => ({ value: r.id, label: `${r.emoji || ''} ${r.name}`.trim(), meta: r.description })),
  ]);
  const providerOptions = $derived([...new Set(models.map((m) => m.providerId))]);
  const providerItems = $derived(
    providerOptions.map((pid) => ({
      value: pid,
      label: models.find((m) => m.providerId === pid)?.providerName ?? pid,
    })),
  );
  const modelItems = $derived(
    models
      .filter((m) => m.providerId === form.providerId)
      .map((m) => ({ value: m.model, label: m.model + (m.isDefault ? '（默认）' : ''), meta: m.modelType ?? undefined })),
  );
  const kbItems = $derived([
    { value: '', label: '不绑定知识库', meta: '' },
    ...kbs.map((k) => ({ value: String(k.id), label: k.name, meta: `${k.docCount} 文档` })),
  ]);

  async function loadAll() {
    try { agentList = await agentApi.list(); } catch { agentList = []; }
    try { roles = await llmApi.getAiRoles() as AiRole[]; } catch { roles = []; }
    await loadModels();
    try { kbs = await kbApi.list(1); } catch { kbs = []; }
  }
  async function loadModels() {
    try { models = await kbApi.listModels(); } catch { models = []; }
  }

  function blankForm() {
    editingId = null;
    form = createBlankAgentForm();
    formErr = '';
    messages = [];
  }
  function editAgent(a: AgentItem) {
    editingId = a.id;
    form = agentToForm(a);
    formErr = '';
    messages = [];
  }
  async function saveAgent() {
    if (!form.name.trim()) { formErr = '请输入智能体名称'; return; }
    formBusy = true; formErr = '';
    const input: AgentInput = {
      name: form.name.trim(), description: form.description || null,
      roleId: form.roleId || null, providerId: form.providerId || null,
      model: form.model || null, kbId: form.kbId || null,
      temperature: form.temperature, maxTokens: form.maxTokens, topP: form.topP,
    };
    try {
      if (editingId === null) {
        const id = await agentApi.create(input);
        await loadAll();
        const created = agentList.find((a) => a.id === id);
        if (created) editAgent(created);
      } else {
        await agentApi.update(editingId, input);
        await loadAll();
        const updated = agentList.find((a) => a.id === editingId);
        if (updated) editAgent(updated);
      }
      toast.success('智能体已保存');
    } catch (e: unknown) { formErr = '保存失败：' + e; }
    finally { formBusy = false; }
  }
  async function deleteAgent(id: number) {
    try {
      await agentApi.remove(id);
      await loadAll();
      if (editingId === id) blankForm();
      toast.success('智能体已删除');
    } catch (e: unknown) { toast.error('删除失败：' + e); }
  }

  async function confirmDeleteAgent() {
    if (!delAgent) return;
    const id = delAgent.id;
    delAgent = null;
    await deleteAgent(id);
  }

  async function sendChat() {
    if (!selectedAgent || !chatInput.trim() || chatBusy) return;
    chatBusy = true; streamText = '';
    const q = chatInput.trim();
    chatInput = '';
    messages = [...messages, { role: 'user', content: q }];
    let acc = '';
    let err = '';
    try {
      await agentApi.chatStream(selectedAgent.id, q, (m: string) => {
        try {
          const f = JSON.parse(m);
          if (f.type === 'delta') { acc += f.content ?? ''; streamText = acc; }
          else if (f.type === 'done') { acc = f.content ?? acc; streamText = acc; }
          else if (f.type === 'error') { err = f.message ?? '调用失败'; }
        } catch { /* 忽略坏帧 */ }
      });
      messages = [...messages, { role: 'assistant', content: err ? '（调用失败：' + err + '）' : acc }];
    } catch (e: unknown) {
      messages = [...messages, { role: 'assistant', content: '（调用失败：' + e + '）' }];
    } finally {
      chatBusy = false;
      streamText = null;
    }
  }

  $effect(() => {
    void messages;
    void streamText;
    if (!msgEl) return;
    msgEl.scrollTop = msgEl.scrollHeight;
  });
  onMount(() => {
    // 大模型管理配置变化时实时刷新模型下拉列表（无需人工刷新）
    const unsub = onLlmConfigChanged(loadModels);
    loadAll();
    blankForm();
    return () => unsub();
  });
</script>

<div class="flex h-full flex-col gap-3 p-1">
  <Tabs bind:value={view}>
    <TabsList>
      <TabsTrigger value="studio">智能体工作台</TabsTrigger>
      <TabsTrigger value="connected">已接入 Agent（{connectedCount}）</TabsTrigger>
    </TabsList>
  </Tabs>

  {#if view === 'studio'}
    <!-- 三栏：左列表 250 / 中表单自适应 / 右对话 460 -->
    <div class="grid min-h-0 flex-1 grid-cols-1 gap-3 lg:grid-cols-[250px_minmax(0,1fr)_460px]">
      <aside class="flex min-h-0 flex-col overflow-hidden rounded-lg border bg-card">
        <div class="flex items-center justify-between border-b px-3 py-2.5">
          <span class="text-sm font-semibold">智能体</span>
          <RippleButton
            onclick={blankForm}
            rippleColor="#22d3ee"
            title="新建智能体"
            class="h-7 rounded-md border border-[var(--border)] bg-transparent px-2.5 text-xs font-medium text-[var(--foreground)] hover:bg-[var(--muted)]"
          >新建</RippleButton>
        </div>
        <div class="flex-1 space-y-2 overflow-y-auto p-2">
          {#each agentList as a}
            <div
              class="group relative cursor-pointer rounded-md border p-2.5 transition-colors hover:border-primary/50 {editingId === a.id ? 'border-primary bg-accent/40' : ''}"
              role="button"
              tabindex="0"
              onclick={() => editAgent(a)}
              onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); editAgent(a); } }}
            >
              <div class="text-sm font-medium">{a.name}</div>
              {#if a.description}<div class="mt-0.5 line-clamp-2 text-xs text-muted-foreground">{a.description}</div>{/if}
              <div class="mt-1.5 flex flex-wrap gap-1">
                {#if a.model}<Badge variant="secondary" class="text-[11px]">{a.model}</Badge>{/if}
                {#if a.kbId}<Badge variant="secondary" class="text-[11px]">知识库</Badge>{/if}
              </div>
              <button
                class="absolute right-1.5 top-1.5 hidden rounded p-1 text-muted-foreground hover:bg-destructive/15 hover:text-destructive group-hover:block"
                title="删除"
                onclick={(e) => { e.stopPropagation(); delAgent = a; }}
              >✕</button>
            </div>
          {/each}
          {#if agentList.length === 0}
            <div class="py-10 text-center text-xs text-muted-foreground">还没有智能体<br />点击右上角「新建」创建</div>
          {/if}
        </div>
      </aside>

      <section class="flex min-h-0 flex-col overflow-hidden rounded-lg border bg-card">
        <div class="border-b px-4 py-2.5">
          <div class="text-sm font-semibold">{editingId === null ? '新建智能体' : '编辑智能体'}</div>
          <div class="text-xs text-muted-foreground">绑定 AI 角色 · 大模型 · 知识库</div>
        </div>
        <div class="flex-1 space-y-4 overflow-y-auto p-4">
          <div class="space-y-1.5">
            <Label for="agent-name">名称</Label>
            <Input id="agent-name" placeholder="例如：金融客服助手" bind:value={form.name} maxlength={50} />
          </div>
          <div class="space-y-1.5">
            <Label for="agent-desc">描述</Label>
            <Textarea id="agent-desc" rows={2} placeholder="用途、适用场景（可选）" bind:value={form.description} />
          </div>
          <div class="space-y-1.5">
            <Label for="agent-role">AI 角色</Label>
            <SelectRoot type="single" value={form.roleId ?? ''} onValueChange={(v) => (form.roleId = v)}>
              <SelectTrigger id="agent-role" class="w-full">
                <span>{roleItems.find((r) => r.value === (form.roleId ?? ''))?.label ?? '不绑定角色'}</span>
              </SelectTrigger>
              <SelectContent>
                {#each roleItems as r}<SelectItem value={r.value}>{r.label}</SelectItem>{/each}
              </SelectContent>
            </SelectRoot>
          </div>
          <div class="space-y-1.5">
            <Label>大模型</Label>
            <div class="flex gap-2">
              <SelectRoot
                type="single"
                value={form.providerId ?? ''}
                onValueChange={(v) => { form.providerId = v; form.model = ''; }}
              >
                <SelectTrigger class="w-full flex-1">
                  <span>{providerItems.find((p) => p.value === form.providerId)?.label ?? '选择提供方'}</span>
                </SelectTrigger>
                <SelectContent>
                  {#each providerItems as p}<SelectItem value={p.value}>{p.label}</SelectItem>{/each}
                </SelectContent>
              </SelectRoot>
              <SelectRoot
                type="single"
                value={form.model ?? ''}
                disabled={!form.providerId}
                onValueChange={(v) => (form.model = v)}
              >
                <SelectTrigger class="w-full flex-[1.4]">
                  <span>{modelItems.find((m) => m.value === form.model)?.label ?? '选择模型'}</span>
                </SelectTrigger>
                <SelectContent>
                  {#each modelItems as m}<SelectItem value={m.value}>{m.label}</SelectItem>{/each}
                </SelectContent>
              </SelectRoot>
            </div>
          </div>
          <div class="space-y-1.5">
            <Label for="agent-kb">知识库</Label>
            <SelectRoot type="single" value={form.kbId === null ? '' : String(form.kbId)} onValueChange={(v) => (form.kbId = v === '' ? null : Number(v))}>
              <SelectTrigger class="w-full">
                <span>{kbItems.find((k) => k.value === (form.kbId === null ? '' : String(form.kbId)))?.label ?? '不绑定知识库'}</span>
              </SelectTrigger>
              <SelectContent>
                {#each kbItems as k}<SelectItem value={k.value}>{k.label}{k.meta ? `（${k.meta}）` : ''}</SelectItem>{/each}
              </SelectContent>
            </SelectRoot>
          </div>
          <div class="space-y-1.5">
            <Label for="agent-temp">温度 {form.temperature?.toFixed(1) ?? '0.7'}</Label>
            <input id="agent-temp" type="range" min="0" max="2" step="0.1" bind:value={form.temperature} class="w-full accent-primary" />
          </div>
          {#if formErr}<div class="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive">{formErr}</div>{/if}
          <RippleButton
            onclick={saveAgent}
            disabled={formBusy}
            rippleColor="#a5f3fc"
            class="h-9 w-full rounded-md border-0 bg-[var(--primary)] px-4 text-sm font-medium text-[var(--primary-foreground)] hover:opacity-90"
          >{formBusy ? '保存中…' : '保存智能体'}</RippleButton>
        </div>
      </section>

      <section class="flex min-h-0 flex-col overflow-hidden rounded-lg border bg-card">
        <div class="flex items-center justify-between border-b px-4 py-2.5">
          <span class="text-sm font-semibold">对话测试</span>
          {#if selectedAgent}
            <Badge variant="secondary">{selectedAgent.model || '未配置模型'}</Badge>
          {:else}
            <span class="text-xs text-muted-foreground">选择或新建智能体后开始对话</span>
          {/if}
        </div>
        <div class="kb-agent-msgs flex-1 overflow-y-auto p-3" bind:this={msgEl}>
          {#if messages.length === 0 && streamText === null}
            <div class="py-12 text-center text-xs text-muted-foreground">
              <div class="mb-1 text-lg">💬</div>
              {selectedAgent ? '向智能体提问，回答将流式展示' : '暂无对话'}
              <div class="mt-1 text-[11px] opacity-70">对话 token 计入「大模型管理」流量与成本</div>
            </div>
          {/if}
          {#each messages as m}
            <div class="mb-2 flex" class:justify-end={m.role === 'user'}>
              <div
                class="max-w-[80%] whitespace-pre-wrap rounded-xl px-3 py-2 text-sm"
                class:bg-primary={m.role === 'user'}
                class:text-primary-foreground={m.role === 'user'}
                class:bg-muted={m.role === 'assistant'}
              >{m.content}</div>
            </div>
          {/each}
          {#if chatBusy && streamText !== null}
            <div class="mb-2 flex">
              <div class="max-w-[80%] whitespace-pre-wrap rounded-xl bg-muted px-3 py-2 text-sm">
                {streamText || '思考中…'}<span class="kb-cursor"></span>
              </div>
            </div>
          {/if}
        </div>
        <div class="flex gap-2 border-t p-3">
          <Textarea
            class="min-h-[52px] flex-1 resize-none"
            rows={2}
            placeholder="输入要发送的内容…"
            bind:value={chatInput}
            onkeydown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendChat(); } }}
          />
          <RippleButton
            onclick={sendChat}
            disabled={!selectedAgent || chatBusy || !chatInput.trim()}
            rippleColor="#a5f3fc"
            class="h-9 self-end rounded-md border-0 bg-[var(--primary)] px-4 text-sm font-medium text-[var(--primary-foreground)] hover:opacity-90"
          >
            {chatBusy ? '生成中…' : '发送'}
          </RippleButton>
        </div>
      </section>
    </div>
  {:else}
    <div class="flex-1 overflow-y-auto">
      <div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
        {#each $agents as a}
          <button
            type="button"
            class="flex items-center gap-3 rounded-lg border bg-card p-3.5 text-left transition-[border-color,box-shadow] hover:border-primary/50 hover:shadow-[0_0_0_1px_color-mix(in_srgb,#22d3ee_18%,transparent),0_10px_30px_-18px_rgba(34,211,238,0.4)]"
            onclick={() => onOpenAgent?.(a)}
          >
            <span class="size-2 shrink-0 rounded-full bg-emerald-500 shadow-[0_0_8px_#22c55e]"></span>
            <span class="min-w-0 flex-1">
              <span class="block truncate text-sm font-semibold">{a.name}</span>
              <span class="block truncate font-mono text-xs text-muted-foreground">{a.remoteAddr}</span>
            </span>
            <span class="shrink-0 text-[11px] text-muted-foreground">接入 {new Date(a.connectedAt).toLocaleString()}</span>
          </button>
        {/each}
        {#if $agents.length === 0}
          <div class="col-span-full flex min-h-[80vh] flex-col items-center justify-center gap-3 rounded-lg border border-dashed py-16 text-center">
            <BotIcon class="size-9 text-[var(--muted-foreground)]/60" />
            <div class="text-sm font-semibold text-[var(--foreground)]">暂无 Agent 接入</div>
            <div class="text-xs text-muted-foreground">
              在另一台设备启动 st_agent 客户端，配置控制台地址后即自动上线；
              接入后可在「智能体工作台」创建智能体并远程下发任务。
            </div>
          </div>
        {/if}
      </div>
    </div>
  {/if}

  <AlertDialogRoot open={delAgent !== null} onOpenChange={(o) => !o && (delAgent = null)}>
    <AlertDialogContent>
      <AlertDialogHeader>
        <AlertDialogTitle>删除智能体「{delAgent?.name}」</AlertDialogTitle>
        <AlertDialogDescription>确定删除该智能体吗？此操作不可恢复。</AlertDialogDescription>
      </AlertDialogHeader>
      <AlertDialogFooter>
        <AlertDialogCancel onclick={() => (delAgent = null)}>取消</AlertDialogCancel>
        <AlertDialogAction onclick={confirmDeleteAgent}>删除</AlertDialogAction>
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialogRoot>
</div>

<style>
  .kb-cursor { display: inline-block; width: 2px; height: 14px; margin-left: 2px; vertical-align: -2px; background: currentColor; animation: kb-blink 1s steps(2) infinite; }
  @keyframes kb-blink { 50% { opacity: 0; } }
</style>
