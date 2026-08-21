<script lang="ts">
  // 微信数据页 → 手动给好友发消息（经 ClawBot 通道）
  import { botApi } from './services/ipc';
  import { toast } from 'svelte-sonner';
  import {
    Dialog as DialogRoot, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter,
  } from '../components/ui/dialog';
  import { Button } from '../components/ui/button';
  import { Textarea } from '../components/ui/textarea';
  import ChannelPicker from './ChannelPicker.svelte';
  import SendIcon from '@lucide/svelte/icons/send';
  import PaperclipIcon from '@lucide/svelte/icons/paperclip';

  let {
    open = $bindable(false),
    defaultPeer = '',
    defaultName = '',
  } = $props<{ open?: boolean; defaultPeer?: string; defaultName?: string }>();

  let accountId = $state(0);
  let peer = $state('');
  let text = $state('');
  let sending = $state(false);

  $effect(() => {
    if (open) {
      peer = defaultPeer || peer;
      text = '';
    }
  });

  async function sendText() {
    if (!accountId) return toast.error('请先选择 ClawBot 账号');
    if (!peer.trim()) return toast.error('请填写推送对象');
    if (!text.trim()) return toast.error('请输入内容');
    sending = true;
    try {
      await botApi.sendText(accountId, peer.trim(), text.trim());
      toast.success(`已发送到 ${peer.trim()}`);
      open = false;
      window.dispatchEvent(new CustomEvent('bot-log-refresh'));
    } catch (e) {
      toast.error(`发送失败：${e}`);
    } finally {
      sending = false;
    }
  }

  async function sendFile() {
    if (!accountId) return toast.error('请先选择 ClawBot 账号');
    if (!peer.trim()) return toast.error('请填写推送对象');
    try {
      const { open: openDialog } = await import('@tauri-apps/plugin-dialog');
      const chosen = await openDialog({ multiple: false, title: '选择要发送的文件' });
      if (typeof chosen !== 'string') return;
      sending = true;
      toast.info('正在上传并发送文件…');
      await botApi.sendMedia(accountId, peer.trim(), chosen);
      toast.success(`文件已发送到 ${peer.trim()}`);
      open = false;
      window.dispatchEvent(new CustomEvent('bot-log-refresh'));
    } catch (e) {
      toast.error(`发送文件失败：${e}`);
    } finally {
      sending = false;
    }
  }
</script>

<DialogRoot open={open} onOpenChange={(v) => (open = v)}>
  <DialogContent class="sm:max-w-[520px]">
    <DialogHeader>
      <DialogTitle>通过 ClawBot 发消息</DialogTitle>
      <DialogDescription>
        {#if defaultName}
          当前会话：{defaultName}（{defaultPeer}）。需先绑定微信机器人，且对方已与机器人建立过会话。
          {#if !defaultPeer.includes('@')}
            注意：当前是本地会话 ID，将自动尝试补全为 ClawBot 的 @im.wechat 格式。
          {/if}
        {:else}
          需先绑定微信机器人，且对方已与机器人建立过会话。
        {/if}
      </DialogDescription>
    </DialogHeader>

    <div class="space-y-4 py-2">
      <ChannelPicker bind:accountId bind:peer compact />

      <Textarea
        bind:value={text}
        rows={5}
        placeholder="输入要推送的文本内容…"
        disabled={sending}
      />
    </div>

    <DialogFooter class="flex items-center justify-between gap-2 sm:justify-between">
      <Button variant="outline" onclick={sendFile} disabled={sending}>
        <PaperclipIcon class="size-4" />
        发送文件
      </Button>
      <Button onclick={sendText} disabled={sending || !text.trim()}>
        <SendIcon class="size-4" />
        {sending ? '发送中…' : '发送'}
      </Button>
    </DialogFooter>
  </DialogContent>
</DialogRoot>
