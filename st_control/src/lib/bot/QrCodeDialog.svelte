<script lang="ts">
  import { onDestroy } from 'svelte';
  import { botApi } from './services/ipc';
  import { toast } from 'svelte-sonner';
  import {
    Dialog as DialogRoot, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter,
  } from '../components/ui/dialog';
  import { Button } from '../components/ui/button';
  import { Spinner } from '../components/ui/spinner';
  import QrCodeIcon from '@lucide/svelte/icons/qr-code';
  import RotateCcwIcon from '@lucide/svelte/icons/rotate-ccw';

  let { open = $bindable(false), accountId = null } = $props<{
    open?: boolean;
    accountId?: number | null;
  }>();

  let sessionId = $state<string | null>(null);
  let imageDataUrl = $state('');
  let statusText = $state('正在获取二维码…');
  let loading = $state(false);
  let pollTimer: ReturnType<typeof setTimeout> | null = null;

  async function start() {
    if (!open) return;
    stopPolling();
    sessionId = null;
    imageDataUrl = '';
    statusText = '正在获取二维码…';
    loading = true;
    try {
      const qr = await botApi.startQr(accountId);
      sessionId = qr.sessionId;
      imageDataUrl = qr.imageDataUrl;
      statusText = '请使用微信扫码，并确认登录';
      schedulePoll(800);
    } catch (e) {
      statusText = `获取二维码失败：${e}`;
      toast.error(`获取二维码失败：${e}`);
    } finally {
      loading = false;
    }
  }

  function schedulePoll(delay: number) {
    if (!open) return;
    stopPolling();
    pollTimer = setTimeout(() => void poll(), delay);
  }

  async function poll() {
    if (!sessionId || !open) return;
    try {
      const r = await botApi.pollQr(sessionId);
      if (r.status === 'wait') {
        statusText = '等待扫码…';
        schedulePoll(1200);
      } else if (r.status === 'scaned') {
        statusText = '已扫码，请在手机上确认登录';
        schedulePoll(1200);
      } else if (r.status === 'scaned_but_redirect') {
        statusText = '扫码成功，正在跳转确认…';
        schedulePoll(1200);
      } else if (r.status === 'confirmed') {
        stopPolling();
        sessionId = null;
        toast.success(accountId ? '已重新绑定微信机器人' : '微信机器人绑定成功');
        open = false;
        window.dispatchEvent(new CustomEvent('bot-accounts-changed'));
      } else if (r.status === 'expired') {
        stopPolling();
        statusText = '二维码已过期，请重新生成';
        sessionId = null;
        toast.warning('二维码已过期');
      } else if (r.status === 'need_verifycode' || r.status === 'verify_code_blocked') {
        stopPolling();
        statusText = '需要验证码或已被限制，请在微信内处理';
      } else if (r.status === 'error' || r.status === 'expired') {
        stopPolling();
        sessionId = null;
        statusText = '二维码异常，请重新生成';
      }
    } catch (e) {
      // 网络/超时等瞬时错误：不中断，继续轮询（显示真实原因）
      statusText = `状态查询失败：${e}（自动重试中…）`;
      schedulePoll(2000);
    }
  }

  function stopPolling() {
    if (pollTimer) {
      clearInterval(pollTimer);
      pollTimer = null;
    }
  }

  function onOpenChange(v: boolean) {
    if (!v) {
      if (sessionId) botApi.cancelQr(sessionId).catch(() => {});
      stopPolling();
      sessionId = null;
      imageDataUrl = '';
    }
    open = v;
  }

  $effect(() => {
    if (open) start();
  });

  onDestroy(() => {
    stopPolling();
    if (sessionId) botApi.cancelQr(sessionId).catch(() => {});
  });
</script>

<DialogRoot open={open} onOpenChange={onOpenChange}>
  <DialogContent class="sm:max-w-[400px]">
    <DialogHeader>
      <DialogTitle class="flex items-center gap-2">
        <QrCodeIcon class="size-4" />
        {accountId ? '重新扫码绑定' : '绑定微信机器人'}
      </DialogTitle>
      <DialogDescription>
        使用微信「扫一扫」扫码并确认。连接有效期约 24 小时，到期后需重新扫码。
      </DialogDescription>
    </DialogHeader>

    <div class="flex flex-col items-center gap-4 py-4">
      <div class="relative flex size-56 items-center justify-center rounded-xl border border-border bg-background">
        {#if loading}
          <div class="flex flex-col items-center gap-2 text-muted-foreground">
            <Spinner class="size-8" />
            <span class="text-sm">获取二维码…</span>
          </div>
        {:else if imageDataUrl}
          <img src={imageDataUrl} alt="微信登录二维码" class="size-52 rounded-lg object-contain" />
        {:else}
          <div class="flex flex-col items-center gap-2 text-muted-foreground">
            <QrCodeIcon class="size-10" />
            <span class="text-sm">暂无二维码</span>
          </div>
        {/if}
      </div>
      <div class="flex items-center gap-2 text-sm text-muted-foreground">
        <span class="size-2 rounded-full bg-emerald-400 animate-pulse"></span>
        {statusText}
      </div>
    </div>

    <DialogFooter class="flex items-center justify-between">
      <Button variant="ghost" size="sm" onclick={start} disabled={loading}>
        <RotateCcwIcon class="size-4" />
        重新生成
      </Button>
      <Button variant="secondary" size="sm" onclick={() => onOpenChange(false)}>
        关闭
      </Button>
    </DialogFooter>
  </DialogContent>
</DialogRoot>
