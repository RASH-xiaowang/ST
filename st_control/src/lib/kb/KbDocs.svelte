<script lang="ts">
  import { kbApi } from './services/ipc';
  import { onMount, onDestroy, untrack } from 'svelte';
  import type { DirNode, DocItem, DocView, KbVersion, UploadTask } from './kbTypes';
  import { formatBytes, formatIsoTime } from '../format';
  import { renderMd } from './markdown';
  import { downloadBlob } from '../download';
  import {
    flattenDirs,
    fileIco,
    parseTags,
    previewMime,
    SOURCE_LABEL as sourceLabel,
    STATUS_LABEL as statusLabel,
  } from './fileUtils';
  import KbIcon from './KbIcon.svelte';
  import KbModal from './KbModal.svelte';
  import { RippleButton } from 'fancy-ui-svelte';
  import { kbChunkCfg } from './kbChunkStore.svelte';
  import { Root as SelectRoot } from '../components/ui/select';
  import {
  SelectContent,
  SelectItem,
  SelectTrigger,
} from '../components/ui/select';
  import { Checkbox } from '../components/ui/checkbox';
  import { Input } from '../components/ui/input';

  interface Props {
    selectedKb: number | null;
    notify: (msg: string, type?: 'success' | 'error' | 'warn') => void;
    refreshKbs: () => Promise<void>;
    selProvider: string;
    selModel: string;
    onTotalDocs?: (n: number) => void;
    // 外部（AI 问答引用跳转）指定打开文档
    openDocId?: { id: number; ts: number } | null;
  }
  let { selectedKb, notify, refreshKbs, selProvider, selModel, onTotalDocs, openDocId }: Props = $props();

  let dirs = $state<DirNode[]>([]);
  let docs = $state<DocItem[]>([]);
  let selectedDirId = $state<number | null>(null);
  let docFilter = $state('');
  let statusFilter = $state('');
  let batchMode = $state(false);
  let page = $state(1);
  let pageSize = $state(50);
  let totalDocs = $state(0);
  let filterTimer: ReturnType<typeof setTimeout> | null = null;
  function onFilterChange() {
    if (filterTimer) clearTimeout(filterTimer);
    filterTimer = setTimeout(() => {
      page = 1;
      if (selectedKb !== null) loadDocs(selectedKb);
    }, 300);
  }

  let uploadMenuOpen = $state(false);
  let fileInputRef = $state<HTMLInputElement | null>(null);
  let folderInputRef = $state<HTMLInputElement | null>(null);

  let dragOver = $state(false);
  let uploadTasks = $state<UploadTask[]>([]);
  // 上传任务悬浮面板：默认展开；折叠后仅显示标题栏，不占布局
  let uploadPanelOpen = $state(true);
  let uploadPanelEl = $state<HTMLElement | null>(null);
  // 新任务加入时自动滚到底部，方便连续上传多个文件时查看最新进度
  $effect(() => {
    const n = uploadTasks.length;
    if (n > 0 && uploadPanelOpen && uploadPanelEl) {
      uploadPanelEl.scrollTop = uploadPanelEl.scrollHeight;
    }
  });

  let viewDoc = $state<DocView | null>(null);
  let viewLoading = $state(false);
  let versions = $state<KbVersion[]>([]);
  let detailTab = $state<'content' | 'chunks' | 'versions'>('content');
  let reprocessing = $state<number | null>(null);

  let diffFromId = $state<number | null>(null);
  let diffToId = $state<number | null>(null);
  let diffData = $state<{ fromVersionNo: number; toVersionNo: number; added: string[]; removed: string[] } | null>(null);
  let diffLoading = $state(false);
  function resetDiff() { diffFromId = null; diffToId = null; diffData = null; diffLoading = false; }

  let moveDocId = $state<number | null>(null);
  let moveTargetDir = $state<number | null>(null);
  let flatDirs = $state<{ id: number; name: string; depth: number }[]>([]);

  // 后台任务轮询（异步上传/重处理后自动刷新文档状态）
  let pollTimer: ReturnType<typeof setInterval> | null = null;
  function startPoll() {
    if (pollTimer) return;
    pollTimer = setInterval(() => {
      if (document.hidden) return; // 页面隐藏时暂停轮询
      if (selectedKb !== null) loadDocs(selectedKb);
    }, 3000);
  }
  function stopPoll() { if (pollTimer) { clearInterval(pollTimer); pollTimer = null; } }

  function totalPages(): number {
    return Math.max(1, Math.ceil(totalDocs / pageSize));
  }
  function prevPage() {
    if (page > 1 && selectedKb !== null) { page--; loadDocs(selectedKb); }
  }
  function nextPage() {
    if (page * pageSize < totalDocs && selectedKb !== null) { page++; loadDocs(selectedKb); }
  }

  function fmtTime(t: string): string {
    return formatIsoTime(t, { showYear: true, utc: true });
  }
  /** 字节格式化：null/undefined 显示占位；保持原实现（无独立 GB 分支） */
  function fmtBytes(n: number | null | undefined): string {
    return formatBytes(n, { nullPlaceholder: '-', units: ['B', 'KB', 'MB'] });
  }

  async function loadDirs(kbId: number) {
    try { dirs = await kbApi.listDirs(kbId); } catch { dirs = []; }
  }
  async function loadDocs(kbId: number) {
    try {
      const res = await kbApi.listDocuments({
        kbId,
        page,
        pageSize,
        keyword: docFilter.trim() || null,
        status: statusFilter || null,
        tag: tagFilter || null,
        dirId: selectedDirId,
      });
      docs = res.items;
      totalDocs = res.total;
      onTotalDocs?.(res.total);
    } catch { docs = []; totalDocs = 0; }
  }
  async function loadTags() {
    if (selectedKb === null) { allTags = []; return; }
    try { allTags = await kbApi.listTags(selectedKb); }
    catch { allTags = []; }
  }
  let allTags = $state<{ tag: string; count: number }[]>([]);
  let tagFilter = $state('');
  $effect(() => {
    const kb = selectedKb;
    viewDoc = null; versions = []; resetDiff(); selectedDirId = null; docs = []; dirs = [];
    page = 1; totalDocs = 0; tagFilter = '';
    if (kb === null) return;
    // untrack：loadDocs 内部会读取 tagFilter/statusFilter/page 等筛选状态，
    // 若被跟踪，任何筛选/翻页变化都会重跑本 effect 并重置 tagFilter（标签筛选被弹回），
    // 还会造成重复加载。加载只应在知识库切换时触发。
    untrack(() => {
      loadDirs(kb);
      loadDocs(kb);
      loadTags();
    });
  });

  function onDragOver(e: DragEvent) { e.preventDefault(); dragOver = true; }
  function onDragLeave() { dragOver = false; }
  async function onDrop(e: DragEvent) {
    e.preventDefault(); dragOver = false;
    if (selectedKb === null) { notify('请先选择知识库', 'warn'); return; }
    const files = e.dataTransfer?.files;
    if (files) for (const f of Array.from(files)) await uploadFile(f);
  }
  async function onFilePick(e: Event) {
    const input = e.target as HTMLInputElement;
    if (input.files && selectedKb !== null) {
      for (const f of Array.from(input.files)) await uploadFile(f);
    }
    input.value = '';
  }
  // 与后端 kb/parse.rs 的 anydoc 支持列表保持一致
  const SUPPORTED_EXT = [
    'txt', 'md', 'markdown', 'csv', 'json', 'log',
    'pdf',
    'doc', 'docx', 'docm', 'rtf', 'odt', 'epub',
    'xls', 'xlsx', 'xlsm', 'xlsb', 'ods',
    'ppt', 'pptx', 'pptm', 'pps', 'ppsx', 'ppsm', 'pot', 'odp',
    'png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp',
  ];
  const ACCEPT_ATTR = SUPPORTED_EXT.map((e) => `.${e}`).join(',');
  const SUPPORTED_EXT_TEXT = 'txt / md / pdf / Word(doc,docx,docm) / Excel(xls,xlsx,xlsm,xlsb) / PPT(ppt,pptx,pptm,pps,ppsx,ppsm,pot) / ODT / ODS / ODP / RTF / EPUB / csv / json / log / 图片';
  async function uploadFile(file: File) {
    if (selectedKb === null) return;
    const ext = (file.name.split('.').pop() || 'txt').toLowerCase();
    if (!SUPPORTED_EXT.includes(ext)) {
      notify(`暂不支持的文件类型：.${ext}（支持 ${SUPPORTED_EXT_TEXT}）`, 'error');
      return;
    }
    if (file.size > 200 * 1024 * 1024) {
      notify(`「${file.name}」超过 200MB 上传上限`, 'error');
      return;
    }
    const task: UploadTask = { file, status: 'pending', msg: '' };
    uploadTasks = [...uploadTasks, task];
    const idx = uploadTasks.length - 1;
    uploadTasks[idx].status = 'uploading';
    try {
      const buf = new Uint8Array(await file.arrayBuffer());
      const res = await kbApi.uploadDocument({
        input: {
          kbId: selectedKb,
          dirId: selectedDirId,
          title: file.name,
          fileType: ext,
          data: Array.from(buf),
          embeddingProvider: selProvider || null,
          embeddingModel: selModel || null,
          chunkStrategy: kbChunkCfg.strategy,
          chunkSize: kbChunkCfg.size,
          chunkOverlap: kbChunkCfg.overlap,
        },
      });
      if (res?.duplicateDocId != null) {
        uploadTasks[idx].status = 'done';
        uploadTasks[idx].msg = `已存在相同内容（${res.duplicateTitle ?? '文档 #' + res.duplicateDocId}），跳过`;
        notify(`「${file.name}」与已有文档内容相同，已跳过`, 'warn');
        return;
      }
      uploadTasks[idx].status = 'done';
      uploadTasks[idx].msg = '已提交后台处理';
      if (selectedKb !== null) { page = 1; await loadDocs(selectedKb); refreshKbs(); }
      notify(`「${file.name}」已提交处理`);
    } catch (err: unknown) {
      uploadTasks[idx].status = 'error';
      uploadTasks[idx].msg = String(err);
      notify(`上传失败：${file.name}（${String(err)}）`, 'error');
    }
  }
  async function onFolderPick(e: Event) {
    const input = e.target as HTMLInputElement;
    if (input.files && selectedKb !== null) {
      for (const f of Array.from(input.files)) await uploadFile(f);
    }
    input.value = '';
  }

  // ─── 批量操作 ───
  let selectedDocs = $state<Set<number>>(new Set());
  let batchBusy = $state(false);
  function toggleSelect(id: number) {
    const s = new Set(selectedDocs);
    if (s.has(id)) s.delete(id); else s.add(id);
    selectedDocs = s;
  }
  const allSelected = $derived(docs.length > 0 && docs.every((d) => selectedDocs.has(d.id)));
  function toggleSelectAll() {
    selectedDocs = allSelected ? new Set() : new Set(docs.map((d) => d.id));
  }
  async function batchDelete() {
    const ids = [...selectedDocs];
    if (ids.length === 0) return;
    if (!confirm(`确认删除选中的 ${ids.length} 个文档？该操作不可撤销。`)) return;
    let ok = 0, err = 0;
    for (const id of ids) {
    try { await kbApi.deleteDocument(id); ok++; } catch { err++; }
    }
    selectedDocs = new Set();
    if (selectedKb !== null) { await loadDocs(selectedKb); refreshKbs(); }
    notify(`已删除 ${ok} 个文档${err ? '，失败 ' + err + ' 个' : ''}`, err ? 'warn' : 'success');
  }
  let batchMoveOpen = $state(false);
  let batchMoveDir = $state<number | null>(null);
  async function doBatchMove() {
    const ids = [...selectedDocs];
    if (ids.length === 0) return;
    let ok = 0, err = 0;
    for (const id of ids) {
    try { await kbApi.moveDoc(id, batchMoveDir); ok++; } catch { err++; }
    }
    batchMoveOpen = false; selectedDocs = new Set();
    if (selectedKb !== null) await loadDocs(selectedKb);
    notify(`已移动 ${ok} 个文档${err ? '，失败 ' + err + ' 个' : ''}`, err ? 'warn' : 'success');
  }

  // ─── 标签 ───
  let tagModal = $state<{ docId: number; docTitle: string; tags: string } | null>(null);
  let tagModalBusy = $state(false);
  let tagModalErr = $state('');
  let batchTagOpen = $state(false);
  let batchTagVal = $state('');
  let batchTagBusy = $state(false);

  async function saveDocTags(docId: number, tags: string[]) {
  await kbApi.setDocTags(docId, tags);
  }
  async function doSaveDocTags() {
    if (tagModal === null || tagModalBusy) return;
    tagModalBusy = true; tagModalErr = '';
    try {
      await saveDocTags(tagModal.docId, parseTags(tagModal.tags));
      tagModal = null;
      if (selectedKb !== null) { await loadDocs(selectedKb); await loadTags(); }
      notify('标签已保存');
    } catch (e: unknown) { tagModalErr = '保存失败：' + e; }
    finally { tagModalBusy = false; }
  }
  async function doBatchTags() {
    const ids = [...selectedDocs];
    const tags = parseTags(batchTagVal);
    if (ids.length === 0) { notify('请先勾选文档', 'warn'); return; }
    if (tags.length === 0) { notify('请输入至少一个标签', 'warn'); return; }
    batchTagBusy = true;
    let ok = 0, err = 0;
    for (const id of ids) {
      try { await saveDocTags(id, tags); ok++; } catch { err++; }
    }
    batchTagOpen = false; batchTagVal = '';
    if (selectedKb !== null) { await loadDocs(selectedKb); await loadTags(); }
    notify(`已为 ${ok} 个文档打标签${err ? '，失败 ' + err + ' 个' : ''}`, err ? 'warn' : 'success');
    batchTagBusy = false;
  }

  // ─── 网页抓取 ───
  let fetchUrlOpen = $state(false);
  let fetchUrlVal = $state('');
  let fetchUrlBusy = $state(false);
  let fetchUrlErr = $state('');
  async function doFetchUrl() {
    if (selectedKb === null || fetchUrlBusy) return;
    const url = fetchUrlVal.trim();
    if (!url) { fetchUrlErr = '请输入 URL'; return; }
    fetchUrlBusy = true; fetchUrlErr = '';
    try {
      const res = await kbApi.fetchUrl({
        input: { url, kbId: selectedKb, dirId: selectedDirId, embeddingProvider: selProvider || null, embeddingModel: selModel || null },
      });
      fetchUrlOpen = false; fetchUrlVal = '';
      notify(`已提交网页抓取：${res.title}`);
      if (selectedKb !== null) { page = 1; await loadDocs(selectedKb); }
    } catch (e: unknown) { fetchUrlErr = '抓取失败：' + e; }
    finally { fetchUrlBusy = false; }
  }

  // ─── Markdown 新建文档（编辑器 + 预览） ───
  let mdDocOpen = $state(false);
  let mdDocTitle = $state('');
  let mdDocBody = $state('');
  let mdDocBusy = $state(false);
  let mdDocErr = $state('');
  function mdPreviewHtml(): string {
    return renderMd(mdDocBody);
  }
  async function doCreateMdDoc() {
    if (selectedKb === null || mdDocBusy) return;
    const title = mdDocTitle.trim();
    const body = mdDocBody.trim();
    if (!title) { mdDocErr = '请输入文档标题'; return; }
    if (!body) { mdDocErr = '请输入文档内容'; return; }
    mdDocBusy = true; mdDocErr = '';
    try {
      const md = `# ${title}\n\n${body}`;
      const buf = new TextEncoder().encode(md);
      await kbApi.uploadDocument({
        input: {
          kbId: selectedKb, dirId: selectedDirId, title: title + '.md', fileType: 'md',
          data: Array.from(buf),
          embeddingProvider: selProvider || null, embeddingModel: selModel || null,
          chunkStrategy: kbChunkCfg.strategy, chunkSize: kbChunkCfg.size, chunkOverlap: kbChunkCfg.overlap,
        },
      });
      mdDocOpen = false; mdDocTitle = ''; mdDocBody = '';
      if (selectedKb !== null) { page = 1; await loadDocs(selectedKb); }
      notify('Markdown 文档已提交处理');
    } catch (e: unknown) { mdDocErr = '创建失败：' + e; }
    finally { mdDocBusy = false; }
  }

  // ─── 分块编辑 ───
  let editChunk = $state<{ id: number; content: string } | null>(null);
  let editChunkVal = $state('');
  let editChunkBusy = $state(false);
  async function doSaveChunk() {
    if (editChunk === null || editChunkBusy) return;
    editChunkBusy = true;
    try {
      const res = await kbApi.updateChunk(editChunk.id, editChunkVal);
      if (res?.warning) notify(res.warning, 'warn');
      else notify('分块已更新并重新向量化');
      editChunk = null;
      if (viewDoc) await openDoc(viewDoc.meta.id);
    } catch (e: unknown) { notify('保存分块失败：' + e, 'error'); }
    finally { editChunkBusy = false; }
  }

  // ─── 全屏预览 ───
  let previewDoc = $state<{ id: number; title: string; fileType: string | null } | null>(null);
  let previewData = $state<{ type: 'pdf' | 'image' | 'text'; url?: string; text?: string } | null>(null);
  let previewLoading = $state(false);
  async function openPreview(doc: { id: number; title: string; fileType: string | null }) {
    previewDoc = doc; previewLoading = true; previewData = null;
    try {
      const res = await kbApi.downloadDocument(doc.id);
      const bin = Uint8Array.from(atob(res.dataBase64), (c) => c.charCodeAt(0));
      const ft = (doc.fileType ?? '').toLowerCase();
      if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp'].includes(ft)) {
        previewData = { type: 'image', url: URL.createObjectURL(new Blob([bin], { type: previewMime(ft) })) };
      } else if (ft === 'pdf') {
        previewData = { type: 'pdf', url: URL.createObjectURL(new Blob([bin], { type: 'application/pdf' })) };
      } else {
        // 文本类（含 docx/xlsx）：优先用后端解析后的正文，避免显示乱码二进制
        let text = '';
        try {
      const dv = await kbApi.getDocument(doc.id);
          text = dv.content ?? '';
        } catch {
          /* 忽略，走原始解码兜底 */
        }
        previewData = { type: 'text', text: text || new TextDecoder().decode(bin) };
      }
    } catch (e: unknown) { notify('预览失败：' + e, 'error'); }
    finally { previewLoading = false; }
  }
  function closePreview() {
    if (previewData?.url) URL.revokeObjectURL(previewData.url);
    previewDoc = null; previewData = null;
  }
  async function retryUpload(i: number) {
    const t = uploadTasks[i];
    if (!t) return;
    uploadTasks = uploadTasks.filter((_, x) => x !== i);
    await uploadFile(t.file);
  }
  function clearTasks() { uploadTasks = []; }

  async function openDoc(docId: number) {
    viewLoading = true; viewDoc = null; versions = []; resetDiff(); detailTab = 'content';
    try {
    viewDoc = await kbApi.getDocument(docId);
    versions = await kbApi.listVersions(docId);
    } catch (e: unknown) { notify('查看失败：' + e, 'error'); }
    finally { viewLoading = false; }
  }
  async function downloadDoc(id: number) {
    try {
      const res = await kbApi.downloadDocument(id);
      const bin = Uint8Array.from(atob(res.dataBase64), (c) => c.charCodeAt(0));
      downloadBlob(new Blob([bin]), res.fileName ?? '');
    } catch (e: unknown) { notify('下载失败：' + e, 'error'); }
  }
  async function batchDownload() {
    if (selectedDocs.size === 0) return;
    batchBusy = true;
    try {
      const res = await kbApi.batchDownload([...selectedDocs]);
      const bin = Uint8Array.from(atob(res.dataBase64), (c) => c.charCodeAt(0));
      downloadBlob(new Blob([bin], { type: 'application/zip' }), res.fileName ?? '知识库文档批量下载.zip');
      notify(`已打包下载 ${res.count} 个文档`);
    } catch (e: unknown) { notify('批量下载失败：' + e, 'error'); }
    finally { batchBusy = false; }
  }
  async function reprocessDoc(id: number) {
    if (reprocessing !== null) return;
    reprocessing = id;
    try {
      const res = await kbApi.reprocessDocument({
        docId: id,
        embeddingProvider: selProvider || null,
        embeddingModel: selModel || null,
        chunkStrategy: kbChunkCfg.strategy,
        chunkSize: kbChunkCfg.size,
        chunkOverlap: kbChunkCfg.overlap,
      });
      notify(`重新处理完成：分片 ${res.chunkCount} · 嵌入 ${res.embedded}`);
      if (selectedKb !== null) await loadDocs(selectedKb);
      if (viewDoc?.meta.id === id) await openDoc(id);
    } catch (e: unknown) { notify('重新处理失败：' + e, 'error'); }
    finally { reprocessing = null; }
  }
  async function removeDoc(docId: number) {
    if (!confirm('确认删除该文档？该操作不可撤销。')) return;
    try {
  await kbApi.deleteDocument(docId);
      if (selectedKb !== null) { await loadDocs(selectedKb); refreshKbs(); }
      if (viewDoc?.meta.id === docId) { viewDoc = null; versions = []; }
      notify('文档已删除');
    } catch (e: unknown) { notify('删除失败：' + e, 'error'); }
  }
  async function restoreVersion(versionId: number) {
    if (!confirm('回滚到该版本？将生成新的版本并重新向量化。')) return;
    try {
  await kbApi.restoreVersion(versionId);
      notify('版本回滚完成');
      if (viewDoc) await openDoc(viewDoc.meta.id);
      if (selectedKb !== null) { await loadDocs(selectedKb); refreshKbs(); }
    } catch (e: unknown) { notify('回滚失败：' + e, 'error'); }
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
    if (viewDoc === null || diffFromId === null || diffToId === null) return;
    diffLoading = true; diffData = null;
    try {
    diffData = await kbApi.versionDiff(viewDoc.meta.id, diffFromId, diffToId);
    } catch (e: unknown) { notify('对比失败：' + e, 'error'); }
    finally { diffLoading = false; }
  }

  function openMoveDoc(id: number) {
    moveDocId = id;
    moveTargetDir = selectedDirId;
    flatDirs = flattenDirs(dirs);
  }
  async function doMoveDoc() {
    if (moveDocId === null) return;
    try {
    await kbApi.moveDoc(moveDocId, moveTargetDir);
      notify('文档已移动');
      moveDocId = null;
      if (selectedKb !== null) await loadDocs(selectedKb);
    } catch (e: unknown) { notify('移动失败：' + e, 'error'); }
  }

  // ─── 文档重命名 ───
  let renameDocOpen = $state(false);
  let renameDocId = $state<number | null>(null);
  let renameDocName = $state('');
  let renameDocBusy = $state(false);
  let renameDocErr = $state('');
  function openRenameDoc(doc: { id: number; title: string }) {
    renameDocId = doc.id; renameDocName = doc.title; renameDocErr = ''; renameDocBusy = false;
    renameDocOpen = true;
  }
  async function doRenameDoc() {
    if (renameDocId === null || renameDocBusy) return;
    const name = renameDocName.trim();
    if (!name) { renameDocErr = '请输入文档名称'; return; }
    renameDocBusy = true; renameDocErr = '';
    try {
    await kbApi.renameDocument(renameDocId, name);
      renameDocOpen = false;
      if (selectedKb !== null) await loadDocs(selectedKb);
      if (viewDoc?.meta.id === renameDocId) viewDoc = { ...viewDoc, meta: { ...viewDoc.meta, title: name } };
      notify('文档已重命名');
    } catch (e: unknown) { renameDocErr = '重命名失败：' + e; }
    finally { renameDocBusy = false; }
  }

  // ─── 上传新版本 ───
  let newVersionBusy = $state(false);
  async function onNewVersionPick(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file || viewDoc === null) return;
    if (!confirm(`为「${viewDoc.meta.title}」上传新版本？\n\n将保留历史版本，并重新解析 → 分片 → 向量化。`)) return;
    const ext = (file.name.split('.').pop() || 'txt').toLowerCase();
    if (!SUPPORTED_EXT.includes(ext)) {
      notify(`暂不支持的文件类型：.${ext}（支持 ${SUPPORTED_EXT_TEXT}）`, 'error');
      return;
    }
    newVersionBusy = true;
    try {
      const buf = new Uint8Array(await file.arrayBuffer());
      await kbApi.uploadNewVersion({
        input: {
          docId: viewDoc.meta.id,
          fileType: ext,
          data: Array.from(buf),
          note: null,
          embeddingProvider: selProvider || null,
          embeddingModel: selModel || null,
          chunkStrategy: kbChunkCfg.strategy,
          chunkSize: kbChunkCfg.size,
          chunkOverlap: kbChunkCfg.overlap,
        },
      });
      notify(`新版本已提交处理：${file.name}`);
      if (selectedKb !== null) await loadDocs(selectedKb);
      await openDoc(viewDoc.meta.id);
    } catch (e: unknown) { notify('上传新版本失败：' + e, 'error'); }
    finally { newVersionBusy = false; }
  }

  // ─── 单篇提炼为 Wiki 页面 ───
  let wikiGenBusy = $state(false);
  async function generateWikiForDoc(docId: number) {
    if (selectedKb === null || wikiGenBusy) return;
    if (!confirm('用 LLM 将本文档提炼为 Wiki 页面？已存在的同名页面会自动合并。')) return;
    wikiGenBusy = true;
    try {
      const res = await kbApi.wikiGenerate({ kbId: selectedKb, docId });
      notify(`已提交 ${res.submitted} 个文档的 Wiki 提炼，可在「活动 → 处理任务」查看进度`);
    } catch (e: unknown) { notify('Wiki 提炼失败：' + e, 'error'); }
    finally { wikiGenBusy = false; }
  }

  $effect(() => {
    const processing = docs.some((d) => d.status === 'processing');
    if (processing) startPoll();
    else stopPoll();
  });
  // 外部跳转（AI 问答引用 → 打开文档）
  $effect(() => {
    const o = openDocId;
    if (o) openDoc(o.id);
  });
  onMount(() => { if (selectedKb !== null) { loadDirs(selectedKb); loadDocs(selectedKb); } });
  onDestroy(stopPoll);
</script>

{#if selectedKb === null}
  <div class="kb-card"><div class="kb-empty">
    <span class="kb-empty-ico"><KbIcon name="folder" size={22} /></span>
    <span>请先在顶栏选择一个知识库</span>
    <span class="kb-empty-sub">或在「概览」页新建知识库</span>
  </div></div>
{:else}
<div style="display:flex;gap:14px;height:100%;min-height:0">
  <!-- 中：文档列表 -->
  <div class="kb-card" style="flex:1;min-width:0;display:flex;flex-direction:column;min-height:0">
    <div class="kb-card-hd" style="flex-direction:column;align-items:stretch;gap:10px;padding:12px 16px">
      <!-- 第一行：搜索 + 批量操作 / 添加文档 / 文档数 -->
      <div style="display:flex;align-items:center;gap:10px;flex-wrap:wrap">
        <div class="kb-searchbox" style="flex:1;min-width:200px;max-width:360px">
          <span><KbIcon name="search" size={14} /></span>
          <Input class="kb-input" placeholder="搜索文档标题或内容…" bind:value={docFilter} oninput={onFilterChange} />
        </div>
        <div style="flex:1"></div>
        <span style="font-size:12.5px;color:var(--kb-text-3)">共 {totalDocs} 个文档</span>
        <label style="display:inline-flex;align-items:center;gap:5px;font-size:12.5px;color:var(--kb-text-2);cursor:pointer">
          <Checkbox checked={batchMode} onCheckedChange={(c) => (batchMode = !!c)} />
          批量操作
        </label>
        <div style="display:flex;gap:8px;align-items:center;position:relative">
          <RippleButton onclick={() => { uploadMenuOpen = !uploadMenuOpen; }} rippleColor="#b8f5a8"
            class="h-9 rounded-[6px] border-0 bg-[var(--kb-btn-bg)] px-4 text-[13px] font-medium text-white hover:brightness-110"><KbIcon name="plus" size={14} weight="bold" />添加文档</RippleButton>
          {#if uploadMenuOpen}
    <div
      style="position:fixed;inset:0;z-index:60"
      role="button"
      aria-label="关闭上传菜单"
      tabindex="-1"
      onclick={(e) => { if (e.target === e.currentTarget) (() => uploadMenuOpen = false)(); }}
      onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ' || e.key === 'Escape') uploadMenuOpen = false; }}
    ></div>
            <div class="kb-menu">
              <button class="kb-menu-item" onclick={() => { uploadMenuOpen = false; fileInputRef?.click(); }}><KbIcon name="file" size={14} />上传文件</button>
              <button class="kb-menu-item" onclick={() => { uploadMenuOpen = false; folderInputRef?.click(); }}><KbIcon name="folderOpen" size={14} />上传文件夹</button>
              <button class="kb-menu-item" onclick={() => { uploadMenuOpen = false; fetchUrlVal = ''; fetchUrlErr = ''; fetchUrlOpen = true; }}><KbIcon name="link" size={14} />抓取网页</button>
            </div>
          {/if}
          <input type="file" multiple hidden bind:this={fileInputRef} accept={ACCEPT_ATTR} onchange={onFilePick} />
          <input type="file" multiple hidden bind:this={folderInputRef} webkitdirectory onchange={onFolderPick} />
        </div>
      </div>
      <!-- 第二行：状态 / 标签筛选 + 次要操作 -->
      <div style="display:flex;gap:8px;align-items:center;flex-wrap:wrap">
        <SelectRoot
          type="single"
          value={statusFilter}
          onValueChange={(v) => { statusFilter = v; page = 1; if (selectedKb !== null) loadDocs(selectedKb); }}
        >
          <SelectTrigger class="kb-shadcn-trigger h-8 w-32">
            <span>{statusFilter ? { ready: '解析完成', processing: '解析中', pending: '待解析', failed: '解析失败' }[statusFilter] ?? statusFilter : '全部状态'}</span>
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="">全部状态</SelectItem>
            <SelectItem value="ready">解析完成</SelectItem>
            <SelectItem value="processing">解析中</SelectItem>
            <SelectItem value="pending">待解析</SelectItem>
            <SelectItem value="failed">解析失败</SelectItem>
          </SelectContent>
        </SelectRoot>
        <SelectRoot
          type="single"
          value={tagFilter}
          onValueChange={(v) => { tagFilter = v; page = 1; if (selectedKb !== null) loadDocs(selectedKb); }}
        >
          <SelectTrigger class="kb-shadcn-trigger h-8 w-auto min-w-32">
            <span>{tagFilter || '全部标签'}</span>
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="">全部标签</SelectItem>
            {#each allTags as t}
              <SelectItem value={t.tag}>{t.tag}（{t.count}）</SelectItem>
            {/each}
          </SelectContent>
        </SelectRoot>
        <div style="flex:1"></div>
        <button class="kb-btn-md" onclick={() => { mdDocTitle = ''; mdDocBody = ''; mdDocErr = ''; mdDocOpen = true; }}><KbIcon name="edit" size={14} />新建文档</button>
      </div>
    </div>

    <div class="kb-scroll" style="flex:1;display:flex;flex-direction:column;overflow:auto;padding:12px 14px">
      <div class="kb-dropzone" class:drag={dragOver}
        role="group"
        ondragover={onDragOver} ondragleave={onDragLeave} ondrop={onDrop}>
        <div class="kb-dropzone-inner">
          <span class="kb-dropzone-ico"><KbIcon name="upload" size={20} color="var(--kb-accent-bright)" /></span>
          <span style="display:flex;flex-direction:column;align-items:flex-start;gap:3px;min-width:0">
            <span class="kb-dropzone-text">
              {#if dragOver}
                松开即可上传，自动解析 → 分片 → 向量化入库
              {:else}
                拖拽文件或文件夹到此处上传，自动解析 → 分片 → 向量化入库（支持 Office / PDF / ODF / RTF / EPUB / 图片 OCR）
              {/if}
            </span>
            {#if !selProvider || !selModel}
              <span class="kb-upload-hint"><KbIcon name="warn" size={13} />未配置嵌入模型：文档可正常上传、解析与全文检索，但无法语义检索。请到「设置 → 模型设置」配置 Embeddings 模型后，对文档执行「重新处理」。</span>
            {/if}
          </span>
        </div>
      </div>

      {#if batchMode}
        <div style="display:flex;align-items:center;gap:8px;margin-top:10px;flex-wrap:wrap;flex:none">
          <label style="display:inline-flex;align-items:center;gap:6px;font-size:12.5px;color:var(--kb-text-2);cursor:pointer">
            <Checkbox checked={allSelected} onCheckedChange={toggleSelectAll} />
            全选（{docs.length}）
          </label>
          {#if selectedDocs.size > 0}
            <span class="kb-badge kb-badge-info">已选 {selectedDocs.size}</span>
            <button class="kb-btn-sm" onclick={() => { batchMoveDir = selectedDirId; batchMoveOpen = true; }}><KbIcon name="move" size={12} />批量移动</button>
            <button class="kb-btn-sm" onclick={() => { batchTagVal = ''; batchTagOpen = true; }}><KbIcon name="tag" size={12} />批量打标签</button>
            <button class="kb-btn-sm" onclick={batchDownload} disabled={batchBusy}><KbIcon name="download" size={12} />{batchBusy ? '打包中…' : '批量下载'}</button>
            <button class="kb-btn-sm kb-dang" onclick={batchDelete}><KbIcon name="trash" size={12} />批量删除</button>
          {/if}
          <div style="flex:1"></div>
          <span style="font-size:11.5px;color:var(--kb-text-3)">勾选文档后可批量移动 / 删除 / 下载</span>
        </div>
      {/if}
      <div style="flex:1;min-height:0;overflow:auto;margin-top:10px">
        <table class="kb-table">
          <thead>
            <tr>
              {#if batchMode}<th style="width:34px"></th>{/if}
              <th>名称</th>
              <th style="width:96px">状态</th>
              <th style="width:88px">大小</th>
              <th style="width:72px">类型</th>
              <th style="width:96px">来源</th>
              <th style="width:132px">更新时间</th>
              <th style="width:150px">操作</th>
            </tr>
          </thead>
          <tbody>
            {#each docs as doc}
              <tr class:kb-row-selected={selectedDocs.has(doc.id)}>
                {#if batchMode}
                  <td><Checkbox checked={selectedDocs.has(doc.id)} onCheckedChange={() => toggleSelect(doc.id)} /></td>
                {/if}
                <td>
                  <div style="display:flex;align-items:center;gap:8px;min-width:0">
                    <span style="flex:none;color:var(--kb-accent-bright)"><KbIcon name={fileIco(doc.fileType)} size={16} /></span>
          <span
            style="font-size:13px;color:var(--kb-text);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;cursor:pointer"
            role="button"
            tabindex="0"
            title={doc.title}
            onclick={(e) => { if (e.target === e.currentTarget) (() => openDoc(doc.id))(); }}
            onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); openDoc(doc.id); } }}
          >{doc.title}</span>
                    {#each (doc.tags ?? []) as tg}
                      <span class="kb-badge kb-badge-info" style="font-size:11.5px;padding:0 6px">{tg}</span>
                    {/each}
                  </div>
                </td>
                <td style="white-space:nowrap">
                  <div style="display:inline-flex;gap:4px;align-items:center;flex-wrap:wrap">
                    <span class="kb-badge" class:kb-badge-ok={doc.status === 'ready'}
                      class:kb-badge-warn={doc.status === 'processing' || doc.status === 'pending'}
                      class:kb-badge-err={doc.status === 'failed'}>{statusLabel[doc.status] ?? doc.status}</span>
                    {#if doc.status === 'ready' && doc.processStatus === 'no_embedding'}
                      <span class="kb-badge kb-badge-warn" title="未配置嵌入模型：文档已解析但未向量化，仅可全文检索">未向量化</span>
                    {:else if doc.status === 'ready' && doc.processStatus === 'embed_error'}
                      <span class="kb-badge kb-badge-err" title="向量化失败：文档已解析但未向量化，请检查嵌入配置后重新处理">向量化失败</span>
                    {/if}
                  </div>
                </td>
                <td style="font-size:12.5px;color:var(--kb-text-2)">{fmtBytes(doc.fileSize)}</td>
                <td><span class="kb-badge kb-badge-mute">.{doc.fileType ?? '?'}</span></td>
                <td style="font-size:12.5px;color:var(--kb-text-2)">{sourceLabel[doc.source ?? 'upload'] ?? '文件上传'}</td>
                <td style="font-size:12px;color:var(--kb-text-3)">{fmtTime(doc.updatedAt ?? doc.createdAt)}</td>
                <td>
                  <div style="display:flex;gap:6px;align-items:center">
                    <button class="kb-btn-sm" onclick={() => openPreview(doc)} title="预览"><KbIcon name="eye" size={12} /></button>
                    <button class="kb-btn-sm" onclick={() => openDoc(doc.id)} title="查看"><KbIcon name="file" size={12} /></button>
                    <button class="kb-btn-sm" onclick={() => tagModal = { docId: doc.id, docTitle: doc.title, tags: (doc.tags ?? []).join('、') }} title="标签"><KbIcon name="tag" size={12} /></button>
                    <button class="kb-btn-sm kb-dang" onclick={() => removeDoc(doc.id)} title="删除"><KbIcon name="trash" size={12} /></button>
                  </div>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
        {#if docs.length === 0}
          <div class="kb-empty" style="min-height:200px"><span class="kb-empty-ico"><KbIcon name="file" size={22} /></span><span>{totalDocs === 0 ? '暂无文档，拖拽文件或点击添加文档' : '没有匹配的文档'}</span></div>
        {/if}
      </div>
    </div>
    <!-- 分页：固定底部右下角，不随内容滚动 -->
    <div class="kb-pagination">
      <span style="font-size:12px;color:var(--kb-text-3)">共 {totalDocs} 条 · 第 {page} / {totalPages()} 页</span>
      <button class="kb-btn-sm" onclick={prevPage} disabled={page <= 1}>上一页</button>
      <button class="kb-btn-sm" onclick={nextPage} disabled={page * pageSize >= totalDocs}>下一页</button>
    </div>
  </div>

  <!-- 右：详情抽屉 -->
  {#if viewDoc}
    <div class="kb-card" style="flex:none;width:420px;display:flex;flex-direction:column;min-height:0">
      <div class="kb-card-hd" style="justify-content:space-between">
        <span style="min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title={viewDoc.meta.title}>{viewDoc.meta.title}</span>
        <button class="kb-btn-sm kb-btn-ghost" onclick={() => { viewDoc = null; versions = []; }}><KbIcon name="close" size={14} /></button>
      </div>
      <div style="display:flex;gap:8px;padding:10px 12px;border-bottom:1px solid var(--kb-border);flex-wrap:wrap;align-items:center">
        <span class="kb-badge kb-badge-mute">{viewDoc.meta.fileType ?? '?'}</span>
        <span class="kb-badge" class:kb-badge-ok={viewDoc.meta.status === 'ready'} class:kb-badge-err={viewDoc.meta.status === 'failed'}>{viewDoc.meta.status}</span>
        <div style="flex:1"></div>
        <button class="kb-btn-sm" onclick={() => viewDoc && downloadDoc(viewDoc.meta.id)}><KbIcon name="download" size={12} />下载</button>
        <button class="kb-btn-sm" onclick={() => viewDoc && openRenameDoc(viewDoc.meta)}><KbIcon name="edit" size={12} />重命名</button>
        <button class="kb-btn-sm" onclick={() => viewDoc && openMoveDoc(viewDoc.meta.id)}><KbIcon name="move" size={12} />移动</button>
        <label class="kb-btn-sm" style="cursor:pointer" title="上传新版本并重新向量化">
          <KbIcon name="fileUp" size={12} />{newVersionBusy ? '处理中…' : '新版本'}
          <input type="file" hidden accept={ACCEPT_ATTR} onchange={onNewVersionPick} />
        </label>
        <button class="kb-btn-sm" onclick={() => viewDoc && generateWikiForDoc(viewDoc.meta.id)}
          disabled={wikiGenBusy || viewDoc.meta.status !== 'ready'}
          title={viewDoc.meta.status === 'ready' ? '用 LLM 将本文档提炼为 Wiki 页面' : '仅就绪文档可提炼'}><KbIcon name="sparkle" size={12} />提炼</button>
        <button class="kb-btn-sm" onclick={() => viewDoc && reprocessDoc(viewDoc.meta.id)} disabled={reprocessing !== null}><KbIcon name="refresh" size={12} />重处理</button>
      </div>
      {#if viewDoc.meta.processStatus === 'no_embedding'}
        <div class="kb-msg warn" style="margin:10px 12px 0"><KbIcon name="warn" size={14} />未配置嵌入模型：本文档已解析但未向量化，无法参与语义检索。配置 Embeddings 模型后点击「重处理」。</div>
      {:else if viewDoc.meta.processStatus === 'embed_error'}
        <div class="kb-msg warn" style="margin:10px 12px 0"><KbIcon name="warn" size={14} />向量化失败：本文档已解析但未向量化。请检查嵌入模型配置后点击「重处理」。</div>
      {/if}
      <div class="kb-seg" style="margin:10px 12px 0">
        <button class="kb-seg-item" class:active={detailTab === 'content'} onclick={() => detailTab = 'content'}>正文</button>
        <button class="kb-seg-item" class:active={detailTab === 'chunks'} onclick={() => detailTab = 'chunks'}>分片 {viewDoc.chunks.length}</button>
        <button class="kb-seg-item" class:active={detailTab === 'versions'} onclick={() => detailTab = 'versions'}>版本 {versions.length}</button>
      </div>
      <div class="kb-scroll" style="flex:1;overflow:auto;padding:12px">
        {#if viewLoading}
          <div class="kb-empty">加载中…</div>
        {:else if detailTab === 'content'}
          <pre style="font-size:12.5px;line-height:1.7;white-space:pre-wrap;word-break:break-all;margin:0;color:var(--app-color-secondary)">{viewDoc.content ?? '（无法解析正文，可能为扫描版 PDF）'}</pre>
        {:else if detailTab === 'chunks'}
          <div style="display:flex;flex-direction:column;gap:8px">
            {#each viewDoc.chunks as c}
              <div style="border:1px solid var(--kb-border);border-radius:8px;padding:8px 10px">
                <div style="display:flex;gap:8px;font-size:11.5px;color:var(--app-color-muted);margin-bottom:4px">
                  <span class="kb-badge kb-badge-info">#{c.seq}</span><span>{c.tokens} tok</span>
                  <button class="kb-btn-sm" style="margin-left:auto" onclick={() => { editChunk = { id: c.id, content: c.content }; editChunkVal = c.content; }}><KbIcon name="edit" size={11} />编辑</button>
                </div>
                <p style="margin:0;font-size:12.5px;line-height:1.6;color:var(--app-color-secondary);word-break:break-all">{c.content}</p>
              </div>
            {/each}
          </div>
        {:else if detailTab === 'versions'}
          <div style="display:flex;flex-direction:column;gap:8px">
            {#each versions as v}
              <div style="display:flex;align-items:center;gap:8px;border:1px solid var(--kb-border);border-radius:8px;padding:7px 10px;font-size:12.5px;flex-wrap:wrap">
                <button class="kb-btn-sm" class:kb-diff-on={diffFromId === v.id} onclick={() => pickDiffFrom(v.id)} title="设为对比基准"><KbIcon name="arrowLeft" size={12} /></button>
                <button class="kb-btn-sm" class:kb-diff-on={diffToId === v.id} onclick={() => pickDiffTo(v.id)} title="设为对比目标"><KbIcon name="arrowRight" size={12} /></button>
                <span style="font-weight:600">v{v.versionNo}</span>
                <span style="flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:var(--app-color-muted)" title={v.note ?? ''}>{v.note ?? ''}</span>
                <span style="font-size:11.5px;color:var(--app-color-muted)">{fmtTime(v.createdAt)}</span>
                <button class="kb-btn-sm" onclick={() => restoreVersion(v.id)}>回滚</button>
              </div>
            {/each}
            {#if versions.length >= 2}
              <div style="display:flex;gap:6px;align-items:center">
                <button class="kb-btn-sm" onclick={doVersionDiff} disabled={diffLoading || diffFromId === null || diffToId === null}>{diffLoading ? '对比中…' : '对比所选版本'}</button>
                <button class="kb-btn-sm" onclick={resetDiff}>清空</button>
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
        {/if}
      </div>
    </div>
  {/if}
</div>

<!-- 上传任务悬浮面板：右下角固定显示，不占布局；文件多时内部滚动，避免撑满列表区 -->
{#if uploadTasks.length > 0}
  <div class="kb-upload-panel" class:kb-upload-collapsed={!uploadPanelOpen}>
    <div class="kb-upload-panel-hd">
      <span style="display:inline-flex;align-items:center;gap:6px;font-size:13px;font-weight:600">
        <KbIcon name="upload" size={14} color="var(--kb-accent-bright)" />
        上传任务
        <span class="kb-badge kb-badge-info">{uploadTasks.length}</span>
      </span>
      <div style="display:flex;align-items:center;gap:4px">
        <button class="kb-btn-sm" onclick={clearTasks} title="清空记录"><KbIcon name="trash" size={12} />清空</button>
        <button class="kb-btn-sm kb-btn-ghost" onclick={() => uploadPanelOpen = !uploadPanelOpen}
          title={uploadPanelOpen ? '收起任务列表' : '展开任务列表'}>
          <KbIcon name={uploadPanelOpen ? 'caretDown' : 'caretRight'} size={12} />
        </button>
      </div>
    </div>
    {#if uploadPanelOpen}
      <div class="kb-upload-panel-body" bind:this={uploadPanelEl}>
        {#each uploadTasks as t, i}
          <div class="kb-upload-item">
            <span style="flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title={t.file.name}>{t.file.name}</span>
            {#if t.status === 'pending'}<span class="kb-badge kb-badge-mute">等待中</span>
            {:else if t.status === 'uploading'}<span class="kb-badge kb-badge-warn">处理中…</span>
            {:else if t.status === 'done'}<span class="kb-badge kb-badge-ok"><KbIcon name="check" size={11} />{t.msg}</span>
            {:else}<span class="kb-badge kb-badge-err"><KbIcon name="close" size={11} />{t.msg}</span>
            {/if}
            {#if t.status === 'error'}
              <button class="kb-btn-sm" onclick={() => retryUpload(i)}>重试</button>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<!-- 移动文档 -->
{#if moveDocId !== null}
  <KbModal open={moveDocId !== null} onClose={() => { moveDocId = null }} ariaLabel="关闭移动文档弹窗">
      <div class="kb-modal">
      <div class="kb-modal-hd"><span>移动文档</span></div>
      <div class="kb-modal-bd">
        <select class="kb-select" bind:value={moveTargetDir}>
          <option value={null}>根目录</option>
          {#each flatDirs as d}
            <option value={d.id}>{'　'.repeat(d.depth)}{d.name}</option>
          {/each}
        </select>
      </div>
      <div class="kb-modal-ft">
        <button class="kb-btn-md" onclick={() => moveDocId = null}>取消</button>
        <button class="kb-btn" onclick={doMoveDoc}>确认移动</button>
      </div>
    </div>
    </KbModal>
{/if}

<!-- 重命名文档 -->
{#if renameDocOpen}
  <KbModal open={renameDocOpen} onClose={() => { if (!renameDocBusy) renameDocOpen = false; }} ariaLabel="关闭重命名弹窗">
    <div class="kb-modal">
      <div class="kb-modal-hd"><KbIcon name="edit" size={16} color="var(--kb-accent-bright)" />重命名文档</div>
      <div class="kb-modal-bd">
        <label class="kb-label">文档名称
          <Input class="kb-input" maxlength={200} bind:value={renameDocName} autofocus onkeydown={(e) => e.key === 'Enter' && doRenameDoc()} />
        </label>
        {#if renameDocErr}<div class="kb-msg err" style="margin-top:8px">{renameDocErr}</div>{/if}
      </div>
      <div class="kb-modal-ft">
        <button class="kb-btn-md" onclick={() => renameDocOpen = false} disabled={renameDocBusy}>取消</button>
        <button class="kb-btn" onclick={doRenameDoc} disabled={renameDocBusy}>{renameDocBusy ? '保存中…' : '保存'}</button>
      </div>
    </div>
  </KbModal>
{/if}

<!-- 批量移动 -->
{#if batchMoveOpen}
  <KbModal open={batchMoveOpen} onClose={() => batchMoveOpen = false} ariaLabel="关闭批量移动弹窗">
    <div class="kb-modal">
      <div class="kb-modal-hd"><KbIcon name="move" size={16} color="var(--kb-accent-bright)" />批量移动（{selectedDocs.size} 个文档）</div>
      <div class="kb-modal-bd">
        <select class="kb-select" bind:value={batchMoveDir}>
          <option value={null}>根目录</option>
          {#each flatDirs as d}
            <option value={d.id}>{'　'.repeat(d.depth)}{d.name}</option>
          {/each}
        </select>
      </div>
      <div class="kb-modal-ft">
        <button class="kb-btn-md" onclick={() => batchMoveOpen = false}>取消</button>
        <button class="kb-btn" onclick={doBatchMove}>确认移动</button>
      </div>
    </div>
  </KbModal>
{/if}

<!-- 文档打标签 -->
{#if tagModal}
  <KbModal open={tagModal !== null} onClose={() => { if (!tagModalBusy) tagModal = null; }} ariaLabel="关闭打标签弹窗">
    <div class="kb-modal">
      <div class="kb-modal-hd"><KbIcon name="tag" size={16} color="var(--kb-accent-bright)" />打标签：{tagModal.docTitle}</div>
      <div class="kb-modal-bd">
        <Input class="kb-input" placeholder="用逗号分隔多个标签，例如：项目A, 技术, 待审阅" bind:value={tagModal.tags}
          onkeydown={(e) => e.key === 'Enter' && doSaveDocTags()} />
        {#if allTags.length > 0}
          <div style="display:flex;flex-wrap:wrap;gap:6px;margin-top:10px">
            {#each allTags as t}
              <button class="kb-btn-sm" onclick={() => {
                const cur = parseTags(tagModal!.tags);
                if (!cur.includes(t.tag)) tagModal!.tags = [...cur, t.tag].join('、');
              }}>+ {t.tag}</button>
            {/each}
          </div>
        {/if}
        {#if tagModalErr}<div class="kb-msg err" style="margin-top:8px">{tagModalErr}</div>{/if}
      </div>
      <div class="kb-modal-ft">
        <button class="kb-btn-md" onclick={() => tagModal = null} disabled={tagModalBusy}>取消</button>
        <button class="kb-btn" onclick={doSaveDocTags} disabled={tagModalBusy}>{tagModalBusy ? '保存中…' : '保存'}</button>
      </div>
    </div>
    </KbModal>
{/if}

<!-- 批量打标签 -->
{#if batchTagOpen}
  <KbModal open={batchTagOpen} onClose={() => { if (!batchTagBusy) batchTagOpen = false; }} ariaLabel="关闭批量打标签弹窗">
    <div class="kb-modal">
      <div class="kb-modal-hd"><KbIcon name="tag" size={16} color="var(--kb-accent-bright)" />批量打标签（{selectedDocs.size} 个文档）</div>
      <div class="kb-modal-bd">
        <Input class="kb-input" placeholder="用逗号分隔多个标签，将应用到全部选中文档" bind:value={batchTagVal}
          onkeydown={(e) => e.key === 'Enter' && doBatchTags()} />
      </div>
      <div class="kb-modal-ft">
        <button class="kb-btn-md" onclick={() => batchTagOpen = false} disabled={batchTagBusy}>取消</button>
        <button class="kb-btn" onclick={doBatchTags} disabled={batchTagBusy}>{batchTagBusy ? '处理中…' : '应用'}</button>
      </div>
    </div>
    </KbModal>
{/if}

<!-- 网页抓取 -->
{#if fetchUrlOpen}
  <KbModal open={fetchUrlOpen} onClose={() => { if (!fetchUrlBusy) fetchUrlOpen = false; }} ariaLabel="关闭抓取网页弹窗">
    <div class="kb-modal">
      <div class="kb-modal-hd"><KbIcon name="link" size={16} color="var(--kb-accent-bright)" />抓取网页</div>
      <div class="kb-modal-bd">
        <label class="kb-label">网页地址
          <Input class="kb-input" placeholder="https://example.com/article" bind:value={fetchUrlVal}
            onkeydown={(e) => e.key === 'Enter' && doFetchUrl()} />
        </label>
        <p style="font-size:12px;color:var(--kb-text-3);margin:8px 0 0;line-height:1.6">系统自动抓取网页标题与正文，转存为 Markdown 文档并进入解析流水线。</p>
        {#if fetchUrlErr}<div class="kb-msg err" style="margin-top:8px">{fetchUrlErr}</div>{/if}
      </div>
      <div class="kb-modal-ft">
        <button class="kb-btn-md" onclick={() => fetchUrlOpen = false} disabled={fetchUrlBusy}>取消</button>
        <button class="kb-btn" onclick={doFetchUrl} disabled={fetchUrlBusy}>{fetchUrlBusy ? '抓取中…' : '开始抓取'}</button>
      </div>
    </div>
    </KbModal>
{/if}

<!-- Markdown 新建文档（编辑器 + 实时预览） -->
{#if mdDocOpen}
  <KbModal open={mdDocOpen} onClose={() => { if (!mdDocBusy) mdDocOpen = false; }} ariaLabel="关闭新建 Markdown 弹窗">
    <div class="kb-modal" style="width:min(860px, calc(100vw - 48px))">
      <div class="kb-modal-hd"><KbIcon name="edit" size={16} color="var(--kb-accent-bright)" />新建 Markdown 文档</div>
      <div class="kb-modal-bd" style="display:flex;flex-direction:column;gap:10px">
        <input class="kb-input" placeholder="文档标题" bind:value={mdDocTitle} />
        <div style="display:grid;grid-template-columns:1fr 1fr;gap:10px;min-height:320px">
          <textarea class="kb-textarea" style="min-height:320px;resize:vertical;font-family:Consolas,monospace;font-size:12.5px;line-height:1.6" placeholder="Markdown 内容（支持标题 / 加粗 / 代码 / 列表）" bind:value={mdDocBody}></textarea>
          <div style="border:1px solid var(--kb-border);border-radius:8px;padding:10px;overflow:auto;font-size:13px;line-height:1.7;color:var(--kb-text)">{@html mdPreviewHtml()}</div>
        </div>
        {#if mdDocErr}<div class="kb-msg err">{mdDocErr}</div>{/if}
      </div>
      <div class="kb-modal-ft">
        <button class="kb-btn-md" onclick={() => mdDocOpen = false} disabled={mdDocBusy}>取消</button>
        <button class="kb-btn" onclick={doCreateMdDoc} disabled={mdDocBusy}>{mdDocBusy ? '提交中…' : '创建文档'}</button>
      </div>
    </div>
    </KbModal>
{/if}

<!-- 分块编辑 -->
{#if editChunk}
  <KbModal open={editChunk !== null} onClose={() => { if (!editChunkBusy) editChunk = null; }} ariaLabel="关闭编辑分块弹窗">
    <div class="kb-modal">
      <div class="kb-modal-hd"><KbIcon name="edit" size={16} color="var(--kb-accent-bright)" />编辑分块</div>
      <div class="kb-modal-bd">
        <textarea class="kb-textarea" style="min-height:220px;resize:vertical;font-size:12.5px;line-height:1.6" bind:value={editChunkVal}></textarea>
        <p style="font-size:11.5px;color:var(--kb-text-3);margin:8px 0 0">保存后将更新全文索引并重新向量化该分块。</p>
      </div>
      <div class="kb-modal-ft">
        <button class="kb-btn-md" onclick={() => editChunk = null} disabled={editChunkBusy}>取消</button>
        <button class="kb-btn" onclick={doSaveChunk} disabled={editChunkBusy}>{editChunkBusy ? '保存中…' : '保存'}</button>
      </div>
    </div>
    </KbModal>
{/if}

<!-- 全屏预览 -->
{#if previewDoc}
  <KbModal open={previewDoc !== null} onClose={() => closePreview()} ariaLabel="关闭预览弹窗">
    <div class="kb-modal" style="width:min(920px, calc(100vw - 64px))">
      <div class="kb-modal-hd" style="justify-content:space-between">
        <span style="min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title={previewDoc.title}>{previewDoc.title}</span>
        <button class="kb-btn-sm kb-btn-ghost" onclick={closePreview}><KbIcon name="close" size={14} /></button>
      </div>
      <div class="kb-modal-bd" style="padding:0">
        {#if previewLoading}
          <div class="kb-empty">加载中…</div>
        {:else if previewData?.type === 'pdf'}
          <iframe src={previewData.url} title="文档预览" style="width:100%;height:72vh;border:none;display:block;background:#fff"></iframe>
        {:else if previewData?.type === 'image'}
          <div style="display:flex;justify-content:center;padding:16px;overflow:auto;max-height:72vh">
            <img src={previewData.url} alt="文档预览" style="max-width:100%;max-height:68vh;border-radius:8px" />
          </div>
        {:else if previewData?.type === 'text'}
          <pre style="padding:16px;margin:0;white-space:pre-wrap;word-break:break-word;font-size:13px;line-height:1.7;color:var(--kb-text);max-height:72vh;overflow:auto">{previewData.text}</pre>
        {:else}
          <div class="kb-empty">无法预览该格式，请下载查看</div>
        {/if}
      </div>
      <div class="kb-modal-ft">
        <button class="kb-btn-md" onclick={() => previewDoc && downloadDoc(previewDoc.id)}><KbIcon name="download" size={13} />下载</button>
      </div>
    </div>
    </KbModal>
{/if}
{/if}

<style>
  .kb-dropzone-inner {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 13px;
    border: 1.5px dashed color-mix(in srgb, var(--kb-accent) 42%, transparent);
    border-radius: 10px;
    color: var(--app-color-muted);
    font-size: 13px;
    background: var(--app-bg-color);
    transition: border-color .15s, background .15s, transform .15s;
  }
  .kb-dropzone.drag .kb-dropzone-inner {
    border-color: var(--kb-accent);
    background: var(--kb-hover);
    color: var(--kb-text-2);
    transform: scale(1.01);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--kb-accent) 35%, transparent);
  }
  .kb-dropzone.drag .kb-dropzone-ico {
    transform: scale(1.12);
    filter: drop-shadow(0 0 6px color-mix(in srgb, var(--kb-accent) 45%, transparent));
  }
  .kb-dropzone-ico {
    display: inline-flex;
    transition: transform .15s;
  }
  .kb-upload-hint {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--kb-warn);
    line-height: 1.5;
  }
  .kb-msg.warn {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    padding: 8px 10px;
    border: 1px solid color-mix(in srgb, var(--app-warning, #faad14) 45%, var(--kb-border));
    border-radius: 8px;
    background: color-mix(in srgb, var(--app-warning, #faad14) 10%, transparent);
    color: var(--kb-warn);
    font-size: 12.5px;
    line-height: 1.6;
  }
  .kb-diff-on { background: var(--kb-hover-strong, #0a0f1e) !important; border-color: var(--app-accent, #1a73e8) !important; color: var(--kb-accent-bright, #6ea8ff) !important; }

  /* ─── 上传任务悬浮面板（右下角固定，不占布局） ─── */
  .kb-upload-panel {
    position: fixed;
    right: 18px;
    bottom: 18px;
    z-index: 61;
    width: min(360px, calc(100vw - 36px));
    display: flex;
    flex-direction: column;
    background: var(--app-bg-color);
    border: 1px solid var(--kb-border-strong);
    border-radius: 10px;
    box-shadow: var(--kb-shadow-lg);
    overflow: hidden;
    animation: kb-pop .16s ease-out;
  }
  .kb-upload-panel-hd {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 10px 8px 12px;
    border-bottom: 1px solid var(--kb-border);
    background: var(--app-bg-color);
    flex: none;
  }
  .kb-upload-panel.kb-upload-collapsed .kb-upload-panel-hd { border-bottom: none; }
  .kb-upload-panel-body {
    max-height: min(280px, 34vh);
    overflow: auto;
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    background: var(--app-bg-color);
  }
  .kb-upload-item {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    padding: 6px 10px;
    border: 1px solid var(--kb-border);
    border-radius: 8px;
    background: var(--app-bg-color);
    flex: none;
  }
</style>
