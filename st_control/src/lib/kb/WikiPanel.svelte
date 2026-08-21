<script lang="ts">
  import { kbApi } from './services/ipc';
  import { tick, untrack } from 'svelte';
  import { formatIsoTime } from '../format';
  import type { WikiPageItem, WikiPageDetail, WikiGraph, WikiDir } from './kbTypes';
  import { radialTreeLayout } from './graphLayout';
  import { edgeLinkTypes, graphNeighborSet, nodeDegreeMap, visibleNodeIds } from './graphUtils';
  import { buildDirSubtree, buildDirTree, filterPagesByDir } from './dirTreeUtils';
  import { renderMd } from './markdown';
  import { lsGet, lsSet } from '../storage';
  import {
    NODE_TYPE_COLORS,
    colorSlug,
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
  // Wiki 页子菜单：Wiki图谱 / 目录树
  // 记住用户上次使用的子视图（localStorage 持久化），切换知识库时保持不重置
  function loadWikiSub(): 'graph' | 'tree' {
    return lsGet('kb_wiki_sub') === 'tree' ? 'tree' : 'graph';
  }
  let wikiSub = $state<'graph' | 'tree'>(loadWikiSub());
  function setWikiSub(v: 'graph' | 'tree') {
    wikiSub = v;
    lsSet('kb_wiki_sub', v);
  }
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
  // 位置数据仅用于命令式 DOM 更新，不进入 Svelte 响应式渲染（避免每帧重建整个 SVG）
  let nodePos: Record<number, { x: number; y: number }> = {};
  let nodeBase: Record<number, { x: number; y: number }> = {};
  let nodeTarget = $state<Record<number, { x: number; y: number }>>({});
  let driftPhase: Record<number, number> = {};
  let svgEl = $state<SVGSVGElement | null>(null);
  let nodeEls = new Map<number, SVGGElement>();
  let lineEls: { el: SVGLineElement; from: number; to: number }[] = [];
  let graphSelect = $state<number | null>(null);
  let graphZoom = $state(1);
  let graphPan = $state({ x: 0, y: 0 });
  // 画布尺寸跟随容器（ResizeObserver 更新），保证图谱占满剩余空间
  let graphBox = $state({ w: 760, h: 520 });
  let graphWrapEl = $state<HTMLElement | null>(null);
  let draggingNode = $state<number | null>(null);
  // 拖拽起点：只有移动超过阈值才算拖拽（普通点击选中节点时节点绝不移动）
  let dragStart = $state<{ x: number; y: number; moved: boolean } | null>(null);
  let panning = $state(false);
  let panStart = $state({ x: 0, y: 0 });
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
    motion: true,                           // 灵动动画（布局过渡 + 漂浮漂移）
    showImplicit: true,                     // 显示隐含关系（共享实体）
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
  let relayoutTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    // 参数 / 画布尺寸变化时自动重排（防抖 120ms：拖动滑杆时不每帧全量重算布局）
    const p = graphParams;
    void p.forceRepulsion; void p.forceAttraction; void p.forceCentripetal; void p.forceEdgeLength;
    void graphBox.w; void graphBox.h;
    const g = graph;
    if (!g || g.nodes.length === 0) return;
    if (relayoutTimer) clearTimeout(relayoutTimer);
    relayoutTimer = setTimeout(() => runForceLayout(g), 120);
  });
  function saveGraphParams() {
    lsSet('kb_wiki_graph_params', JSON.stringify(graphParams));
  }
  function resetGraphParams() {
    graphParams = {
      nodeScale: 1, edgeWidth: 1.5, edgeOpacity: 0.85, showLabels: true, showArrows: true, labelOpacity: 0.9, motion: true,
      showImplicit: true, showOrphans: true, createdOnly: true, ignorePatterns: '', colorGroups: [],
      forceRepulsion: 2600, forceAttraction: 0.04, forceCentripetal: 0.02, forceEdgeLength: 1,
    };
    localOnly = false;
    graphFilter = '';
    saveGraphParams();
  }
  loadGraphParams();
  const graphLinkTypes = $derived(edgeLinkTypes(graph));
  const graphNeighbors = $derived(
    graphSelect === null
      ? new Set<number>()
      : graphNeighborSet(graph?.edges ?? [], graphSelect)
  );
  let hoverNode = $state<number | null>(null);
  const graphHoverNeighbors = $derived(
    hoverNode === null
      ? new Set<number>()
      : graphNeighborSet(graph?.edges ?? [], hoverNode)
  );
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
  const graphEdgeColors = $derived([...new Set((graph?.edges ?? []).map((e) => edgeColor(e.linkType)))]);
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
  // 各节点的显示半径（用于连线端点缩进，避免箭头被节点遮挡）
  const nodeRadii = $derived.by(() => {
    const m: Record<number, number> = {};
    if (graph) {
      for (const nd of graph.nodes) {
        m[nd.id] = (7 + Math.min(nd.inDegree + nd.outDegree, 6)) * graphParams.nodeScale;
      }
    }
    return m;
  });

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
      view = 'graph'; detail = null; graph = null; genMsg = ''; err = '';
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
    return formatIsoTime(t, { showYear: true });
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
      runForceLayout(graph);
      graphSelect = null;
      hoverNode = null;
      graphZoom = 1;
      graphPan = { x: 0, y: 0 };
    } catch (e: unknown) {
      err = '加载图谱失败：' + e;
    } finally {
      graphBusy = false;
    }
  }
  async function openGraphView() {
    await loadGraphData();
    if (graph) view = 'graph';
  }
  // 列表页自动加载图谱数据，供「Wiki 图谱」面板展示缩略图
  $effect(() => {
    if (view === 'list' && kbId !== null && !graph && !graphBusy) {
      loadGraphData();
    }
  });

  // 图谱布局引擎：层级放射树 —— 根节点居中，子节点围绕父节点形成圆，
  // 每个子节点再作为父节点让孙节点环绕自己，依次向外展开。
  // 子节点按子树规模分配角度楔形（叶子等分圆周），保证节点互不遮挡。
  function runForceLayout(g: WikiGraph) {
    const w = graphBox.w, h = graphBox.h;
    if (g.nodes.length === 0) { nodePos = {}; return; }
    const pos = radialTreeLayout(g, w, h, graphParams);
    nodeTarget = pos;
    if (!graphParams.motion) {
      // 关闭动画：直接吸附到目标位置
      nodeBase = { ...pos };
      nodePos = { ...pos };
      applyPositions();
      return;
    }
    // 动画开启：新节点从画布中心飞出，旧节点从当前位置平滑过渡到新位置
    const cx = w / 2, cy = h / 2;
    const nb = { ...nodeBase };
    for (const key of Object.keys(pos)) {
      const id = Number(key);
      if (!nb[id]) nb[id] = { x: cx, y: cy };
    }
    for (const key of Object.keys(nb)) {
      if (!pos[Number(key)]) delete nb[Number(key)];
    }
    nodeBase = nb;
  }

  // 收集 SVG 中已渲染的节点/连线元素引用（结构变化后重新收集）
  function collectGraphEls() {
    nodeEls = new Map();
    lineEls = [];
    if (!svgEl) return;
    svgEl.querySelectorAll<SVGGElement>('[data-gnode]').forEach((g) => {
      const id = Number(g.getAttribute('data-gnode'));
      if (Number.isFinite(id)) nodeEls.set(id, g);
    });
    svgEl.querySelectorAll<SVGLineElement>('[data-gline]').forEach((l) => {
      const from = Number(l.getAttribute('data-gfrom'));
      const to = Number(l.getAttribute('data-gto'));
      if (Number.isFinite(from) && Number.isFinite(to)) lineEls.push({ el: l, from, to });
    });
  }

  // 命令式更新节点位置与连线端点（不经过 Svelte 响应式，避免每帧全量重建 SVG）
  function applyPositions(display?: Record<number, { x: number; y: number }>) {
    const src = display ?? nodePos;
    for (const [id, el] of nodeEls) {
      const p = src[id];
      if (p) el.setAttribute('transform', `translate(${p.x.toFixed(2)} ${p.y.toFixed(2)})`);
    }
    for (const le of lineEls) {
      const a = src[le.from], b = src[le.to];
      if (!a || !b) continue;
      const dx = b.x - a.x, dy = b.y - a.y;
      const len = Math.hypot(dx, dy) || 1;
      const r1 = (nodeRadii[le.from] ?? 12) + 6;
      const r2 = (nodeRadii[le.to] ?? 12) + 6;
      le.el.setAttribute('x1', (a.x + (dx / len) * r1).toFixed(2));
      le.el.setAttribute('y1', (a.y + (dy / len) * r1).toFixed(2));
      le.el.setAttribute('x2', (b.x - (dx / len) * r2).toFixed(2));
      le.el.setAttribute('y2', (b.y - (dy / len) * r2).toFixed(2));
    }
  }

  // 画布尺寸跟随容器（ResizeObserver），图谱始终占满剩余空间
  $effect(() => {
    const el = graphWrapEl;
    // 目录树子页可见时图谱被隐藏，暂停监听（避免 0 尺寸触发无意义重排）
    if (!el || view !== 'graph' || wikiSub !== 'graph') return;
    const update = () => {
      const r = el.getBoundingClientRect();
      if (r.width > 4 && r.height > 4) {
        const nw = Math.round(r.width), nh = Math.round(r.height);
        if (nw !== graphBox.w || nh !== graphBox.h) {
          graphBox = { w: nw, h: nh };
        }
      }
    };
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  });

  // 灵动动画：布局过渡 + 节点缓慢漂浮漂移（rAF 循环；离开图谱视图或关闭动画时停止）
  $effect(() => {
    // 切到目录树子页时暂停动画，切回图谱时恢复（DOM 常驻，位置不丢）
    if (view !== 'graph' || wikiSub !== 'graph' || !graph || graph.nodes.length === 0 || !graphParams.motion) return;
    const reduced = typeof window !== 'undefined'
      && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    let rafId = 0;
    let last = performance.now();
    const tick = (now: number) => {
      const dt = Math.min(0.05, (now - last) / 1000);
      last = now;
      const t = now / 1000;
      // 每帧只构建纯数据并直接写 SVG 属性，不触发 Svelte 重渲染
      const display: Record<number, { x: number; y: number }> = {};
      for (const key of Object.keys(nodeTarget)) {
        const id = Number(key);
        if (draggingNode === id) {
          const dp = nodePos[id];
          if (dp) display[id] = dp;
          continue;
        }
        const tg = nodeTarget[id];
        const base = nodeBase[id] ?? tg;
        // 平滑追逐目标（指数缓动）
        const k = 1 - Math.exp(-dt * 5.5);
        base.x += (tg.x - base.x) * k;
        base.y += (tg.y - base.y) * k;
        // 微小漂浮漂移（尊重系统「减少动态效果」设置）
        let dx = 0, dy = 0;
        if (!reduced) {
          let ph = driftPhase[id];
          if (ph === undefined) { ph = Math.random() * Math.PI * 2; driftPhase[id] = ph; }
          const amp = 3.2;
          dx = Math.sin(t * 0.9 + ph) * amp;
          dy = Math.cos(t * 0.62 + ph * 1.3) * amp;
        }
        display[id] = { x: base.x + dx, y: base.y + dy };
      }
      applyPositions(display);
      nodePos = display;
      rafId = requestAnimationFrame(tick);
    };
    rafId = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(rafId);
  });

  // 关闭灵动动画时：节点直接吸附到目标布局位置
  $effect(() => {
    if (!graphParams.motion && graph && graph.nodes.length > 0) {
      nodeBase = { ...nodeTarget };
      nodePos = { ...nodeTarget };
      applyPositions();
    }
  });

  // 结构 / 外观参数变化后：重新收集节点、连线元素引用，并立即同步一次位置
  $effect(() => {
    if (view !== 'graph' || !graph || graph.nodes.length === 0) return;
    const v = graphVisible;
    const sel = graphSelect;
    const p = graphParams;
    void v; void sel; void p.nodeScale; void p.showLabels; void p.labelOpacity;
    void p.showArrows; void p.edgeWidth; void p.edgeOpacity; void p.showImplicit;
    void p.showOrphans; void p.createdOnly; void p.ignorePatterns; void p.colorGroups;
    void nodeTarget; void nodeRadii;
    tick().then(() => {
      collectGraphEls();
      applyPositions();
    });
  });

  function onNodeDown(ev: PointerEvent, id: number) {
    ev.stopPropagation();
    draggingNode = id;
    dragStart = { x: ev.clientX, y: ev.clientY, moved: false };
    (ev.currentTarget as Element).setPointerCapture?.(ev.pointerId);
  }
  // 精确换算：屏幕坐标 → 图谱世界坐标。
  // 直接求 SVG 实时变换矩阵（含 viewBox 缩放、边框、子像素、平移缩放）的逆矩阵，
  // 避免手工估算缩放比造成的分辨率偏差。
  function screenToGraph(clientX: number, clientY: number): { x: number; y: number } {
    if (!svgEl) return { x: 0, y: 0 };
    const g = svgEl.querySelector<SVGGraphicsElement>('g[data-graph-canvas]');
    const ctm = g?.getScreenCTM();
    if (!ctm) return { x: 0, y: 0 };
    const pt = new DOMPoint(clientX, clientY).matrixTransform(ctm.inverse());
    return { x: pt.x, y: pt.y };
  }
  function onPointerMove(ev: PointerEvent) {
    if (draggingNode !== null) {
      // 点击与拖拽分离：移动距离未超过阈值时保持节点原位，仅作选中
      if (dragStart && !dragStart.moved) {
        if (Math.hypot(ev.clientX - dragStart.x, ev.clientY - dragStart.y) < 4) return;
        dragStart.moved = true;
      }
      const p = screenToGraph(ev.clientX, ev.clientY);
      const x = p.x, y = p.y;
      nodePos[draggingNode] = { x, y };
      nodeBase[draggingNode] = { x, y };
      applyPositions();
    } else if (panning) {
      const cur = screenToGraph(ev.clientX, ev.clientY);
      const start = screenToGraph(panStart.x, panStart.y);
      graphPan = { x: graphPan.x + (cur.x - start.x), y: graphPan.y + (cur.y - start.y) };
      panStart = { x: ev.clientX, y: ev.clientY };
    }
  }
  function onPointerUp() {
    draggingNode = null;
    dragStart = null;
    panning = false;
  }
  function onSvgDown(ev: PointerEvent) {
    panning = true;
    panStart = { x: ev.clientX, y: ev.clientY };
  }
  function onWheel(ev: WheelEvent) {
    ev.preventDefault();
    const p = screenToGraph(ev.clientX, ev.clientY);
    const oldZoom = graphZoom;
    const newZoom = Math.max(0.3, Math.min(3, oldZoom * (ev.deltaY < 0 ? 1.08 : 0.92)));
    if (newZoom === oldZoom) return;
    // 缩放锚点 = 鼠标位置：缩放前后，鼠标所指的世界坐标保持在同一屏幕位置
    const wx = (p.x - graphPan.x) / oldZoom;
    const wy = (p.y - graphPan.y) / oldZoom;
    graphZoom = newZoom;
    graphPan = { x: p.x - wx * newZoom, y: p.y - wy * newZoom };
  }
  function onNodeClick(ev: MouseEvent, id: number) {
    ev.stopPropagation();
    graphSelect = graphSelect === id ? null : id;
    if (id > 0) track('wiki_graph_click', { kbId, pageId: id });
  }

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
{:else if view === 'list'}
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
      <button class="kb-btn-sm" onclick={openGraphView} disabled={graphBusy} title="知识图谱"><KbIcon name="graph" size={13} />图谱</button>
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
  <!-- 子菜单：Wiki图谱 / 目录树 -->
  <div style="display:flex;align-items:center;gap:10px;flex:none;margin-bottom:10px">
    <div class="kb-seg kb-seg-tabs">
      <button class="kb-seg-item" class:active={wikiSub === 'graph'} onclick={() => setWikiSub('graph')}><KbIcon name="graph" size={14} />Wiki图谱</button>
      <button class="kb-seg-item" class:active={wikiSub === 'tree'} onclick={() => setWikiSub('tree')}><KbIcon name="folder" size={14} />目录树</button>
    </div>
  </div>
  <div class="kb-wiki-graph-view" style="flex:1;min-width:0" style:display={wikiSub === 'graph' ? 'flex' : 'none'}>
    <div class="kb-wiki-graph-head">
      <button class="kb-btn-sm" onclick={() => { view = 'list'; graph = null; }}><KbIcon name="arrowLeft" size={13} />返回列表</button>
      <div style="display:flex;gap:6px;align-items:center">
        {#if graph && graph.nodes.length > 0}
          <button class="kb-btn-sm" onclick={() => graph && runForceLayout(graph)}><KbIcon name="refresh" size={13} />重新布局</button>
          <button class="kb-btn-sm" class:kb-graph-cfg-on={graphCfgOpen} onclick={() => graphCfgOpen = !graphCfgOpen} title="图谱设置"><KbIcon name="settings" size={15} /></button>
        {/if}
        <button class="kb-btn-sm" onclick={startCreate}><KbIcon name="plus" size={13} weight="bold" />新建</button>
      </div>
    </div>
    {#if err}<div class="kb-wiki-err">{err}</div>{/if}
    {#if graph && graph.nodes.length > 0}
      <div style="display:flex;align-items:center;gap:10px;flex-wrap:wrap;margin-bottom:8px;flex:none">
        <span class="kb-wiki-graph-stats">显示 {graphVisible.size} / {graph.nodes.length} 个页面 · {graph.edges.length} 条链接</span>
        <!-- 节点类型图例 -->
        <div style="display:flex;gap:10px;align-items:center;flex-wrap:wrap">
          {#each nodeTypeStats as [t, c]}
            <span style="display:inline-flex;align-items:center;gap:4px;font-size:11.5px;color:var(--kb-text-3)" title="节点类型：{t}">
              <span style="width:9px;height:9px;border-radius:50%;background:{NODE_TYPE_COLORS[t] ?? '#8d99ae'};display:inline-block"></span>{t} {c}
            </span>
          {/each}
        </div>
        <div style="flex:1"></div>
        <div style="display:flex;gap:4px;align-items:center">
          <button class="kb-btn-sm" onclick={() => graphZoom = Math.min(3, graphZoom * 1.2)} title="放大"><KbIcon name="plus" size={12} /></button>
          <button class="kb-btn-sm" onclick={() => graphZoom = Math.max(0.3, graphZoom / 1.2)} title="缩小"><KbIcon name="minus" size={12} /></button>
          <button class="kb-btn-sm" onclick={() => { graphZoom = 1; graphPan = { x: 0, y: 0 }; }} title="重置视图"><KbIcon name="arrowsOut" size={12} /></button>
        </div>
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
        <div style="flex:1;min-width:0;min-height:0;position:relative" bind:this={graphWrapEl}>
          <svg class="kb-wiki-graph" viewBox="0 0 {graphBox.w} {graphBox.h}" preserveAspectRatio="none" bind:this={svgEl}
            role="application" aria-label="知识图谱"
            onpointerdown={onSvgDown} onpointermove={onPointerMove} onpointerup={onPointerUp} onpointerleave={onPointerUp}
            onwheel={onWheel}>
            <defs>
              {#each graphEdgeColors as c}
                <marker id={`kb-arrow-${colorSlug(c)}`} viewBox="0 0 10 10" refX="8" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
                  <path d="M0,0 L10,5 L0,10 z" fill={c} />
                </marker>
              {/each}
            </defs>
            <g data-graph-canvas transform="translate({graphPan.x} {graphPan.y}) scale({graphZoom})">
              {#each graph.edges as e (e.from + ':' + e.to + ':' + e.linkType)}
                {#if graphVisible.has(e.from) && graphVisible.has(e.to) && (graphParams.showImplicit || e.linkType !== 'entity')}
                  {@const ec = edgeColor(e.linkType)}
                  {@const isImpl = e.linkType === 'entity'}
                  <line data-gline data-gfrom={e.from} data-gto={e.to}
                    class="kb-wiki-graph-line" class:kb-edge-implicit={isImpl}
                    class:kb-graph-dim={(graphSelect !== null && e.from !== graphSelect && e.to !== graphSelect) || (hoverNode !== null && hoverNode !== graphSelect && e.from !== hoverNode && e.to !== hoverNode)}
                    stroke={ec} stroke-width={graphParams.edgeWidth} stroke-opacity={graphParams.edgeOpacity}
                    stroke-dasharray={isImpl ? '6 4' : undefined}
                    marker-end={graphParams.showArrows ? `url(#kb-arrow-${colorSlug(ec)})` : undefined} />
                {/if}
              {/each}
              {#each graph.nodes as nd (nd.id)}
                {@const deg = nd.inDegree + nd.outDegree}
                {@const nr = (7 + Math.min(deg, 6)) * graphParams.nodeScale}
                {#if graphVisible.has(nd.id)}
                  <g data-gnode={nd.id}
                    role="button" tabindex="0"
                    class="kb-wiki-graph-node"
                    class:kb-graph-dim={(graphSelect !== null && !graphNeighbors.has(nd.id)) || (hoverNode !== null && hoverNode !== graphSelect && !graphHoverNeighbors.has(nd.id))}
                    class:kb-graph-sel={graphSelect === nd.id}
                    transform="translate(0 0)"
                    onpointerenter={() => hoverNode = nd.id}
                    onpointerleave={() => hoverNode = null}
                    onpointerdown={(ev) => onNodeDown(ev, nd.id)}
                    onkeydown={(ev) => { if (ev.key === 'Enter' || ev.key === ' ') { ev.preventDefault(); (ev.currentTarget as SVGElement).dispatchEvent(new MouseEvent('click', { bubbles: true })); } }}
                    onclick={(ev) => onNodeClick(ev, nd.id)}
                    ondblclick={(ev) => {
                      ev.stopPropagation();
                      if (nd.status === 'missing') { graphCfgOpen = false; startCreateWithTitle(nd.title); }
                      else openDetail(nd.pageId);
                    }}>
                    {#if deg >= 4}<circle class="kb-wiki-graph-hub" r={nr + 5} fill="none" />{/if}
                    <circle class="kb-wiki-graph-circle" class:kb-node-ghost={nd.status === 'missing'} r={nr} fill={nodeColor(nd.status, nd, graphParams.colorGroups ?? [])} />
                    {#if graphParams.showLabels}
                      <text class="kb-wiki-graph-label" opacity={graphParams.labelOpacity} y={-nr - 6} text-anchor="middle">{nd.title.length > 14 ? nd.title.slice(0, 14) + '…' : nd.title}</text>
                    {/if}
                  </g>
                {/if}
              {/each}
            </g>
          </svg>
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
  <!-- 目录树视图 -->
  <div style="flex:1;min-height:0;display:flex;gap:14px" style:display={wikiSub === 'graph' ? 'none' : 'flex'}>
    <div class="kb-card" style="flex:none;width:300px;display:flex;flex-direction:column;min-height:0">
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
    <div style="flex:1;min-width:0;display:flex;flex-direction:column;gap:8px">
      {#if filteredPages.length === 0}
        <div class="kb-empty" style="flex:1"><span>该目录下暂无页面</span></div>
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
              <span class="kb-wiki-item-time">{fmtTime(p.updatedAt)}</span>
            </div>
          </button>
        {/each}
      {/if}
    </div>
  </div>
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
  .kb-wiki-graph {
    width: 100%; height: 100%; display: block; touch-action: none;
    border: 1px solid var(--kb-border);
    border-radius: var(--kb-radius-sm, 6px);
    background:
      radial-gradient(circle, color-mix(in srgb, var(--app-accent) 7%, transparent) 1px, transparent 1px),
      var(--kb-surface);
    background-size: 22px 22px;
  }
  .kb-wiki-graph-line {
    fill: none;
    stroke-linecap: round;
    transition: opacity .12s;
  }
  .kb-wiki-graph-circle {
    fill-opacity: .92;
    stroke: color-mix(in srgb, var(--app-bg-color) 55%, #ffffff);
    stroke-width: 1.5;
    cursor: grab; transition: fill .15s, r .15s;
  }
  .kb-wiki-graph-circle.kb-node-ghost {
    stroke-dasharray: 3 3;
    opacity: .82;
  }
  .kb-wiki-graph-hub {
    stroke: color-mix(in srgb, var(--app-accent) 50%, transparent);
    stroke-width: 1.2;
    stroke-dasharray: 3 3;
    pointer-events: none;
  }
  .kb-wiki-graph-node:hover .kb-wiki-graph-circle {
    fill: var(--kb-accent-hover);
    filter: drop-shadow(0 0 5px color-mix(in srgb, var(--app-accent) 55%, transparent));
  }
  .kb-wiki-graph-node:active { cursor: grabbing; }
  .kb-wiki-graph-node.kb-graph-sel .kb-wiki-graph-circle {
    stroke: color-mix(in srgb, var(--app-bg-color) 45%, #ffffff);
    stroke-width: 2.4;
    animation: kb-node-pulse 1.8s ease-in-out infinite;
  }
  @keyframes kb-node-pulse {
    0%, 100% { filter: drop-shadow(0 0 4px color-mix(in srgb, var(--app-accent) 50%, transparent)); }
    50% { filter: drop-shadow(0 0 12px color-mix(in srgb, var(--app-accent) 82%, transparent)); }
  }
  .kb-wiki-graph-node.kb-graph-dim, .kb-wiki-graph-line.kb-graph-dim {
    opacity: .12;
  }
  .kb-wiki-graph-label {
    font-size: 11.5px; fill: var(--kb-text-2); pointer-events: none;
  }
</style>
