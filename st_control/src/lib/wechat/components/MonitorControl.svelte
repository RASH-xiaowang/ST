<!--
  监控状态与启停控制 + 实时指标面板。
  自 WeChatPanel.svelte 抽出：props = status / loading / canStart / onStart / onStop；
  启动按钮可用性（DB 已检查且无失败状态）由父组件计算后传入。
-->
<script lang="ts">
  import type { MonitorStatus } from '../types';
  import WechatHoverButton from './WechatHoverButton.svelte';

  let {
    status,
    loading,
    canStart,
    onStart,
    onStop,
  }: {
    status: MonitorStatus;
    loading: boolean;
    canStart: boolean;
    onStart: () => void;
    onStop: () => void;
  } = $props();
</script>

<div class="wc-monitor-ctrl">
  <span
    class="wc-monitor-badge"
    class:wc-monitor-running={status.running}
    title={status.status || (status.running ? 'running' : 'stopped')}
  >
    {status.running ? '监控运行中' : '监控未启动'}
  </span>
  {#if status.running}
    <WechatHoverButton
      text={loading ? '停止中…' : '停止'}
      onclick={(e) => { e.stopPropagation(); onStop(); }}
      disabled={loading}
    />
  {:else}
    <WechatHoverButton
      text={loading ? '启动中…' : '启动监控'}
      onclick={(e) => { e.stopPropagation(); onStart(); }}
      disabled={loading || !canStart}
    />
  {/if}
</div>

{#if status.running}
  <div class="wc-metrics-panel">
    <div class="wc-metric-row">
      <span class="wc-metric" title="待前端确认的消息数">未确认: {status.pending_acks ?? 0}</span>
      <span class="wc-metric" title="已推送消息总数">已发送: {status.sent_total ?? 0}</span>
      <span class="wc-metric" title="WebSocket 回退通道发送数">WS: {status.sent_ws_count ?? 0}</span>
    </div>
    {#if status.latency && (status.latency.count ?? 0) > 0}
      <div class="wc-metric-row">
        <span class="wc-metric" title="端到端平均延迟">
          平均延迟: {Math.round((status.latency.sum_ms ?? 0) / (status.latency.count ?? 1))}ms
        </span>
        <span class="wc-metric" title="延迟分布 <50/<200/<500/<1000/>=1000 ms">
          分布: {(status.latency.buckets ?? [0,0,0,0,0]).join('/')}
        </span>
      </div>
    {/if}
  </div>
{/if}

<style>
  .wc-monitor-ctrl { display:inline-flex;align-items:center;gap:6px; }
  .wc-monitor-badge { display:inline-flex;align-items:center;gap:5px;font-size:11.5px;padding:2px 8px;border-radius:5px;background:color-mix(in srgb,var(--wc-muted) 12%,transparent);color:var(--wc-muted);border:1px solid color-mix(in srgb,var(--wc-muted) 28%,transparent);white-space:nowrap; }
  .wc-monitor-badge::before { content:'';display:inline-block;width:5px;height:5px;border-radius:50%;background:currentColor; }
  .wc-monitor-running { color:var(--app-success,#16a34a) !important; border-color:color-mix(in srgb,var(--app-success,#16a34a) 38%,transparent) !important; background:color-mix(in srgb,var(--app-success,#16a34a) 10%,transparent) !important; }
  .wc-monitor-running::before { background:#0a0; }
  .wc-metrics-panel { display:flex;flex-direction:column;gap:3px;margin-left:8px;padding:4px 8px;border-radius:4px;background:color-mix(in srgb,var(--wc-bg-elevated, #fff) 80%, transparent);border:1px solid var(--wc-border);font-size:11.5px;color:var(--wc-text2); }
  .wc-metric-row { display:flex;gap:10px;align-items:center; }
  .wc-metric { white-space:nowrap; }
</style>
