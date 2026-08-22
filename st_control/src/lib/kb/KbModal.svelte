<!--
  知识库模态弹窗容器（mask + 关闭交互）。
  自 KbDocs 等收敛：统一的 mask 结构（role/aria/tabindex/键盘关闭 +
  target 自检）；内容（kb-modal 及内部 hd/bd/ft）由调用方提供。
  样式依赖全局 kbui.css 的 .kb-modal-*（无需 scoped CSS 迁移）。
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    open,
    onClose,
    ariaLabel = '关闭弹窗',
    children,
  }: {
    open: boolean;
    onClose: () => void;
    ariaLabel?: string;
    children?: Snippet;
  } = $props();

  // 打开期间：全局 Esc 关闭（不依赖 mask 聚焦，避免焦点在输入框/下拉时关不掉弹窗）
  $effect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  });
</script>

{#if open}
  <div
    class="kb-modal-mask"
    role="button"
    aria-label={ariaLabel}
    tabindex="-1"
    onclick={(e) => { if (e.target === e.currentTarget) onClose(); }}
    onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ' || e.key === 'Escape') { e.preventDefault(); onClose(); } }}
  >
    {@render children?.()}
  </div>
{/if}
