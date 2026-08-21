<!--
  ThemeFlickeringGrid：fancy-ui-svelte FlickeringGrid 的主题感知封装
  - 读取当前主题色（--brand / --primary / --app-accent）并解析为 FlickeringGrid
    需要的十六进制颜色（canvas fillStyle 不能直接使用 CSS 变量）。
  - 通过 MutationObserver 监听 :root 上样式/类名变化，主题色改变时自动刷新网格颜色。
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { FlickeringGrid } from 'fancy-ui-svelte';
  import { cssColorToHex } from '../colorUtils';

  let {
    squareSize = 4,
    gridGap = 6,
    flickerChance = 0.3,
    maxOpacity = 0.3,
    width,
    height,
    class: className = '',
  }: {
    squareSize?: number;
    gridGap?: number;
    flickerChance?: number;
    maxOpacity?: number;
    width?: number;
    height?: number;
    class?: string;
  } = $props();

  const FALLBACK_COLOR = '#22d3ee';
  const THEME_COLOR_VARS = ['--brand', '--primary', '--app-accent'];

  let gridColor = $state(FALLBACK_COLOR);

  /** 将任意 CSS 颜色解析为 #rrggbb（通过 1x1 canvas 采样，兼容 oklab / color-mix 等） */
  /** 解析 :root 上的主题色变量，跟随 var() 链，最后回退到默认青蓝 */
  function resolveThemeColor(): string {
    const root = document.documentElement;
    const computed = getComputedStyle(root);
    for (const name of THEME_COLOR_VARS) {
      let value = computed.getPropertyValue(name).trim();
      if (!value) continue;
      // 解析 --primary: var(--brand) 这类引用链
      let guard = 0;
      while (value.startsWith('var(') && guard < 8) {
        const m = /^var\(\s*(--[a-zA-Z0-9_-]+)\s*(?:,.*)?\)/.exec(value);
        if (!m) break;
        value = computed.getPropertyValue(m[1]).trim();
        guard++;
      }
      if (!value) continue;
      const hex = cssColorToHex(value);
      if (hex) return hex;
    }
    return FALLBACK_COLOR;
  }

  function refreshColor() {
    gridColor = resolveThemeColor();
  }

  onMount(() => {
    refreshColor();
    // 个性化设置通过修改 :root 内联样式应用主题；类名切换（dark 等）同样监听
    const observer = new MutationObserver(refreshColor);
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['style', 'class'],
    });
    // 主题色也可能来自样式表（如 app.css 中的 --brand），监听 <style>/<link> 变化
    const headObserver = new MutationObserver(refreshColor);
    headObserver.observe(document.head, {
      childList: true,
      subtree: true,
      characterData: true,
      attributes: true,
    });
    return () => {
      observer.disconnect();
      headObserver.disconnect();
    };
  });
</script>

<FlickeringGrid
  {squareSize}
  {gridGap}
  {flickerChance}
  {maxOpacity}
  {width}
  {height}
  color={gridColor}
  class={className}
/>
