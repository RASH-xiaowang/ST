<script lang="ts">
  import { Channel } from '@tauri-apps/api/core';
  import { kbApi, type KbRagInput } from './services/ipc';
  import { kbConfirm } from './KbConfirm.svelte';
  import { onMount, untrack } from 'svelte';
  import type { RetrievedChunk, ModelInfo, KbSummary, QaSessionItem, QaMessageItem, SearchLogItem, RecommendItem } from './kbTypes';
  import { highlightSegments, parseCitations, extractChineseTerms } from './chatUtils';
  import { MODE_LABEL } from './fileUtils';
  import { formatIsoTime } from '../format';
  import KbIcon from './KbIcon.svelte';
  import KbModal from './KbModal.svelte';
  import KbSelect, { type KbSelectItem } from './KbSelect.svelte';
  import { track } from './analytics.svelte';
  import { Checkbox } from '../components/ui/checkbox';
  import { Button } from '../components/ui/button';
  import { Empty, EmptyTitle, EmptyDescription } from '../components/ui/empty';

  interface Props {
    selectedKb: number | null;
    kbs: KbSummary[];
    notify: (msg: string, type?: 'success' | 'error' | 'warn') => void;
    models: ModelInfo[];
    // 从侧边栏「历史对话」指定要打开的会话
    openSession: { id: number; ts: number } | null;
    // 会话列表变化时通知父级刷新侧边栏「历史对话」
    onSessionsChanged?: () => void;
    // 引用「打开文档」回调：切换到文档页并打开详情
    onOpenDoc?: (docId: number, kbId?: number | null) => void;
  }
  let { selectedKb, kbs, notify, models, openSession, onSessionsChanged, onOpenDoc }: Props = $props();

  // 问答页独立维护「生成模型」（对话类）；检索向量由后端自动用嵌入模型
  let chatProvider = $state('');
  let chatModel = $state('');

  // 问答可选的模型：显式标记为「对话/chat」的模型 + 未标记类型的模型（如
  // deepseek-v4-pro 未写 model_type 但仍是对话模型，应可选）；
  // 显式标记为嵌入/重排序/生图/视频/语音等非对话类型的模型不展示。
  const NON_CHAT_TYPES = ['embedding', '嵌入', 'rerank', '重排序', 'image', '生图', 'video', '视频', 'speech', '语音', 'audio', '音频'];
  const usableModels = $derived(
    models.filter((m) => {
      const t = (m.modelType ?? '').trim().toLowerCase();
      return !t || !NON_CHAT_TYPES.includes(t);
    }),
  );
  const providerOptions = $derived([...new Set(usableModels.map((m) => m.providerId))]);

  // 自定义下拉数据（深色选项面板，替代原生 select 的系统样式）
  const kbItems = $derived<KbSelectItem[]>([
    { value: 'all', label: '全部知识库' },
    ...kbs.map((kb) => ({ value: kb.id, label: kb.name, meta: `${kb.docCount} 文档` })),
  ]);
  const providerItems = $derived<KbSelectItem[]>(
    providerOptions.map((pid) => ({
      value: pid,
      label: usableModels.find((m) => m.providerId === pid)?.providerName ?? pid,
    })),
  );
  const modelItems = $derived<KbSelectItem[]>(
    usableModels
      .filter((m) => m.providerId === chatProvider)
      .map((m) => ({
        value: m.model,
        label: m.model + (m.isDefault ? '（默认）' : ''),
        meta: m.modelType === '对话' ? '对话' : undefined,
      })),
  );
  // 检索/问答的固定选项
  const SEARCH_MODE_ITEMS: KbSelectItem[] = [
    { value: 'hybrid', label: '混合', meta: 'BM25 + 向量语义融合，推荐' },
    { value: 'vector', label: '仅向量', meta: '纯语义相似度匹配' },
    { value: 'bm25', label: '仅全文', meta: '关键词精确匹配，速度快' },
  ];
  const SEARCH_TOPK_ITEMS: KbSelectItem[] = [
    { value: 5, label: '5', meta: '快速检索，结果精简' },
    { value: 10, label: '10', meta: '平衡精度与速度' },
    { value: 20, label: '20', meta: '全面检索，结果更多' },
  ];
  const RAG_MODE_ITEMS: KbSelectItem[] = [
    { value: 'hybrid', label: '混合', meta: 'BM25 + 向量语义融合，推荐' },
    { value: 'vector', label: '向量', meta: '纯语义相似度匹配' },
    { value: 'bm25', label: '全文', meta: '关键词精确匹配，速度快' },
  ];
  const RAG_TOPK_ITEMS: KbSelectItem[] = [
    { value: 3, label: '3', meta: '最精简，适合简单问题' },
    { value: 5, label: '5', meta: '平衡精度与速度' },
    { value: 8, label: '8', meta: '较全面的上下文' },
    { value: 10, label: '10', meta: '最全面，适合复杂问题' },
  ];
  function onProviderChange(v: string | number) {
    const p = String(v);
    const list = usableModels.filter((m) => m.providerId === p);
    const m = list.find((x) => x.isDefault) ?? list[0];
    chatProvider = p;
    chatModel = m ? m.model : '';
    if (m) persistInference(p, m.model);
  }
  function onModelChange(v: string | number) {
    chatModel = String(v);
    persistInference(chatProvider, chatModel);
  }

  async function loadChatDefault() {
    try {
    const dft = await kbApi.getDefaultChatModel();
      const providerOk = providerOptions.includes(chatProvider);
      const modelOk = usableModels.some((m) => m.providerId === chatProvider && m.model === chatModel);
      if (!providerOk || !modelOk) {
        chatProvider = dft[0];
        chatModel = dft[1];
      }
    } catch { /* 未配置模型时忽略 */ }
  }

  async function persistInference(p: string, m: string) {
    if (!p || !m) return;
    try { await kbApi.setModelSettings('inference', p, m); } catch { /* 忽略 */ }
  }

  // 大模型管理配置变化（models 刷新）后，校验当前选择是否仍有效
  $effect(() => { models; loadChatDefault(); });

  type Mode = 'search' | 'qa';
  let mode = $state<Mode>('search');
  // 检索/问答知识库：'all' 表示全部可见知识库，否则为具体知识库 id
  let kbSel = $state<number | 'all'>(untrack(() => selectedKb) ?? 'all');
  // 从知识库工作区进入 AI问答时，跟随当前工作区知识库
  $effect(() => {
    if (selectedKb !== null && kbSel !== selectedKb) kbSel = selectedKb;
  });
  const effKbId = $derived(kbSel === 'all' ? null : kbSel);

  // ─── 检索 ───
  let query = $state('');
  let searchMode = $state<'hybrid' | 'vector' | 'bm25'>('hybrid');
  let searchTopK = $state<number>(10);
  let searchResults = $state<RetrievedChunk[]>([]);
  let searching = $state(false);
  let searchLogs = $state<SearchLogItem[]>([]);
  let showHistory = $state(false);

  // ─── 问答 ───
  let ragQuery = $state('');
  let ragMode = $state<'hybrid' | 'vector' | 'bm25'>('hybrid');
  let ragTopK = $state<number>(5);
  let ragLoading = $state(false);
  // RAG 流式生成控制：停止标记 / 活动 Channel 引用 / 超时看门狗
  let ragStopping = $state(false);
  let ragChannel: Channel<string> | null = null; // 保留引用用于停止时释放（赋值语义）
  void ragChannel; // 抑制未读取警告（实际通过赋值使用）
  let activeRagId: number | null = null; // 当前活跃 RAG 流式 ID（用于精准取消）
  let ragTimeout: ReturnType<typeof setTimeout> | null = null;
  const RAG_TIMEOUT_MS = 120000; // 生成超时兜底：超过 2 分钟自动停止
  let streamText = $state<string | null>(null);
  let msgEl = $state<HTMLDivElement | null>(null);
  // 消息 / 流式更新时自动滚动到底部
  function scrollToBottom() {
    if (!msgEl) return;
    requestAnimationFrame(() => { if (msgEl) msgEl.scrollTop = msgEl.scrollHeight; });
  }
  $effect(() => {
    void qaMessages;
    void streamText;
    scrollToBottom();
  });
  let qaSessions = $state<QaSessionItem[]>([]);
  let curQaSession = $state<number | null>(null);
  let qaMessages = $state<QaMessageItem[]>([]);
  let qaLoading = $state(false);

  // ─── 引用详情 ───
  let recommendations = $state<RecommendItem[]>([]);
  let recLoading = $state(false);
  let citeOpen = $state<{ title: string; section: string | null; content: string; docId: number; chunkId: number | null; kbId: number | null } | null>(null);

  async function loadRecommendations() {
    recLoading = true;
    try {
    recommendations = await kbApi.recommendQuestions(effKbId, 8);
    } catch {
      recommendations = [];
    } finally {
      recLoading = false;
    }
  }
  // 仅落地页加载推荐（进入问答/检索工作区后不再重复拉取）
  $effect(() => {
    if (started) return;
    void effKbId;
    loadRecommendations();
  });

  function useRecommendation(q: string) {
    track('recommend_click', { kbId: effKbId, detail: q });
    if (!started) {
      landingText = q;
    } else if (mode === 'qa') {
      ragQuery = q;
    } else {
      query = q;
    }
  }

  async function openCitation(c: { doc_id?: number; chunk_id?: number; kb_id?: number; doc_title?: string; section?: string | null }) {
    if (!c.doc_id) return;
    citeOpen = { title: c.doc_title ?? '文档', section: c.section ?? null, content: '', docId: c.doc_id, chunkId: c.chunk_id ?? null, kbId: c.kb_id ?? null };
    track('citation_click', { kbId: effKbId, docId: c.doc_id, detail: c.chunk_id != null ? String(c.chunk_id) : null });
    try {
    const dv = await kbApi.getDocument(c.doc_id);
      const chunk = dv.chunks.find((x) => x.id === c.chunk_id);
      if (chunk) citeOpen = { ...citeOpen, content: chunk.content };
    } catch {
      /* 内容不可用时保持空 */
    }
  }

  // ─── 检索片段人工编辑（RAG 前） ───
  interface DraftChunk { chunkId: number; docTitle: string; section: string | null; content: string; score: number; source: string; }
  let draftChunks = $state<DraftChunk[]>([]);
  let draftBusy = $state(false);
  let draftSelected = $state<Set<number>>(new Set());
  let draftEditId = $state<number | null>(null);
  let draftEditText = $state('');

  async function runDraftSearch() {
    if (!ragQuery.trim()) { notify('请输入问题', 'warn'); return; }
    draftBusy = true;
    try {
      const res = await kbApi.search({
        input: { userId: 1, kbId: effKbId, query: ragQuery, topK: 10, mode: ragMode, providerId: chatProvider || null, model: chatModel || null },
      });
      draftChunks = res.map((r) => ({ chunkId: r.chunk_id, docTitle: r.doc_title, section: r.section, content: r.content, score: r.score, source: r.source }));
      draftSelected = new Set(draftChunks.map((c) => c.chunkId));
      draftEditId = null;
      notify(`已检索到 ${draftChunks.length} 个片段，可勾选/排序/编辑后生成回答`);
    } catch (e: unknown) { notify('检索片段失败：' + e, 'error'); }
    finally { draftBusy = false; }
  }
  function draftToggle(id: number) {
    const s = new Set(draftSelected);
    if (s.has(id)) s.delete(id); else s.add(id);
    draftSelected = s;
  }
  function draftUp(i: number) {
    if (i <= 0) return;
    const a = [...draftChunks];
    [a[i - 1], a[i]] = [a[i], a[i - 1]];
    draftChunks = a;
  }
  function draftDown(i: number) {
    if (i >= draftChunks.length - 1) return;
    const a = [...draftChunks];
    [a[i + 1], a[i]] = [a[i], a[i + 1]];
    draftChunks = a;
  }
  function draftRemove(i: number) {
    const removed = draftChunks[i];
    draftChunks = draftChunks.filter((_, x) => x !== i);
    if (removed) {
      const s = new Set(draftSelected);
      s.delete(removed.chunkId);
      draftSelected = s;
    }
    if (draftEditId === removed?.chunkId) draftEditId = null;
  }
  function startDraftEdit(c: DraftChunk) {
    if (draftEditId === c.chunkId) { draftEditId = null; return; }
    draftEditId = c.chunkId;
    draftEditText = c.content;
  }

  async function loadSearchLogs() {
    try { searchLogs = await kbApi.searchHistory(20); }
    catch { searchLogs = []; }
  }
  async function loadQaSessions() {
    try { qaSessions = await kbApi.listSessions(); }
    catch { qaSessions = []; }
  }
  async function openQaSession(id: number) {
    curQaSession = id; qaLoading = true;
    try { qaMessages = await kbApi.listMessages(id); }
    catch (e: unknown) { qaMessages = []; notify('读取消息失败：' + e, 'error'); }
    finally { qaLoading = false; }
  }
  async function newQaSession() {
    try {
    const id = await kbApi.createSession(effKbId, '问答 ' + new Date().toLocaleString());
      curQaSession = id; qaMessages = [];
      await loadQaSessions();
      onSessionsChanged?.();
      notify('已创建新会话');
    } catch (e: unknown) { notify('创建会话失败：' + e, 'error'); }
  }
  async function deleteQaSession(id: number) {
    if (!await kbConfirm({ message: '删除该问答会话？', danger: true, confirmText: '删除' })) return;
    try {
    await kbApi.deleteSession(id);
      if (curQaSession === id) { curQaSession = null; qaMessages = []; }
      await loadQaSessions();
      onSessionsChanged?.();
    } catch (e: unknown) { notify('删除失败：' + e, 'error'); }
  }

  function switchMode(m: Mode) {
    mode = m;
    if (m === 'qa') { loadQaSessions(); if (curQaSession !== null) openQaSession(curQaSession); }
    if (m === 'search') loadSearchLogs();
  }

  // ─── 检索执行 ───
  async function doSearch() {
    if (!query.trim()) return;
    const searchQuery = query.trim();
    query = ''; // 立即清空输入框
    searching = true;
    try {
      searchResults = await kbApi.search({
        input: { userId: 1, kbId: effKbId, query: searchQuery, topK: searchTopK, mode: searchMode, providerId: chatProvider || null, model: chatModel || null },
      });
      // 回退策略：混合检索中文整句 0 命中时，用提取的关键词重试
      if (searchResults.length === 0 && searchMode === 'hybrid' && /[\u4e00-\u9fa5]/.test(searchQuery)) {
        const terms = extractChineseTerms(searchQuery);
        if (terms.length > 0) {
          const fallbackQuery = terms.slice(0, 5).join(' ');  // 取前 5 个关键词
          const fallbackResults = await kbApi.search({
            input: { userId: 1, kbId: effKbId, query: fallbackQuery, topK: searchTopK, mode: searchMode, providerId: chatProvider || null, model: chatModel || null },
          });
          if (fallbackResults.length > 0) {
            searchResults = fallbackResults;
            notify(`已用关键词「${fallbackQuery}」匹配到 ${fallbackResults.length} 条结果`, 'success');
          }
        }
      }
      track('search', { kbId: effKbId, detail: JSON.stringify({ mode: searchMode, topK: searchTopK, hitCount: searchResults.length }) });
      loadSearchLogs();
    } catch (e: unknown) { notify('检索失败：' + e, 'error'); }
    finally { searching = false; }
  }

  // ─── RAG 执行 ───
  async function doRag() {
    if (!ragQuery.trim()) return;
    const userQuery = ragQuery.trim();
    ragQuery = ''; // 立即清空输入框
    ragLoading = true; streamText = ''; ragStopping = false;

    // 立即在 UI 中显示用户消息（乐观更新）
    const userMsg: QaMessageItem = {
      id: Date.now(),
      role: 'user',
      content: userQuery,
      citations: '',
      createdAt: new Date().toISOString(),
    };
    qaMessages = [...qaMessages, userMsg];
    scrollToBottom();

    // 首次提问自动创建会话：保证回答落库、历史对话有记录
    if (curQaSession === null) {
      try {
        const title = userQuery.slice(0, 24) || '问答';
        const id = await kbApi.createSession(effKbId, title);
        curQaSession = id;
        await loadQaSessions();
        onSessionsChanged?.();
      } catch (e: unknown) {
        notify('创建会话失败：' + e, 'error');
        ragLoading = false;
        // 移除乐观添加的用户消息
        qaMessages = qaMessages.filter((m) => m.id !== userMsg.id);
        return;
      }
    }
    const overrides = draftChunks
      .filter((c) => draftSelected.has(c.chunkId))
      .map((c) => ({
        chunkId: c.chunkId,
        content: draftEditId === c.chunkId ? draftEditText : c.content,
      }));
    const input: KbRagInput = {
      userId: 1,
      kbId: effKbId,
      query: userQuery,
      providerId: chatProvider || null,
      model: chatModel || null,
      topK: ragTopK,
      mode: ragMode,
      sessionId: curQaSession,
    };
    if (overrides.length > 0) input.chunks = overrides;
    // 流式问答：Channel 接收 delta/done/error 帧，逐字渲染
    const channel = new Channel<string>();
    ragChannel = channel;
    let acc = '';
    let streamErr = '';
    // 超时兜底：生成超过阈值仍未结束则自动停止并提示
    if (ragTimeout) clearTimeout(ragTimeout);
    ragTimeout = setTimeout(() => {
      if (ragLoading) {
        notify('生成超时，已自动停止', 'warn');
        stopRag();
      }
    }, RAG_TIMEOUT_MS);
    channel.onmessage = (msg: string) => {
      try {
        const f = JSON.parse(msg);
        if (f.type === 'delta') {
          acc += f.content ?? '';
          streamText = acc;
        } else if (f.type === 'done') {
          acc = f.content ?? acc;
          streamText = acc;
        } else if (f.type === 'error') {
          streamErr = f.message ?? '生成失败';
        }
      } catch {
        /* 忽略坏帧 */
      }
    };
    try {
      const result = await kbApi.ragStreamWithChannel(input, channel);
      // 捕获 ragId 用于精准取消（后端返回 { streamed, model, provider, ragId }）
      if (result && typeof result === 'object' && 'ragId' in result) {
        activeRagId = (result as { ragId?: number }).ragId ?? null;
      }
      if (streamErr) {
        notify('RAG 失败：' + streamErr, 'error');
      } else if (curQaSession !== null) {
        qaMessages = await kbApi.listMessages(curQaSession);
        loadQaSessions();
        onSessionsChanged?.();
      }
    } catch (e: unknown) {
      notify('RAG 失败：' + e, 'error');
    } finally {
      if (ragTimeout) { clearTimeout(ragTimeout); ragTimeout = null; }
      ragChannel = null;
      activeRagId = null;
      ragLoading = false;
      streamText = null;
      if (ragStopping) {
        ragStopping = false;
        notify('已停止生成', 'warn');
      }
    }
  }

  // ─── 停止 RAG 流式生成 ───
  function stopRag() {
    if (!ragLoading) return;
    ragStopping = true;
    // 精准取消：传入当前活跃 RAG ID，避免误取消其他并发请求
    kbApi.ragCancel(activeRagId ?? undefined).catch(() => { /* 忽略取消命令失败 */ });
    ragChannel = null; // 释放 Channel 引用
    activeRagId = null;
  }

  // ─── 高亮分段（前端本地计算） ───
  function fmtTime(t: string): string {
    return formatIsoTime(t, { showYear: false, utc: true });
  }
  // 侧边栏「历史对话」点击：进入问答模式并打开对应会话
  $effect(() => {
    if (openSession) {
      mode = 'qa';
      started = true;
      loadQaSessions();
      openQaSession(openSession.id);
    }
  });

  // ─── 「开始新的对话」落地页 ───
  let started = $state(false);
  let landingText = $state('');
  function sendFromLanding() {
    const t = landingText.trim();
    if (!t) return;
    landingText = '';
    started = true;
    if (mode === 'qa') {
      ragQuery = t;
      doRag();
    } else {
      query = t;
      doSearch();
    }
  }
  function restart() {
    started = false;
    landingText = '';
    searchResults = [];
    streamText = null;
    draftChunks = [];
  }

  onMount(() => { loadSearchLogs(); });
</script>

<div class="h-full flex flex-col min-h-0">
  {#if !started}
    <!-- 开始新的对话 -->
    <div class="kb-chat-landing">
      <Empty class="border-0 p-4">
        <KbIcon name="sparkle" size={32} color="var(--kb-accent-bright)" />
        <EmptyTitle class="text-xl">开始新的对话</EmptyTitle>
        <EmptyDescription>向你的知识库提问，获取回答与分析</EmptyDescription>
      </Empty>
      <div class="kb-chat-composer">
        <div class="kb-chat-inputrow">
          <textarea class="kb-chat-input" rows="3" placeholder="请输入您想要咨询的问题或需要帮助的内容..."
            bind:value={landingText}
            onkeydown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendFromLanding(); } }}></textarea>
          <button class="kb-chat-send" onclick={sendFromLanding} disabled={!landingText.trim()} title="发送">
            <KbIcon name="arrowUp" size={20} weight="bold" />
          </button>
        </div>
        <div class="kb-chat-controls">
          <div class="kb-chat-field" title="选择提问模式">
            <span class="kb-chat-field-label">模式</span>
            <div class="kb-seg kb-chat-mode-seg">
              <button class="kb-seg-item" class:active={mode === 'search'} onclick={() => mode = 'search'}><KbIcon name="search" size={13} />检索</button>
              <button class="kb-seg-item" class:active={mode === 'qa'} onclick={() => mode = 'qa'}><KbIcon name="chat" size={13} />问答</button>
            </div>
          </div>
          <div class="kb-chat-field" title="选择知识库">
            <span class="kb-chat-field-label">知识库</span>
            <KbSelect icon="book" style="min-width:170px" items={kbItems} value={kbSel}
              onchange={(v) => (kbSel = v as number | 'all')} />
          </div>
          <div class="kb-chat-field" title="选择推理模型">
            <span class="kb-chat-field-label">推理模型</span>
            <div class="kb-chat-model-pair">
              <KbSelect style="min-width:150px" items={providerItems} value={chatProvider} placeholder="提供方…"
                onchange={onProviderChange} />
              <KbSelect style="min-width:190px" items={modelItems} value={chatModel} placeholder="模型…"
                disabled={chatProvider === ''} onchange={onModelChange} />
            </div>
          </div>
        </div>
        <div class="kb-chat-disclaimer">内容由 AI 生成，仅供参考</div>
      </div>
      {#if recommendations.length > 0}
        <div class="kb-chat-suggest">
          <div class="kb-chat-suggest-head"><KbIcon name="sparkle" size={13} />热门问题推荐{recLoading ? '…' : ''}</div>
          <div class="kb-chat-suggest-row">
            {#each recommendations as r}
              <button class="kb-reco-chip" type="button" onclick={() => useRecommendation(r.question)} title="点击填充到输入框">{r.question}</button>
            {/each}
          </div>
        </div>
      {/if}
    </div>
  {:else}
  <!-- 顶部工具条 -->
  <div class="kb-chat-head">
    <div class="kb-seg kb-chat-mode-seg">
      <button class="kb-seg-item" class:active={mode === 'search'} onclick={() => switchMode('search')}><KbIcon name="search" size={14} />检索</button>
      <button class="kb-seg-item" class:active={mode === 'qa'} onclick={() => switchMode('qa')}><KbIcon name="chat" size={14} />问答</button>
    </div>
    <div style="flex:1"></div>
    <KbSelect icon="book" style="min-width:170px;max-width:220px" items={kbItems} value={kbSel}
      onchange={(v) => (kbSel = v as number | 'all')} />
    <KbSelect style="min-width:130px;max-width:180px" items={providerItems} value={chatProvider} placeholder="提供方…"
      onchange={onProviderChange} />
    <KbSelect style="min-width:170px;max-width:220px" items={modelItems} value={chatModel} placeholder="模型…"
      disabled={chatProvider === ''} onchange={onModelChange} />
    <Button variant="outline" size="sm" onclick={restart} title="回到开始页">
      <KbIcon name="home" size={13} />重新开始
    </Button>
  </div>

  {#if mode === 'search'}
  <div class="kb-chat-search kb-scroll">
    <!-- 检索框 -->
    <div class="kb-chat-searchbar">
      <div class="kb-chat-searchinput">
        <KbIcon name="search" size={15} />
        <input class="kb-chat-searchfield" placeholder="输入关键词检索知识库（混合检索 = 向量 + BM25）" bind:value={query}
          onkeydown={(e) => e.key === 'Enter' && doSearch()} />
      </div>
      <Button onclick={doSearch} disabled={searching || !query.trim()}>
        {searching ? '检索中…' : '检索'}
      </Button>
    </div>
    <div class="kb-chat-searchtools">
      <label class="kb-chat-tool-label">模式
        <KbSelect style="min-width:112px" items={SEARCH_MODE_ITEMS} value={searchMode}
          onchange={(v) => (searchMode = v as 'hybrid' | 'vector' | 'bm25')} />
      </label>
      <label class="kb-chat-tool-label">条数
        <KbSelect style="min-width:86px" items={SEARCH_TOPK_ITEMS} value={searchTopK}
          onchange={(v) => (searchTopK = Number(v))} />
      </label>
      <div style="flex:1"></div>
      <Button variant="ghost" size="sm" onclick={() => { showHistory = !showHistory; if (showHistory) loadSearchLogs(); }}>
        <KbIcon name="activity" size={13} />{showHistory ? '隐藏历史' : '检索历史'}
      </Button>
    </div>

    {#if showHistory}
      <div class="kb-chat-history">
        <div class="kb-chat-history-head"><KbIcon name="activity" size={13} />最近检索</div>
        {#each searchLogs as l}
          <button class="kb-chat-history-item" type="button" onclick={() => { query = l.query; showHistory = false; doSearch(); }}>
            <span style="flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">「{l.query}」</span>
  <span class="kb-badge kb-badge-info">{MODE_LABEL[l.mode] ?? l.mode}</span>
            <span class="kb-chat-history-meta">{l.hitCount} 条 · {fmtTime(l.createdAt)}</span>
          </button>
        {/each}
        {#if searchLogs.length === 0}<div class="kb-chat-empty-line">暂无记录</div>{/if}
      </div>
    {/if}

    <!-- 结果 -->
    <div class="kb-chat-results">
      {#each searchResults as r, i}
        <article class="kb-chat-result">
          <header class="kb-chat-result-head">
            <span class="kb-chat-rank">{i + 1}</span>
            <span class="kb-chat-result-title" title={r.doc_title}>{r.doc_title || ('文档 #' + r.doc_id)}</span>
            {#if r.section}<span class="kb-badge kb-badge-mute">{r.section}</span>{/if}
            {#if r.page_no}<span class="kb-badge kb-badge-mute">P{r.page_no}</span>{/if}
            <div style="flex:1"></div>
  <span class="kb-chat-source" class:vector={r.source === 'vector'} class:bm25={r.source === 'bm25'} class:hybrid={r.source === 'hybrid'}>{MODE_LABEL[r.source] ?? r.source}</span>
            <span class="kb-chat-score">{(r.score * 100).toFixed(1)}%</span>
          </header>
          <p class="kb-chat-result-body">
            {#each highlightSegments(r.content, query) as seg}
              <span class:kb-hl={seg.hit}>{seg.text}</span>
            {/each}
          </p>
        </article>
      {/each}
      {#if searchResults.length === 0 && !searching}
        <div class="kb-empty"><span class="kb-empty-ico"><KbIcon name="search" size={22} /></span><span>{query.trim() ? '未检索到相关内容' : '输入关键词开始检索'}</span></div>
      {/if}
    </div>
  </div>

  {:else}
  <!-- 问答模式 -->
  <div class="kb-chat-qa">
    <!-- 会话列表 -->
    <aside class="kb-chat-sessions">
      <Button class="kb-chat-new-session" onclick={newQaSession}><KbIcon name="plus" size={14} weight="bold" />新建会话</Button>
      {#each qaSessions as s}
        <div
          class="kb-chat-session"
          class:active={curQaSession === s.id}
          role="button"
          tabindex="0"
          onclick={() => openQaSession(s.id)}
          onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); openQaSession(s.id); } }}
        >
          <span class="kb-chat-session-title" title={s.title ?? ''}>{s.title ?? ('会话 #' + s.id)}</span>
          <button class="kb-btn-sm kb-btn-ghost" onclick={(e) => { e.stopPropagation(); deleteQaSession(s.id); }} title="删除会话"><KbIcon name="close" size={12} /></button>
          <span class="kb-chat-session-time">{fmtTime(s.updatedAt)}</span>
        </div>
      {/each}
      {#if qaSessions.length === 0}<div class="kb-chat-empty-line">暂无会话</div>{/if}
    </aside>

    <!-- 对话区 -->
    <section class="kb-chat-conversation">
      <div class="kb-chat-messages" bind:this={msgEl}>
        {#if qaLoading}
          <div class="kb-empty">加载中…</div>
        {:else}
          {#each qaMessages as m}
            <div class="kb-chat-msg" class:user={m.role === 'user'}>
              <div class="kb-chat-bubble" class:kb-bubble-user={m.role === 'user'} class:kb-bubble-ai={m.role === 'assistant'}>{m.content}</div>
              {#if m.role === 'assistant' && parseCitations(m.citations).length > 0}
                <div class="kb-chat-cites">
                  {#each parseCitations(m.citations) as c, ci}
                    <button type="button" class="kb-cite-chip" title={c.content ?? ''} onclick={() => openCitation(c)}>[{ci + 1}] {c.doc_title ?? '引用'}{c.section ? ' · ' + c.section : ''}</button>
                  {/each}
                </div>
              {/if}
              <div class="kb-chat-msg-time">{fmtTime(m.createdAt)}</div>
            </div>
          {/each}
          {#if qaMessages.length === 0 && !qaLoading && !ragLoading}
            <div class="kb-empty"><span class="kb-empty-ico"><KbIcon name="idea" size={22} /></span><span>选择左侧会话，或新建会话后开始提问</span></div>
          {/if}
          {#if ragLoading && streamText !== null}
            <div class="kb-chat-msg">
              <div class="kb-chat-bubble kb-bubble-ai">
                {#if streamText}{streamText}{:else}思考中…{/if}<span class="kb-cursor"></span>
              </div>
            </div>
          {/if}
        {/if}
      </div>

      <!-- 输入区 -->
      <div class="kb-chat-composer2">
        <div class="kb-chat-composer2-tools">
          <label class="kb-chat-tool-label">模式
            <KbSelect style="min-width:112px" items={RAG_MODE_ITEMS} value={ragMode}
              onchange={(v) => (ragMode = v as 'hybrid' | 'vector' | 'bm25')} />
          </label>
          <label class="kb-chat-tool-label">引用
            <KbSelect style="min-width:86px" items={RAG_TOPK_ITEMS} value={ragTopK}
              onchange={(v) => (ragTopK = Number(v))} />
          </label>
          {#if curQaSession === null}
            <span class="kb-badge kb-badge-warn">回答不入会话</span>
          {:else}
            <span class="kb-badge kb-badge-ok">回答将写入当前会话</span>
          {/if}
        </div>
        {#if draftChunks.length > 0}
          <div style="border:1px solid var(--kb-border);border-radius:10px;margin-bottom:8px;max-height:260px;overflow:auto">
            <div style="display:flex;align-items:center;gap:8px;padding:7px 10px;border-bottom:1px solid var(--kb-border);font-size:12px;color:var(--kb-text-2)">
              <span>检索片段（{draftSelected.size}/{draftChunks.length}）</span>
              <span style="font-size:11.5px;color:var(--kb-text-3)">勾选后生成回答将只使用这些片段</span>
              <div style="flex:1"></div>
              <button class="kb-btn-sm" onclick={() => { draftChunks = []; draftEditId = null; }}>清空</button>
            </div>
            {#each draftChunks as c, i}
              <div style="display:flex;gap:8px;padding:7px 10px;border-bottom:1px solid var(--kb-border-subtle);align-items:flex-start">
                <Checkbox checked={draftSelected.has(c.chunkId)} onCheckedChange={() => draftToggle(c.chunkId)} class="mt-0.5 shrink-0" />
                <div style="flex:1;min-width:0">
                  <div style="font-size:12px;font-weight:600;color:var(--kb-text)">{c.docTitle || ('片段 #' + c.chunkId)}{c.section ? ' · ' + c.section : ''}</div>
                  {#if draftEditId === c.chunkId}
                    <textarea class="kb-textarea" style="margin-top:4px;font-size:12px;min-height:60px" bind:value={draftEditText}></textarea>
                  {:else}
                    <div style="font-size:11.5px;color:var(--kb-text-2);margin-top:2px;line-height:1.5;word-break:break-all;display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical;overflow:hidden">{c.content}</div>
                  {/if}
                </div>
                <div style="display:flex;flex-direction:column;gap:2px;flex:none">
                  <button class="kb-btn-sm" style="padding:1px 6px" onclick={() => draftUp(i)} disabled={i === 0} title="上移"><KbIcon name="arrowUp" size={11} /></button>
                  <button class="kb-btn-sm" style="padding:1px 6px" onclick={() => draftDown(i)} disabled={i === draftChunks.length - 1} title="下移"><KbIcon name="arrowDown" size={11} /></button>
                </div>
                <div style="display:flex;flex-direction:column;gap:2px;flex:none">
                  <button class="kb-btn-sm" style="padding:1px 6px" onclick={() => startDraftEdit(c)} title="编辑内容"><KbIcon name="edit" size={11} /></button>
                  <button class="kb-btn-sm kb-dang" style="padding:1px 6px" onclick={() => draftRemove(i)} title="移除"><KbIcon name="trash" size={11} /></button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
        <div class="kb-chat-composer2-row">
          <textarea class="kb-textarea kb-chat-qa-input" rows="2" placeholder="向知识库提问（基于检索上下文生成回答）" bind:value={ragQuery}
            onkeydown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); doRag(); } }}></textarea>
          <Button variant="outline" onclick={runDraftSearch} disabled={draftBusy || !ragQuery.trim()} title="先检索片段，可勾选/排序/编辑后再生成">
            <KbIcon name="search" size={14} />{draftBusy ? '检索中…' : '检索片段'}
          </Button>
          {#if ragLoading}
            <Button variant="destructive" onclick={stopRag} title="停止生成">
              <KbIcon name="stop" size={14} />停止
            </Button>
          {/if}
          <Button onclick={doRag} disabled={ragLoading || !ragQuery.trim()}>
            {ragLoading ? '生成中…' : '发送'}
          </Button>
        </div>
      </div>
    </section>
  </div>
  {/if}
  {/if}
</div>

<!-- 引用来源详情 -->
{#if citeOpen}
  <KbModal open={citeOpen !== null} onClose={() => citeOpen = null} ariaLabel="关闭引用来源弹窗">
    <div class="kb-modal">
      <div class="kb-modal-hd"><KbIcon name="file" size={16} color="var(--kb-accent-bright)" />引用来源</div>
      <div class="kb-modal-bd">
        <div style="font-size:14px;font-weight:600;color:var(--kb-text);word-break:break-word">{citeOpen.title}</div>
        {#if citeOpen.section}<div style="font-size:12px;color:var(--kb-text-3);margin-top:4px">章节：{citeOpen.section}</div>{/if}
        <div style="border:1px solid var(--kb-border);border-radius:8px;padding:10px;margin-top:10px;font-size:12.5px;line-height:1.7;color:var(--kb-text-2);white-space:pre-wrap;word-break:break-word;max-height:42vh;overflow:auto">{citeOpen.content || '（该分片内容暂不可用，可在文档中查看）'}</div>
      </div>
      <div class="kb-modal-ft">
        <Button variant="outline" onclick={() => citeOpen = null}>关闭</Button>
        <Button onclick={() => { const c = citeOpen; if (!c) return; const d = c.docId; const k = c.kbId; citeOpen = null; onOpenDoc?.(d, k); }}>打开文档</Button>
      </div>
    </div>
  </KbModal>
{/if}

<style>
  .kb-hl { background: color-mix(in srgb, var(--app-warning, #faad14) 30%, transparent); color: inherit; border-radius: 2px; padding: 0 1px; }
  .kb-bubble-user { background: linear-gradient(180deg, var(--kb-btn-bg), var(--kb-btn-bg-hover)); color: #fff; border-bottom-right-radius: 4px; box-shadow: inset 0 1px 0 rgba(255, 255, 255, .1); }
  .kb-bubble-ai { background: var(--kb-surface); color: var(--kb-text); border: 1px solid var(--kb-border); border-bottom-left-radius: 4px; }
  .kb-cursor {
    display: inline-block;
    width: 2px;
    height: 14px;
    margin-left: 2px;
    vertical-align: text-bottom;
    background: var(--kb-accent-bright);
    animation: kb-blink 1s steps(2, start) infinite;
  }
  @keyframes kb-blink { to { visibility: hidden; } }
  .kb-cite-chip { font-size: 11.5px; padding: 2px 9px; border-radius: 999px; background: var(--kb-surface); border: 1px solid color-mix(in srgb, var(--app-accent, #1a73e8) 44%, var(--app-bg-color)); color: var(--kb-accent-bright); cursor: pointer; max-width: 260px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: inherit; transition: background .12s, border-color .12s; }
  .kb-cite-chip:hover { background: var(--kb-hover-strong); border-color: var(--kb-accent); }
  .kb-reco-chip {
    font-size: 12px; font-family: inherit; cursor: pointer;
    color: var(--kb-accent-bright); background: var(--kb-surface);
    border: 1px solid color-mix(in srgb, var(--app-accent, #1a73e8) 40%, var(--app-bg-color));
    border-radius: 999px; padding: 5px 12px;
    transition: background .12s, border-color .12s;
  }
  .kb-reco-chip:hover { background: var(--kb-hover-strong); border-color: var(--kb-accent); }

  /* ─── 「开始新的对话」落地页 ─── */
  .kb-chat-landing {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 28px 24px;
  }
  .kb-chat-composer {
    width: min(760px, 100%);
    min-width: 0;
    margin-top: 18px;
    display: flex;
    flex-direction: column;
    background: var(--kb-surface);
    border: 1px solid var(--kb-border);
    border-radius: 16px;
    padding: 14px 14px 10px;
    box-shadow: var(--kb-shadow-sm);
    overflow: hidden;
  }
  .kb-chat-inputrow {
    display: flex;
    gap: 10px;
    align-items: flex-end;
  }
  .kb-chat-input {
    flex: 1;
    min-height: 56px;
    padding: 11px 14px;
    font-size: 13px;
    font-family: inherit;
    line-height: 1.6;
    color: var(--kb-text);
    background: var(--app-bg-color);
    border: 1px solid var(--kb-border);
    border-radius: 12px;
    resize: none;
    transition: border-color .14s, box-shadow .14s;
  }
  .kb-chat-input:focus {
    outline: none;
    border-color: var(--kb-accent);
    box-shadow: var(--kb-focus-ring);
  }
  .kb-chat-input::placeholder { color: var(--kb-text-3); }
  .kb-chat-send {
    flex: none;
    width: 40px;
    height: 40px;
    border-radius: 10px;
    border: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(135deg, #8ee27a 0%, var(--kb-ok, #52c41a) 100%);
    color: #fff;
    cursor: pointer;
    box-shadow: 0 2px 10px color-mix(in srgb, var(--app-success, #52c41a) 40%, transparent);
    transition: filter .14s, transform .06s;
  }
  .kb-chat-send:hover { filter: brightness(1.08); }
  .kb-chat-send:active { transform: translateY(1px); }
  .kb-chat-send:disabled { opacity: .45; cursor: not-allowed; box-shadow: none; }

  /* 控制条：带标签的下拉框，位于输入框下方、同一「舞台」内 */
  .kb-chat-controls {
    display: flex;
    align-items: flex-end;
    gap: 12px;
    flex-wrap: wrap;
    padding-top: 12px;
    margin-top: 12px;
    border-top: 1px solid var(--kb-border-subtle);
  }
  .kb-chat-field {
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
  }
  .kb-chat-field-label {
    font-size: 11.5px;
    letter-spacing: .05em;
    color: var(--kb-text-3);
    white-space: nowrap;
  }
  .kb-chat-mode-seg { box-sizing: border-box; height: 32px; }
  .kb-chat-mode-seg .kb-seg-item { height: 26px; padding: 0 12px; }
  .kb-chat-model-pair {
    display: flex;
    gap: 8px;
    min-width: 0;
    max-width: 100%;
  }
  /* 模型选择器：固定最大宽度，防止长名称撑破布局 */
  .kb-chat-model-pair :global(.kb-dselect) {
    max-width: 180px;
    min-width: 0;
    flex: 1;
  }
  .kb-chat-model-pair :global(.kb-select-trigger) {
    overflow: hidden;
    min-width: 0;
  }
  .kb-chat-model-pair :global(.kb-select-label) {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 140px;
  }
  .kb-chat-disclaimer {
    font-size: 11.5px;
    color: var(--kb-text-3);
    padding-top: 8px;
  }
  .kb-chat-suggest {
    display: flex;
    flex-direction: column;
    gap: 9px;
    width: min(760px, 100%);
    margin-top: 16px;
  }
  .kb-chat-suggest-head {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--kb-text-3);
  }
  .kb-chat-suggest-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  /* ─── AI 问答工作区 ─── */
  .kb-chat-head {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding: 10px 14px;
    border-bottom: 1px solid var(--kb-border-subtle);
    background: var(--app-bg-color);
  }
  .kb-chat-search {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .kb-chat-searchbar { display: flex; gap: 8px; align-items: center; }
  .kb-chat-searchinput {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 12px;
    background: var(--app-bg-color);
    border: 1px solid var(--kb-border);
    border-radius: 10px;
    color: var(--kb-text-3);
    transition: border-color .14s, box-shadow .14s;
  }
  .kb-chat-searchinput:focus-within {
    border-color: var(--kb-accent);
    box-shadow: var(--kb-focus-ring);
  }
  .kb-chat-searchfield {
    flex: 1;
    border: none;
    background: transparent;
    color: var(--kb-text);
    font-size: 13.5px;
    font-family: inherit;
    outline: none;
  }
  .kb-chat-searchtools {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .kb-chat-tool-label {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
    color: var(--kb-text-3);
  }
  .kb-chat-history {
    border: 1px solid var(--kb-border);
    border-radius: 12px;
    background: var(--app-bg-color);
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .kb-chat-history-head {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--kb-text-2);
  }
  .kb-chat-history-item {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    font-family: inherit;
    padding: 7px 9px;
    border: none;
    border-radius: 8px;
    background: var(--kb-hover);
    color: var(--kb-text);
    cursor: pointer;
    text-align: left;
    transition: background .12s;
  }
  .kb-chat-history-item:hover { background: var(--kb-hover-strong); }
  .kb-chat-history-meta { font-size: 11.5px; color: var(--kb-text-3); }
  .kb-chat-empty-line {
    font-size: 12px;
    color: var(--kb-text-3);
    text-align: center;
    padding: 10px;
  }
  .kb-chat-results { display: flex; flex-direction: column; gap: 10px; }
  .kb-chat-result {
    border: 1px solid var(--kb-border);
    border-radius: 12px;
    background: var(--app-bg-color);
    padding: 12px 14px;
    transition: border-color .14s;
  }
  .kb-chat-result:hover { border-color: var(--kb-border-strong); }
  .kb-chat-result-head {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin-bottom: 7px;
  }
  .kb-chat-rank {
    width: 22px;
    height: 22px;
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 7px;
    background: var(--kb-hover-strong);
    color: var(--kb-accent-bright);
    font-size: 11.5px;
    font-weight: 700;
  }
  .kb-chat-result-title {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--kb-text);
    max-width: 260px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .kb-chat-source {
    font-size: 11.5px;
    padding: 1px 8px;
    border-radius: 999px;
    border: 1px solid var(--kb-border);
    color: var(--kb-text-3);
  }
  .kb-chat-source.vector {
    color: var(--kb-accent-bright);
    border-color: color-mix(in srgb, var(--app-accent) 45%, var(--app-bg-color));
  }
  .kb-chat-source.bm25 {
    color: var(--kb-ok);
    border-color: color-mix(in srgb, var(--app-success) 45%, var(--app-bg-color));
  }
  .kb-chat-source.hybrid {
    color: var(--kb-warn);
    border-color: color-mix(in srgb, var(--app-warning) 45%, var(--app-bg-color));
  }
  .kb-chat-score {
    font-size: 11.5px;
    color: var(--kb-text-3);
    font-variant-numeric: tabular-nums;
  }
  .kb-chat-result-body {
    margin: 0;
    font-size: 13px;
    line-height: 1.7;
    color: var(--kb-text-2);
    word-break: break-all;
  }

  /* 问答 */
  .kb-chat-qa {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .kb-chat-sessions {
    flex: none;
    width: 216px;
    border-right: 1px solid var(--kb-border);
    overflow-y: auto;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    background: var(--app-bg-color);
  }
  .kb-chat-session {
    display: grid;
    grid-template-columns: 1fr auto;
    grid-template-rows: auto auto;
    align-items: center;
    padding: 8px 9px;
    border-radius: 9px;
    cursor: pointer;
    transition: background .12s;
  }
  .kb-chat-session:hover { background: var(--kb-hover); }
  .kb-chat-session.active {
    background: var(--kb-active);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--app-accent) 34%, transparent);
  }
  .kb-chat-session-title {
    font-size: 12.5px;
    color: var(--kb-text);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .kb-chat-session.active .kb-chat-session-title { color: var(--kb-accent-bright); }
  .kb-chat-session-time {
    grid-column: 1;
    font-size: 11.5px;
    color: var(--kb-text-3);
  }
  .kb-chat-conversation {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .kb-chat-messages {
    flex: 1;
    overflow-y: auto;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .kb-chat-msg {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
  }
  .kb-chat-msg.user { align-items: flex-end; }
  .kb-chat-bubble {
    max-width: 86%;
    padding: 9px 12px;
    border-radius: 12px;
    font-size: 13px;
    line-height: 1.7;
    word-break: break-word;
    white-space: pre-wrap;
  }
  .kb-chat-cites {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 6px;
    max-width: 86%;
  }
  .kb-chat-msg-time {
    font-size: 11.5px;
    color: var(--kb-text-3);
    margin-top: 3px;
  }
  .kb-chat-composer2 {
    border-top: 1px solid var(--kb-border);
    padding: 10px 14px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .kb-chat-composer2-tools {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .kb-chat-composer2-row {
    display: flex;
    gap: 8px;
    align-items: flex-end;
  }
  .kb-chat-qa-input { flex: 1; }
</style>
