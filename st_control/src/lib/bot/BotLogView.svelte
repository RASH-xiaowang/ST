<script lang="ts">
  import { onMount } from 'svelte';
  import { botApi } from './services/ipc';
  import { toast } from 'svelte-sonner';
  import { Button } from '../components/ui/button';
  import { Badge } from '../components/ui/badge';
  import { Spinner } from '../components/ui/spinner';
  import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
  import Trash2Icon from '@lucide/svelte/icons/trash-2';
  import InboxIcon from '@lucide/svelte/icons/inbox';
  import ArrowDownLeftIcon from '@lucide/svelte/icons/arrow-down-left';
  import ArrowUpRightIcon from '@lucide/svelte/icons/arrow-up-right';
  import { MSG_TYPE_LABELS } from './types';
  import type { BotLog } from './types';

  let { accountId } = $props<{ accountId: number }>();

  let logs = $state<BotLog[]>([]);
  let total = $state(0);
  let page = $state(1);
  let loading = $state(false);
  let filter = $state<'all' | 'in' | 'out' | 'failed'>('all');

  async function load() {
    if (!accountId) return;
    loading = true;
    try {
      const r = await botApi.listLogs(accountId, page, 50);
      logs = r.items;
      total = r.total;
    } catch (e) {
      toast.error(`加载日志失败：${e}`);
    } finally {
      loading = false;
    }
  }

  async function clear() {
    if (!confirm('确认清空该账号的全部消息日志？')) return;
    try {
      await botApi.clearLogs(accountId);
      logs = [];
      total = 0;
      toast.success('日志已清空');
    } catch (e) {
      toast.error(`清空失败：${e}`);
    }
  }

  function refresh() {
    page = 1;
    load();
  }

  onMount(load);
  $effect(() => {
    if (accountId) load();
  });

  // 外部刷新（新消息 / 新发送）
  $effect(() => {
    const handler = () => load();
    window.addEventListener('bot-log-refresh', handler);
    return () => window.removeEventListener('bot-log-refresh', handler);
  });

  const filtered = $derived(
    logs.filter((l) => {
      if (filter === 'all') return true;
      if (filter === 'failed') return l.status === 'failed';
      return l.direction === filter;
    }),
  );

  const filters = $derived([
    { key: 'all' as const, label: '全部' },
    { key: 'in' as const, label: '收到' },
    { key: 'out' as const, label: '发送' },
    { key: 'failed' as const, label: '失败' },
  ]);
</script>

<div class="flex h-full min-h-0 flex-col">
  <div class="flex items-center justify-between gap-2 px-0.5 pb-2">
    <div class="flex items-center gap-1">
      {#each filters as f}
        <button
          type="button"
          class="rounded-md px-2 py-1 text-[11px] transition-colors {filter === f.key
            ? 'bg-accent text-foreground'
            : 'text-muted-foreground hover:bg-accent/50 hover:text-foreground'}"
          onclick={() => (filter = f.key)}
        >
          {f.label}
        </button>
      {/each}
      <span class="ml-1 text-[11px] text-muted-foreground">共 {total} 条</span>
    </div>
    <div class="flex items-center gap-0.5">
      <Button variant="ghost" size="sm" onclick={refresh} disabled={loading}>
        <RefreshCwIcon class="size-3.5" />
        刷新
      </Button>
      <Button variant="ghost" size="sm" onclick={clear}>
        <Trash2Icon class="size-3.5" />
        清空
      </Button>
    </div>
  </div>

  <div class="min-h-0 flex-1 overflow-y-auto rounded-lg border border-border">
    {#if loading && logs.length === 0}
      <div class="flex items-center justify-center py-12 text-muted-foreground">
        <Spinner class="size-5" />
      </div>
    {:else if filtered.length === 0}
      <div class="flex flex-col items-center gap-2 py-14 text-muted-foreground">
        <InboxIcon class="size-8" />
        <span class="text-sm">{logs.length === 0 ? '暂无消息记录' : '没有符合条件的记录'}</span>
      </div>
    {:else}
      <div class="divide-y divide-border">
        {#each filtered as log}
          {@const out = log.direction === 'out'}
          {@const failed = log.status === 'failed'}
          <div class="group flex items-start gap-2.5 px-3 py-2.5 hover:bg-accent/40">
            <span
              class="mt-1 flex size-5 shrink-0 items-center justify-center rounded-full {out
                ? 'bg-sky-500/12 text-sky-500'
                : 'bg-emerald-500/12 text-emerald-500'}"
              title={out ? '发送' : '收到'}
            >
              {#if out}
                <ArrowUpRightIcon class="size-3" />
              {:else}
                <ArrowDownLeftIcon class="size-3" />
              {/if}
            </span>
            <div class="min-w-0 flex-1">
              <div class="flex flex-wrap items-center gap-1.5">
                <span class="text-xs font-medium">{out ? '发送' : '收到'}</span>
                <Badge variant="outline" class="px-1.5 py-0 text-[11px]">
                  {MSG_TYPE_LABELS[log.msgType] ?? log.msgType}
                </Badge>
                {#if failed}
                  <Badge variant="outline" class="border-rose-500/30 px-1.5 py-0 text-[11px] text-rose-500">
                    失败
                  </Badge>
                {/if}
                <span class="ml-auto shrink-0 font-mono text-[11px] text-muted-foreground">
                  {log.createdAt}
                </span>
              </div>
              <div class="mt-1 line-clamp-2 break-all text-sm text-foreground/90" title={log.content}>
                {log.content}
              </div>
              {#if log.peer}
                <div class="mt-0.5 truncate font-mono text-[11px] text-muted-foreground" title={log.peer}>
                  {log.peer}
                </div>
              {/if}
              {#if failed && log.error}
                <div class="mt-1 line-clamp-3 rounded-md border border-rose-500/20 bg-rose-500/5 px-2 py-1.5 text-[11px] leading-relaxed text-rose-500" title={log.error}>
                  {log.error}
                </div>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
