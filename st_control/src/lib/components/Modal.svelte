<!--
  通用弹窗外壳（overlay + frame）：
  统一点击外部关闭（frame 内 stopPropagation）、Escape 关闭、role/aria/tabindex。
  样式类与内容由调用方通过 overlayClass/frameClass 与 children 提供，
  以便不同模块（modal-overlay/modal、st-overlay/st-frame 等）保持各自视觉。
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    open,
    onClose,
    overlayClass = 'modal-overlay',
    frameClass = 'modal',
    frameStyle,
    overlayRole = 'dialog',
    labelledBy,
    children,
  }: {
    open: boolean;
    onClose: () => void;
    overlayClass?: string;
    frameClass?: string;
    frameStyle?: string;
    overlayRole?: string;
    labelledBy?: string;
    children?: Snippet;
  } = $props();
</script>

{#if open}
  <div
    class={overlayClass}
    role={overlayRole}
    tabindex="-1"
    onclick={onClose}
    onkeydown={(e) => { if (e.key === 'Escape') onClose(); }}
  >
    <div
      class={frameClass}
      style={frameStyle}
      role="dialog"
      aria-modal="true"
      aria-labelledby={labelledBy}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => { if (e.key === 'Escape') onClose(); }}
    >
      {@render children?.()}
    </div>
  </div>
{/if}

<style>
  /* 通用弹窗壳样式：调用方经 overlayClass/frameClass 选择对应类 */
  .modal-overlay {
    position: fixed;
    inset: 0;
    z-index: 100;
    background: rgba(2, 6, 14, 0.62);
    backdrop-filter: blur(4px);
    display: grid;
    place-items: center;
    padding: 24px;
  }
  .modal {
    width: min(640px, 94vw);
    max-height: 88vh;
    display: flex;
    flex-direction: column;
    background: var(--popover);
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    box-shadow: 0 24px 70px rgba(0, 0, 0, 0.5);
    overflow: hidden;
  }
  .st-overlay {
    position: fixed;
    inset: 0;
    z-index: 100;
    background: rgba(4, 8, 14, 0.58);
    backdrop-filter: blur(5px);
    display: grid;
    place-items: center;
    padding: 24px;
  }
  /* 固定尺寸：960×720（上限 88vh），两栏框架 */
  .st-frame {
    width: 960px;
    max-width: 94vw;
    height: min(720px, 88vh);
    display: flex;
    flex-direction: column;
    background: var(--popover);
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    box-shadow: 0 24px 70px rgba(0, 0, 0, 0.45);
    overflow: hidden;
  }
</style>
