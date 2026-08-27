<script lang="ts">
  import { kbApi } from './services/ipc';
  import type { DocView, KbVersion } from './kbTypes';
  import { formatIsoTime } from '../format';
  import { renderMd } from './markdown';
  import { kbConfirm } from './KbConfirm.svelte';
  import KbIcon from './KbIcon.svelte';
  import { Button } from '../components/ui/button';
  import { Badge } from '../components/ui/badge';
  import { Empty, EmptyTitle, EmptyDescription } from '../components/ui/empty';

  interface Props {
    doc: DocView;
    versions: KbVersion[];
    selProvider: string;
    selModel: string;
    notify: (msg: string, type?: 'success' | 'error' | 'warn') => void;
    onClose: () => void;
    onRefresh: () => Promise<void>;
    onDownload: (id: number) => void;
    onRename: (doc: { id: number; title: string }) => void;
    onMove: (id: number) => void;
    onReprocess: (id: number) => void;
    onGenerateWiki: (id: number) => void;
    reprocessing: number | null;
    wikiGenBusy: boolean;
    newVersionBusy: boolean;
    onNewVersionPick: (e: Event) => void;
    ACCEPT_ATTR: string;
  }
  let {
    doc, versions, selProvider, selModel, notify, onClose, onRefresh,
    onDownload, onRename, onMove, onReprocess, onGenerateWiki,
    reprocessing, wikiGenBusy, newVersionBusy, onNewVersionPick, ACCEPT_ATTR,
  }: Props = $props();

  let detailTab = $state<'content' | 'chunks' | 'versions' | 'summary'>('content');
  let contentEditing = $state(false);
  let contentEditVal = $state('');
  let contentEditBusy = $state(false);

  // ─── 多模态分析 ───
  let multimodalBusy = $state(false);
  let multimodalResult = $state<string | null>(null);
  const isMultimodalType = $derived(
    ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'pdf'].includes((doc?.meta?.fileType ?? '').toLowerCase())
  );
  async function runMultimodalAnalysis() {
    if (!doc || multimodalBusy) return;
    multimodalBusy = true;
    try {
      const res = await kbApi.multimodalAnalyze(doc.meta.id);
      multimodalResult = res.summary;
      notify('多模态分析完成');
    } catch (e: unknown) {
      notify('多模态分析失败：' + e, 'error');
    } finally {
      multimodalBusy = false;
    }
  }

  // 版本对比
  let diffFromId = $state<number | null>(null);
  let diffToId = $state<number | null>(null);
  let diffData = $state<{ fromVersionNo: number; toVersionNo: number; added: string[]; removed: string[] } | null>(null);
  let diffLoading = $state(false);

  // 分块编辑
  let editChunk = $state<{ id: number; content: string } | null>(null);
  let editChunkVal = $state('');
  let editChunkBusy = $state(false);

  function fmtTime(t: string): string {
    return formatIsoTime(t, { showYear: true, utc: true });
  }

  function startContentEdit() {
    contentEditVal = doc?.content ?? '';
    contentEditing = true;
  }
  function cancelContentEdit() { contentEditing = false; }
  async function saveContentEdit() {
    if (!doc || contentEditBusy) return;
    contentEditBusy = true;
    try {
      const buf = new TextEncoder().encode(contentEditVal);
      const b64 = btoa(Array.from(buf, (b) => String.fromCharCode(b)).join(''));
      await kbApi.uploadNewVersion({
        input: {
          docId: doc.meta.id,
          fileType: doc.meta.fileType ?? 'md',
          dataBase64: b64,
          note: '在线编辑',
          embeddingProvider: selProvider || null,
          embeddingModel: selModel || null,
        },
      });
      contentEditing = false;
      notify('文档已保存并重新处理');
      await onRefresh();
    } catch (e: unknown) { notify('保存失败：' + e, 'error'); }
    finally { contentEditBusy = false; }
  }

  async function saveChunk() {
    if (editChunk === null || editChunkBusy) return;
    editChunkBusy = true;
    try {
      const res = await kbApi.updateChunk(editChunk.id, editChunkVal);
      if (res?.warning) notify(res.warning, 'warn');
      else notify('分块已更新并重新向量化');
      editChunk = null;
      await onRefresh();
    } catch (e: unknown) { notify('保存分块失败：' + e, 'error'); }
    finally { editChunkBusy = false; }
  }

  function pickDiffFrom(id: number) {
    diffFromId = id;
    if (diffToId === id) diffToId = versions.find((v) => v.id !== id)?.id ?? null;
  }
  function pickDiffTo(id: number) {
    diffToId = id;
    if (diffFromId === id) diffFromId = versions.find((v) => v.id !== id)?.id ?? null;
  }
  async function doVersionDiff() {
    if (!doc || diffFromId === null || diffToId === null) return;
    diffLoading = true; diffData = null;
    try {
      diffData = await kbApi.versionDiff(doc.meta.id, diffFromId, diffToId);
    } catch (e: unknown) { notify('对比失败：' + e, 'error'); }
    finally { diffLoading = false; }
  }
  function resetDiff() { diffFromId = null; diffToId = null; diffData = null; diffLoading = false; }

  async function restoreVersion(versionId: number) {
    if (!await kbConfirm({ message: '回滚到该版本？将生成新的版本并重新向量化。' })) return;
    try {
      await kbApi.restoreVersion(versionId);
      notify('版本回滚完成');
      await onRefresh();
    } catch (e: unknown) { notify('回滚失败：' + e, 'error'); }
  }
</script>

<div class="kb-card" style="flex:none;width:420px;display:flex;flex-direction:column;min-height:0">
  <div class="kb-card-hd" style="justify-content:space-between">
    <span style="min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title={doc.meta.title}>{doc.meta.title}</span>
    <Button variant="ghost" size="icon-sm" onclick={onClose}><KbIcon name="close" size={14} /></Button>
  </div>
  <div style="display:flex;gap:8px;padding:10px 12px;border-bottom:1px solid var(--kb-border);flex-wrap:wrap;align-items:center">
    <Badge variant="outline" class="text-[10px]">{doc.meta.fileType ?? '?'}</Badge>
    <Badge variant={doc.meta.status === 'ready' ? 'default' : doc.meta.status === 'failed' ? 'destructive' : 'secondary'} class="text-[10px]">{doc.meta.status}</Badge>
    <div style="flex:1"></div>
    <Button variant="outline" size="sm" onclick={() => onDownload(doc.meta.id)}><KbIcon name="download" size={12} />下载</Button>
    <Button variant="outline" size="sm" onclick={() => onRename(doc.meta)}><KbIcon name="edit" size={12} />重命名</Button>
    <Button variant="outline" size="sm" onclick={() => onMove(doc.meta.id)}><KbIcon name="move" size={12} />移动</Button>
    <label class="inline-flex">
      <Button variant="outline" size="sm" title="上传新版本并重新向量化">
        <KbIcon name="fileUp" size={12} />{newVersionBusy ? '处理中…' : '新版本'}
      </Button>
      <input type="file" hidden accept={ACCEPT_ATTR} onchange={onNewVersionPick} />
    </label>
    <Button variant="outline" size="sm" onclick={() => onGenerateWiki(doc.meta.id)}
      disabled={wikiGenBusy || doc.meta.status !== 'ready'}
      title={doc.meta.status === 'ready' ? '用 LLM 将本文档提炼为 Wiki 页面' : '仅就绪文档可提炼'}><KbIcon name="sparkle" size={12} />提炼</Button>
    {#if isMultimodalType}
      <Button variant="outline" size="sm" onclick={runMultimodalAnalysis} disabled={multimodalBusy}
        title="用多模态 AI 分析图片/PDF 内容"><KbIcon name="sparkle" size={12} />{multimodalBusy ? '分析中…' : 'AI分析'}</Button>
    {/if}
    <Button variant="outline" size="sm" onclick={() => onReprocess(doc.meta.id)} disabled={reprocessing !== null}><KbIcon name="refresh" size={12} />重处理</Button>
  </div>
  {#if doc.meta.processStatus === 'no_embedding'}
    <div class="kb-msg warn" style="margin:10px 12px 0"><KbIcon name="warn" size={14} />未配置嵌入模型：本文档已解析但未向量化，无法参与语义检索。配置 Embeddings 模型后点击「重处理」。</div>
  {:else if doc.meta.processStatus === 'embed_error'}
    <div class="kb-msg warn" style="margin:10px 12px 0"><KbIcon name="warn" size={14} />向量化失败：本文档已解析但未向量化。请检查嵌入模型配置后点击「重处理」。</div>
  {/if}
  <div class="kb-seg" style="margin:10px 12px 0">
    <button class="kb-seg-item" class:active={detailTab === 'content'} onclick={() => detailTab = 'content'}>正文</button>
    <button class="kb-seg-item" class:active={detailTab === 'chunks'} onclick={() => detailTab = 'chunks'}>分片 {doc.chunks.length}</button>
    <button class="kb-seg-item" class:active={detailTab === 'versions'} onclick={() => detailTab = 'versions'}>版本 {versions.length}</button>
    {#if multimodalResult || (doc.meta as Record<string, unknown>).multimodal_summary}
      <button class="kb-seg-item" class:active={detailTab === 'summary'} onclick={() => detailTab = 'summary'}>AI摘要</button>
    {/if}
  </div>
  <div class="kb-scroll" style="flex:1;overflow:auto;padding:12px">
    {#if detailTab === 'content'}
      {#if contentEditing}
        <div style="display:flex;flex-direction:column;gap:8px">
          <div style="display:grid;grid-template-columns:1fr 1fr;gap:8px;min-height:300px">
            <textarea class="kb-textarea" style="min-height:300px;resize:vertical;font-family:Consolas,monospace;font-size:12.5px;line-height:1.6"
              bind:value={contentEditVal}></textarea>
            <div style="border:1px solid var(--kb-border);border-radius:8px;padding:10px;overflow:auto;font-size:13px;line-height:1.7">{@html renderMd(contentEditVal)}</div>
          </div>
          <div style="display:flex;gap:8px">
            <Button onclick={saveContentEdit} disabled={contentEditBusy}>{contentEditBusy ? '保存中…' : '保存'}</Button>
            <Button variant="outline" onclick={cancelContentEdit}>取消</Button>
          </div>
        </div>
      {:else}
        <div style="display:flex;gap:8px;margin-bottom:8px">
          <Button variant="outline" size="sm" onclick={startContentEdit}><KbIcon name="edit" size={12} />编辑正文</Button>
        </div>
        <pre style="font-size:12.5px;line-height:1.7;white-space:pre-wrap;word-break:break-all;margin:0;color:var(--app-color-secondary)">{doc.content ?? '（无法解析正文，可能为扫描版 PDF）'}</pre>
      {/if}
    {:else if detailTab === 'chunks'}
      <div style="display:flex;flex-direction:column;gap:8px">
        {#each doc.chunks as c}
          <div style="border:1px solid var(--kb-border);border-radius:8px;padding:8px 10px">
            <div style="display:flex;gap:8px;font-size:11.5px;color:var(--app-color-muted);margin-bottom:4px">
              <Badge variant="secondary" class="text-[10px]">#{c.seq}</Badge><span>{c.tokens} tok</span>
              <Button variant="ghost" size="icon-sm" style="margin-left:auto" onclick={() => { editChunk = { id: c.id, content: c.content }; editChunkVal = c.content; }}><KbIcon name="edit" size={11} /></Button>
            </div>
            <p style="margin:0;font-size:12.5px;line-height:1.6;color:var(--app-color-secondary);word-break:break-all">{c.content}</p>
          </div>
        {/each}
      </div>
    {:else if detailTab === 'versions'}
      <div style="display:flex;flex-direction:column;gap:8px">
        {#each versions as v}
          <div style="display:flex;align-items:center;gap:8px;border:1px solid var(--kb-border);border-radius:8px;padding:7px 10px;font-size:12.5px;flex-wrap:wrap">
            <Button variant={diffFromId === v.id ? 'default' : 'ghost'} size="icon-sm" onclick={() => pickDiffFrom(v.id)} title="设为对比基准"><KbIcon name="arrowLeft" size={12} /></Button>
            <Button variant={diffToId === v.id ? 'default' : 'ghost'} size="icon-sm" onclick={() => pickDiffTo(v.id)} title="设为对比目标"><KbIcon name="arrowRight" size={12} /></Button>
            <span style="font-weight:600">v{v.versionNo}</span>
            <span style="flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:var(--app-color-muted)" title={v.note ?? ''}>{v.note ?? ''}</span>
            <span style="font-size:11.5px;color:var(--app-color-muted)">{fmtTime(v.createdAt)}</span>
            <Button variant="outline" size="sm" onclick={() => restoreVersion(v.id)}>回滚</Button>
          </div>
        {/each}
        {#if versions.length >= 2}
          <div style="display:flex;gap:6px;align-items:center">
            <Button variant="outline" size="sm" onclick={doVersionDiff} disabled={diffLoading || diffFromId === null || diffToId === null}>{diffLoading ? '对比中…' : '对比所选版本'}</Button>
            <Button variant="ghost" size="sm" onclick={resetDiff}>清空</Button>
          </div>
        {/if}
        {#if diffData}
          <div style="border:1px solid var(--kb-border);border-radius:8px;overflow:hidden">
            <div style="padding:6px 10px;font-size:12px;color:var(--app-color-muted);border-bottom:1px solid var(--kb-border)">v{diffData.fromVersionNo} → v{diffData.toVersionNo}：+{diffData.added.length} / -{diffData.removed.length}</div>
            <div style="max-height:240px;overflow:auto;font-size:12px;line-height:1.6">
              {#each diffData.removed as line}<div style="padding:1px 10px;background:color-mix(in srgb, var(--app-danger) 14%, transparent);color:#ff8587;word-break:break-all">- {line}</div>{/each}
              {#each diffData.added as line}<div style="padding:1px 10px;background:color-mix(in srgb, var(--app-success) 14%, transparent);color:#7bd95c;word-break:break-all">+ {line}</div>{/each}
              {#if diffData.added.length === 0 && diffData.removed.length === 0}<div style="padding:8px 10px;color:var(--app-color-muted)">两个版本内容一致</div>{/if}
            </div>
          </div>
        {/if}
      </div>
    {:else if detailTab === 'summary'}
      <div style="display:flex;flex-direction:column;gap:12px">
        {#if multimodalResult}
          <div style="padding:12px;background:var(--kb-surface-2);border-radius:8px;border:1px solid var(--kb-border-subtle)">
            <div style="font-size:12px;color:var(--kb-text-3);margin-bottom:8px;display:flex;align-items:center;gap:6px">
              <KbIcon name="sparkle" size={14} />AI 多模态分析结果
            </div>
            <div style="font-size:13px;line-height:1.7;color:var(--kb-text);white-space:pre-wrap">{multimodalResult}</div>
          </div>
        {:else if (doc.meta as Record<string, unknown>).multimodal_summary}
          <div style="padding:12px;background:var(--kb-surface-2);border-radius:8px;border:1px solid var(--kb-border-subtle)">
            <div style="font-size:12px;color:var(--kb-text-3);margin-bottom:8px;display:flex;align-items:center;gap:6px">
              <KbIcon name="sparkle" size={14} />AI 多模态分析结果
            </div>
            <div style="font-size:13px;line-height:1.7;color:var(--kb-text);white-space:pre-wrap">{(doc.meta as Record<string, unknown>).multimodal_summary as string}</div>
          </div>
        {:else}
          <Empty class="min-h-[150px]">
            <KbIcon name="sparkle" size={24} color="var(--kb-text-3)" />
            <EmptyTitle class="text-sm">暂无 AI 摘要</EmptyTitle>
            <EmptyDescription>点击工具栏「AI分析」按钮生成多模态摘要</EmptyDescription>
          </Empty>
        {/if}
      </div>
    {/if}
  </div>
</div>

<!-- 分块编辑弹窗 -->
{#if editChunk}
  <div style="position:fixed;inset:0;z-index:100;display:flex;align-items:center;justify-content:center;background:rgba(0,0,0,.4)" onclick={() => { if (!editChunkBusy) editChunk = null; }} onkeydown={(e) => e.key === 'Escape' && (editChunk = null)} role="dialog" aria-modal="true" aria-label="编辑分块" tabindex="-1">
    <div class="kb-modal" role="presentation" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
      <div class="kb-modal-hd"><KbIcon name="edit" size={16} color="var(--kb-accent-bright)" />编辑分块</div>
      <div class="kb-modal-bd">
        <textarea class="kb-textarea" style="min-height:220px;resize:vertical;font-size:12.5px;line-height:1.6" bind:value={editChunkVal}></textarea>
        <p style="font-size:11.5px;color:var(--kb-text-3);margin:8px 0 0">保存后将更新全文索引并重新向量化该分块。</p>
      </div>
      <div class="kb-modal-ft">
        <button class="kb-btn-md" onclick={() => editChunk = null} disabled={editChunkBusy}>取消</button>
        <button class="kb-btn" onclick={saveChunk} disabled={editChunkBusy}>{editChunkBusy ? '保存中…' : '保存'}</button>
      </div>
    </div>
  </div>
{/if}
