<script lang="ts">
  import type { Snippet } from 'svelte';

  let { active, children }: { active: boolean; children: Snippet } = $props();
</script>

<section class="panel panel-full" class:panel-hidden={!active}>
  {@render children()}
</section>

<style>
  /* 面板结构样式归属本组件：Svelte 5 不再把父组件作用域类附加到子组件根节点，
   * 若写在 App.svelte 中无法命中本组件根元素，会导致面板高度/显隐失效（所有
   * 面板同屏堆叠）。故由本组件自持。 */
  .panel {
    position: relative;
    z-index: 1;
    height: 100%;
    overflow: auto;
    background: transparent;
  }
  .panel-full {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .panel-hidden {
    display: none !important;
  }
</style>
