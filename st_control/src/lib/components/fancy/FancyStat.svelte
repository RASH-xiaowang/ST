<!--
  FancyStat：仪表台统计卡（数值平滑滚动 + 可选后缀/小数位）
  用于监控 / 数据看板 / 大模型用量等数值展示场景。
-->
<script lang="ts">
  import LiveNumber from './LiveNumber.svelte';

  let {
    value,
    label,
    suffix = '',
    duration = 700,
    decimalPlaces = 0,
    gradientColor = '#22d3ee',
    gradientOpacity = 0.14,
    gradientSize = 260,
    valueClass = '',
    class: className = '',
  }: {
    value: number;
    label: string;
    suffix?: string;
    duration?: number;
    decimalPlaces?: number;
    valueClass?: string;
    class?: string;
    gradientColor?: string;
    gradientOpacity?: number;
    gradientSize?: number;
  } = $props();

  // 旧聚光参数保留以兼容既有调用点；视觉统一为仪表台平卡
  $effect(() => {
    void [gradientColor, gradientOpacity, gradientSize];
  });
</script>

<div class="fstat-card {className}">
  <div class="fstat">
    <span class="fstat-num">
      <LiveNumber {value} {duration} {decimalPlaces} class={valueClass} />{suffix}
    </span>
    <span class="fstat-lbl">{label}</span>
  </div>
</div>

<style>
  .fstat-card {
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--card);
    overflow: hidden;
  }
  .fstat {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 16px 18px;
    background: transparent;
    border: none;
  }
  .fstat-num {
    font-size: 24px;
    font-weight: 700;
    line-height: 1.1;
    color: var(--foreground);
    font-variant-numeric: tabular-nums;
    letter-spacing: -0.01em;
  }
  .fstat-lbl {
    font-size: 12px;
    color: var(--muted-foreground);
  }
</style>
