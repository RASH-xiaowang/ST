<script lang="ts">
  /**
   * FAQ 管理组件
   * 功能：查看/添加/删除 FAQ 问答对
   */
  import { kbApi } from './services/ipc';
  import { kbConfirm } from './KbConfirm.svelte';
  import KbIcon from './KbIcon.svelte';
  import { Button } from '../components/ui/button';
  import { Empty, EmptyTitle, EmptyDescription } from '../components/ui/empty';
  import { Skeleton } from '../components/ui/skeleton';

  interface Props {
    kbId: number;
    isAdmin: boolean;
    notify: (msg: string, type?: 'success' | 'error' | 'warn') => void;
  }
  let { kbId, isAdmin, notify }: Props = $props();

  let faqs = $state<Record<string, unknown>[]>([]);
  let loading = $state(false);
  let err = $state('');

  // 添加 FAQ
  let addOpen = $state(false);
  let newQuestion = $state('');
  let newAnswer = $state('');
  let newCategory = $state('');
  let addBusy = $state(false);
  let addErr = $state('');

  // 批量导入
  let importOpen = $state(false);
  let importJson = $state('');
  let importBusy = $state(false);
  let importErr = $state('');

  async function loadFaqs() {
    loading = true; err = '';
    try {
      faqs = await kbApi.faqList(kbId);
    } catch (e: unknown) {
      err = '加载 FAQ 失败：' + e;
    } finally {
      loading = false;
    }
  }

  function openAdd() {
    newQuestion = ''; newAnswer = ''; newCategory = '';
    addErr = ''; addBusy = false; addOpen = true;
  }

  async function doAdd() {
    if (!newQuestion.trim()) { addErr = '请输入问题'; return; }
    if (!newAnswer.trim()) { addErr = '请输入答案'; return; }
    addBusy = true; addErr = '';
    try {
      await kbApi.faqImport(kbId, [{
        question: newQuestion.trim(),
        answer: newAnswer.trim(),
        category: newCategory.trim() || undefined,
      }]);
      addOpen = false;
      await loadFaqs();
      notify('FAQ 已添加');
    } catch (e: unknown) {
      addErr = '添加失败：' + e;
    } finally {
      addBusy = false;
    }
  }

  function openImport() {
    importJson = ''; importErr = ''; importBusy = false; importOpen = true;
  }

  async function doImport() {
    importBusy = true; importErr = '';
    try {
      const entries = JSON.parse(importJson);
      if (!Array.isArray(entries)) { importErr = '请输入 JSON 数组'; return; }
      const valid = entries.filter((e: Record<string, unknown>) => e.question && e.answer);
      if (valid.length === 0) { importErr = '无有效问答对'; return; }
      const res = await kbApi.faqImport(kbId, valid);
      importOpen = false;
      await loadFaqs();
      notify(`已导入 ${res.imported} 条 FAQ`);
    } catch (e: unknown) {
      importErr = '导入失败：' + e;
    } finally {
      importBusy = false;
    }
  }

  async function deleteFaq(id: number, question: string) {
    if (!await kbConfirm({
      title: '删除 FAQ',
      message: `确定删除以下 FAQ？\n\n问题：${question}`,
      danger: true,
      confirmText: '删除',
    })) return;
    try {
      await kbApi.faqDelete(kbId, id);
      await loadFaqs();
      notify('FAQ 已删除');
    } catch (e: unknown) {
      notify('删除失败：' + e, 'error');
    }
  }

  // 初始加载
  $effect(() => { kbId; loadFaqs(); });
</script>

<div class="kb-faq">
  <div class="kb-faq-hd">
    <h3 class="kb-faq-title"><KbIcon name="list" size={16} />FAQ 问答对</h3>
    {#if isAdmin}
      <div class="kb-faq-actions">
        <Button variant="outline" size="sm" onclick={openImport}><KbIcon name="upload" size={12} />批量导入</Button>
        <Button size="sm" onclick={openAdd}><KbIcon name="plus" size={12} weight="bold" />添加</Button>
      </div>
    {/if}
  </div>

  <p class="kb-faq-hint">FAQ 问答对在检索时优先命中，直接给出标准答案。</p>

  {#if err}
    <div class="kb-msg err">{err}</div>
  {/if}

  {#if loading}
    <div class="flex flex-col gap-2 p-2">
      {#each Array(3) as _}
        <Skeleton class="h-[60px] rounded-lg" />
      {/each}
    </div>
  {:else if faqs.length === 0}
    <Empty class="min-h-[100px] p-4">
      <KbIcon name="list" size={20} color="var(--kb-text-3)" />
      <EmptyTitle class="text-sm">暂无 FAQ</EmptyTitle>
      {#if isAdmin}
        <EmptyDescription>点击「添加」或「批量导入」创建 FAQ 问答对</EmptyDescription>
      {/if}
    </Empty>
  {:else}
    <div class="kb-faq-list">
      {#each faqs as faq}
        <div class="kb-faq-item">
          <div class="kb-faq-item-header">
            <span class="kb-faq-question">Q: {String(faq.question || '')}</span>
            {#if faq.category}
              <span class="kb-faq-category">{String(faq.category)}</span>
            {/if}
            {#if isAdmin}
              <button class="kb-btn-sm kb-dang" onclick={() => deleteFaq(Number(faq.id), String(faq.question || ''))} title="删除">
                <KbIcon name="trash" size={12} />
              </button>
            {/if}
          </div>
          <div class="kb-faq-answer">A: {String(faq.answer || '')}</div>
          <div class="kb-faq-meta">更新于 {String(faq.updatedAt || '')}</div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<!-- 添加 FAQ 弹窗 -->
{#if addOpen}
  <div class="kb-modal-overlay" onclick={() => { if (!addBusy) addOpen = false; }} onkeydown={(e) => e.key === 'Escape' && (addOpen = false)} role="dialog" aria-modal="true" tabindex="-1">
    <div class="kb-modal-box" role="presentation" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
      <div class="kb-modal-hd"><KbIcon name="plus" size={16} color="var(--kb-accent-bright)" />添加 FAQ</div>
      <div class="kb-modal-bd">
        <div style="display:flex;flex-direction:column;gap:12px">
          <label class="kb-label">问题 *
            <textarea class="kb-textarea" rows="2" placeholder="用户可能提出的问题" bind:value={newQuestion}></textarea>
          </label>
          <label class="kb-label">答案 *
            <textarea class="kb-textarea" rows="3" placeholder="标准答案" bind:value={newAnswer}></textarea>
          </label>
          <label class="kb-label">分类（可选）
            <input class="kb-input" placeholder="如：产品、技术、常见问题" bind:value={newCategory} />
          </label>
          {#if addErr}<div class="kb-msg err">{addErr}</div>{/if}
        </div>
      </div>
      <div class="kb-modal-ft">
        <button class="kb-btn" onclick={() => addOpen = false} disabled={addBusy}>取消</button>
        <button class="kb-btn-md" onclick={doAdd} disabled={addBusy}>{addBusy ? '添加中…' : '添加'}</button>
      </div>
    </div>
  </div>
{/if}

<!-- 批量导入弹窗 -->
{#if importOpen}
  <div class="kb-modal-overlay" onclick={() => { if (!importBusy) importOpen = false; }} onkeydown={(e) => e.key === 'Escape' && (importOpen = false)} role="dialog" aria-modal="true" tabindex="-1">
    <div class="kb-modal-box" role="presentation" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
      <div class="kb-modal-hd"><KbIcon name="upload" size={16} color="var(--kb-accent-bright)" />批量导入 FAQ</div>
      <div class="kb-modal-bd">
        <p class="kb-faq-import-hint">输入 JSON 数组，每项包含 question、answer、category（可选）字段：</p>
        <textarea class="kb-textarea kb-faq-import-textarea" rows="8" bind:value={importJson}
          placeholder={'[\n  {"question": "如何重置密码？", "answer": "点击设置 > 修改密码", "category": "账户"},\n  {"question": "支持哪些格式？", "answer": "PDF、Word、TXT、Markdown 等"}\n]'}></textarea>
        {#if importErr}<div class="kb-msg err" style="margin-top:8px">{importErr}</div>{/if}
      </div>
      <div class="kb-modal-ft">
        <button class="kb-btn" onclick={() => importOpen = false} disabled={importBusy}>取消</button>
        <button class="kb-btn-md" onclick={doImport} disabled={importBusy}>{importBusy ? '导入中…' : '导入'}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .kb-faq { display: flex; flex-direction: column; gap: 12px; }
  .kb-faq-hd { display: flex; align-items: center; justify-content: space-between; }
  .kb-faq-title { font-size: 14px; font-weight: 600; margin: 0; display: flex; align-items: center; gap: 6px; }
  .kb-faq-actions { display: flex; gap: 6px; }
  .kb-faq-hint { font-size: 12px; color: var(--kb-text-3); margin: 0; }
  .kb-faq-list { display: flex; flex-direction: column; gap: 8px; max-height: 400px; overflow-y: auto; }
  .kb-faq-item { padding: 12px; border: 1px solid var(--kb-border); border-radius: 8px; }
  .kb-faq-item-header { display: flex; align-items: center; gap: 8px; margin-bottom: 6px; }
  .kb-faq-question { font-size: 13px; font-weight: 600; color: var(--kb-text); flex: 1; }
  .kb-faq-category { font-size: 11px; padding: 2px 6px; border-radius: 4px; background: var(--kb-hover-strong); color: var(--kb-text-2); }
  .kb-faq-answer { font-size: 13px; color: var(--kb-text-2); line-height: 1.6; margin-bottom: 6px; }
  .kb-faq-meta { font-size: 11.5px; color: var(--kb-text-3); }
  .kb-dang { color: var(--app-danger); }
  .kb-dang:hover { background: color-mix(in srgb, var(--app-danger) 10%, transparent); }
  .kb-faq-import-hint { font-size: 12px; color: var(--kb-text-3); margin: 0 0 8px; }
  .kb-faq-import-textarea { font-family: var(--font-mono); font-size: 12px; }
  .kb-modal-overlay { position: fixed; inset: 0; z-index: 100; background: rgba(0,0,0,0.4); display: grid; place-items: center; }
  .kb-modal-box { background: var(--app-bg-color); border: 1px solid var(--kb-border); border-radius: 12px; width: min(480px, 90vw); max-height: 80vh; overflow: auto; }
  .kb-modal-hd { display: flex; align-items: center; gap: 8px; padding: 16px; border-bottom: 1px solid var(--kb-border-subtle); font-size: 14px; font-weight: 600; }
  .kb-modal-bd { padding: 16px; }
  .kb-modal-ft { display: flex; justify-content: flex-end; gap: 8px; padding: 12px 16px; border-top: 1px solid var(--kb-border-subtle); }
</style>
