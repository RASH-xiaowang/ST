<!--
  FancyCard：仪表台通用内容卡（骨白面板 + 刻线边框）
  - 保留旧 props（glow/beam/gradient*）以兼容既有调用，视觉一律为平卡
  外观令牌对齐 st_control 设计系统（--card / --border / --radius-lg）。
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    children,
    class: className = '',
    slotClass = 'w-full h-full',
    glow = false,
    beam = false,
    glowColor = ['#22d3ee', '#0ea5e9'],
    beamColorFrom = '#22d3ee',
    beamColorTo = '#6366f1',
    radius = 'var(--radius-lg)',
    gradientColor = '#22d3ee',
    gradientOpacity = 0.1,
    gradientSize = 260,
  }: {
    children?: Snippet;
    class?: string;
    slotClass?: string;
    radius?: string;
    glow?: boolean;
    beam?: boolean;
    glowColor?: string | string[];
    beamColorFrom?: string;
    beamColorTo?: string;
    gradientColor?: string;
    gradientOpacity?: number;
    gradientSize?: number;
  } = $props();

  // 旧 FancyUI 参数保留以兼容既有调用点；视觉统一为仪表台平卡，参数不再参与渲染
  $effect(() => {
    void [glow, beam, glowColor, beamColorFrom, beamColorTo, gradientColor, gradientOpacity, gradientSize];
  });
</script>

<div class="fancy-card-wrap {className}" style="--fancy-radius: {radius}">
  <div class="fancy-card-surface {slotClass}">
    {#if children}{@render children()}{/if}
  </div>
</div>

<style>
  .fancy-card-wrap {
    position: relative;
    border-radius: var(--fancy-radius, var(--radius-lg));
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .fancy-card-surface {
    border: 1px solid var(--border);
    border-radius: var(--fancy-radius, var(--radius-lg));
    background: var(--card);
    color: var(--foreground);
    overflow: hidden;
  }
</style>
