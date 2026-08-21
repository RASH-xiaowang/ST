<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { stepState, type StepKey } from './steps';
import { fileMetaOf } from './fileMeta';
  import { botApi } from './services/ipc';
  import { toast } from 'svelte-sonner';
  import { Button } from '../components/ui/button';
  import { Textarea } from '../components/ui/textarea';
  import { Badge } from '../components/ui/badge';
  import { Card, CardContent } from '../components/ui/card';
  import { Spinner } from '../components/ui/spinner';
  import QrCodeDialog from './QrCodeDialog.svelte';
  import ChannelConfigDialog from './ChannelConfigDialog.svelte';
  import BotLogView from './BotLogView.svelte';
  import { STATUS_META, countdown, PLATFORM_META } from './types';
  import type { BotAccount, BotPlatform, BotStatusSummary, QqbotContact } from './types';
  import BotIcon from '@lucide/svelte/icons/bot';
  import PlusIcon from '@lucide/svelte/icons/plus';
  import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
  import Trash2Icon from '@lucide/svelte/icons/trash-2';
  import SendIcon from '@lucide/svelte/icons/send';
  import PaperclipIcon from '@lucide/svelte/icons/paperclip';
  import MessageSquareIcon from '@lucide/svelte/icons/message-square';
  import QrCodeIcon from '@lucide/svelte/icons/qr-code';
  import AlertTriangleIcon from '@lucide/svelte/icons/alert-triangle';
  import PencilIcon from '@lucide/svelte/icons/pencil';
  import CheckIcon from '@lucide/svelte/icons/check';
  import XIcon from '@lucide/svelte/icons/x';
  import FileIcon from '@lucide/svelte/icons/file';
  import ImageIcon from '@lucide/svelte/icons/image';
  import FilmIcon from '@lucide/svelte/icons/film';
  import MusicIcon from '@lucide/svelte/icons/music';
  import LoaderCircleIcon from '@lucide/svelte/icons/loader-circle';
  import ShieldCheckIcon from '@lucide/svelte/icons/shield-check';
  import Settings2Icon from '@lucide/svelte/icons/settings-2';
  import ZapIcon from '@lucide/svelte/icons/zap';
  import UsersIcon from '@lucide/svelte/icons/users';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import { listen } from '@tauri-apps/api/event';

  let accounts = $state<BotAccount[]>([]);
  let summary = $state<BotStatusSummary>({ total: 0, online: 0, expired: 0, error: 0 });
  let loading = $state(true);
  let platform = $state<BotPlatform>('wechat');
  let qrOpen = $state(false);
  let qrAccountId = $state<number | null>(null);
  let configOpen = $state(false);
  let editingAccount = $state<BotAccount | null>(null);
  let selectedId = $state<number | null>(null);
  let renamingId = $state<number | null>(null);
  let renameDraft = $state('');
  let testingId = $state<number | null>(null);

  // 发送台
  let sendText = $state('');
  let selectedFile = $state<string | null>(null);
  let sending = $state(false);
  let sendStage = $state<'idle' | 'preparing' | 'uploading' | 'sending' | 'done' | 'error'>('idle');
  let sendError = $state('');
  let traceMode = $state<'idle' | 'text' | 'media'>('idle');
  // QQ 官方机器人发送目标（可在发送时临时覆盖）
  let qqTargetType = $state<'private' | 'group'>('private');
  let qqTarget = $state('');
  // QQ 官方机器人：网关自动收集到的 openid 目标
  let qqbotContacts = $state<QqbotContact[]>([]);
  let qqbotContactsLoading = $state(false);

  async function loadQqbotContacts(accountId: number | null) {
    if (!accountId) {
      qqbotContacts = [];
      return;
    }
    qqbotContactsLoading = true;
    try {
      qqbotContacts = await botApi.listQqbotContacts(accountId);
    } catch {
      qqbotContacts = [];
    } finally {
      qqbotContactsLoading = false;
    }
  }

  let unlisteners: UnlistenFn[] = [];

  async function refresh() {
    try {
      const [accs, s] = await Promise.all([botApi.listAccounts(), botApi.statusSummary()]);
      accounts = accs;
      summary = s;
      const list = accs.filter((a) => a.platform === platform);
      if (selectedId && !list.some((a) => a.id === selectedId)) {
        selectedId = list[0]?.id ?? null;
      }
      if (!selectedId && list.length) selectedId = list[0].id;
    } catch (e) {
      toast.error(`加载账号失败：${e}`);
    } finally {
      loading = false;
    }
  }

  function openBind() {
    qrAccountId = null;
    qrOpen = true;
  }

  function openAddConfig() {
    editingAccount = null;
    configOpen = true;
  }

  function openEditConfig(acc: BotAccount) {
    editingAccount = acc;
    configOpen = true;
  }

  function switchPlatform(p: BotPlatform) {
    platform = p;
    const list = accounts.filter((a) => a.platform === p);
    selectedId = list[0]?.id ?? null;
  }

  async function testChannel(acc: BotAccount) {
    testingId = acc.id;
    try {
      await botApi.testChannel(acc.id);
      toast.success('测试消息已发送，请到目标群 / 会话查看');
    } catch (e) {
      toast.error(`测试失败：${e}`);
    } finally {
      testingId = null;
    }
  }

  function rebind(accountId: number) {
    qrAccountId = accountId;
    qrOpen = true;
  }

  function startRename(accountId: number, name: string) {
    renameDraft = name;
    renamingId = accountId;
  }

  function cancelRename() {
    renamingId = null;
    renameDraft = '';
  }

  async function saveRename() {
    if (renamingId === null) return;
    if (!renameDraft.trim()) return toast.error('名称不能为空');
    try {
      await botApi.renameAccount(renamingId, renameDraft.trim());
      toast.success('已重命名');
      renamingId = null;
      await refresh();
    } catch (e) {
      toast.error(`重命名失败：${e}`);
    }
  }

  async function unbind(accountId: number, name: string) {
    if (!confirm(`确认解绑「${name}」？解绑后该微信机器人将断开。`)) return;
    try {
      await botApi.unbindAccount(accountId);
      toast.success('已解绑');
      await refresh();
    } catch (e) {
      toast.error(`解绑失败：${e}`);
    }
  }

  async function pickFile() {
    if (!selectedId) return;
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const chosen = await open({
        multiple: false,
        title: '选择要发送的文件 / 图片',
      });
      if (typeof chosen !== 'string') return; // 用户取消
      selectedFile = chosen;
      sendStage = 'idle';
      sendError = '';
    } catch (err) {
      toast.error(`选择文件失败：${err}`);
    }
  }

  function clearFile() {
    selectedFile = null;
  }

  async function send() {
    if (!selectedId) return toast.error('请先选择账号');
    const acc = accounts.find((a) => a.id === selectedId);
    if (!acc) return;
    let target = '';
    if (platform === 'wechat') {
      target = acc.ownerId ?? '';
      if (!target) return toast.error('该账号未记录绑定微信 ID，无法推送');
    } else if (platform === 'qqbot') {
      const id = qqTarget.trim() || acc.targetId || '';
      if (!id) {
        return toast.error('请输入推送目标 openid（或从下方列表选择）');
      }
      // QQ 官方机器人：openid 是 32 位十六进制串；纯数字的 QQ 号/群号必失败，直接拦截并指引
      if (/^\d{5,}$/.test(id)) {
        return toast.error(
          'QQ 官方机器人目标需填 openid（QQ 号 / 群号无效）：在群里 @机器人 发消息后，群 openid 会自动收集到列表，点击选择即可',
        );
      }
      target = `${qqTargetType}:${id}`;
    }

    const hasText = sendText.trim().length > 0;
    const file = selectedFile;
    const hasFile = !!file;
    if (!hasText && !hasFile) return toast.error('请输入内容或选择文件');

    sending = true;
    sendError = '';
    traceMode = hasFile ? 'media' : 'text';
    try {
      if (hasText) {
        sendStage = 'sending';
        await botApi.sendText(selectedId, target, sendText.trim());
      }
      if (hasFile) {
        sendStage = 'preparing';
        await botApi.sendMedia(selectedId, target, file!);
      }
      sendStage = 'done';
      toast.success(platform === 'wechat' ? '消息已发送' : `已发送到${PLATFORM_META[platform].label}`);
      sendText = '';
      selectedFile = null;
      window.dispatchEvent(new CustomEvent('bot-log-refresh'));
    } catch (err) {
      const msg = String(err);
      sendStage = 'error';
      sendError = msg;
      toast.error(`发送失败：${msg}`);
    } finally {
      sending = false;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      if (!sending) send();
    }
  }

  onMount(() => {
    refresh();
    void (async () => {
      unlisteners.push(
        await listen('bot://status', () => refresh()),
        await listen('bot://message', () => {
          window.dispatchEvent(new CustomEvent('bot-log-refresh'));
        }),
        await listen('bot://log', () => {
          window.dispatchEvent(new CustomEvent('bot-log-refresh'));
        }),
        await listen('bot://expiring', (e) => {
          const p = e.payload as { accountId: number; minutesLeft: number };
          toast.warning(`微信机器人（账号 ${p.accountId}）将在 ${p.minutesLeft} 分钟后过期，请重新扫码`);
        }),
      );
    })();
    window.addEventListener('bot-accounts-changed', refresh);
    const timer = setInterval(refresh, 60_000);
    return () => {
      window.removeEventListener('bot-accounts-changed', refresh);
      clearInterval(timer);
      unlisteners.forEach((u) => u());
    };
  });

  onDestroy(() => {
    unlisteners.forEach((u) => u());
  });

  const selected = $derived(accounts.find((a) => a.id === selectedId) ?? null);
  const remaining = $derived(selected ? countdown(selected.expiresAt) : { text: '--', urgent: false });
  const platformAccounts = $derived(accounts.filter((a) => a.platform === platform));
  const platformList = $derived(Object.keys(PLATFORM_META) as BotPlatform[]);
  const platformCounts = $derived(
    Object.fromEntries(
      platformList.map((p) => [p, accounts.filter((a) => a.platform === p).length]),
    ) as Record<BotPlatform, number>,
  );

  // 切换平台 / 账号时同步发送目标；qqbot 同时加载网关收集的 openid
  $effect(() => {
    const acc = selected;
    if (!acc || acc.platform !== 'qqbot') return;
    try {
      const c = acc.configJson ? JSON.parse(acc.configJson) : {};
      qqTargetType = c.target_type === 'group' ? 'group' : 'private';
      qqTarget = acc.targetId || c.target_id || '';
    } catch {
      /* 忽略配置解析失败 */
    }
    void loadQqbotContacts(acc.id);
  });

  // 文件类型与展示名
  const fileMeta = $derived(selectedFile ? fileMetaOf(selectedFile) : null);

  // 三段式发送进度
  const steps = $derived<{ key: StepKey; label: string }[]>([
    { key: 'prep', label: '准备' },
    { key: 'upload', label: '上传' },
    { key: 'send', label: '送达' },
  ]);

</script>

<QrCodeDialog bind:open={qrOpen} accountId={qrAccountId} />
<ChannelConfigDialog bind:open={configOpen} platform={platform} account={editingAccount} />

<div class="flex h-full min-h-0 flex-col gap-3 p-4">
  <!-- 顶栏 -->
  <div class="flex flex-wrap items-center justify-between gap-3">
    <div class="flex items-center gap-3">
      <div class="flex size-9 items-center justify-center rounded-xl border border-border bg-card">
        <BotIcon class="size-4.5 text-primary" />
      </div>
      <div>
        <div class="text-[15px] font-bold leading-tight">消息通道</div>
        <div class="mt-0.5 text-xs text-muted-foreground">
          微信 ClawBot（官方 iLink）· 多账号扫码 · 双向收发
        </div>
      </div>
    </div>
    <div class="flex items-center gap-2">
      <Badge variant="outline" class="gap-1.5 border-emerald-500/30 text-emerald-500">
        <span class="size-1.5 rounded-full bg-emerald-400 shadow-[0_0_6px] shadow-emerald-400/70"></span>
        在线 {summary.online}
      </Badge>
      {#if summary.expired > 0}
        <Badge variant="outline" class="gap-1.5 border-amber-500/30 text-amber-500">
          <span class="size-1.5 rounded-full bg-amber-400"></span>
          已过期 {summary.expired}
        </Badge>
      {/if}
      {#if summary.error > 0}
        <Badge variant="outline" class="gap-1.5 border-rose-500/30 text-rose-500">
          <AlertTriangleIcon class="size-3" />
          异常 {summary.error}
        </Badge>
      {/if}
      {#if platform === 'wechat'}
        <Button onclick={openBind} size="sm">
          <PlusIcon class="size-4" />
          绑定微信
        </Button>
      {:else}
        <Button onclick={openAddConfig} size="sm">
          <PlusIcon class="size-4" />
          添加{PLATFORM_META[platform].label}
        </Button>
      {/if}
    </div>
  </div>

  <!-- 平台切换 -->
  <div class="flex items-center gap-1 rounded-xl border border-border bg-card p-1">
    {#each platformList as p}
      {@const meta = PLATFORM_META[p]}
      <button
        type="button"
        class="flex flex-1 items-center justify-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium transition-colors {platform === p
          ? 'bg-accent font-semibold text-foreground shadow-[inset_0_-2px_0_0_var(--brand)]'
          : 'text-muted-foreground hover:bg-accent/50 hover:text-foreground'}"
        onclick={() => switchPlatform(p)}
      >
        {meta.label}
        <span class="rounded-full bg-muted px-1.5 py-px font-mono text-[11px] text-muted-foreground">
          {platformCounts[p]}
        </span>
      </button>
    {/each}
  </div>

  {#if accounts.length >= 5}
    <div class="flex items-center gap-2 rounded-lg border border-amber-500/25 bg-amber-500/5 px-3 py-2 text-xs text-amber-500">
      <AlertTriangleIcon class="size-3.5 shrink-0" />
      当前已绑定 {accounts.length} 个账号。官方接口存在限速风险，建议不超过 5 个（不做强制限制）。
    </div>
  {/if}

  {#if loading}
    <div class="flex flex-1 items-center justify-center text-muted-foreground">
      <Spinner class="size-6" />
    </div>
  {:else if platformAccounts.length === 0}
    <Card class="flex flex-1 items-center justify-center border-dashed">
      <CardContent class="flex flex-col items-center gap-3 py-16 text-center">
        <div class="flex size-14 items-center justify-center rounded-2xl border border-border bg-muted/40">
          <BotIcon class="size-7 text-muted-foreground" />
        </div>
        <div>
          <div class="font-medium">
            {platform === 'wechat' ? '还没有绑定微信机器人' : `还没有添加${PLATFORM_META[platform].label}通道`}
          </div>
          <div class="mt-1 max-w-sm text-sm text-muted-foreground">
            {PLATFORM_META[platform].desc}
          </div>
        </div>
        {#if platform === 'wechat'}
          <Button onclick={openBind}>
            <QrCodeIcon class="size-4" />
            立即绑定
          </Button>
        {:else}
          <Button onclick={openAddConfig}>
            <PlusIcon class="size-4" />
            添加配置
          </Button>
        {/if}
      </CardContent>
    </Card>
  {:else}
    <div class="grid min-h-0 flex-1 grid-cols-1 gap-3 xl:grid-cols-[300px_minmax(0,1fr)]">
      <!-- ── 账号栏 ── -->
      <div class="min-h-0 space-y-2 overflow-y-auto pr-0.5">
        {#each platformAccounts as acc (acc.id)}
          {@const active = selectedId === acc.id}
          {@const meta = STATUS_META[acc.status]}
          <div
            role="button"
            tabindex="0"
            class="group w-full cursor-pointer rounded-xl border p-3 text-left transition-colors {active
              ? 'border-primary/45 bg-accent/50'
              : 'border-border bg-card hover:bg-accent/30'}"
            onclick={() => (selectedId = acc.id)}
            onkeydown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                selectedId = acc.id;
              }
            }}
          >
            <div class="flex items-center gap-2">
              <span class="size-2 shrink-0 rounded-full {meta.dot} {acc.status === 'online'
                ? 'shadow-[0_0_6px] shadow-emerald-400/70'
                : ''}"></span>
              {#if renamingId === acc.id}
                <input
                  bind:value={renameDraft}
                  class="h-7 min-w-0 flex-1 rounded-md border border-border bg-background px-2 text-sm focus:outline-none"
                  placeholder="账号名称"
                  onclick={(e) => e.stopPropagation()}
                  onkeydown={(e) => {
                    e.stopPropagation();
                    if (e.key === 'Enter') saveRename();
                    if (e.key === 'Escape') cancelRename();
                  }}
                />
                <span class="flex shrink-0 items-center gap-0.5">
                  <button
                    type="button"
                    class="flex size-6 items-center justify-center rounded-md text-emerald-500 hover:bg-emerald-500/10"
                    onclick={(e) => { e.stopPropagation(); saveRename(); }}
                    title="保存"
                  >
                    <CheckIcon class="size-3.5" />
                  </button>
                  <button
                    type="button"
                    class="flex size-6 items-center justify-center rounded-md text-muted-foreground hover:bg-accent"
                    onclick={(e) => { e.stopPropagation(); cancelRename(); }}
                    title="取消"
                  >
                    <XIcon class="size-3.5" />
                  </button>
                </span>
              {:else}
                <span class="min-w-0 flex-1 truncate text-sm font-medium">{acc.name}</span>
              {/if}
              <Badge variant="outline" class={PLATFORM_META[acc.platform].badge}>
                {PLATFORM_META[acc.platform].short}
              </Badge>
              {#if acc.status !== 'online'}
                <Badge variant="outline" class={meta.cls}>{meta.label}</Badge>
              {/if}
            </div>
            <div class="mt-2 flex items-center justify-between gap-2 text-[11px] text-muted-foreground">
              <span class="truncate font-mono">{acc.botId.slice(0, 10) || '--'}</span>
              <span class={countdown(acc.expiresAt).urgent ? 'text-amber-500' : ''}>
                {countdown(acc.expiresAt).text}
              </span>
            </div>
            {#if acc.lastError}
              <div class="mt-1.5 line-clamp-2 text-[11px] leading-snug text-rose-500" title={acc.lastError}>
                {acc.lastError}
              </div>
            {/if}
            <div class="mt-2 flex items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100">
              <button
                type="button"
                class="flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-accent"
                onclick={(e) => { e.stopPropagation(); startRename(acc.id, acc.name); }}
                title="重命名账号"
              >
                <PencilIcon class="size-3" />
                改名
              </button>
              {#if acc.platform === 'wechat'}
                <button
                  type="button"
                  class="flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-accent"
                  onclick={(e) => { e.stopPropagation(); rebind(acc.id); }}
                  title="重新扫码"
                >
                  <RefreshCwIcon class="size-3" />
                  重扫
                </button>
              {:else}
                <button
                  type="button"
                  class="flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-accent"
                  onclick={(e) => { e.stopPropagation(); openEditConfig(acc); }}
                  title="编辑通道配置"
                >
                  <Settings2Icon class="size-3" />
                  编辑
                </button>
                <button
                  type="button"
                  class="flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-accent"
                  onclick={(e) => { e.stopPropagation(); testChannel(acc); }}
                  disabled={testingId === acc.id}
                  title="发送测试消息"
                >
                  {#if testingId === acc.id}
                    <LoaderCircleIcon class="size-3 animate-spin" />
                  {:else}
                    <ZapIcon class="size-3" />
                  {/if}
                  测试
                </button>
              {/if}
              <button
                type="button"
                class="flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] text-rose-500/80 hover:bg-rose-500/10"
                onclick={(e) => { e.stopPropagation(); unbind(acc.id, acc.name); }}
                title="解绑账号"
              >
                <Trash2Icon class="size-3" />
                解绑
              </button>
            </div>
          </div>
        {/each}
      </div>

      <!-- ── 工作区：发送台 + 日志 ── -->
      {#if selected}
        <div class="grid min-h-0 grid-cols-1 gap-3 2xl:grid-cols-[minmax(0,5fr)_minmax(0,6fr)]">
          <!-- 发送台 -->
          <Card class="min-h-0 overflow-hidden">
            <CardContent class="flex h-full min-h-0 flex-col gap-3 p-4">
              <div class="flex items-center gap-2 border-b border-border pb-3">
                <SendIcon class="size-4 text-primary" />
                <span class="text-sm font-bold">发送消息</span>
                <span class="ml-auto flex items-center gap-1.5 text-[11px] text-muted-foreground">
                  <span class="size-1.5 rounded-full bg-emerald-400 shadow-[0_0_6px] shadow-emerald-400/70"></span>
                  {platform === 'wechat' ? '推送给绑定微信本人' : `推送到${PLATFORM_META[platform].label}`}
                </span>
              </div>

              <!-- 推送对象 -->
              {#if platform === 'qqbot'}
                <div class="flex items-center gap-2 rounded-lg border border-border bg-muted/40 px-3 py-2">
                  <UsersIcon class="size-3.5 shrink-0 text-primary" />
                  <span class="shrink-0 text-xs font-medium">推送目标</span>
                  <div class="flex shrink-0 rounded-md border border-border bg-background p-0.5">
                    <button
                      type="button"
                      class="rounded px-2 py-1 text-[11px] transition-colors {qqTargetType === 'private' ? 'bg-accent text-foreground' : 'text-muted-foreground hover:text-foreground'}"
                      onclick={() => (qqTargetType = 'private')}
                    >
                      私聊
                    </button>
                    <button
                      type="button"
                      class="rounded px-2 py-1 text-[11px] transition-colors {qqTargetType === 'group' ? 'bg-accent text-foreground' : 'text-muted-foreground hover:text-foreground'}"
                      onclick={() => (qqTargetType = 'group')}
                    >
                      群聊
                    </button>
                  </div>
                  <input
                    bind:value={qqTarget}
                    list="qqbot-openid-list"
                    class="min-w-0 flex-1 rounded-md border border-border bg-background px-2 py-1 font-mono text-xs focus:outline-none"
                    placeholder={qqTargetType === 'group' ? '群 group_openid（默认取配置）' : '用户 openid（默认取配置）'}
                  />
                  <datalist id="qqbot-openid-list">
                    {#each qqbotContacts.filter((c) => c.kind === qqTargetType) as c}
                      <option value={c.openid}>{c.lastContent}</option>
                    {/each}
                  </datalist>
                </div>
                <div class="space-y-1.5">
                  <div class="flex items-center justify-between px-1">
                      <span class="text-[11px] font-medium text-muted-foreground">
                        openid 自动收集
                      </span>
                      <button
                        type="button"
                        class="flex items-center gap-1 text-[11px] text-primary hover:underline"
                        onclick={() => selected && void loadQqbotContacts(selected.id)}
                      >
                        <RefreshCwIcon class="size-3" />
                        刷新
                      </button>
                    </div>
                    {#if qqbotContactsLoading}
                      <p class="flex items-center gap-1.5 px-1 text-[11px] text-muted-foreground">
                        <LoaderCircleIcon class="size-3 animate-spin" />
                        正在读取…
                      </p>
                    {:else if qqbotContacts.length === 0}
                      <p class="rounded-lg border border-dashed border-border bg-muted/30 px-2 py-2 text-[11px] leading-relaxed text-muted-foreground">
                        还没有收集到 openid。让目标用户 / 群给机器人发一条消息，就会自动出现在这里，点击即可选中。
                        （需在 QQ 开放平台机器人控制台「消息配置」启用 C2C 消息与群消息事件）
                      </p>
                    {:else}
                      <div class="max-h-36 space-y-1 overflow-y-auto rounded-lg border border-border bg-muted/20 p-1">
                        {#each qqbotContacts as c}
                          <button
                            type="button"
                            class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors hover:bg-accent/60"
                            onclick={() => {
                              qqTargetType = c.kind;
                              qqTarget = c.openid;
                            }}
                          >
                            <Badge variant="outline" class={c.kind === 'group' ? 'border-cyan-500/30 text-cyan-400' : 'border-violet-500/30 text-violet-400'}>
                              {c.kind === 'group' ? '群' : '用户'}
                            </Badge>
                            <span class="min-w-0 flex-1">
                              <span class="block truncate font-mono text-[11px]">{c.openid}</span>
                              <span class="block truncate text-[10px] text-muted-foreground">
                                {c.lastContent || '（暂无消息内容）'} · {c.lastSeenAt}
                              </span>
                            </span>
                          </button>
                        {/each}
                      </div>
                    {/if}
                    {#if qqTargetType === 'group' && !qqbotContactsLoading && qqbotContacts.filter((c) => c.kind === 'group').length === 0}
                      <div class="rounded-lg border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-[11px] leading-relaxed text-amber-700 dark:text-amber-400">
                        <span class="font-semibold">还没有「群 openid」：</span>
                        群号不能直接作为发送目标。请把机器人加入 QQ 群 → 在群里
                        <span class="mx-0.5 rounded bg-background/60 px-1 font-mono">@机器人</span>
                        发一句话（开放平台机器人控制台「消息配置」需启用群消息事件）→
                        群 openid 会自动收集到上方列表，点击即可选中发送。
                      </div>
                    {/if}
                    {#if qqTargetType === 'group' && qqbotContacts.some((c) => c.kind === 'group')}
                      <p class="px-1 text-[11px] leading-relaxed text-muted-foreground">
                        群消息发送会自动优先使用「被动回复」：只要群里 5 分钟内有人 @过机器人，
                        即可直接发送，无需群主动消息权限；窗口过后发送则走主动消息
                        （错误码 40034105 表示机器人未开通群主动权限）。
                      </p>
                    {/if}
                    <p class="px-1 text-[11px] leading-relaxed text-muted-foreground">
                      目标需填 openid（不是 QQ 号）；官方限制——主动消息需对方 24 小时内与机器人互动过（错误码 11255）。
                    </p>
                  </div>
              {:else}
                <div class="flex flex-wrap items-center gap-2 rounded-lg border border-border bg-muted/40 px-3 py-2">
                  <ShieldCheckIcon class="size-3.5 shrink-0 text-emerald-500" />
                  <span class="text-xs font-medium">
                    {platform === 'wechat' ? '绑定微信本人' : '群机器人'}
                  </span>
                  {#if platform === 'wechat' && selected.ownerId}
                    <span class="min-w-0 flex-1 truncate font-mono text-[11px] text-muted-foreground" title={selected.ownerId}>
                      {selected.ownerId}
                    </span>
                  {:else if platform === 'wechat'}
                    <span class="text-[11px] text-muted-foreground">未记录微信 ID</span>
                  {:else}
                    <span class="min-w-0 flex-1 truncate text-[11px] text-muted-foreground">
                      无需选择推送对象，消息直接发送到机器人所在群
                    </span>
                  {/if}
                  {#if platform === 'wechat'}
                    <span class="shrink-0 text-[11px] {remaining.urgent ? 'text-amber-500' : 'text-muted-foreground'}">
                      剩余 {remaining.text}
                    </span>
                  {/if}
                </div>
              {/if}

              <!-- 附件 -->
              <div class="space-y-1.5">
                <div class="flex items-center justify-between">
                  <span class="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">附件</span>
                  {#if fileMeta}
                    <span class="text-[11px] text-muted-foreground">可再附带一段文本一起发送</span>
                  {/if}
                </div>
                {#if fileMeta}
                  <div class="flex items-center gap-2 rounded-lg border border-border bg-card px-3 py-2">
                    {#if fileMeta.kind === 'image'}
                      <ImageIcon class="size-4 shrink-0 text-primary" />
                    {:else if fileMeta.kind === 'video'}
                      <FilmIcon class="size-4 shrink-0 text-primary" />
                    {:else if fileMeta.kind === 'audio'}
                      <MusicIcon class="size-4 shrink-0 text-primary" />
                    {:else}
                      <FileIcon class="size-4 shrink-0 text-primary" />
                    {/if}
                    <span class="min-w-0 flex-1 truncate text-sm" title={selectedFile ?? ''}>{fileMeta.name}</span>
                    <Badge variant="outline" class="shrink-0 text-[11px]">
                      {fileMeta.kind === 'image' ? '图片' : fileMeta.kind === 'video' ? '视频' : fileMeta.kind === 'audio' ? '音频' : '文件'}
                    </Badge>
                    <button
                      type="button"
                      class="flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
                      onclick={clearFile}
                      title="移除附件"
                    >
                      <XIcon class="size-3.5" />
                    </button>
                  </div>
                {:else}
                  <Button variant="outline" class="w-full" onclick={pickFile} disabled={sending}>
                    <PaperclipIcon class="size-4" />
                    选择文件 / 图片
                  </Button>
                {/if}
              </div>

              <!-- 文本 -->
              <Textarea
                bind:value={sendText}
                rows={4}
                placeholder="输入要推送的文本内容…（可留空，仅发附件）"
                onkeydown={onKeydown}
                disabled={sending}
              />

              <div class="flex items-center justify-between gap-3">
                <span class="hidden text-[11px] text-muted-foreground sm:block">
                  Ctrl + Enter 快捷发送
                </span>
                <Button class="min-w-36" onclick={send} disabled={sending}>
                  {#if sending}
                    <LoaderCircleIcon class="size-4 animate-spin" />
                    发送中…
                  {:else}
                    <SendIcon class="size-4" />
                    发送
                  {/if}
                </Button>
              </div>

              <!-- 发送进度 -->
              {#if sendStage !== 'idle'}
                <div class="rounded-lg border border-border bg-card p-3">
                  {#if traceMode === 'media'}
                    <div class="flex items-center gap-2">
                      {#each steps as s}
                        {@const st = stepState(s.key, traceMode, sendStage, sendError)}
                        <div class="flex min-w-0 flex-1 items-center gap-1.5">
                          <span
                            class="flex size-5 shrink-0 items-center justify-center rounded-full border {st === 'done'
                              ? 'border-emerald-500/40 bg-emerald-500/10 text-emerald-500'
                              : st === 'error'
                                ? 'border-rose-500/40 bg-rose-500/10 text-rose-500'
                                : st === 'active'
                                  ? 'border-primary/50 bg-primary/10 text-primary'
                                  : 'border-border text-muted-foreground'}"
                          >
                            {#if st === 'done'}
                              <CheckIcon class="size-3" />
                            {:else if st === 'error'}
                              <XIcon class="size-3" />
                            {:else if st === 'active'}
                              <LoaderCircleIcon class="size-3 animate-spin" />
                            {:else}
                              <span class="size-1 rounded-full bg-current"></span>
                            {/if}
                          </span>
                          <span
                            class="truncate text-[11px] {st === 'pending' ? 'text-muted-foreground' : 'text-foreground'}"
                          >
                            {s.label}
                          </span>
                        </div>
                        {#if s !== steps[steps.length - 1]}
                          <span class="h-px w-3 shrink-0 bg-border"></span>
                        {/if}
                      {/each}
                    </div>
                  {:else}
                    <div class="flex items-center gap-1.5 text-[11px] {sendStage === 'done' ? 'text-emerald-500' : 'text-muted-foreground'}">
                      {#if sendStage === 'done'}
                        <CheckIcon class="size-3" />
                        文本已发送
                      {:else if sendStage === 'error'}
                        <XIcon class="size-3" />
                        文本发送失败
                      {:else}
                        <LoaderCircleIcon class="size-3 animate-spin" />
                        正在发送文本…
                      {/if}
                    </div>
                  {/if}
                  {#if sendStage === 'done' && traceMode === 'media'}
                    <div class="mt-2 flex items-center gap-1.5 text-[11px] text-emerald-500">
                      <CheckIcon class="size-3" />
                      {platform === 'wechat' ? '已送达绑定微信本人' : `已送达${PLATFORM_META[platform].label}`}
                    </div>
                  {:else if sendStage === 'error'}
                    <div class="mt-2 max-h-24 overflow-y-auto rounded-md border border-rose-500/25 bg-rose-500/5 px-2.5 py-2 text-[11px] leading-relaxed text-rose-500">
                      {sendError}
                    </div>
                    <div class="mt-1.5 text-[11px] text-muted-foreground">
                      可稍后重试，或在下方消息日志中查看完整错误记录。
                    </div>
                  {/if}
                </div>
              {/if}
            </CardContent>
          </Card>

          <!-- 消息日志 -->
          <Card class="min-h-0 overflow-hidden">
            <CardContent class="flex h-full min-h-0 flex-col p-4">
              <div class="flex items-center gap-2 border-b border-border pb-3">
                <MessageSquareIcon class="size-4 text-primary" />
                <span class="text-sm font-bold">消息日志</span>
                {#if selected.lastActiveAt}
                  <span class="ml-auto text-[11px] text-muted-foreground">
                    最近活跃 {selected.lastActiveAt}
                  </span>
                {/if}
              </div>
              <div class="mt-3 min-h-0 flex-1">
                <BotLogView accountId={selected.id} />
              </div>
            </CardContent>
          </Card>
        </div>
      {/if}
    </div>
  {/if}
</div>
