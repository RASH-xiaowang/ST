<script lang="ts">
  /**
   * 审计日志查看器（仅全局管理员可用）
   * 功能：查看关键操作日志（登录/登出/备份/用户管理等）
   */
  import { kbApi } from './services/ipc';
  import KbIcon from './KbIcon.svelte';
  import { Button } from '../components/ui/button';
  import { Badge } from '../components/ui/badge';
  import { Empty, EmptyTitle } from '../components/ui/empty';
  import { Skeleton } from '../components/ui/skeleton';

  interface Props {
    hideHeader?: boolean;
  }
  let { hideHeader = false }: Props = $props();

  let logs = $state<Record<string, unknown>[]>([]);
  let loading = $state(false);
  let err = $state('');
  let limit = $state(50);

  const ACTION_LABELS: Record<string, string> = {
    login: '登录',
    logout: '登出',
    create_kb: '创建知识库',
    delete_kb: '删除知识库',
    create_user: '创建用户',
    delete_user: '删除用户',
    backup: '备份',
    restore: '恢复',
  };

  function actionLabel(action: string): string {
    return ACTION_LABELS[action] || action;
  }

  async function loadLogs() {
    loading = true; err = '';
    try {
      // 防御：IPC 异常时后端可能返回 null，避免 logs 为 null 导致渲染崩溃（logs.length）
      logs = (await kbApi.listAuditLogs(limit)) ?? [];
    } catch (e: unknown) {
      err = '加载审计日志失败：' + e;
    } finally {
      loading = false;
    }
  }

  // 初始加载
  loadLogs();
</script>

<div class="kb-audit">
  {#if !hideHeader}
  <div class="kb-audit-hd">
    <h3 class="kb-audit-title"><KbIcon name="shield" size={16} />审计日志</h3>
    <div class="kb-audit-actions">
      <select class="kb-select-sm" bind:value={limit} onchange={() => loadLogs()}>
        <option value={20}>20 条</option>
        <option value={50}>50 条</option>
        <option value={100}>100 条</option>
        <option value={200}>200 条</option>
      </select>
      <Button variant="outline" size="sm" onclick={loadLogs} disabled={loading}>
        <KbIcon name="refresh" size={12} />刷新
      </Button>
    </div>
  </div>
  {:else}
  <div class="kb-audit-hd">
    <div></div>
    <div class="kb-audit-actions">
      <select class="kb-select-sm" bind:value={limit} onchange={() => loadLogs()}>
        <option value={20}>20 条</option>
        <option value={50}>50 条</option>
        <option value={100}>100 条</option>
        <option value={200}>200 条</option>
      </select>
      <Button variant="outline" size="sm" onclick={loadLogs} disabled={loading}>
        <KbIcon name="refresh" size={12} />刷新
      </Button>
    </div>
  </div>
  {/if}

  {#if err}
    <div class="kb-msg err">{err}</div>
  {/if}

  {#if loading}
    <div class="flex flex-col gap-2 p-2">
      {#each Array(5) as _}
        <Skeleton class="h-[36px] rounded-lg" />
      {/each}
    </div>
  {:else if logs.length === 0}
    <Empty class="min-h-[100px] p-4">
      <KbIcon name="shield" size={20} color="var(--kb-text-3)" />
      <EmptyTitle class="text-sm">暂无审计日志</EmptyTitle>
    </Empty>
  {:else}
    <div class="kb-audit-list">
      {#each logs as log}
        <div class="kb-audit-row">
          <span class="kb-audit-time">{String(log.createdAt || '')}</span>
          <Badge variant={log.action === 'login' ? 'default' : log.action === 'backup' ? 'secondary' : 'outline'} class="text-[10px]">
            {actionLabel(String(log.action || ''))}
          </Badge>
          <span class="kb-audit-user">{String(log.username || '—')}</span>
          <span class="kb-audit-target">{String(log.targetType || '')}{log.targetId ? `#${log.targetId}` : ''}</span>
          {#if log.detail}
            <span class="kb-audit-detail">{String(log.detail)}</span>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .kb-audit { display: flex; flex-direction: column; gap: 12px; }
  .kb-audit-hd { display: flex; align-items: center; justify-content: space-between; }
  .kb-audit-title { font-size: 14px; font-weight: 600; margin: 0; display: flex; align-items: center; gap: 6px; }
  .kb-audit-actions { display: flex; gap: 6px; }
  .kb-select-sm {
    padding: 3px 8px; border: 1px solid var(--kb-border); border-radius: 6px;
    background: var(--kb-card); font-size: 12px; color: var(--kb-text); cursor: pointer;
  }
  .kb-audit-list { display: flex; flex-direction: column; gap: 2px; max-height: 400px; overflow-y: auto; }
  .kb-audit-row {
    display: flex; align-items: center; gap: 10px;
    padding: 8px 10px; border: 1px solid var(--kb-border); border-radius: 6px;
    font-size: 12px;
  }
  .kb-audit-time { font-family: var(--font-mono); color: var(--kb-text-3); flex: none; white-space: nowrap; }
  .kb-audit-user { font-weight: 500; color: var(--kb-text); }
  .kb-audit-target { color: var(--kb-text-3); }
  .kb-audit-detail { color: var(--kb-text-3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 200px; }
</style>
