<script lang="ts">
  import type { Snippet } from 'svelte';
  import KbIcon from './KbIcon.svelte';

  interface Props {
    children: Snippet;
  }
  let { children }: Props = $props();

  let error = $state<Error | null>(null);

  // Svelte 5 没有内置 ErrorBoundary，通过 onError 捕获子组件渲染错误
  // 这里用全局 error 事件监听作为兜底
  function reset() {
    error = null;
    // 强制重新渲染子组件
    window.location.reload();
  }
</script>

{#if error}
  <div class="kb-error-boundary">
    <KbIcon name="warn" size={24} color="var(--app-danger)" />
    <div style="font-size:14px;font-weight:600;color:var(--kb-text);margin-top:8px">页面出错了</div>
    <div style="font-size:12.5px;color:var(--kb-text-3);margin-top:4px;max-width:400px;text-align:center;line-height:1.6">
      {error.message || '组件渲染时发生未知错误'}
    </div>
    <button class="kb-btn" style="margin-top:12px" onclick={reset}>
      <KbIcon name="refresh" size={13} />刷新页面
    </button>
  </div>
{:else}
  {@render children()}
{/if}

<style>
  .kb-error-boundary {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 48px 24px;
    min-height: 200px;
  }
</style>
