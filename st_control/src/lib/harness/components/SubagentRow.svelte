<script lang="ts">
  // ============================================================
  // SubagentRow — 子代理目录树行（DSH ui-subagent SubagentCatalog 迁移）
  // 递归渲染后代；行点击打开子会话；状态点 = 运行中(蓝)/未运行(绿)。
  // ============================================================
  import type { SubagentNode } from "../types";
  import GitForkIcon from "@lucide/svelte/icons/git-fork";
  import SubagentRow from "./SubagentRow.svelte";

  let { node, onOpen }: {
    node: SubagentNode;
    /** 打开子代理会话（由 HarnessTab 提供 selectSession） */
    onOpen?: (id: string) => void;
  } = $props();

  let expanded = $state(false);
</script>

<div class="hns-subagent-node" role="treeitem" aria-level="1" aria-selected={false} aria-expanded={expanded}>
  <button
    class="hns-subagent-row"
    onclick={() => {
      if (node.has_children && node.children.length > 0) {
        expanded = !expanded;
      } else {
        onOpen?.(node.id);
      }
    }}
    title={node.title || node.id}
  >
    <span
      class="hns-subagent-dot"
      class:running={node.activity === "running"}
      aria-hidden="true"
    ></span>
    <span class="hns-subagent-label">{node.title || node.id}</span>
    <span class="hns-subagent-meta">
      {node.mode === "continuable" ? "可继续" : "一次性"}
      <span class="hns-subagent-meta-sep">·</span>
      {node.activity === "running" ? "正在运行" : "当前未运行"}
    </span>
    {#if node.has_children && node.children.length > 0}
      <span class="hns-subagent-chev" aria-hidden="true">{expanded ? "▾" : "▸"}</span>
    {:else}
      <GitForkIcon class="size-3 hns-subagent-fork" />
    {/if}
  </button>
  {#if expanded && node.children.length > 0}
    <div class="hns-subagent-children" role="group">
      {#each node.children as c (c.id)}
        <SubagentRow node={c} {onOpen} />
      {/each}
    </div>
  {/if}
</div>

<style>
  .hns-subagent-node { display: flex; flex-direction: column; }
  .hns-subagent-children { margin-left: 16px; border-left: 1px solid rgba(128, 128, 128, .18); padding-left: 8px; }
  .hns-subagent-row {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    padding: 5px 8px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--hns-text, inherit);
    font-size: 12px;
    text-align: left;
    cursor: pointer;
  }
  .hns-subagent-row:hover { background: color-mix(in srgb, var(--hns-accent, #4176e6) 10%, transparent); }
  .hns-subagent-dot {
    flex: none;
    width: 8px; height: 8px;
    border-radius: 50%;
    background: #2ea043;
  }
  .hns-subagent-dot.running {
    background: var(--hns-accent, #4176e6);
    animation: hns-subagent-pulse 1.2s ease-in-out infinite;
  }
  @keyframes hns-subagent-pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: .35; }
  }
  .hns-subagent-label {
    flex: 1; min-width: 0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-weight: 500;
  }
  .hns-subagent-meta { flex: none; font-size: 10.5px; color: var(--hns-muted, #888); }
  .hns-subagent-meta-sep { margin: 0 2px; }
  .hns-subagent-chev { flex: none; color: var(--hns-muted, #888); font-size: 10px; }
</style>
