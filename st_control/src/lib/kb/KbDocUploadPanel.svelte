<script lang="ts">
  import type { UploadTask } from './kbTypes';
  import KbIcon from './KbIcon.svelte';
  import { Button } from '../components/ui/button';
  import { Badge } from '../components/ui/badge';

  interface Props {
    tasks: UploadTask[];
    onClear: () => void;
    onRetry: (index: number) => void;
  }
  let { tasks, onClear, onRetry }: Props = $props();

  let panelOpen = $state(true);
  let panelEl = $state<HTMLElement | null>(null);

  // 新任务加入时自动滚到底部
  $effect(() => {
    const n = tasks.length;
    if (n > 0 && panelOpen && panelEl) {
      panelEl.scrollTop = panelEl.scrollHeight;
    }
  });
</script>

{#if tasks.length > 0}
  <div class="kb-upload-panel" class:kb-upload-collapsed={!panelOpen}>
    <div class="kb-upload-panel-hd">
      <span style="display:inline-flex;align-items:center;gap:6px;font-size:13px;font-weight:600">
        <KbIcon name="upload" size={14} color="var(--kb-accent-bright)" />
        上传任务
        <Badge variant="secondary" class="text-[10px]">{tasks.length}</Badge>
      </span>
      <div style="display:flex;align-items:center;gap:4px">
        <Button variant="ghost" size="sm" onclick={onClear} title="清空记录"><KbIcon name="trash" size={12} />清空</Button>
        <Button variant="ghost" size="icon-sm" onclick={() => panelOpen = !panelOpen}
          title={panelOpen ? '收起任务列表' : '展开任务列表'}>
          <KbIcon name={panelOpen ? 'caretDown' : 'caretRight'} size={12} />
        </Button>
      </div>
    </div>
    {#if panelOpen}
      <div class="kb-upload-panel-body" bind:this={panelEl}>
        {#each tasks as t, i}
          <div class="kb-upload-item">
            <span style="flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title={t.file.name}>{t.file.name}</span>
            {#if t.status === 'pending'}<Badge variant="outline" class="text-[10px]">等待中</Badge>
            {:else if t.status === 'uploading'}<Badge variant="secondary" class="text-[10px]">处理中…</Badge>
            {:else if t.status === 'done'}<Badge variant="default" class="text-[10px]"><KbIcon name="check" size={11} />{t.msg}</Badge>
            {:else}<Badge variant="destructive" class="text-[10px]"><KbIcon name="close" size={11} />{t.msg}</Badge>
            {/if}
            {#if t.status === 'error'}
              <Button variant="outline" size="sm" onclick={() => onRetry(i)}>重试</Button>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}
