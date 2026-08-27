<script lang="ts">
  /** 调用记录日志模块 — 迁移自 viewapi api-log 分区 */
  import { getApiRuntimeLog, clearApiRuntimeLog, subscribeApiLog } from '../services/api';
  import type { ApiLogEntry } from '../types';
  import { onMount } from 'svelte';

  let logs = $state<ApiLogEntry[]>([]);
  let filter = $state('');

  const filteredLogs = $derived.by(() => {
    if (!filter.trim()) return logs;
    const q = filter.toLowerCase();
    return logs.filter(l => l.path.toLowerCase().includes(q) || (l.error || '').toLowerCase().includes(q));
  });

  function refresh() { logs = getApiRuntimeLog().reverse(); }

  function exportLogs() {
    const json = JSON.stringify(logs, null, 2);
    const blob = new Blob([json], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url; a.download = `api-logs-${Date.now()}.json`; a.click();
    URL.revokeObjectURL(url);
  }

  function clearAll() {
    clearApiRuntimeLog();
    logs = [];
  }

  function formatDuration(ms: number | null): string {
    if (ms == null) return '—';
    return ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(1)}s`;
  }

  function formatTime(iso: string): string {
    try {
      return new Date(iso).toLocaleTimeString('zh-CN', { hour12: false });
    } catch {
      return iso;
    }
  }

  onMount(() => {
    refresh();
    const unsub = subscribeApiLog(refresh);
    return unsub;
  });
</script>

<div class="wa-mod">
  <div class="wa-card">
    <h3 class="wa-card-title">调用记录日志</h3>
    <p class="wa-hint">自动记录通过本面板发起的所有 POST 请求（含请求体、HTTP 状态、响应体、耗时）。</p>
    <div class="wa-toolbar">
      <button class="wa-btn wa-btn-primary" onclick={exportLogs}>导出日志 (JSON)</button>
      <button class="wa-btn" onclick={refresh}>刷新</button>
      <button class="wa-btn" onclick={clearAll}>清空</button>
      <input type="text" class="wa-filter-input" bind:value={filter} placeholder="搜索路径或异常..." />
    </div>
  </div>

  <div class="wa-card wa-card-fill">
    <div class="wa-table-wrap">
      <table class="wa-table">
        <thead>
          <tr>
            <th>时间</th>
            <th>路径</th>
            <th>HTTP</th>
            <th>ret</th>
            <th>异常</th>
            <th>耗时</th>
          </tr>
        </thead>
        <tbody>
          {#each filteredLogs as log}
            <tr class:error={log.error}>
              <td class="wa-td-time">{formatTime(log.at)}</td>
              <td class="wa-td-path">{log.path}</td>
              <td class="wa-td-http">{log.responseHttpStatus ?? '—'}</td>
              <td class="wa-td-ret">{(log.responseBody as Record<string, number>)?.ret ?? '—'}</td>
              <td class="wa-td-error">{log.error || '—'}</td>
              <td class="wa-td-dur">{formatDuration(log.durationMs)}</td>
            </tr>
          {:else}
            <tr><td colspan="6" class="wa-td-empty">暂无调用记录</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
  </div>
</div>

<style>
  .wa-mod { height: 100%; display: flex; flex-direction: column; gap: 12px; }
  .wa-card { background: var(--card); border: 1px solid var(--border); border-radius: var(--radius-lg); padding: 16px; }
  .wa-card-fill { flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: hidden; }
  .wa-card-title { font-size: 14px; font-weight: 600; margin: 0 0 8px; }
  .wa-hint { font-size: 12px; color: var(--muted-foreground); margin: 0 0 12px; }
  .wa-toolbar { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .wa-filter-input { flex: 1; min-width: 200px; padding: 6px 10px; border: 1px solid var(--border); border-radius: 6px; font-size: 13px; background: var(--card); color: var(--foreground); }
  .wa-btn { padding: 6px 14px; border: 1px solid var(--border); border-radius: 6px; background: var(--card); font-size: 13px; cursor: pointer; color: var(--foreground); }
  .wa-btn:hover { background: var(--muted); }
  .wa-btn-primary { background: var(--primary); color: var(--primary-foreground); border-color: var(--primary); }
  .wa-table-wrap { flex: 1; min-height: 0; overflow: auto; }
  .wa-table { width: 100%; border-collapse: collapse; font-size: 12px; }
  .wa-table th { padding: 8px; text-align: left; font-weight: 600; background: var(--muted); border-bottom: 1px solid var(--border); position: sticky; top: 0; z-index: 1; }
  .wa-table td { padding: 6px 8px; border-bottom: 1px solid var(--border); vertical-align: top; }
  .wa-table tr.error td { color: var(--destructive, #dc2626); }
  .wa-td-time { white-space: nowrap; font-family: var(--font-mono); }
  .wa-td-path { font-family: var(--font-mono); font-weight: 500; }
  .wa-td-http { text-align: center; }
  .wa-td-ret { text-align: center; }
  .wa-td-error { max-width: 200px; overflow: hidden; text-overflow: ellipsis; }
  .wa-td-dur { text-align: right; font-family: var(--font-mono); }
  .wa-td-empty { text-align: center; color: var(--muted-foreground); padding: 24px 0; }
</style>
