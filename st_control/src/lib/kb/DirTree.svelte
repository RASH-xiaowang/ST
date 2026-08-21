<script lang="ts">
  import type { DirNode } from './kbTypes';
  import DirTree from './DirTree.svelte';
  import KbIcon from './KbIcon.svelte';

  interface Props {
    nodes: DirNode[];
    expanded: Record<number, boolean>;
    onToggle: (id: number) => void;
    onAdd: (parentId: number | null) => void;
    onSelect?: (id: number) => void;
    onRename?: (id: number) => void;
    onDelete?: (id: number) => void;
    selectedId?: number | null;
  }

  let { nodes, expanded, onToggle, onAdd, onSelect, onRename, onDelete, selectedId = null }: Props = $props();

  function isOpen(id: number) {
    return expanded[id] === true;
  }
</script>

<ul class="dir-tree">
  {#each nodes as node (node.id)}
    <li class="dir-node">
      <div class="dir-row" class:selected={selectedId === node.id}>
        <button class="dir-caret" onclick={() => onToggle(node.id)} aria-label="展开/收起">
          {#if node.children.length > 0}
            <span class="caret" class:open={isOpen(node.id)}><KbIcon name="caretRight" size={12} /></span>
          {:else}
            <span class="caret leaf"><span class="leaf-dot"></span></span>
          {/if}
        </button>
        <span
          class="dir-name"
          role="button"
          tabindex="0"
          onclick={() => onSelect?.(node.id)}
          onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onSelect?.(node.id); } }}
        >{node.name}</span>
        <button class="dir-act" title="重命名" onclick={() => onRename?.(node.id)}><KbIcon name="edit" size={13} /></button>
        <button class="dir-act" title="删除（含子目录与文档）" onclick={() => onDelete?.(node.id)}><KbIcon name="trash" size={13} /></button>
        <button class="dir-add" title="在此目录下新建子目录" onclick={() => onAdd(node.id)}><KbIcon name="plus" size={13} weight="bold" /></button>
      </div>
      {#if node.children.length > 0 && isOpen(node.id)}
        <DirTree nodes={node.children} {expanded} {onToggle} {onAdd} {onSelect} {onRename} {onDelete} {selectedId} />
      {/if}
    </li>
  {/each}
</ul>

<style>
  .dir-tree { list-style: none; margin: 0; padding-left: 0; }
  .dir-node { margin: 0; }
  .dir-row {
    display: flex; align-items: center; gap: 4px;
    padding: 4px 6px; border-radius: var(--kb-radius-sm, 6px); cursor: default;
    transition: background .12s;
  }
  .dir-row:hover { background: var(--kb-hover); }
  .dir-row.selected { background: var(--kb-hover-strong); box-shadow: inset 0 0 0 1px var(--kb-border-strong), inset 0 0 14px color-mix(in srgb, var(--app-accent) 7%, transparent), 0 0 10px -4px color-mix(in srgb, var(--app-accent) 50%, transparent); }
  .dir-caret {
    background: none; border: none; cursor: pointer; padding: 0 2px;
    color: var(--app-color-muted, #999); font-size: 11.5px; width: 14px;
  }
  .caret.leaf { color: var(--app-color-very-muted, #bbb); cursor: default; }
  .caret.open { transform: rotate(90deg); display: inline-block; transition: transform .12s; }
  .leaf-dot { display: inline-block; width: 6px; height: 6px; border-radius: 50%; background: var(--app-color-very-muted, #ccc); }
  .dir-name {
    flex: 1; cursor: pointer; user-select: none;
    color: var(--kb-text); font-size: 13px;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .dir-row.selected .dir-name { color: var(--kb-accent-bright); font-weight: 500; }
  .dir-add {
    background: none; border: none; color: var(--kb-accent-bright); cursor: pointer;
    font-size: 14px; line-height: 1; border-radius: var(--app-radius-xs, 4px); padding: 0 6px;
    opacity: 0; transition: opacity .12s;
  }
  .dir-row:hover .dir-add { opacity: 1; }
  .dir-add:hover { background: var(--kb-hover); color: var(--kb-accent-bright); }
  .dir-act {
    background: none; border: none; color: var(--kb-text-3); cursor: pointer;
    font-size: 12px; line-height: 1; border-radius: var(--app-radius-xs, 4px); padding: 0 4px;
    visibility: hidden;
  }
  .dir-row:hover .dir-act { visibility: visible; }
  .dir-act:hover { background: var(--kb-hover); color: var(--kb-err); }
</style>
