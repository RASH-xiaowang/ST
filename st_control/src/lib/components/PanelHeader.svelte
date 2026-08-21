<!-- 面板通用头部：图标 + 标题 + 徽标 + 右侧操作区（均通过 Snippet 传入） -->
<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    icon,
    badge,
    actions,
  }: {
    title: string;
    icon?: Snippet;
    badge?: Snippet;
    actions?: Snippet;
  } = $props();
</script>

<header class="ph-root">
  <div class="flex items-center gap-3">
    <span class="ph-ico">{#if icon}{@render icon()}{/if}</span>
    <span class="ph-title">{title}</span>
    {#if badge}{@render badge()}{/if}
  </div>
  {#if actions}
    <div class="flex items-center gap-3">{@render actions()}</div>
  {/if}
</header>

<style>
  .ph-root {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 20px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  .ph-root::after {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    bottom: -1px;
    height: 1px;
    pointer-events: none;
    background: linear-gradient(90deg, transparent, color-mix(in oklab, var(--primary) 30%, transparent) 22%, color-mix(in oklab, var(--primary) 30%, transparent) 72%, transparent);
    box-shadow: 0 0 10px color-mix(in oklab, var(--primary) 18%, transparent);
  }
  .ph-title {
    font-size: 15px;
    font-weight: 700;
    letter-spacing: -0.01em;
    color: var(--foreground);
  }
  .ph-ico {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: 10px;
    background: color-mix(in oklab, var(--brand) 12%, var(--card));
    color: var(--brand-strong);
    border: 1px solid color-mix(in oklab, var(--brand) 24%, transparent);
    flex: none;
  }
</style>
