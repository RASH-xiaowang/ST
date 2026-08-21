<!--
  WechatHoverButton：微信数据管理按钮统一封装（仪表台平键）
  - 骨白键面 + 刻线边框 + 按下位移，青蓝 hover 语义
  - 紧凑尺寸、单行不换行、不被 flex 压缩
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    text = '',
    class: className = '',
    children,
    onclick,
    onkeydown,
    disabled,
    title,
    type = 'button' as 'button' | 'submit' | 'reset',
    'aria-label': ariaLabel,
  }: {
    text?: string;
    class?: string;
    children?: Snippet;
    onclick?: (event: MouseEvent) => void;
    onkeydown?: (event: KeyboardEvent) => void;
    disabled?: boolean;
    title?: string;
    type?: 'button' | 'submit' | 'reset';
    'aria-label'?: string;
  } = $props();
</script>

<button
  {type}
  {onclick}
  {onkeydown}
  {disabled}
  {title}
  aria-label={ariaLabel}
  class="wc-ihb {className}"
>
  {#if children}{@render children()}{:else}{text}{/if}
</button>

<style>
  /* 图标与文本同一行并垂直居中 */
  :global(.wc-ihb) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 28px;
    padding: 0 12px;
    border: 1px solid var(--wc-border, var(--border));
    border-radius: 7px;
    background: var(--wc-card, var(--card));
    color: var(--wc-text2, var(--foreground));
    font-size: 12px;
    font-weight: 500;
    line-height: 1;
    white-space: nowrap;
    flex-shrink: 0;
    cursor: pointer;
    transition: background 0.14s ease, border-color 0.14s ease, color 0.14s ease, transform 0.06s ease;
  }
  :global(.wc-ihb:hover:not(:disabled)) {
    border-color: color-mix(in srgb, var(--wc-theme, var(--brand)) 48%, var(--wc-border, var(--border)));
    background: color-mix(in srgb, var(--wc-theme, var(--brand)) 9%, var(--wc-card, var(--card)));
    color: var(--wc-text, var(--foreground));
  }
  :global(.wc-ihb:active:not(:disabled)) {
    transform: translateY(1px);
  }
  :global(.wc-ihb:disabled) {
    opacity: 0.48;
    cursor: not-allowed;
  }
  :global(.wc-ihb:focus-visible) {
    outline: 2px solid color-mix(in srgb, var(--wc-theme, var(--brand)) 55%, transparent);
    outline-offset: 1px;
  }
  :global(.wc-ihb > svg) {
    flex-shrink: 0;
  }
  :global(.wc-ihb-active) {
    border-color: color-mix(in srgb, var(--wc-theme, var(--brand)) 62%, var(--wc-border, var(--border)));
    background: color-mix(in srgb, var(--wc-theme, var(--brand)) 14%, var(--wc-card, var(--card)));
    color: var(--wc-text, var(--foreground));
    font-weight: 600;
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--wc-theme, var(--brand)) 22%, transparent);
  }
</style>
