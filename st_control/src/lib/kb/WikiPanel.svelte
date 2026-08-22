<script lang="ts">
  import { kbApi } from './services/ipc';
  import { untrack } from 'svelte';
  import { formatIsoTime } from '../format';
  import type { WikiPageItem, WikiPageDetail, WikiGraph, WikiGraphNode, WikiDir } from './kbTypes';
  import { edgeLinkTypes, nodeDegreeMap, visibleNodeIds } from './graphUtils';
  import { buildWikiGraph, communityColor, type BuiltWikiGraph, type WEdge, type WNode } from './wikiGraphModel';
  import WikiGraphCanvas from './WikiGraphCanvas.svelte';
  import { buildDirSubtree, buildDirTree, filterPagesByDir } from './dirTreeUtils';
  import { renderMd } from './markdown';
  import { lsGet, lsSet } from '../storage';
  import {
    NODE_TYPE_COLORS,
    edgeColor,
    nodeColor,
    nodeTypeName,
  } from './graphStyle';
  import KbIcon from './KbIcon.svelte';
  import { track } from './analytics.svelte';
  import { Checkbox } from '../components/ui/checkbox';
  import { Slider } from '../components/ui/slider';

  interface Props {
    kbId: number | null;
  }
  let { kbId }: Props = $props();

  // ─── 视图状态 ───
  type View = 'list' | 'detail' | 'edit' | 'graph';
  let view = $state<View>('list');
  // 详情/编辑的返回目标：从图谱工作区进入时回到图谱，避免误跳回列表
  let detailBack = $state<'list' | 'graph'>('list');
  let editBack = $state<'list' | 'graph'>('list');
  let pages = $state<WikiPageItem[]>([]);
  let loading = $state(false);
  let err = $state('');
  let searchText = $state('');
  let searching = $state(false);
  let wikiDirs = $state<WikiDir[]>([]);
  let dirFilter = $state<number | ''>('');
  // 每个目录的子孙目录集合（含自身），用于「按目录筛选」与计数口径一致
  const dirSubtree = $derived(buildDirSubtree(wikiDirs));
  const filteredPages = $derived(
    filterPagesByDir(pages, dirFilter === '' ? null : dirFilter, dirSubtree),
  );
  // 目录树（扁平数据 → 有序树列表）
  const dirTree = $derived(buildDirTree(wikiDirs));

  // ─── 详情 ───
  let detail = $state<WikiPageDetail | null>(null);
  let detailLoading = $state(false);

  // ─── 编辑 ───
  let editingId = $state<number | null>(null);
  let editTitle = $state('');
  let editSummary = $state('');
  let editContent = $state('');
  let saveBusy = $state(false);
  let editTextarea = $state<HTMLTextAreaElement | null>(null);
  let wikiSuggest = $state<{ start: number; end: number; items: WikiPageItem[] } | null>(null);
  let suggestIdx = $state(0);

  function onEditInput() {
    const pos = editTextarea?.selectionStart ?? editContent.length;
    const before = editContent.slice(0, pos);
    const m = /\[\[\s*([^\[\]]*)$/.exec(before);
    if (m) {
      const query = m[1].trim();
      const items = pages
        .filter((p) => !query || p.title.toLowerCase().includes(query.toLowerCase()))
        .slice(0, 8);
      wikiSuggest = { start: pos - m[1].length - 2, end: pos, items };
      suggestIdx = 0;
    } else {
      wikiSuggest = null;
    }
  }
  function acceptSuggestion() {
    if (!wikiSuggest || wikiSuggest.items.length === 0) return;
    const item = wikiSuggest.items[Math.min(suggestIdx, wikiSuggest.items.length - 1)];
    editContent = editContent.slice(0, wikiSuggest.start) + `[[${item.title}]]` + editContent.slice(wikiSuggest.end);
    wikiSuggest = null;
    requestAnimationFrame(() => editTextarea?.focus());
  }
  function onEditKeydown(e: KeyboardEvent) {
    if (!wikiSuggest || wikiSuggest.items.length === 0) return;
    const n = wikiSuggest.items.length;
    if (e.key === 'ArrowDown') { e.preventDefault(); suggestIdx = (suggestIdx + 1) % n; }
    else if (e.key === 'ArrowUp') { e.preventDefault(); suggestIdx = (suggestIdx - 1 + n) % n; }
    else if (e.key === 'Enter' || e.key === 'Tab') { e.preventDefault(); acceptSuggestion(); }
    else if (e.key === 'Escape') { wikiSuggest = null; }
  }

  // ─── 图谱 ───
  let graph = $state<WikiGraph | null>(null);
  let graphBusy = $state(false);
  let graphSelect = $state<number | null>(null);
  const COLOR_PALETTE = ['#5b8ff9', '#5ad8a6', '#f6bd16', '#b37feb', '#4fd1c5', '#ff7a7a', '#ff9f43', '#8d99ae'];
  // 图谱可配置参数（持久化到本地，重启后保留）
  let graphCfgOpen = $state(false);
  let graphParams = $state({
    // ── 外观 ──
    nodeScale: 1.0,                         // 节点大小倍率
    edgeWidth: 1.5,                         // 连线粗细
    edgeOpacity: 0.85,                      // 连线透明度
    showLabels: true,                       // 显示节点标签
    showArrows: true,                       // 显示连线箭头
    labelOpacity: 0.9,                      // 文本透明度
    motion: true,                           // 灵动动画（力导向布局演化）
    showImplicit: true,                     // 显示隐含关系（共享实体）
    colorByCommunity: true,                 // 按社区着色（与社交图谱观感一致）
    // ── 筛选 ──
    showOrphans: true,                      // 显示孤立文件（无链接的笔记）
    createdOnly: true,                      // 仅显示已创建的笔记
    ignorePatterns: '',                     // 忽略文件（每行一个，* 通配）
    colorGroups: [] as { query: string; color: string }[], // 颜色组
    // ── 力度（力导向） ──
    forceRepulsion: 2600,                   // 力导向：斥力
    forceAttraction: 0.04,                  // 力导向：引力
    forceCentripetal: 0.02,                 // 力导向：向心力（把节点拉向中心）
    forceEdgeLength: 1.0,                   // 力导向：连线长度（理想边长倍率）
  });
  function loadGraphParams() {
    try {
      const raw = lsGet('kb_wiki_graph_params');
      if (raw) {
        const saved = JSON.parse(raw) as Record<string, unknown>;
        // 只恢复当前仍存在的参数，丢弃 layout/spread 等已移除项
        for (const k of Object.keys(graphParams)) {
          if (typeof saved[k] !== 'undefined') (graphParams as unknown as Record<string, unknown>)[k] = saved[k];
        }
      }
    } catch { /* 忽略损坏配置 */ }
  }
  function saveGraphParams() {
    lsSet('kb_wiki_graph_params', JSON.stringify(graphParams));
  }
  function resetGraphParams() {
    graphParams = {
      nodeScale: 1, edgeWidth: 1.5, edgeOpacity: 0.85, showLabels: true, showArrows: true, labelOpacity: 0.9, motion: true,
      showImplicit: true, colorByCommunity: true, showOrphans: true, createdOnly: true, ignorePatterns: '', colorGroups: [],
      forceRepulsion: 2600, forceAttraction: 0.04, forceCentripetal: 0.02, forceEdgeLength: 1,
    };
    localOnly = false;
    graphFilter = '';
    saveGraphParams();
  }
  loadGraphParams();
  const graphLinkTypes = $derived(edgeLinkTypes(graph));
  let graphFilter = $state('');
  let localOnly = $state(false);
  // 每个节点的总连接度（显式 + 隐含，用于「孤立文件」判断）
  const nodeTotalDegree = $derived(nodeDegreeMap(graph?.edges ?? []));
  // 忽略文件模式（支持 * 通配，逗号或换行分隔）
  const ignorePatterns = $derived.by(() =>
    (graphParams.ignorePatterns ?? '').split(/[\n,]/).map((s) => s.trim()).filter(Boolean),
  );
  const graphVisible = $derived(visibleNodeIds(graph, {
    nodeDegree: nodeTotalDegree,
    ignorePatterns,
    createdOnly: graphParams.createdOnly,
    showOrphans: graphParams.showOrphans,
    query: graphFilter.trim().toLowerCase(),
    localOnly,
    anchorId: graphSelect,
  }));
  const nodeTypeStats = $derived.by(() => {
    const m: Record<string, number> = {};
    if (graph) for (const nd of graph.nodes) { const t = nodeTypeName(nd); m[t] = (m[t] ?? 0) + 1; }
    return Object.entries(m).sort((a, b) => b[1] - a[1]);
  });
  function addColorGroup() {
    graphParams.colorGroups = [...(graphParams.colorGroups ?? []), { query: '', color: COLOR_PALETTE[0] }];
    saveGraphParams();
  }
  function removeColorGroup(i: number) {
    graphParams.colorGroups = (graphParams.colorGroups ?? []).filter((_, x) => x !== i);
    saveGraphParams();
  }
  // Canvas 图谱：由可见节点 + 布局参数构建力导向模型（力度/外观参数由画布组件读取）
  let graphCanvasRef = $state<{ resetView: () => void; relayout: () => void } | undefined>();
  const builtGraph: BuiltWikiGraph = $derived(buildWikiGraph(graph, graphVisible, {
    nodeScale: graphParams.nodeScale,
    forceEdgeLength: graphParams.forceEdgeLength,
    showImplicit: graphParams.showImplicit,
  }));
  // 着色模式/颜色组变化时递增，触发画布立即重绘（仿真关闭时也能即时生效）
  const graphRedrawKey = $derived(`${graphParams.colorByCommunity}:${JSON.stringify(graphParams.colorGroups ?? [])}`);
  // 选中节点被筛选隐藏时清除选中，避免画布整图置灰
  $effect(() => {
    if (graphSelect !== null && !builtGraph.nodes.some((n) => n.pageId === graphSelect)) {
      graphSelect = null;
    }
  });
  function wikiNodeColor(n: WNode): string {
    if (graphParams.colorByCommunity) return communityColor(n.community);
    const pseudo = { title: n.label, docTitle: n.docTitle, dirName: n.dirName } as WikiGraphNode;
    return nodeColor(n.status, pseudo, graphParams.colorGroups ?? []);
  }
  function tooltipFor(n: WNode): string {
    const parts: string[] = [nodeTypeName({ title: n.label, docTitle: n.docTitle, dirName: n.dirName } as WikiGraphNode)];
    parts.push(n.status === 'missing' ? '尚未创建' : n.status === 'draft' ? '草稿' : '已创建');
    if (n.docTitle) parts.push('来源：' + n.docTitle);
    parts.push(`入链 ${n.inDegree} · 出链 ${n.outDegree}`);
    return parts.join(' · ');
  }
  // ─── LLM 提炼 ───
  let genBusy = $state(false);
  let genMsg = $state('');
  // ─── 摘要与实体提取 ───
  let extractBusy = $state(false);
  let extractMsg = $state('');

  async function doExtract() {
    if (!detail || extractBusy) return;
    extractBusy = true; extractMsg = '';
    try {
    await kbApi.wikiExtract(detail.id);
      extractMsg = '已提交摘要与实体提取，正在后台生成…';
      // 轮询刷新状态
      for (let i = 0; i < 20; i++) {
        await new Promise((r) => setTimeout(r, 2500));
        await openDetail(detail.id);
        if (detail?.extractStatus !== 'pending') break;
      }
      if (detail?.extractStatus === 'done') extractMsg = '摘要与实体提取完成';
      else if (detail?.extractStatus === 'failed') extractMsg = '提取失败，请检查推理模型配置后重试';
    } catch (e: unknown) {
      extractMsg = '提取失败：' + e;
    } finally {
      extractBusy = false;
    }
  }

  async function doExtractAll() {
    if (kbId === null || extractBusy) return;
    if (!confirm('用 LLM 为知识库内尚未提取的页面生成摘要与实体？可能耗时较长。')) return;
    extractBusy = true; extractMsg = '';
    try {
    const res = await kbApi.wikiExtractAll(kbId);
      extractMsg = res.submitted === 0 ? '所有页面均已提取过摘要与实体' : `已提交 ${res.submitted} 个页面的摘要与实体提取`;
    } catch (e: unknown) { extractMsg = '批量提取失败：' + e; }
    finally { extractBusy = false; }
  }

  // ─── 加载 / 知识库切换 ───
  $effect(() => {
    const id = kbId;
    if (id !== null) {
      // 进入 Wiki 默认落在「页面列表」，避免直接停在空图谱/图谱详情造成困惑；
      // 图谱由列表页「图谱」按钮或图谱视图内部进入。
      view = 'list'; detail = null; graph = null; genMsg = ''; err = '';
      dirFilter = '';
      // untrack：loadGraphData 内部读写 graphBusy，若被跟踪会导致
      // 「加载完成 → graphBusy 变化 → 副作用重跑 → 再次加载」的无限循环
      untrack(() => {
        loadGraphData();
        loadPages();
        loadDirs();
      });
    }
  });

  async function loadPages() {
    if (kbId === null) { pages = []; return; }
    loading = true; err = '';
    try {
    pages = await kbApi.wikiListPages(kbId);
    } catch (e: unknown) {
      err = '加载 Wiki 页面失败：' + e;
    } finally {
      loading = false;
    }
  }

  async function doSearch() {
    if (kbId === null) return;
    const q = searchText.trim();
    if (!q) { loadPages(); return; }
    searching = true; err = '';
    try {
    pages = await kbApi.wikiSearch(kbId, q, 30);
    } catch (e: unknown) {
      err = '搜索失败：' + e;
    } finally {
      searching = false;
    }
  }

  function statusLabel(s: string): string {
    const map: Record<string, string> = { draft: '草稿', published: '已发布', archived: '已归档', ready: '就绪' };
    return map[s] ?? s;
  }

  function fmtTime(t: string): string {
    return formatIsoTime(t, { showYear: true, utc: true });
  }

  // ─── 详情 ───
  async function openDetail(id: number) {
    // 记录返回目标：从图谱工作区（含目录树子页）进入 → 返回图谱；从列表进入 → 返回列表
    if (view === 'graph') detailBack = 'graph';
    else if (view === 'list') detailBack = 'list';
    view = 'detail'; detailLoading = true; err = '';
    try {
    detail = await kbApi.wikiGetPage(id);
    } catch (e: unknown) {
      err = '读取页面失败：' + e;
      view = 'list';
    } finally {
      detailLoading = false;
    }
  }

  async function openLink(l: { pageId: number }) {
    await openDetail(l.pageId);
  }

  async function deletePage() {
    if (!detail) return;
    if (!confirm(`删除页面「${detail.title}」？此操作不可恢复。`)) return;
    try {
    await kbApi.wikiDeletePage(detail.id);
      detail = null;
      view = detailBack;
      loadPages();
    } catch (e: unknown) {
      err = '删除失败：' + e;
    }
  }

  // ─── 编辑 ───
  function startCreate() {
    detailBack = view === 'graph' ? 'graph' : 'list';
    editBack = detailBack;
    editingId = null;
    editTitle = ''; editSummary = ''; editContent = '';
    view = 'edit';
  }

  async function loadDirs() {
    if (kbId === null) { wikiDirs = []; return; }
    try { wikiDirs = await kbApi.wikiDirs(kbId); }
    catch { wikiDirs = []; }
  }

  function startCreateWithTitle(title?: string) {
    detailBack = view === 'graph' ? 'graph' : 'list';
    editBack = detailBack;
    editingId = null;
    editTitle = title ?? '';
    editSummary = ''; editContent = '';
    view = 'edit';
  }

  function startEdit() {
    if (!detail) return;
    editingId = detail.id;
    editTitle = detail.title; editSummary = detail.summary || ''; editContent = detail.contentMd || '';
    view = 'edit';
  }

  function cancelEdit() {
    view = editingId === null ? editBack : 'detail';
  }

  async function savePage() {
    if (kbId === null || saveBusy) return;
    const title = editTitle.trim();
    if (!title) { err = '标题不能为空'; return; }
    saveBusy = true; err = '';
    try {
      const input = { kbId, title, summary: editSummary, contentMd: editContent };
      if (editingId === null) {
    const newId = await kbApi.wikiCreatePage(input);
        loadPages();
        await openDetail(newId);
      } else {
    await kbApi.wikiUpdatePage(editingId, input);
        loadPages();
        await openDetail(editingId);
      }
    } catch (e: unknown) {
      err = '保存失败：' + e;
    } finally {
      saveBusy = false;
    }
  }

  // ─── 图谱 ───
  async function loadGraphData() {
    if (kbId === null || graphBusy) return;
    graphBusy = true; err = '';
    try {
      graph = await kbApi.wikiGraph(kbId);
      graphSelect = null;
    } catch (e: unknown) {
      err = '加载图谱失败：' + e;
    } finally {
      graphBusy = false;
    }
  }
  // 列表页自动加载图谱数据，供「Wiki 图谱」面板展示缩略图
  $effect(() => {
    if (view === 'list' && kbId !== null && !graph && !graphBusy) {
      loadGraphData();
    }
  });


  // ─── LLM 提炼 ───
  async function doGenerate() {
    if (kbId === null || genBusy) return;
    if (!confirm('将调用 LLM 提炼知识库中已就绪的文档为 Wiki 页面，已存在的页面会自动合并。\n\n是否继续？（可能耗时较长）')) return;
    genBusy = true; genMsg = '';
    try {
    const res = await kbApi.wikiGenerate({ kbId, providerId: null, model: null });
      genMsg = `已提交 ${res.submitted} 个文档的后台提炼，可在「活动 → 处理任务」查看进度`;
    } catch (e: unknown) {
      genMsg = '生成失败：' + e;
    } finally {
      genBusy = false;
    }
  }

  let renderedMd = $state('');

  // 详情 Markdown 渲染（渲染结果缓存）
  $effect(() => {
    if (view === 'detail' && detail) {
      renderedMd = renderMd(detail.contentMd);
    }
  });

  // Wiki 链接点击（事件委托）
  // 事件委托容器：点击与键盘 Enter 共用（仅使用 e.target 定位链接）
  function onDetailClick(e: Event) {
    const el = (e.target as HTMLElement).closest('[data-wiki-page]') as HTMLElement | null;
    if (!el) return;
    const title = el.getAttribute('data-wiki-page');
    if (title) jumpToPage(title);
  }

  async function jumpToPage(title: string) {
    if (kbId === null) return;
    const hit = pages.find((p) => p.title === title || p.slug === title);
    if (hit) { await openDetail(hit.id); return; }
    err = '';
    try {
    const res = await kbApi.wikiSearch(kbId, title, 8);
      const found = res.find((p) => p.title === title || p.slug === title);
      if (found) {
        await openDetail(found.id);
      } else {
        err = `页面「${title}」不存在`;
      }
    } catch (e: unknown) {
      err = '跳转失败：' + e;
    }
  }
</script>

{#if kbId === null}
  <div class="kb-wiki-empty">请先在顶栏选择一个知识库</div>
{:else}
  <div style="flex:1;min-height:0;display:flex;flex-direction:column;gap:10px">
  {#if view === 'list' || view === 'graph'}
    <!-- Wiki 工作区子页：页面 / 图谱（进入默认落在页面，图谱由子页签进入） -->
    <div style="display:flex;align-items:center;gap:10px;flex:none">
      <div class="kb-seg kb-seg-tabs">
        <button class="kb-seg-item" class:active={view === 'list'} onclick={() => view = 'list'}><KbIcon name="list" size={14} />页面（{pages.length}）</button>
        <button class="kb-seg-item" class:active={view === 'graph'} onclick={() => { view = 'graph'; if (!graph && !graphBusy) loadGraphData(); }}><KbIcon name="graph" size={14} />图谱</button>
      </div>
      <div style="flex:1"></div>
      <span style="font-size:12px;color:var(--kb-text-3)">{view === 'list' ? 'Wiki 页面：由文档提炼或手动创建，支持双链与知识图谱' : '图谱：拖动平移 · 滚轮缩放 · 双击打开页面'}</span>
    </div>
  {/if}
  {#if view === 'list'}
  <div style="display:flex;gap:14px;flex:1;min-height:0">
  <!-- 左：目录树 -->
  <div style="flex:none;width:264px;display:flex;flex-direction:column;gap:14px;min-height:0">
    <div class="kb-card" style="flex:1;min-height:0;display:flex;flex-direction:column">
      <div class="kb-card-hd"><KbIcon name="folder" size={15} color="var(--kb-accent-bright)" />目录树</div>
      <div class="kb-scroll" style="flex:1;overflow:auto;padding:8px">
        <button class="kb-dir-item" class:active={dirFilter === ''} onclick={() => dirFilter = ''}>
          <KbIcon name="folderOpen" size={13} />全部页面（{pages.length}）
        </button>
        {#each dirTree as d}
          <button class="kb-dir-item" style="padding-left:{10 + d.depth * 16}px" class:active={dirFilter === d.id} onclick={() => dirFilter = d.id}>
            <KbIcon name="folder" size={13} />{d.name}（{d.count}）
          </button>
        {/each}
        {#if wikiDirs.length === 0}
          <div style="font-size:11.5px;color:var(--kb-text-3);text-align:center;padding:12px">暂无目录</div>
        {/if}
      </div>
    </div>
  </div>
  <!-- 右：页面列表 -->
  <div style="flex:1;min-width:0;display:flex;flex-direction:column;min-height:0">
  <div class="kb-wiki-head">
    <span class="kb-wiki-title"><KbIcon name="wiki" size={17} color="var(--kb-accent-bright)" />Wiki 页面</span>
    <div class="kb-wiki-head-btns">
      <button class="kb-btn-sm" onclick={doGenerate} disabled={genBusy}><KbIcon name="sparkle" size={13} />{genBusy ? '提炼中…' : '提炼'}</button>
      <button class="kb-btn-sm" onclick={doExtractAll} disabled={extractBusy}><KbIcon name="list" size={13} />{extractBusy ? '提取中…' : '摘要/实体'}</button>
      <button class="kb-btn-sm kb-wiki-new" onclick={startCreate}><KbIcon name="plus" size={13} weight="bold" />新建</button>
    </div>
  </div>

  <div class="kb-wiki-search">
    <input
      class="kb-input"
      placeholder="搜索 Wiki 页面（BM25 全文）"
      bind:value={searchText}
      onkeydown={(e) => e.key === 'Enter' && doSearch()}
    />
    <select class="kb-select" style="width:auto" bind:value={dirFilter}>
      <option value={''}>全部目录</option>
      {#each wikiDirs as d}
        <option value={d.id}>{d.name}（{d.count}）</option>
      {/each}
    </select>
    <button class="kb-btn" onclick={doSearch} disabled={searching}>{searching ? '…' : '搜索'}</button>
  </div>

  {#if genMsg}<div class="kb-wiki-gen">{genMsg}</div>{/if}
  {#if extractMsg}<div class="kb-wiki-gen">{extractMsg}</div>{/if}
  {#if err}<div class="kb-wiki-err">{err}</div>{/if}

  <div class="kb-wiki-list">
    {#if loading}
      <div class="kb-empty">加载中…</div>
    {:else if pages.length === 0}
      <div class="kb-empty">
        {searchText.trim() ? '未找到匹配页面' : '暂无 Wiki 页面'}
        {#if !searchText.trim()}
          <div class="kb-empty-sub">点击「提炼」从文档自动生成，或「新建」手动创建</div>
        {/if}
      </div>
    {:else}
      {#each filteredPages as p}
        <button class="kb-wiki-item" type="button" onclick={() => openDetail(p.id)}>
          <div class="kb-wiki-item-title">
            <span class="kb-wiki-item-name">{p.title}</span>
            <span class="kb-wiki-item-status" class:draft={p.status === 'draft'}>{statusLabel(p.status)}</span>
          </div>
          {#if p.summary}<div class="kb-wiki-item-summary">{p.summary}</div>{/if}
          <div class="kb-wiki-item-meta">
            {#if p.docTitle}<span class="kb-wiki-item-doc"><KbIcon name="file" size={12} />{p.docTitle}</span>{/if}
            <span><KbIcon name="link" size={12} />{p.outLinks}</span>
            <span><KbIcon name="arrowLeft" size={12} />{p.inLinks}</span>
            {#if p.entityCount > 0}<span><KbIcon name="list" size={12} />实体 {p.entityCount}</span>{/if}
            <span class="kb-wiki-item-time">{fmtTime(p.updatedAt)}</span>
          </div>
        </button>
      {/each}
    {/if}
  </div>
  </div>
  </div>
{:else if view === 'detail'}
  {#if detailLoading}
    <div class="kb-wiki-empty">读取中…</div>
  {:else if detail}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions —— 详情链接事件委托容器，链接本身可聚焦 -->
    <div
      class="kb-wiki-detail"
      role="document"
      tabindex="-1"
      onclick={onDetailClick}
      onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); onDetailClick(e); } }}
    >
      <div class="kb-wiki-detail-head">
        <button class="kb-btn-sm" onclick={() => { view = detailBack; detail = null; }}><KbIcon name="arrowLeft" size={13} />返回</button>
        <div class="kb-wiki-detail-btns">
          <button class="kb-btn-sm" onclick={doExtract} disabled={extractBusy} title="用 LLM 提取摘要与实体">
            <KbIcon name="list" size={13} />{extractBusy ? '提取中…' : '摘要/实体'}
          </button>
          <button class="kb-btn-sm" onclick={startEdit}><KbIcon name="edit" size={13} />编辑</button>
          <button class="kb-btn-sm kb-dang" onclick={deletePage}><KbIcon name="trash" size={13} />删除</button>
        </div>
      </div>
      <h1 class="kb-wiki-detail-title">{detail.title}</h1>
      <div class="kb-wiki-detail-meta">
        <span class="kb-wiki-item-status" class:draft={detail.status === 'draft'}>{statusLabel(detail.status)}</span>
        {#if detail.docTitle}<span><KbIcon name="file" size={12} />来源：{detail.docTitle}</span>{/if}
        {#if detail.extractStatus === 'pending'}<span class="kb-badge kb-badge-warn">摘要/实体提取中…</span>
        {:else if detail.extractStatus === 'failed'}<span class="kb-badge kb-badge-err">摘要/实体提取失败</span>
        {:else if detail.extractStatus === 'done'}<span class="kb-badge kb-badge-ok">已提取摘要/实体</span>{/if}
        <span>更新于 {fmtTime(detail.updatedAt)}</span>
      </div>
      {#if err}<div class="kb-wiki-err">{err}</div>{/if}
      {#if extractMsg}<div class="kb-wiki-gen">{extractMsg}</div>{/if}
      {#if detail.summary}
        <div class="kb-wiki-detail-summary">{detail.summary}</div>
      {/if}
      {#if detail.contentMd && detail.contentMd.trim()}
        <div class="wiki-md-body">{@html renderedMd}</div>
      {:else}
        <div class="kb-empty">（空白页面，点击「编辑」撰写内容）</div>
      {/if}

      {#if detail.entities.length > 0}
        <div class="kb-wiki-links">
          <div class="kb-wiki-links-title"><KbIcon name="list" size={13} />实体（{detail.entities.length}）</div>
          <div class="kb-wiki-links-row">
            {#each detail.entities as en}
              <span class="kb-wiki-entity" title={en.description ?? en.name}>
                <span class="kb-wiki-entity-name">{en.name}</span>
                {#if en.entityType}<span class="kb-wiki-entity-type">{en.entityType}</span>{/if}
              </span>
            {/each}
          </div>
        </div>
      {/if}

      {#if detail.outLinks.length > 0}
        <div class="kb-wiki-links">
          <div class="kb-wiki-links-title"><KbIcon name="arrowRight" size={13} />本页引用（{detail.outLinks.length}）</div>
          <div class="kb-wiki-links-row">
            {#each detail.outLinks as l}
              <button class="kb-wiki-chip" type="button" title={l.snippet ?? ''} onclick={() => openLink(l)}>[[{l.title}]]</button>
            {/each}
          </div>
        </div>
      {/if}
      {#if detail.inLinks.length > 0}
        <div class="kb-wiki-links">
          <div class="kb-wiki-links-title"><KbIcon name="arrowLeft" size={13} />反向链接（{detail.inLinks.length}）</div>
          <div class="kb-wiki-backlinks">
            {#each detail.inLinks as l}
              <button class="kb-wiki-backlink" type="button" onclick={() => openLink(l)}>
                <span class="kb-wiki-backlink-title"><KbIcon name="link" size={12} />{l.title}</span>
                {#if l.snippet}<span class="kb-wiki-backlink-snippet">{l.snippet}</span>{/if}
              </button>
            {/each}
          </div>
        </div>
      {/if}
      {#if detail.unlinkedMentions.length > 0}
        <div class="kb-wiki-links">
          <div class="kb-wiki-links-title"><KbIcon name="search" size={13} />提及但未链接（{detail.unlinkedMentions.length}）</div>
          <div class="kb-wiki-backlinks">
            {#each detail.unlinkedMentions as l}
              <button class="kb-wiki-backlink" type="button" onclick={() => openLink(l)}>
                <span class="kb-wiki-backlink-title"><KbIcon name="fileDashed" size={12} />{l.title}</span>
                {#if l.snippet}<span class="kb-wiki-backlink-snippet">{l.snippet}</span>{/if}
              </button>
            {/each}
          </div>
        </div>
      {/if}
      {#if detail.unresolved.length > 0}
        <div class="kb-wiki-links">
          <div class="kb-wiki-links-title"><KbIcon name="fileDashed" size={13} />失效链接（待创建）</div>
          <div class="kb-wiki-links-row">
            {#each detail.unresolved as t}
              <button class="kb-wiki-chip kb-wiki-chip-missing" type="button" title="点击创建该页面"
                onclick={() => startCreateWithTitle(t)}>[[{t}]]</button>
            {/each}
          </div>
        </div>
      {/if}
    </div>
  {:else}
    <div class="kb-wiki-empty">页面不存在</div>
  {/if}
{:else if view === 'edit'}
  <div class="kb-wiki-edit">
    <div class="kb-wiki-edit-head">
      <span class="kb-wiki-edit-title">{editingId === null ? '新建页面' : '编辑页面'}</span>
      <button class="kb-btn-sm" onclick={cancelEdit} disabled={saveBusy}>取消</button>
    </div>
    {#if err}<div class="kb-wiki-err">{err}</div>{/if}
    <input class="kb-input kb-wiki-edit-title-input" placeholder="页面标题" bind:value={editTitle} />
    <input class="kb-input" placeholder="摘要（可选）" bind:value={editSummary} />
    <div style="position:relative">
      <textarea
        class="kb-input kb-wiki-edit-content"
        placeholder="Markdown 内容（输入 [[ 可引用已有页面）"
        bind:this={editTextarea}
        bind:value={editContent}
        oninput={onEditInput}
        onkeydown={onEditKeydown}
        onblur={() => setTimeout(() => { if (wikiSuggest) wikiSuggest = null; }, 150)}
      ></textarea>
      {#if wikiSuggest && wikiSuggest.items.length > 0}
        <div style="position:absolute;left:8px;right:8px;bottom:10px;max-height:200px;overflow:auto;background:var(--app-bg-color);border:1px solid var(--kb-border-strong);border-radius:8px;box-shadow:0 8px 24px rgba(0,0,0,.4);z-index:10">
          {#each wikiSuggest.items as p, i}
            <button type="button" style="display:block;width:100%;text-align:left;padding:7px 10px;border:none;background:{i === suggestIdx ? 'var(--kb-hover-strong)' : 'transparent'};color:var(--kb-text);font-size:12.5px;cursor:pointer;font-family:inherit"
              onmousedown={(e) => e.preventDefault()}
              onclick={() => { suggestIdx = i; acceptSuggestion(); }}>
              <span style="color:var(--kb-accent-bright)">[[{p.title}]]</span>
              {#if p.docTitle}<span style="color:var(--kb-text-3);font-size:11.5px"> · {p.docTitle}</span>{/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>
    <div class="kb-wiki-edit-foot">
      <button class="kb-btn" onclick={savePage} disabled={saveBusy}>{saveBusy ? '保存中…' : '保存'}</button>
    </div>
  </div>
{:else if view === 'graph'}
  <div style="flex:1;min-height:0;display:flex;flex-direction:column">
  <div class="kb-wiki-graph-view" style="flex:1;min-width:0">
    <div class="kb-wiki-graph-head">
      <div style="display:flex;gap:6px;align-items:center">
        {#if graph && graph.nodes.length > 0}
          <button class="kb-btn-sm" onclick={() => graphCanvasRef?.relayout()} title="按社区重新布局"><KbIcon name="refresh" size={13} />重新布局</button>
          <button class="kb-btn-sm" onclick={() => graphCanvasRef?.resetView()} title="重置视图"><KbIcon name="arrowsOut" size={12} /></button>
          <button class="kb-btn-sm" class:kb-graph-cfg-on={graphCfgOpen} onclick={() => graphCfgOpen = !graphCfgOpen} title="图谱设置"><KbIcon name="settings" size={15} /></button>
        {/if}
        <button class="kb-btn-sm" onclick={startCreate}><KbIcon name="plus" size={13} weight="bold" />新建</button>
      </div>
    </div>
    {#if err}<div class="kb-wiki-err">{err}</div>{/if}
    {#if graph && graph.nodes.length > 0}
      <div style="display:flex;align-items:center;gap:10px;flex-wrap:wrap;margin-bottom:8px;flex:none">
        <span class="kb-wiki-graph-stats">显示 {graphVisible.size} / {graph.nodes.length} 个页面 · {builtGraph.edges.length} 条链接</span>
        <!-- 图例：按社区着色时展示社区簇，否则展示节点类型 -->
        {#if graphParams.colorByCommunity}
          <div style="display:flex;gap:10px;align-items:center;flex-wrap:wrap">
            {#each Array.from({ length: builtGraph.communityCount }, (_, i) => i) as c}
              <span style="display:inline-flex;align-items:center;gap:4px;font-size:11.5px;color:var(--kb-text-3)" title="社区簇 {c + 1}">
                <span style="width:9px;height:9px;border-radius:50%;background:{communityColor(c)};display:inline-block"></span>簇 {c + 1}
              </span>
            {/each}
            {#if builtGraph.nodes.some((n) => n.community < 0)}
              <span style="display:inline-flex;align-items:center;gap:4px;font-size:11.5px;color:var(--kb-text-3)" title="未分组节点（无连接）">
                <span style="width:9px;height:9px;border-radius:50%;background:{communityColor(-1)};display:inline-block"></span>未分组
              </span>
            {/if}
          </div>
        {:else}
          <div style="display:flex;gap:10px;align-items:center;flex-wrap:wrap">
            {#each nodeTypeStats as [t, c]}
              <span style="display:inline-flex;align-items:center;gap:4px;font-size:11.5px;color:var(--kb-text-3)" title="节点类型：{t}">
                <span style="width:9px;height:9px;border-radius:50%;background:{NODE_TYPE_COLORS[t] ?? '#8d99ae'};display:inline-block"></span>{t} {c}
              </span>
            {/each}
          </div>
        {/if}
        <div style="flex:1"></div>
        {#if graphLinkTypes.length > 0}
          <div style="display:flex;gap:10px;align-items:center">
            {#each graphLinkTypes as lt}
              <span style="display:inline-flex;align-items:center;gap:4px;font-size:11.5px;color:var(--kb-text-3)">
                <span style="width:14px;height:0;border-top:{lt === 'entity' ? '2px dashed' : '3px solid'} {edgeColor(lt)};border-radius:2px;display:inline-block"></span>
                {lt === 'entity' ? '实体共享' : lt}
              </span>
            {/each}
          </div>
        {/if}
      </div>
      <div style="flex:1;display:flex;gap:10px;min-height:0">
        <div style="flex:1;min-width:0;min-height:0;position:relative">
          <WikiGraphCanvas
            bind:this={graphCanvasRef}
            graph={builtGraph}
            selectedId={graphSelect}
            onSelect={(n) => {
              graphSelect = n ? n.pageId : null;
              if (n && n.pageId > 0) track('wiki_graph_click', { kbId, pageId: n.pageId });
            }}
            onOpen={(n) => {
              if (n.status === 'missing') { graphCfgOpen = false; startCreateWithTitle(n.label); }
              else openDetail(n.pageId);
            }}
            settings={{
              nodeScale: graphParams.nodeScale,
              edgeWidth: graphParams.edgeWidth,
              edgeOpacity: graphParams.edgeOpacity,
              showLabels: graphParams.showLabels,
              labelOpacity: graphParams.labelOpacity,
              motion: graphParams.motion,
              showArrows: graphParams.showArrows,
              forceCentripetal: graphParams.forceCentripetal,
              forceRepulsion: graphParams.forceRepulsion,
              forceAttraction: graphParams.forceAttraction,
              forceEdgeLength: graphParams.forceEdgeLength,
            }}
            nodeColor={wikiNodeColor}
            edgeColor={(e: WEdge) => edgeColor(e.linkType)}
            edgeDash={(e: WEdge) => e.linkType === 'entity'}
            tooltip={tooltipFor}
            redrawKey={graphRedrawKey}
          />
          {#if graphCfgOpen}
            <div style="position:absolute;top:0;right:0;bottom:0;z-index:4;width:300px;background:var(--kb-surface-2);border-left:1px solid var(--kb-border-strong);display:flex;flex-direction:column">
              <div style="display:flex;align-items:center;gap:8px;padding:10px 12px;border-bottom:1px solid var(--kb-border);flex:none">
                <span style="font-size:13px;font-weight:600;color:var(--kb-text)">图谱设置</span>
                <div style="flex:1"></div>
                <button class="kb-btn-sm" onclick={resetGraphParams} title="重置所有更改"><KbIcon name="refresh" size={12} />重置</button>
                <button class="kb-btn-sm kb-btn-ghost" onclick={() => graphCfgOpen = false}><KbIcon name="close" size={14} /></button>
              </div>
              <div class="kb-scroll" style="flex:1;overflow:auto;padding:12px;display:flex;flex-direction:column;gap:16px">
                <!-- 筛选 -->
                <div class="kb-graph-cfg-group">
                  <div class="kb-graph-cfg-title">筛选</div>
                  <div style="display:flex;flex-direction:column;gap:10px;align-items:stretch">
                    <label class="kb-label" style="min-width:0">搜索框
                      <input class="kb-input" placeholder="搜索笔记（按标题 / 来源）…" bind:value={graphFilter} />
                    </label>
                    <label class="kb-check"><Checkbox checked={localOnly} onCheckedChange={(c) => (localOnly = !!c)} />仅显示邻居</label>
                    <label class="kb-check"><Checkbox checked={graphParams.createdOnly}
                      onCheckedChange={(c) => { graphParams.createdOnly = !!c; saveGraphParams(); }} />仅显示已创建的笔记</label>
                    <label class="kb-check"><Checkbox checked={graphParams.showOrphans}
                      onCheckedChange={(c) => { graphParams.showOrphans = !!c; saveGraphParams(); }} />孤立文件（无任何链接的笔记）</label>
                    <label class="kb-label" style="min-width:0">忽略文件（每行一个，* 通配）
                      <textarea class="kb-textarea" rows="2" placeholder="草稿*&#10;*待整理*" bind:value={graphParams.ignorePatterns} onchange={saveGraphParams}></textarea>
                    </label>
                    <label class="kb-check" title="共享同一实体的笔记即使没有显式链接，也会以虚线相连"><Checkbox checked={graphParams.showImplicit}
                      onCheckedChange={(c) => { graphParams.showImplicit = !!c; saveGraphParams(); }} />隐含关系（共享实体）</label>
                  </div>
                </div>
                <!-- 颜色组 -->
                <div class="kb-graph-cfg-group">
                  <div class="kb-graph-cfg-title" style="display:flex;justify-content:space-between;align-items:center">
                    <span>颜色组</span>
                    <button class="kb-btn-sm" onclick={addColorGroup}><KbIcon name="plus" size={12} />新建颜色组</button>
                  </div>
                  {#each graphParams.colorGroups ?? [] as g, gi}
                    <div style="display:flex;flex-direction:column;gap:8px;padding:8px;border:1px solid var(--kb-border-subtle);border-radius:6px;background:var(--app-bg-color)">
                      <div style="display:flex;gap:6px;align-items:center">
                        <input class="kb-input" style="flex:1;height:28px;font-size:12px" placeholder="搜索词，如：产品、AI…" bind:value={g.query} onchange={saveGraphParams} />
                        <button class="kb-btn-sm kb-btn-ghost" onclick={() => removeColorGroup(gi)} title="删除该颜色组"><KbIcon name="close" size={12} /></button>
                      </div>
                      <div style="display:flex;gap:6px;flex-wrap:wrap;align-items:center">
                        {#each COLOR_PALETTE as c}
                          <button onclick={() => { g.color = c; saveGraphParams(); }} aria-label="选择颜色"
                            style="width:18px;height:18px;border-radius:50%;background:{c};border:2px solid {g.color === c ? 'var(--kb-accent-bright)' : 'transparent'};cursor:pointer;padding:0"></button>
                        {/each}
                      </div>
                    </div>
                  {/each}
                  {#if (graphParams.colorGroups ?? []).length === 0}
                    <div style="font-size:11.5px;color:var(--kb-text-3);line-height:1.6">按搜索词将笔记分组并染上指定颜色，突出知识库中的主题簇。</div>
                  {/if}
                </div>
                <!-- 外观 -->
                <div class="kb-graph-cfg-group">
                  <div class="kb-graph-cfg-title">外观</div>
                  <div class="kb-graph-cfg-items" style="flex-direction:column;align-items:stretch;gap:10px">
                    <label class="kb-label kb-graph-slider-label">节点大小
                      <Slider type="multiple" min={0.6} max={1.8} step={0.05} value={[graphParams.nodeScale]} onValueChange={(v: number[]) => { graphParams.nodeScale = v[0]; saveGraphParams(); }} />
                    </label>
                    <label class="kb-label kb-graph-slider-label">连线粗细
                      <Slider type="multiple" min={0.5} max={3} step={0.1} value={[graphParams.edgeWidth]} onValueChange={(v: number[]) => { graphParams.edgeWidth = v[0]; saveGraphParams(); }} />
                    </label>
                    <label class="kb-label kb-graph-slider-label">文本透明度
                      <Slider type="multiple" min={0.2} max={1} step={0.05} value={[graphParams.labelOpacity]} onValueChange={(v: number[]) => { graphParams.labelOpacity = v[0]; saveGraphParams(); }} />
                    </label>
                    <label class="kb-check"><Checkbox checked={graphParams.showArrows}
                      onCheckedChange={(c) => { graphParams.showArrows = !!c; saveGraphParams(); }} />箭头</label>
                    <label class="kb-check"><Checkbox checked={graphParams.showLabels}
                      onCheckedChange={(c) => { graphParams.showLabels = !!c; saveGraphParams(); }} />显示标签</label>
                    <label class="kb-check"><Checkbox checked={graphParams.motion}
                      onCheckedChange={(c) => { graphParams.motion = !!c; saveGraphParams(); }} />播放动画</label>
                    <label class="kb-check" title="按社区簇着色（与微信社交图谱观感一致）"><Checkbox checked={graphParams.colorByCommunity}
                      onCheckedChange={(c) => { graphParams.colorByCommunity = !!c; saveGraphParams(); }} />按社区着色</label>
                  </div>
                </div>
                <!-- 力度 -->
                <div class="kb-graph-cfg-group">
                  <div class="kb-graph-cfg-title">力度</div>
                  <div style="font-size:11.5px;color:var(--kb-text-3);line-height:1.6">控制作用于每个节点的力量：数值越高，图谱越紧凑或越松散。</div>
                  <div class="kb-graph-cfg-items" style="flex-direction:column;align-items:stretch;gap:10px">
                    <label class="kb-label kb-graph-slider-label">图谱向心力
                      <Slider type="multiple" min={0} max={0.1} step={0.005} value={[graphParams.forceCentripetal]} onValueChange={(v: number[]) => { graphParams.forceCentripetal = v[0]; saveGraphParams(); }} />
                    </label>
                    <label class="kb-label kb-graph-slider-label">节点间排斥力
                      <Slider type="multiple" min={500} max={6000} step={100} value={[graphParams.forceRepulsion]} onValueChange={(v: number[]) => { graphParams.forceRepulsion = v[0]; saveGraphParams(); }} />
                    </label>
                    <label class="kb-label kb-graph-slider-label">相连节点吸引力
                      <Slider type="multiple" min={0.005} max={0.12} step={0.005} value={[graphParams.forceAttraction]} onValueChange={(v: number[]) => { graphParams.forceAttraction = v[0]; saveGraphParams(); }} />
                    </label>
                    <label class="kb-label kb-graph-slider-label">连线长度
                      <Slider type="multiple" min={0.5} max={2} step={0.05} value={[graphParams.forceEdgeLength]} onValueChange={(v: number[]) => { graphParams.forceEdgeLength = v[0]; saveGraphParams(); }} />
                    </label>
                  </div>
                </div>
              </div>
            </div>
          {/if}
          {#if graphSelect !== null}
            {@const sel = graph.nodes.find((n) => n.id === graphSelect)}
            {#if sel}
              {@const implCount = graph.edges.filter((e) => e.linkType === 'entity' && (e.from === sel.id || e.to === sel.id)).length}
              <div style="position:absolute;top:10px;{graphCfgOpen ? 'left:10px;right:auto' : 'right:10px'};z-index:2;width:220px;border:1px solid var(--kb-border-strong);border-radius:10px;padding:12px;background:var(--kb-surface-2);box-shadow:var(--kb-shadow)">
                <div style="font-size:13.5px;font-weight:600;color:var(--kb-text);word-break:break-word">{sel.title}</div>
                {#if sel.status === 'missing'}<div style="font-size:11.5px;color:var(--kb-text-3);margin-top:3px">尚未创建的笔记</div>
                {:else if sel.docTitle}<div style="font-size:11.5px;color:var(--kb-text-3);margin-top:3px">来源：{sel.docTitle}</div>{/if}
                <div style="display:flex;gap:8px;margin-top:8px;flex-wrap:wrap">
                  <span class="kb-badge kb-badge-info">入链 {sel.inDegree}</span>
                  <span class="kb-badge kb-badge-mute">出链 {sel.outDegree}</span>
                  {#if implCount > 0}
                    <span class="kb-badge" style="color:#4fd1c5;border-color:color-mix(in srgb,#4fd1c5 45%,var(--app-bg-color))">实体关联 {implCount}</span>
                  {/if}
                </div>
                {#if sel.status === 'missing'}
                  <button class="kb-btn kb-btn-sm" style="width:100%;margin-top:10px" onclick={() => { graphCfgOpen = false; startCreateWithTitle(sel.title); }}>
                    <KbIcon name="plus" size={13} weight="bold" />创建该笔记
                  </button>
                {:else}
                  <button class="kb-btn kb-btn-sm" style="width:100%;margin-top:10px" onclick={() => openDetail(sel.pageId)}>
                    <KbIcon name="arrowRight" size={13} weight="bold" />打开页面
                  </button>
                {/if}
              </div>
            {/if}
          {/if}
        </div>
      </div>
    {:else}
      <div class="kb-empty">
        暂无图谱数据
        <div class="kb-empty-sub">点击「提炼」从文档自动生成页面</div>
      </div>
    {/if}
  </div>
  </div>
  {/if}
  </div>

{/if}

<style>
  .kb-wiki-empty {
    color: var(--kb-text-3);
    font-size: 13px;
    text-align: center;
    padding: 40px 12px;
  }
  .kb-wiki-head {
    display: flex; justify-content: space-between; align-items: center;
    font-weight: 600; margin-bottom: 8px; color: var(--kb-text);
  }
  .kb-wiki-title { display: inline-flex; align-items: center; gap: 7px; }
  .kb-wiki-head-btns { display: flex; gap: 6px; }
  .kb-wiki-new { background: var(--kb-btn-bg, #145cb9); border-color: var(--kb-btn-bg, #145cb9); color: #fff; }
  .kb-wiki-new:hover { background: var(--kb-btn-bg-hover, #0f468f); border-color: var(--kb-btn-bg-hover, #0f468f); color: #fff; }
  .kb-wiki-search { display: flex; gap: 6px; margin-bottom: 8px; }
  .kb-wiki-search .kb-input { flex: 1; }
  .kb-wiki-gen {
    font-size: 12px; color: var(--kb-ok, #7bd95c); background: var(--app-bg-color);
    border: 1px solid color-mix(in srgb, var(--app-success, #52c41a) 44%, var(--app-bg-color)); border-radius: var(--kb-radius-sm, 6px); padding: 6px 8px; margin-bottom: 8px;
  }
  .kb-wiki-err {
    font-size: 12px; color: var(--kb-err, #ff8587); background: var(--app-bg-color);
    border: 1px solid color-mix(in srgb, var(--app-danger, #ff4d4f) 44%, var(--app-bg-color)); border-radius: var(--kb-radius-sm, 6px); padding: 6px 8px; margin-bottom: 8px;
    word-break: break-word;
  }
  .kb-wiki-list { display: flex; flex-direction: column; gap: 8px; }
  .kb-dir-item {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 6px 8px;
    margin-bottom: 2px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--kb-text-2);
    font-size: 12.5px;
    font-family: inherit;
    cursor: pointer;
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    transition: background .12s, color .12s;
  }
  .kb-dir-item:hover { background: var(--kb-hover); color: var(--kb-text); }
  .kb-dir-item.active { background: var(--kb-hover-strong); color: var(--kb-accent-bright); font-weight: 600; box-shadow: inset 2px 0 0 var(--kb-accent), inset 0 0 14px color-mix(in srgb, var(--app-accent) 7%, transparent); }
  .kb-wiki-item {
    text-align: left; display: block; width: 100%;
    background: var(--app-bg-color);
    border: 1px solid var(--kb-border);
    border-radius: var(--kb-radius-sm, 6px);
    padding: 8px 10px; cursor: pointer;
    font-family: inherit; color: var(--kb-text);
    transition: border-color .15s, box-shadow .15s;
  }
  .kb-wiki-item:hover {
    border-color: var(--kb-accent, #1a73e8); box-shadow: 0 2px 8px color-mix(in srgb, var(--app-accent) 22%, transparent);
  }
  .kb-wiki-item-title { display: flex; justify-content: space-between; align-items: center; gap: 8px; }
  .kb-wiki-item-name { font-weight: 600; font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .kb-wiki-item-status {
    flex-shrink: 0; font-size: 11.5px; padding: 1px 6px; border-radius: 10px;
    background: var(--app-bg-color); color: var(--kb-accent-bright); border: 1px solid color-mix(in srgb, var(--app-accent, #1a73e8) 48%, var(--app-bg-color));
  }
  .kb-wiki-item-status.draft { background: var(--app-bg-color); color: var(--kb-warn, #f0c05a); border-color: color-mix(in srgb, var(--app-warning, #faad14) 46%, var(--app-bg-color)); }
  .kb-wiki-item-summary {
    font-size: 12px; color: var(--kb-text-3);
    margin-top: 4px;     overflow: hidden; text-overflow: ellipsis; display: -webkit-box;
    -webkit-line-clamp: 2; -webkit-box-orient: vertical; line-clamp: 2;
  }
  .kb-wiki-item-meta {
    display: flex; gap: 8px; align-items: center; flex-wrap: wrap;
    font-size: 11.5px; color: var(--app-color-secondary, #999); margin-top: 6px;
  }
  .kb-wiki-item-meta span { display: inline-flex; align-items: center; gap: 3px; }
  .kb-wiki-item-doc { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 120px; }
  .kb-wiki-item-time { margin-left: auto; }
  .kb-empty-sub { font-size: 12px; color: var(--app-color-secondary, #999); margin-top: 6px; }

  /* ── 详情 ── */
  .kb-wiki-detail { padding-bottom: 8px; }
  .kb-wiki-detail-head {
    display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px;
  }
  .kb-wiki-detail-btns { display: flex; gap: 6px; }
  .kb-wiki-detail-title { font-size: 18px; margin: 4px 0 6px; color: var(--kb-text); word-break: break-word; }
  .kb-wiki-detail-meta {
    display: flex; align-items: center; gap: 10px; flex-wrap: wrap;
    font-size: 12px; color: var(--kb-text-3); margin-bottom: 10px;
  }
  .kb-wiki-detail-summary {
    font-size: 13px; color: var(--app-color-secondary, #555);
    background: var(--app-bg-color, #fafafa);
    /* impeccable-disable-next-line side-tab -- 摘要状态色刻线 */
    border-left: 3px solid var(--kb-accent, #1a73e8); border-radius: 4px;
    padding: 8px 10px; margin-bottom: 12px;
  }
  .kb-wiki-links { margin-top: 14px; }
  .kb-wiki-links-title { font-size: 12px; font-weight: 600; color: var(--kb-text-2); margin-bottom: 6px; }
  .kb-wiki-links-row { display: flex; flex-wrap: wrap; gap: 6px; }
  .kb-wiki-chip {
    font-size: 12px; font-family: inherit;
    background: var(--app-bg-color); border: 1px solid color-mix(in srgb, var(--app-accent, #1a73e8) 44%, var(--app-bg-color)); color: var(--kb-accent-bright, #6ea8ff);
    border-radius: 12px; padding: 2px 10px; cursor: pointer;
  }
  .kb-wiki-chip:hover { background: var(--kb-hover, #0a0f1e); }
  .kb-wiki-chip-missing { border-style: dashed; color: var(--app-color-secondary, #999); border-color: var(--kb-border-strong, #444); }
  .kb-wiki-chip-missing:hover { color: var(--kb-accent-bright, #6ea8ff); border-color: var(--kb-accent, #1a73e8); }
  .kb-wiki-backlinks { display: flex; flex-direction: column; gap: 6px; }
  .kb-wiki-backlink {
    display: flex; flex-direction: column; gap: 3px; width: 100%; text-align: left;
    background: var(--app-bg-color); border: 1px solid var(--kb-border); border-radius: 8px;
    padding: 7px 10px; cursor: pointer; font-family: inherit;
    transition: border-color .12s, background .12s;
  }
  .kb-wiki-backlink:hover { border-color: var(--kb-accent); background: var(--kb-hover); }
  .kb-wiki-backlink-title {
    display: inline-flex; align-items: center; gap: 5px;
    font-size: 12.5px; font-weight: 600; color: var(--kb-accent-bright, #6ea8ff);
  }
  .kb-wiki-backlink-snippet {
    font-size: 11.5px; color: var(--app-color-secondary, #999); line-height: 1.5;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .kb-wiki-entity {
    display: inline-flex; align-items: center; gap: 5px;
    background: var(--app-bg-color);
    border: 1px solid color-mix(in srgb, var(--app-accent, #1a73e8) 36%, var(--app-bg-color));
    border-radius: 8px; padding: 3px 8px; cursor: default;
  }
  .kb-wiki-entity-name { font-size: 12px; font-weight: 600; color: var(--kb-accent-bright, #6ea8ff); }
  .kb-wiki-entity-type {
    font-size: 11.5px; color: var(--app-color-secondary, #999);
    background: var(--kb-hover, #0a0f1e); border-radius: 6px; padding: 0 5px; line-height: 1.5;
  }

  /* ── Markdown ── */
  .wiki-md-body {
    font-size: 13px; line-height: 1.75; color: var(--kb-text);
    word-break: break-word;
  }
  .wiki-md-body :global(h1), .wiki-md-body :global(h2), .wiki-md-body :global(h3),
  .wiki-md-body :global(h4), .wiki-md-body :global(h5), .wiki-md-body :global(h6) {
    margin: 14px 0 8px; line-height: 1.4; color: var(--kb-text);
  }
  .wiki-md-body :global(h1) { font-size: 16px; }
  .wiki-md-body :global(h2) { font-size: 15px; }
  .wiki-md-body :global(h3) { font-size: 14px; }
  .wiki-md-body :global(h4), .wiki-md-body :global(h5), .wiki-md-body :global(h6) { font-size: 13px; }
  .wiki-md-body :global(p) { margin: 6px 0; }
  .wiki-md-body :global(ul), .wiki-md-body :global(ol) { margin: 6px 0; padding-left: 22px; }
  .wiki-md-body :global(li) { margin: 2px 0; }
  .wiki-md-body :global(code) {
    background: var(--app-bg-color, #f5f5f5); border: 1px solid var(--kb-border, #eee);
    border-radius: 4px; padding: 1px 5px; font-size: 12px; font-family: Consolas, monospace;
  }
  .wiki-md-body :global(pre.wiki-md-code) {
    background: #1e1e2e; color: #e5e7eb; border-radius: 8px; padding: 10px 12px;
    overflow-x: auto; margin: 8px 0;
  }
  .wiki-md-body :global(pre.wiki-md-code code) {
    background: none; border: none; padding: 0; color: inherit; font-size: 12px;
  }
  .wiki-md-body :global(blockquote.wiki-md-quote) {
    /* impeccable-disable-next-line side-tab -- markdown 引用块刻线 */
    border-left: 3px solid var(--kb-border-strong); margin: 8px 0; padding: 2px 0 2px 12px;
    color: var(--kb-text-2);
  }
  .wiki-md-body :global(hr.wiki-md-hr) { border: none; border-top: 1px solid var(--kb-border, #e0e0e0); margin: 12px 0; }
  .wiki-md-body :global(img.wiki-md-img) { max-width: 100%; border-radius: 8px; }
  .wiki-md-body :global(a.wiki-md-a) { color: var(--kb-accent-bright, #6ea8ff); text-decoration: none; }
  .wiki-md-body :global(a.wiki-md-a:hover) { text-decoration: underline; }
  .wiki-md-body :global(button.wiki-md-wl) {
    font-family: inherit; font-size: 13px; cursor: pointer;
    color: var(--kb-accent-bright, #6ea8ff); background: var(--app-bg-color); border: 1px solid color-mix(in srgb, var(--app-accent, #1a73e8) 44%, var(--app-bg-color));
    border-radius: 5px; padding: 0 6px; margin: 0 1px;
  }
  .wiki-md-body :global(button.wiki-md-wl:hover) { background: var(--kb-hover, #0a0f1e); }

  /* ── 编辑 ── */
  .kb-wiki-edit { display: flex; flex-direction: column; gap: 8px; }
  .kb-wiki-edit-head { display: flex; justify-content: space-between; align-items: center; font-weight: 600; }
  .kb-wiki-edit-title-input { font-weight: 600; }
  .kb-wiki-edit-content {
    min-height: 320px; resize: vertical; font-family: Consolas, 'Courier New', monospace;
    font-size: 12.5px; line-height: 1.6;
  }
  .kb-wiki-edit-foot { display: flex; justify-content: flex-end; }

  /* ── 图谱 ── */
  .kb-wiki-graph-view { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .kb-wiki-graph-head { display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px; flex: none; }
  .kb-wiki-graph-stats { font-size: 12px; color: var(--kb-text-3); }
  .kb-graph-cfg-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 12px;
    border: 1px solid var(--kb-border-subtle);
    border-radius: var(--kb-radius-sm, 6px);
    background: var(--app-bg-color);
  }
  .kb-graph-cfg-title {
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: .08em;
    color: var(--kb-text-3);
  }
  .kb-graph-cfg-items {
    display: flex;
    gap: 16px;
    flex-wrap: wrap;
    align-items: flex-end;
  }
  .kb-check {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    color: var(--kb-text-2);
    cursor: pointer;
    padding-bottom: 8px;
  }
  .kb-graph-slider-label { display: flex; flex-direction: column; gap: 8px; }
  .kb-graph-slider-label :global([data-slot="slider"]) { flex: none; }
  .kb-btn-sm.kb-graph-cfg-on { background: var(--kb-hover-strong); border-color: var(--kb-accent); color: var(--kb-accent-bright); }
</style>
