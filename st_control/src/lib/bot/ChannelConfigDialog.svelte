<script lang="ts">
  import { toast } from 'svelte-sonner';
  import {
    Dialog as DialogRoot, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter,
  } from '../components/ui/dialog';
  import { Button } from '../components/ui/button';
  import { Input } from '../components/ui/input';
  import { Label } from '../components/ui/label';
  import { PLATFORM_META } from './types';
  import { botApi } from './services/ipc';
  import type { BotAccount, BotPlatform } from './types';
  import BotIcon from '@lucide/svelte/icons/bot';
  import LoaderCircleIcon from '@lucide/svelte/icons/loader-circle';
  import SaveIcon from '@lucide/svelte/icons/save';

  // 仅剩 QQ 官方机器人通道（J-23：企业微信 / 钉钉 / OneBot 已移除）
  let { open = $bindable(false), platform = 'qqbot', account = null } = $props<{
    open?: boolean;
    platform?: BotPlatform;
    account?: BotAccount | null;
  }>();

  // 表单状态
  let name = $state('');
  let appId = $state('');
  let appSecret = $state('');
  let targetType = $state<'private' | 'group'>('private');
  let targetId = $state('');
  let saving = $state(false);

  $effect(() => {
    if (!open) return;
    const acc = account;
    name = acc?.name ?? '';
    targetId = acc?.targetId ?? '';
    appId = appSecret = '';
    targetType = 'private';
    if (acc?.configJson) {
      try {
        const c = JSON.parse(acc.configJson);
        appId = c.app_id ?? '';
        appSecret = c.app_secret ?? '';
        targetType = c.target_type === 'group' ? 'group' : 'private';
        if (c.target_type === 'group' && targetId) targetType = 'group';
      } catch {
        /* 配置损坏时按空表单处理 */
      }
    }
  });

  function buildConfig(): string {
    return JSON.stringify({
      app_id: appId.trim(),
      app_secret: appSecret.trim(),
      target_type: targetType,
      target_id: targetId.trim(),
    });
  }

  function validate(): string | null {
    if (!name.trim()) return '请输入账号名称';
    if (!appId.trim()) return '请输入机器人 AppID';
    if (!appSecret.trim()) return '请输入机器人 Secret';
    if (!targetId.trim()) return '请输入推送目标 openid（从机器人收到的消息事件中获取）';
    return null;
  }

  async function save() {
    const err = validate();
    if (err) return toast.error(err);
    saving = true;
    try {
      if (account) {
        await botApi.updateChannel(account.id, name.trim(), buildConfig(), targetId.trim());
        toast.success('通道配置已更新');
      } else {
        await botApi.addChannel(platform, name.trim(), buildConfig(), targetId.trim());
        toast.success(`${PLATFORM_META[platform as BotPlatform].label}通道已添加`);
      }
      open = false;
      window.dispatchEvent(new CustomEvent('bot-accounts-changed'));
    } catch (e) {
      toast.error(`${account ? '更新' : '添加'}失败：${e}`);
    } finally {
      saving = false;
    }
  }

  function onOpenChange(v: boolean) {
    open = v;
  }
</script>

<DialogRoot open={open} onOpenChange={onOpenChange}>
  <DialogContent class="sm:max-w-[460px]">
    <DialogHeader>
      <DialogTitle class="flex items-center gap-2">
        <BotIcon class="size-4" />
        {account ? '编辑通道配置' : `添加${PLATFORM_META[platform as BotPlatform].label}通道`}
      </DialogTitle>
      <DialogDescription>
        {PLATFORM_META[platform as BotPlatform].desc}
      </DialogDescription>
    </DialogHeader>

    <div class="space-y-3 py-2">
      <div class="space-y-1.5">
        <Label>账号名称</Label>
        <Input bind:value={name} placeholder="例如：QQ 通知机器人" />
      </div>

      <div class="space-y-1.5">
        <Label>机器人 AppID</Label>
        <Input bind:value={appId} placeholder="QQ 开放平台机器人的 AppID" class="font-mono text-xs" />
      </div>
      <div class="space-y-1.5">
        <Label>机器人 Secret</Label>
        <Input bind:value={appSecret} type="password" placeholder="机器人的 ClientSecret" class="font-mono text-xs" />
        <p class="text-[11px] text-muted-foreground">
          QQ 官方机器人只需 AppID + Secret 即可配置；系统自动换取 access_token 发送消息。
        </p>
      </div>
      <div class="space-y-1.5">
        <Label>默认推送目标（openid）</Label>
        <div class="flex items-center gap-2">
          <div class="flex rounded-lg border border-border p-0.5">
            <button
              type="button"
              class="rounded-md px-3 py-1.5 text-xs transition-colors {targetType === 'private' ? 'bg-accent text-foreground' : 'text-muted-foreground hover:text-foreground'}"
              onclick={() => (targetType = 'private')}
            >
              私聊
            </button>
            <button
              type="button"
              class="rounded-md px-3 py-1.5 text-xs transition-colors {targetType === 'group' ? 'bg-accent text-foreground' : 'text-muted-foreground hover:text-foreground'}"
              onclick={() => (targetType = 'group')}
            >
              群聊
            </button>
          </div>
          <Input bind:value={targetId} placeholder={targetType === 'group' ? '群 group_openid' : '用户 openid'} class="flex-1 font-mono text-xs" />
        </div>
        <p class="text-[11px] text-muted-foreground">
          目标填 openid 而不是 QQ 号。系统已连接官方网关：用户 / 群给机器人发过消息后，
          openid 会自动收集到「发送台 → 推送目标」列表里，直接点击选择即可，无需手动填写。
          官方限制：主动消息需对方 24 小时内与机器人互动过。
        </p>
      </div>
    </div>

    <DialogFooter>
      <Button variant="secondary" onclick={() => (open = false)}>取消</Button>
      <Button onclick={save} disabled={saving}>
        {#if saving}
          <LoaderCircleIcon class="size-4 animate-spin" />
          保存中…
        {:else}
          <SaveIcon class="size-4" />
          保存
        {/if}
      </Button>
    </DialogFooter>
  </DialogContent>
</DialogRoot>
