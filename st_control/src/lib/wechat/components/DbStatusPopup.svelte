<!--
  DB 状态弹窗：展示微信解密数据库状态摘要。
  自 WeChatPanel.svelte 抽出：props 面 = loading / lines / onClose / onRefresh；
  点击外部关闭仍由父组件 document 级监听处理（依赖 .wc-db-status-popup 定位）。
-->
<script lang="ts">
  import WechatHoverButton from './WechatHoverButton.svelte';

  let {
    loading,
    lines,
    onClose,
    onRefresh,
  }: {
    loading: boolean;
    lines: string[];
    onClose: () => void;
    onRefresh: () => void;
  } = $props();
</script>

<div
  class="wc-db-status-popup"
  role="dialog"
  tabindex="-1"
  onclick={(e) => e.stopPropagation()}
  onkeydown={(e) => e.key === 'Escape' && onClose()}
>
  <div class="wc-db-popup-header">
    <span>解密数据库状态</span>
    <button class="wc-db-popup-close" onclick={onClose}>×</button>
  </div>
  {#if loading}
    <div class="wc-db-popup-loading"><span class="wc-loading-inline"></span>正在检查…</div>
  {:else if lines.length === 0}
    <div class="wc-db-popup-empty">暂无状态数据</div>
  {:else}
    <div class="wc-db-popup-body">
      {#each lines as line}
        <p>{line}</p>
      {/each}
    </div>
    <div class="wc-db-popup-footer">
      <WechatHoverButton text="刷新" onclick={onRefresh} />
    </div>
  {/if}
</div>

<style>
  @keyframes wc-spin { to { transform:rotate(360deg); } }
  .wc-db-status-popup { position:fixed; top:52px; right:14px; background:var(--wc-card); border:1px solid var(--wc-border); border-radius:8px; padding:0; z-index:100; box-shadow:0 4px 20px rgba(0,0,0,0.15); font-size:11.5px; line-height:1.6; min-width:280px; max-width:420px; overflow:hidden; }
  .wc-db-popup-header { display:flex;align-items:center;justify-content:space-between;padding:8px 12px;border-bottom:1px solid var(--wc-border);font-weight:600;font-size:12px; }
  .wc-db-popup-close { background:none;border:none;font-size:16px;cursor:pointer;color:var(--wc-muted);padding:0 2px;line-height:1; }
  .wc-db-popup-close:hover { color:var(--wc-text); }
  .wc-db-popup-body { padding:10px 14px; }
  .wc-db-popup-body p { margin:5px 0; }
  .wc-db-popup-loading { display:flex;align-items:center;justify-content:center;gap:6px;padding:20px;color:var(--wc-muted); }
  .wc-db-popup-empty { padding:20px;text-align:center;color:var(--wc-muted); }
  .wc-db-popup-footer { padding:8px 12px;border-top:1px solid var(--wc-border);display:flex;justify-content:flex-end; }
  .wc-loading-inline { display:inline-block;width:14px;height:14px;margin-right:6px;border:2px solid var(--wc-border);border-top-color:var(--wc-text);border-radius:50%;animation:wc-spin .7s linear infinite;vertical-align:middle; }
</style>
