<script lang="ts">
  import { kbApi } from './services/ipc';
  import { onMount, onDestroy, untrack } from 'svelte';
  import type { DocItem, DocView, KbVersion, UploadTask } from './kbTypes';
  import { formatBytes, formatIsoTime } from '../format';
  import { kbConfirm } from './KbConfirm.svelte';
  import { renderMd } from './markdown';
  import { downloadBlob } from '../download';
  import {
    fileIco,
    parseTags,
    SOURCE_LABEL as sourceLabel,
    STATUS_LABEL as statusLabel,
  } from './fileUtils';
  import KbIcon from './KbIcon.svelte';
  import KbModal from './KbModal.svelte';
  import KbDocUploadPanel from './KbDocUploadPanel.svelte';
  import KbDocDetail from './KbDocDetail.svelte';
  import ResourcePreview from './ResourcePreview.svelte';
  import { kbChunkCfg } from './kbChunkStore.svelte';
  import { Root as SelectRoot } from '../components/ui/select';
  import {
  SelectContent,
  SelectItem,
  SelectTrigger,
} from '../components/ui/select';
  import { Checkbox } from '../components/ui/checkbox';
  import { Input } from '../components/ui/input';
  import { Button } from '../components/ui/button';
  import { Badge } from '../components/ui/badge';
  import { Skeleton } from '../components/ui/skeleton';
  import { Empty, EmptyTitle, EmptyDescription } from '../components/ui/empty';
  import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '../components/ui/table';
  import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from '../components/ui/dropdown-menu';

  interface Props {
    selectedKb: number | null;
    notify: (msg: string, type?: 'success' | 'error' | 'warn') => void;
    refreshKbs: () => Promise<void>;
    selProvider: string;
    selModel: string;
    onTotalDocs?: (n: number) => void;
    // 外部（AI 问答引用跳转）指定打开文档
    openDocId?: { id: number; ts: number } | null;
    // 顶部全局搜索框回车后传入的关键词（ts 用于重复触发）
    searchInit?: { query: string; ts: number } | null;
  }
  let { selectedKb, notify, refreshKbs, selProvider, selModel, onTotalDocs, openDocId, searchInit }: Props = $props();

  let docs = $state<DocItem[]>([]);
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

  let fileInputRef = $state<HTMLInputElement | null>(null);
  let folderInputRef = $state<HTMLInputElement | null>(null);
  let docsLoading = $state(false);

  let dragOver = $state(false);
  let uploadTasks = $state<UploadTask[]>([]);

  let viewDoc = $state<DocView | null>(null);
  let versions = $state<KbVersion[]>([]);
  let reprocessing = $state<number | null>(null);

  let moveDocId = $state<number | null>(null);

  // 后台任务轮询（异步上传/重处理后自动刷新文档状态）
  // 指数退避：3s → 6s → 12s → 24s → 最大 30s，新任务或检测到处理中时重置
  let pollTimer: ReturnType<typeof setTimeout> | null = null;
  let pollInterval = 3000;
  const POLL_MIN = 3000;
  const POLL_MAX = 30000;
  function startPoll() {
    if (pollTimer) return;
    pollInterval = POLL_MIN; // 新任务启动时重置为最快轮询
    schedulePoll();
  }
  function schedulePoll() {
    if (pollTimer) { clearTimeout(pollTimer); pollTimer = null; }
    pollTimer = setTimeout(() => {
      pollTimer = null;
      if (document.hidden) { schedulePoll(); return; } // 页面隐藏时跳过本轮，保持调度
      if (selectedKb !== null) loadDocs(selectedKb);
      // 退避：下次间隔翻倍（上限 30s）
      pollInterval = Math.min(pollInterval * 2, POLL_MAX);
      schedulePoll();
    }, pollInterval);
  }
  function stopPoll() { if (pollTimer) { clearTimeout(pollTimer); pollTimer = null; } }

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

  async function loadDocs(kbId: number) {
    docsLoading = true;
    try {
      const res = await kbApi.listDocuments({
        kbId,
        page,
        pageSize,
        keyword: docFilter.trim() || null,
        status: statusFilter || null,
        tag: tagFilter || null,
        dirId: null,
      });
      docs = res.items;
      totalDocs = res.total;
      onTotalDocs?.(res.total);
    } catch { docs = []; totalDocs = 0; }
    finally { docsLoading = false; }
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
    viewDoc = null; versions = []; docs = [];
    page = 1; totalDocs = 0; tagFilter = '';
    if (kb === null) return;
    // untrack：loadDocs 内部会读取 tagFilter/statusFilter/page 等筛选状态，
    // 若被跟踪，任何筛选/翻页变化都会重跑本 effect 并重置 tagFilter（标签筛选被弹回），
    // 还会造成重复加载。加载只应在知识库切换时触发。
    untrack(() => {
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
    pollInterval = POLL_MIN; // 新上传开始，重置轮询为最快
    try {
      // 分块 base64 编码（内存有界）：用 Blob.slice 每次只读 2MB，编码完即释放，
      // 不再持有整文件 ArrayBuffer，峰值内存 ≈ base64 长度（约 1.33× 文件大小），
      // 远低于原先「整文件 ArrayBuffer + base64」的约 2.33×。32KB 子切片保证
      // btoa 输入为 3 的倍数（无 padding）。
      const READ_CHUNK = 2 * 1024 * 1024; // 每次读取 2MB
      const B64_CHUNK = 32766; // base64 编码子切片（3 的倍数）
      let base64 = '';
      for (let off = 0; off < file.size; off += READ_CHUNK) {
        const sliceBlob = file.slice(off, Math.min(off + READ_CHUNK, file.size));
        const bytes = new Uint8Array(await sliceBlob.arrayBuffer());
        for (let i = 0; i < bytes.length; i += B64_CHUNK) {
          const s = bytes.subarray(i, Math.min(i + B64_CHUNK, bytes.length));
          base64 += btoa(Array.from(s, (b) => String.fromCharCode(b)).join(''));
        }
        // 本片 bytes 出作用域后即可被 GC，不累积整文件副本
      }
      const res = await kbApi.uploadDocument({
        input: {
          kbId: selectedKb,
          dirId: null,
          title: file.name,
          fileType: ext,
          dataBase64: base64,
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
      // 上传所有文件（不保留目录结构）
      for (const f of Array.from(input.files)) {
        await uploadFile(f);
      }
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
    if (!await kbConfirm({ message: `确认删除选中的 ${ids.length} 个文档？该操作不可撤销。`, danger: true, confirmText: '确认删除' })) return;
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

  // ─── 全屏预览 ───
  let previewDoc = $state<{ id: number; title: string; fileType: string | null } | null>(null);
  let previewData = $state<{ type: string; url?: string; text?: string; base64?: string } | null>(null);
  let previewLoading = $state(false);
  async function openPreview(doc: { id: number; title: string; fileType: string | null }) {
    previewDoc = doc; previewLoading = true; previewData = null;
    try {
      const res = await kbApi.downloadDocument(doc.id);
      const ft = (doc.fileType ?? '').toLowerCase();
      const isMedia = ['mp3', 'wav', 'm4a', 'ogg', 'flac', 'aac', 'mp4', 'avi', 'mov', 'mkv', 'webm'].includes(ft);
      const isImage = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp'].includes(ft);
      const isPdf = ft === 'pdf';
      const isOffice = ['docx', 'doc', 'xlsx', 'xls', 'pptx', 'ppt', 'odt', 'ods', 'odp', 'rtf', 'epub'].includes(ft);

      if (isImage || isPdf || isMedia) {
        // 媒体类型：传递 base64 给 ResourcePreview 组件自行处理
        previewData = { type: isImage ? 'image' : isPdf ? 'pdf' : 'media', base64: res.dataBase64 };
      } else if (isOffice) {
        // Office 文档：提取文本内容用于预览
        let text = '';
        try {
          const dv = await kbApi.getDocument(doc.id);
          text = dv.content ?? '';
        } catch { /* 忽略 */ }
        previewData = { type: 'office', text };
      } else {
        let text = '';
        try {
          const dv = await kbApi.getDocument(doc.id);
          text = dv.content ?? '';
        } catch { /* 忽略 */ }
        if (!text) {
          const bin = Uint8Array.from(atob(res.dataBase64), (c) => c.charCodeAt(0));
          text = new TextDecoder().decode(bin);
        }
        previewData = { type: 'text', text };
      }
    } catch (e: unknown) { notify('预览失败：' + e, 'error'); }
    finally { previewLoading = false; }
  }
  function closePreview() {
    if (previewData?.url) URL.revokeObjectURL(previewData.url);
    previewDoc = null; previewData = null;
  }

  // ─── 网页抓取（单个） ───
  let fetchUrlOpen = $state(false);
  let fetchUrlVal = $state('');
  let fetchUrlBusy = $state(false);
  let fetchUrlErr = $state('');
  let fetchUrlStep = $state(''); // 当前步骤：connecting / downloading / extracting / saving / done
  async function doFetchUrl() {
    if (selectedKb === null || fetchUrlBusy) return;
    const url = fetchUrlVal.trim();
    if (!url) { fetchUrlErr = '请输入 URL'; return; }
    fetchUrlBusy = true; fetchUrlErr = ''; fetchUrlStep = 'connecting';
    try {
      // 模拟步骤进度（后端是同步调用，前端展示步骤让用户感知进度）
      const stepTimer = setTimeout(() => { if (fetchUrlStep === 'connecting') fetchUrlStep = 'downloading'; }, 500);
      const extractTimer = setTimeout(() => { if (fetchUrlStep === 'downloading') fetchUrlStep = 'extracting'; }, 2000);
      const saveTimer = setTimeout(() => { if (fetchUrlStep === 'extracting') fetchUrlStep = 'saving'; }, 4000);

      const res = await kbApi.fetchUrl({
        input: { url, kbId: selectedKb, dirId: null, embeddingProvider: selProvider || null, embeddingModel: selModel || null },
      });

      clearTimeout(stepTimer); clearTimeout(extractTimer); clearTimeout(saveTimer);
      fetchUrlStep = 'done';
      await new Promise(r => setTimeout(r, 600)); // 短暂展示完成状态

      fetchUrlOpen = false; fetchUrlVal = '';
      notify(`已提交网页抓取：${res.title}`);
      if (selectedKb !== null) { page = 1; await loadDocs(selectedKb); }
    } catch (e: unknown) { fetchUrlErr = '抓取失败：' + e; }
    finally { fetchUrlBusy = false; fetchUrlStep = ''; }
  }

  // ─── 批量网页抓取 ───
  let batchFetchOpen = $state(false);
  let batchFetchUrls = $state('');
  let batchFetchBusy = $state(false);
  let batchFetchErr = $state('');
  let batchFetchProgress = $state({ current: 0, total: 0, currentUrl: '' });
  async function doBatchFetch() {
    if (selectedKb === null || batchFetchBusy) return;
    const urls = batchFetchUrls.split('\n').map((u) => u.trim()).filter((u) => u.length > 0);
    if (urls.length === 0) { batchFetchErr = '请输入至少一个 URL（每行一个）'; return; }
    batchFetchBusy = true; batchFetchErr = '';
    batchFetchProgress = { current: 0, total: urls.length, currentUrl: '' };
    try {
      // 逐个抓取以提供进度反馈
      let ok = 0, err = 0;
      for (let i = 0; i < urls.length; i++) {
        batchFetchProgress = { current: i + 1, total: urls.length, currentUrl: urls[i] };
        try {
          await kbApi.fetchUrl({
            input: { url: urls[i], kbId: selectedKb, dirId: null, embeddingProvider: selProvider || null, embeddingModel: selModel || null },
          });
          ok++;
        } catch { err++; }
      }
      batchFetchOpen = false; batchFetchUrls = '';
      notify(`批量抓取完成：成功 ${ok} 个${err ? `，失败 ${err} 个` : ''}`);
      if (selectedKb !== null) { page = 1; await loadDocs(selectedKb); }
    } catch (e: unknown) { batchFetchErr = '批量抓取失败：' + e; }
    finally { batchFetchBusy = false; batchFetchProgress = { current: 0, total: 0, currentUrl: '' }; }
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
          kbId: selectedKb, dirId: null, title: title + '.md', fileType: 'md',
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

  async function retryUpload(i: number) {
    const t = uploadTasks[i];
    if (!t) return;
    uploadTasks = uploadTasks.filter((_, x) => x !== i);
    await uploadFile(t.file);
  }
  function clearTasks() { uploadTasks = []; }

  async function openDoc(docId: number) {
    viewDoc = null; versions = [];
    try {
    viewDoc = await kbApi.getDocument(docId);
    versions = await kbApi.listVersions(docId);
    } catch (e: unknown) { notify('查看失败：' + e, 'error'); }
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
    if (!await kbConfirm({ message: '确认删除该文档？该操作不可撤销。', danger: true, confirmText: '确认删除' })) return;
    try {
  await kbApi.deleteDocument(docId);
      if (selectedKb !== null) { await loadDocs(selectedKb); refreshKbs(); }
      if (viewDoc?.meta.id === docId) { viewDoc = null; versions = []; }
      notify('文档已删除');
    } catch (e: unknown) { notify('删除失败：' + e, 'error'); }
  }

  function openMoveDoc(id: number) {
    moveDocId = id;
  }
  async function doMoveDoc() {
    if (moveDocId === null) return;
    try {
    await kbApi.moveDoc(moveDocId, null);
      notify('文档已移动到根目录');
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
    if (!await kbConfirm({ message: `为「${viewDoc.meta.title}」上传新版本？\n\n将保留历史版本，并重新解析 → 分片 → 向量化。` })) return;
    const ext = (file.name.split('.').pop() || 'txt').toLowerCase();
    if (!SUPPORTED_EXT.includes(ext)) {
      notify(`暂不支持的文件类型：.${ext}（支持 ${SUPPORTED_EXT_TEXT}）`, 'error');
      return;
    }
    newVersionBusy = true;
    try {
      const rawBuf = await file.arrayBuffer();
      const bytes = new Uint8Array(rawBuf);
      const CHUNK = 32766; // 必须是 3 的倍数，保证 base64 无 padding
      let base64 = '';
      for (let i = 0; i < bytes.length; i += CHUNK) {
        const slice = bytes.subarray(i, Math.min(i + CHUNK, bytes.length));
        base64 += btoa(Array.from(slice, (b) => String.fromCharCode(b)).join(''));
      }
      await kbApi.uploadNewVersion({
        input: {
          docId: viewDoc.meta.id,
          fileType: ext,
          dataBase64: base64,
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
    if (!await kbConfirm({ message: '用 LLM 将本文档提炼为 Wiki 页面？已存在的同名页面会自动合并。' })) return;
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
  // 顶部全局搜索：关键词变化时填入文档过滤并立即检索（untrack 避免写回 docFilter 触发重跑）
  $effect(() => {
    const init = searchInit;
    if (!init) return;
    untrack(() => {
      docFilter = init.query;
      page = 1;
      if (selectedKb !== null) loadDocs(selectedKb);
    });
  });

  onMount(() => { if (selectedKb !== null) { loadDocs(selectedKb); } });
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
  <!-- 文档列表 -->
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
        <div class="flex gap-2 items-center relative">
          <DropdownMenu>
            <DropdownMenuTrigger>
              <Button size="sm">
                <KbIcon name="plus" size={14} weight="bold" />添加文档
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onclick={() => fileInputRef?.click()}>
                <KbIcon name="file" size={14} />上传文件
              </DropdownMenuItem>
              <DropdownMenuItem onclick={() => folderInputRef?.click()}>
                <KbIcon name="folderOpen" size={14} />上传文件夹
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem onclick={() => { fetchUrlVal = ''; fetchUrlErr = ''; fetchUrlOpen = true; }}>
                <KbIcon name="link" size={14} />抓取网页
              </DropdownMenuItem>
              <DropdownMenuItem onclick={() => { batchFetchUrls = ''; batchFetchErr = ''; batchFetchOpen = true; }}>
                <KbIcon name="link" size={14} />批量抓取网页
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
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
        <div class="flex-1"></div>
        <Button variant="outline" size="sm" onclick={() => { mdDocTitle = ''; mdDocBody = ''; mdDocErr = ''; mdDocOpen = true; }}>
          <KbIcon name="edit" size={14} />新建文档
        </Button>
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
        <div class="flex items-center gap-2 mt-2.5 flex-wrap flex-shrink-0">
          <label class="inline-flex items-center gap-1.5 text-xs text-muted-foreground cursor-pointer">
            <Checkbox checked={allSelected} onCheckedChange={toggleSelectAll} />
            全选（{docs.length}）
          </label>
          {#if selectedDocs.size > 0}
            <Badge variant="secondary">已选 {selectedDocs.size}</Badge>
            <Button variant="outline" size="sm" onclick={() => { batchMoveDir = null; batchMoveOpen = true; }}>
              <KbIcon name="move" size={12} />批量移动
            </Button>
            <Button variant="outline" size="sm" onclick={() => { batchTagVal = ''; batchTagOpen = true; }}>
              <KbIcon name="tag" size={12} />批量标签
            </Button>
            <Button variant="outline" size="sm" onclick={batchDownload} disabled={batchBusy}>
              <KbIcon name="download" size={12} />{batchBusy ? '打包中…' : '批量下载'}
            </Button>
            <Button variant="destructive" size="sm" onclick={batchDelete}>
              <KbIcon name="trash" size={12} />批量删除
            </Button>
          {/if}
          <span class="flex-1"></span>
          <span class="text-[11px] text-muted-foreground">勾选文档后可批量移动 / 删除 / 下载</span>
        </div>
      {/if}
      <div style="flex:1;min-height:0;overflow:auto;margin-top:10px">
        {#if docsLoading}
          <div style="display:flex;flex-direction:column;gap:8px;padding:16px 0">
            {#each Array(6) as _}
              <Skeleton class="h-[48px] rounded-lg" />
            {/each}
          </div>
        {:else if docs.length === 0}
          <Empty class="min-h-[200px]">
            <KbIcon name="file" size={28} color="var(--kb-text-3)" />
            <EmptyTitle>{totalDocs === 0 ? '暂无文档' : '没有匹配的文档'}</EmptyTitle>
            <EmptyDescription>{totalDocs === 0 ? '拖拽文件到此处或点击上方「添加文档」开始' : '尝试调整搜索条件或筛选器'}</EmptyDescription>
          </Empty>
        {:else}
        <Table>
          <TableHeader>
            <TableRow>
              {#if batchMode}<TableHead class="w-[34px]"></TableHead>{/if}
              <TableHead>名称</TableHead>
              <TableHead class="w-[96px]">状态</TableHead>
              <TableHead class="w-[88px]">大小</TableHead>
              <TableHead class="w-[72px] hidden md:table-cell">类型</TableHead>
              <TableHead class="w-[96px] hidden lg:table-cell">来源</TableHead>
              <TableHead class="w-[132px] hidden sm:table-cell">更新时间</TableHead>
              <TableHead class="w-[150px]">操作</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {#each docs as doc}
              <TableRow class={selectedDocs.has(doc.id) ? 'bg-muted/50' : ''}>
                {#if batchMode}
                  <TableCell><Checkbox checked={selectedDocs.has(doc.id)} onCheckedChange={() => toggleSelect(doc.id)} /></TableCell>
                {/if}
                <TableCell>
                  <div class="flex items-center gap-2 min-w-0">
                    <span class="text-accent-foreground flex-shrink-0"><KbIcon name={fileIco(doc.fileType)} size={16} /></span>
                    <button class="text-sm text-foreground truncate cursor-pointer hover:underline text-left"
                      title={doc.title} onclick={() => openDoc(doc.id)}>
                      {doc.title}
                    </button>
                    {#each (doc.tags ?? []) as tg}
                      <Badge variant="secondary" class="text-[10px] px-1.5 py-0">{tg}</Badge>
                    {/each}
                  </div>
                  {#if doc.snippet}
                    <div class="text-xs text-muted-foreground truncate mt-0.5">
                      {#each doc.snippet.split(/(【.+?】)/) as part}
                        {#if part.startsWith('【') && part.endsWith('】')}<span class="bg-yellow-100 dark:bg-yellow-900/30 rounded px-0.5">{part.slice(1, -1)}</span>{:else}{part}{/if}
                      {/each}
                    </div>
                  {/if}
                </TableCell>
                <TableCell>
                  <div class="inline-flex gap-1 items-center flex-wrap">
                    <Badge variant={doc.status === 'ready' ? 'default' : doc.status === 'failed' ? 'destructive' : 'secondary'}>
                      {statusLabel[doc.status] ?? doc.status}
                    </Badge>
                    {#if doc.status === 'ready' && doc.processStatus === 'no_embedding'}
                      <Badge variant="outline" class="text-[10px]" title="未配置嵌入模型">未向量化</Badge>
                    {:else if doc.status === 'ready' && doc.processStatus === 'embed_error'}
                      <Badge variant="destructive" class="text-[10px]" title="向量化失败">向量化失败</Badge>
                    {/if}
                  </div>
                </TableCell>
                <TableCell class="text-xs text-muted-foreground">{fmtBytes(doc.fileSize)}</TableCell>
                <TableCell class="hidden md:table-cell"><Badge variant="outline" class="text-[10px]">.{doc.fileType ?? '?'}</Badge></TableCell>
                <TableCell class="text-xs text-muted-foreground hidden lg:table-cell">{sourceLabel[doc.source ?? 'upload'] ?? '文件上传'}</TableCell>
                <TableCell class="text-xs text-muted-foreground hidden sm:table-cell">{fmtTime(doc.updatedAt ?? doc.createdAt)}</TableCell>
                <TableCell>
                  <div class="flex gap-1 items-center">
                    <Button variant="ghost" size="icon-sm" onclick={() => openPreview(doc)} title="预览">
                      <KbIcon name="eye" size={12} />
                    </Button>
                    <Button variant="ghost" size="icon-sm" onclick={() => openDoc(doc.id)} title="查看">
                      <KbIcon name="file" size={12} />
                    </Button>
                    <DropdownMenu>
                      <DropdownMenuTrigger>
                        <Button variant="ghost" size="icon-sm"><KbIcon name="more" size={12} /></Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        <DropdownMenuItem onclick={() => tagModal = { docId: doc.id, docTitle: doc.title, tags: (doc.tags ?? []).join('、') }}>
                          <KbIcon name="tag" size={12} />编辑标签
                        </DropdownMenuItem>
                        <DropdownMenuItem onclick={() => reprocessDoc(doc.id)}>
                          <KbIcon name="refresh" size={12} />重新处理
                        </DropdownMenuItem>
                        <DropdownMenuSeparator />
                        <DropdownMenuItem class="text-destructive" onclick={() => removeDoc(doc.id)}>
                          <KbIcon name="trash" size={12} />删除
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </div>
                </TableCell>
              </TableRow>
            {/each}
          </TableBody>
        </Table>
        {/if}
      </div>
    </div>
    <!-- 分页：固定底部 -->
    <div class="flex items-center gap-2 justify-end px-4 py-2.5 border-t border-border flex-shrink-0 bg-background">
      <span class="text-xs text-muted-foreground">共 {totalDocs} 条 · 第 {page}/{totalPages()} 页</span>
      <Button variant="outline" size="sm" onclick={prevPage} disabled={page <= 1}>上一页</Button>
      <Button variant="outline" size="sm" onclick={nextPage} disabled={page * pageSize >= totalDocs}>下一页</Button>
    </div>
  </div>

  <!-- 右：详情抽屉 -->
  {#if viewDoc}
    <KbDocDetail
      doc={viewDoc}
      versions={versions}
      {selProvider}
      {selModel}
      {notify}
      onClose={() => { viewDoc = null; versions = []; }}
      onRefresh={async () => { if (viewDoc) await openDoc(viewDoc.meta.id); if (selectedKb !== null) await loadDocs(selectedKb); }}
      onDownload={downloadDoc}
      onRename={openRenameDoc}
      onMove={openMoveDoc}
      onReprocess={reprocessDoc}
      onGenerateWiki={generateWikiForDoc}
      {reprocessing}
      {wikiGenBusy}
      {newVersionBusy}
      onNewVersionPick={onNewVersionPick}
      {ACCEPT_ATTR}
    />
  {/if}
</div>

<!-- 上传任务悬浮面板 -->
<KbDocUploadPanel tasks={uploadTasks} onClear={clearTasks} onRetry={retryUpload} />

<!-- 移动文档 -->
{#if moveDocId !== null}
  <KbModal open={moveDocId !== null} onClose={() => { moveDocId = null }} ariaLabel="关闭移动文档弹窗">
      <div class="kb-modal">
      <div class="kb-modal-hd"><span>移动文档</span></div>
      <div class="kb-modal-bd">
        <p class="text-sm text-muted-foreground">文档将移动到根目录（目录功能已移除）</p>
      </div>
      <div class="kb-modal-ft">
        <Button variant="outline" onclick={() => moveDocId = null}>取消</Button>
        <Button onclick={doMoveDoc}>确认移动</Button>
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
        <Button variant="outline" onclick={() => renameDocOpen = false} disabled={renameDocBusy}>取消</Button>
        <Button onclick={doRenameDoc} disabled={renameDocBusy}>{renameDocBusy ? '保存中…' : '保存'}</Button>
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
        <p class="text-sm text-muted-foreground">文档将移动到根目录（目录功能已移除）</p>
      </div>
      <div class="kb-modal-ft">
        <Button variant="outline" onclick={() => batchMoveOpen = false}>取消</Button>
        <Button onclick={doBatchMove}>确认移动</Button>
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
        <Button variant="outline" onclick={() => tagModal = null} disabled={tagModalBusy}>取消</Button>
        <Button onclick={doSaveDocTags} disabled={tagModalBusy}>{tagModalBusy ? '保存中…' : '保存'}</Button>
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
        <Button variant="outline" onclick={() => batchTagOpen = false} disabled={batchTagBusy}>取消</Button>
        <Button onclick={doBatchTags} disabled={batchTagBusy}>{batchTagBusy ? '处理中…' : '应用'}</Button>
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
            onkeydown={(e) => e.key === 'Enter' && doFetchUrl()} disabled={fetchUrlBusy} />
        </label>
        <p style="font-size:12px;color:var(--kb-text-3);margin:8px 0 0;line-height:1.6">系统自动抓取网页标题与正文，转存为 Markdown 文档并进入解析流水线。</p>

        {#if fetchUrlBusy}
          <div style="margin-top:12px;display:flex;flex-direction:column;gap:8px">
            <div style="display:flex;align-items:center;gap:8px;font-size:12.5px">
              <span style="color:{fetchUrlStep === 'connecting' ? 'var(--kb-accent-bright)' : 'var(--kb-ok)'}; font-weight:{fetchUrlStep === 'connecting' ? '600' : '400'}">
                {#if fetchUrlStep === 'connecting'}⏳{:else}✓{/if} 连接服务器
              </span>
              <span style="color:var(--kb-text-3)">→</span>
              <span style="color:{fetchUrlStep === 'downloading' ? 'var(--kb-accent-bright)' : fetchUrlStep === 'extracting' || fetchUrlStep === 'saving' || fetchUrlStep === 'done' ? 'var(--kb-ok)' : 'var(--kb-text-3)'}; font-weight:{fetchUrlStep === 'downloading' ? '600' : '400'}">
                {#if fetchUrlStep === 'downloading'}⏳{:else if fetchUrlStep === 'extracting' || fetchUrlStep === 'saving' || fetchUrlStep === 'done'}✓{:else}○{/if} 下载网页
              </span>
              <span style="color:var(--kb-text-3)">→</span>
              <span style="color:{fetchUrlStep === 'extracting' ? 'var(--kb-accent-bright)' : fetchUrlStep === 'saving' || fetchUrlStep === 'done' ? 'var(--kb-ok)' : 'var(--kb-text-3)'}; font-weight:{fetchUrlStep === 'extracting' ? '600' : '400'}">
                {#if fetchUrlStep === 'extracting'}⏳{:else if fetchUrlStep === 'saving' || fetchUrlStep === 'done'}✓{:else}○{/if} 提取正文
              </span>
              <span style="color:var(--kb-text-3)">→</span>
              <span style="color:{fetchUrlStep === 'saving' ? 'var(--kb-accent-bright)' : fetchUrlStep === 'done' ? 'var(--kb-ok)' : 'var(--kb-text-3)'}; font-weight:{fetchUrlStep === 'saving' ? '600' : '400'}">
                {#if fetchUrlStep === 'saving'}⏳{:else if fetchUrlStep === 'done'}✓{:else}○{/if} 保存入库
              </span>
            </div>
            <div style="height:4px;border-radius:2px;background:var(--kb-border);overflow:hidden">
              <div style="height:100%;border-radius:2px;background:var(--kb-accent-bright);transition:width 0.3s;width:{fetchUrlStep === 'connecting' ? '15%' : fetchUrlStep === 'downloading' ? '40%' : fetchUrlStep === 'extracting' ? '70%' : fetchUrlStep === 'saving' ? '90%' : '100%'}"></div>
            </div>
          </div>
        {/if}

        {#if fetchUrlErr}<div class="kb-msg err" style="margin-top:8px">{fetchUrlErr}</div>{/if}
      </div>
      <div class="kb-modal-ft">
        <Button variant="outline" onclick={() => fetchUrlOpen = false} disabled={fetchUrlBusy}>取消</Button>
        <Button onclick={doFetchUrl} disabled={fetchUrlBusy || !fetchUrlVal.trim()}>
          {fetchUrlBusy ? '抓取中…' : '开始抓取'}
        </Button>
      </div>
    </div>
    </KbModal>
{/if}

<!-- 批量网页抓取 -->
{#if batchFetchOpen}
  <KbModal open={batchFetchOpen} onClose={() => { if (!batchFetchBusy) batchFetchOpen = false; }} ariaLabel="关闭批量抓取弹窗">
    <div class="kb-modal">
      <div class="kb-modal-hd"><KbIcon name="link" size={16} color="var(--kb-accent-bright)" />批量抓取网页</div>
      <div class="kb-modal-bd">
        <label class="kb-label">网页地址（每行一个 URL）
          <textarea class="kb-textarea" rows="6" placeholder="https://example.com/page1&#10;https://example.com/page2&#10;https://example.com/page3" bind:value={batchFetchUrls} disabled={batchFetchBusy}></textarea>
        </label>
        <p style="font-size:12px;color:var(--kb-text-3);margin:8px 0 0;line-height:1.6">每行一个 URL，系统自动抓取标题与正文并转存为 Markdown 文档。</p>

        {#if batchFetchBusy}
          <div style="margin-top:12px;display:flex;flex-direction:column;gap:8px">
            <div style="display:flex;align-items:center;justify-content:space-between;font-size:12.5px">
              <span style="color:var(--kb-accent-bright);font-weight:600">正在抓取第 {batchFetchProgress.current} / {batchFetchProgress.total} 个</span>
              <span style="color:var(--kb-text-3)">{Math.round(batchFetchProgress.current / batchFetchProgress.total * 100)}%</span>
            </div>
            <div style="height:4px;border-radius:2px;background:var(--kb-border);overflow:hidden">
              <div style="height:100%;border-radius:2px;background:var(--kb-accent-bright);transition:width 0.3s;width:{(batchFetchProgress.current / batchFetchProgress.total * 100)}%"></div>
            </div>
            <div style="font-size:11.5px;color:var(--kb-text-3);overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title={batchFetchProgress.currentUrl}>
              {batchFetchProgress.currentUrl || '准备中…'}
            </div>
          </div>
        {/if}

        {#if batchFetchErr}<div class="kb-msg err" style="margin-top:8px">{batchFetchErr}</div>{/if}
      </div>
      <div class="kb-modal-ft">
        <Button variant="outline" onclick={() => batchFetchOpen = false} disabled={batchFetchBusy}>取消</Button>
        <Button onclick={doBatchFetch} disabled={batchFetchBusy || !batchFetchUrls.trim()}>{batchFetchBusy ? '抓取中…' : '开始抓取'}</Button>
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
        <Button variant="outline" onclick={() => mdDocOpen = false} disabled={mdDocBusy}>取消</Button>
        <Button onclick={doCreateMdDoc} disabled={mdDocBusy}>{mdDocBusy ? '提交中…' : '创建文档'}</Button>
      </div>
    </div>
    </KbModal>
{/if}

<!-- 全屏预览（使用 ResourcePreview 组件） -->
{#if previewDoc}
  <KbModal open={previewDoc !== null} onClose={closePreview} ariaLabel="关闭预览弹窗">
    <div class="kb-modal" style="width:min(960px, calc(100vw - 48px));height:min(85vh, 800px)">
      <ResourcePreview
        title={previewDoc.title}
        fileType={previewDoc.fileType}
        dataBase64={previewData?.base64 ?? null}
        textContent={previewData?.type === 'text' ? previewData.text ?? null : null}
        loading={previewLoading}
        onClose={closePreview}
        onDownload={() => previewDoc && downloadDoc(previewDoc.id)}
      />
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




</style>
