<!--
  LiveNumber：数值变化时从旧值平滑滚动到新值（对比 NumberTicker 只播一次，
  本组件适合实时刷新的指标，如 Agent 数 / 消息量 / 系统负载）。
-->
<script lang="ts">
  let {
    value,
    duration = 650,
    decimalPlaces = 0,
    class: className = '',
  }: {
    value: number;
    duration?: number;
    decimalPlaces?: number;
    class?: string;
  } = $props();

  // svelte-ignore state_referenced_locally —— 初始化快照有意：display/shown 由下方 $effect 驱动
  let display = $state(value);
  // svelte-ignore state_referenced_locally —— 同上
  let shown = $state(value);

  $effect(() => {
    const from = shown;
    const to = value;
    if (from === to) return;
    const start = performance.now();
    let raf = 0;
    const tick = (now: number) => {
      const p = Math.min(1, (now - start) / duration);
      const eased = 1 - Math.pow(1 - p, 3);
      display = from + (to - from) * eased;
      if (p < 1) {
        raf = requestAnimationFrame(tick);
      } else {
        shown = to;
        display = to;
      }
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });

  let formatted = $derived(
    new Intl.NumberFormat('zh-CN', {
      minimumFractionDigits: decimalPlaces,
      maximumFractionDigits: decimalPlaces,
    }).format(Number(display.toFixed(decimalPlaces)))
  );
</script>

<span class={className}>{formatted}</span>
