<script lang="ts">
  // 推送对象选择器：选择已绑定的 ClawBot 账号 + 目标联系人
  // 供自动化规则/任务派发、微信数据页手动发消息复用
  import { onMount } from 'svelte';
  import { botApi } from './services/ipc';
  import {
    Select as SelectRoot, SelectContent, SelectItem, SelectTrigger,
  } from '../components/ui/select';
  import { Input } from '../components/ui/input';
  import { Label } from '../components/ui/label';
  import type { AccountContact, BotAccount } from './types';

  let {
    accountId = $bindable<number>(0),
    peer = $bindable<string>(''),
    compact = false,
  } = $props<{ accountId?: number; peer?: string; compact?: boolean }>();

  let accounts = $state<BotAccount[]>([]);
  let contacts = $state<AccountContact[]>([]);
  let contactsLoading = $state(false);

  async function loadAccounts() {
    try {
      const list = await botApi.listAccounts();
      accounts = list.filter((a) => a.status === 'online' || a.status === 'expiring');
      if (accounts.length && !accounts.some((a) => a.id === accountId)) {
        accountId = accounts[0].id;
      }
    } catch {
      accounts = [];
    }
  }

  async function loadContacts() {
    if (!accountId) return;
    contactsLoading = true;
    try {
      contacts = await botApi.listContacts(accountId);
    } catch {
      contacts = [];
    } finally {
      contactsLoading = false;
    }
  }

  function onAccountChange(v: string) {
    accountId = Number(v);
  }

  function onPeerChange(v: string) {
    peer = v;
  }

  onMount(() => {
    loadAccounts();
  });

  $effect(() => {
    if (accountId) loadContacts();
  });
</script>

<div class="grid gap-3 {compact ? 'grid-cols-1' : 'grid-cols-1 sm:grid-cols-2'}">
  <div class="space-y-1.5">
    <Label class="text-xs text-muted-foreground">ClawBot 账号</Label>
    <SelectRoot type="single" value={accountId ? String(accountId) : ''} onValueChange={onAccountChange}>
      <SelectTrigger class="h-9 w-full">
        <span>{accounts.find((a) => a.id === accountId)?.name ?? '选择账号…'}</span>
      </SelectTrigger>
      <SelectContent>
        {#if accounts.length === 0}
          <div class="px-2 py-3 text-center text-xs text-muted-foreground">暂无可用的在线账号</div>
        {:else}
          {#each accounts as a}
            <SelectItem value={String(a.id)}>{a.name}</SelectItem>
          {/each}
        {/if}
      </SelectContent>
    </SelectRoot>
  </div>

  <div class="space-y-1.5">
    <Label class="text-xs text-muted-foreground">推送对象</Label>
    {#if contacts.length > 0}
      <SelectRoot type="single" value={peer} onValueChange={onPeerChange}>
        <SelectTrigger class="h-9 w-full">
          <span class="block truncate">{peer || '选择联系人…'}</span>
        </SelectTrigger>
        <SelectContent>
          {#each contacts as c}
            <SelectItem value={c.peer}>
              <span class="block max-w-52 truncate">{c.peer}</span>
            </SelectItem>
          {/each}
        </SelectContent>
      </SelectRoot>
    {:else}
      <Input
        bind:value={peer}
        placeholder={contactsLoading ? '加载联系人…' : '联系人 ID（wxid@im.wechat）'}
      />
    {/if}
  </div>
</div>
