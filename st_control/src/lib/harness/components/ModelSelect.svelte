<script lang="ts">
  // ============================================================
  // ModelSelect — 模型座（DSH ui-model-selection ModelSelect 迁移）
  // 会话头右侧：一个紧凑按钮替代 提供方/模型/推理等级 三个原生下拉；
  // 点击弹出三级菜单（提供方 → 模型 → effort，仅模型声明时显示）。
  // ============================================================
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import CheckIcon from "@lucide/svelte/icons/check";
  import SparklesIcon from "@lucide/svelte/icons/sparkles";
  import { onMount } from "svelte";

  let {
    providers = [],
    models = [],
    modelEfforts = [],
    providerId = "",
    modelId = "",
    effortId = "",
    onProviderChange,
    onModelChange,
    onEffortChange,
  }: {
    providers: { id: string; name: string }[];
    models: string[];
    modelEfforts: string[];
    providerId: string;
    modelId: string;
    effortId: string;
    onProviderChange?: (id: string) => void;
    onModelChange?: (model: string) => void;
    onEffortChange?: (effort: string) => void;
  } = $props();

  let open = $state(false);
  let rootRef: HTMLDivElement | null = $state(null);

  /** 外部点击关闭（弹层在根节点外） */
  onMount(() => {
    function onDocPointer(e: PointerEvent) {
      if (open && rootRef && !rootRef.contains(e.target as Node)) {
        open = false;
      }
    }
    document.addEventListener("pointerdown", onDocPointer);
    return () => document.removeEventListener("pointerdown", onDocPointer);
  });

  const currentProvider = $derived(providers.find((p) => p.id === providerId));
</script>

<div class="hns-model-seat" bind:this={rootRef}>
  <button
    class="hns-model-seat-btn"
    class:on={open}
    onclick={() => (open = !open)}
    title="选择提供方 / 模型 / 推理等级"
    aria-haspopup="menu"
    aria-expanded={open}
  >
    <SparklesIcon class="size-3.5" />
    <span class="hns-model-seat-name">{modelId || "选择模型"}</span>
    {#if currentProvider}
      <span class="hns-model-seat-provider">{currentProvider.name}</span>
    {/if}
    <ChevronDownIcon class="size-3" />
  </button>

  {#if open}
    <div class="hns-model-pop" role="menu">
      <div class="hns-model-section">
        <div class="hns-model-head">提供方</div>
        {#each providers as p (p.id)}
          <button
            class="hns-model-row"
            class:on={p.id === providerId}
            role="menuitemradio"
            aria-checked={p.id === providerId}
            onclick={() => {
              if (p.id !== providerId) onProviderChange?.(p.id);
              open = false;
            }}
          >
            <span class="hns-model-row-name">{p.name}</span>
            {#if p.id === providerId}<CheckIcon class="size-3" />{/if}
          </button>
        {/each}
      </div>
      <div class="hns-model-section">
        <div class="hns-model-head">模型</div>
        {#each models as m (m)}
          <button
            class="hns-model-row"
            class:on={m === modelId}
            role="menuitemradio"
            aria-checked={m === modelId}
            onclick={() => {
              onModelChange?.(m);
              open = false;
            }}
          >
            <span class="hns-model-row-name">{m}</span>
            {#if m === modelId}<CheckIcon class="size-3" />{/if}
          </button>
        {/each}
      </div>
      {#if modelEfforts.length > 0}
        <div class="hns-model-section">
          <div class="hns-model-head">推理等级</div>
          <button
            class="hns-model-row"
            class:on={effortId === ""}
            role="menuitemradio"
            aria-checked={effortId === ""}
            onclick={() => {
              onEffortChange?.("");
              open = false;
            }}
          >
            <span class="hns-model-row-name">跟随默认</span>
            {#if effortId === ""}<CheckIcon class="size-3" />{/if}
          </button>
          {#each modelEfforts as ef (ef)}
            <button
              class="hns-model-row"
              class:on={effortId === ef}
              role="menuitemradio"
              aria-checked={effortId === ef}
              onclick={() => {
                onEffortChange?.(ef);
                open = false;
              }}
            >
              <span class="hns-model-row-name">{ef}</span>
              {#if effortId === ef}<CheckIcon class="size-3" />{/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .hns-model-seat { position: relative; flex: none; }
  .hns-model-seat-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    max-width: 260px;
    padding: 0 10px;
    font-size: 12px;
    color: var(--hns-text, inherit);
    background: var(--hns-surface);
    border: 1px solid var(--hns-border);
    border-radius: 8px;
    cursor: pointer;
    white-space: nowrap;
    transition: border-color .15s, color .15s;
  }
  .hns-model-seat-btn:hover, .hns-model-seat-btn.on {
    border-color: color-mix(in srgb, var(--hns-accent) 45%, var(--hns-border));
    color: var(--hns-accent);
  }
  .hns-model-seat-btn > :global(svg):first-child { color: var(--hns-accent); flex: none; }
  .hns-model-seat-name {
    overflow: hidden;
    text-overflow: ellipsis;
    font-weight: 600;
    min-width: 0;
  }
  .hns-model-seat-provider {
    font-size: 10.5px;
    color: var(--hns-muted);
    max-width: 90px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .hns-model-pop {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 70;
    min-width: 230px;
    max-width: 300px;
    max-height: 380px;
    overflow: auto;
    background: var(--hns-card);
    border: 1px solid var(--hns-border);
    border-radius: 10px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, .18);
    padding: 6px;
  }
  .hns-model-section + .hns-model-section {
    border-top: 1px solid var(--hns-border-light, rgba(128, 128, 128, .14));
    margin-top: 4px;
    padding-top: 4px;
  }
  .hns-model-head {
    font-size: 10.5px;
    font-weight: 700;
    color: var(--hns-muted, #888);
    letter-spacing: .04em;
    padding: 4px 8px 3px;
  }
  .hns-model-row {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    padding: 5px 8px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--hns-text, inherit);
    font-size: 12px;
    text-align: left;
    cursor: pointer;
  }
  .hns-model-row:hover { background: color-mix(in srgb, var(--hns-accent) 10%, transparent); }
  .hns-model-row.on { color: var(--hns-accent); font-weight: 600; }
  .hns-model-row-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hns-model-row > :global(svg) { flex: none; color: var(--hns-accent); }
</style>
