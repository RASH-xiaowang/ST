<!--
  社交关系图谱（迁移自 WeQ「群友圈子 / 群聊网络」）
  - 群友圈子：以「我」为中心，节点 = 与我共同的联系人，连线 = 共同群数量
  - 群聊网络：节点 = 群聊，连线 = 共同成员数量
  - 社区检测着色、头像剪裁、拖拽/缩放/平移、悬停聚焦
-->
<script module lang="ts">
  import { errText } from '../../format';
  // 模块级缓存：切换页签/重建组件时复用已聚合数据，避免重复扫描
  let graphModuleCache: { data: GraphRawData } | null = null;
  // 本会话是否已做过一次后台刷新（避免每次进入都重复全量扫描）
  let graphSessionRefreshed = false;
</script>

<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import WechatHoverButton from "./WechatHoverButton.svelte";
  import { getRelationshipGraph, getRelationshipGraphCached, writeFile } from "../services/ipc";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { fmtCount, rankOf, relTime } from "../utils/display";
  import { createMsg } from "../../services/msg.svelte";
  import { utf8ToBase64 } from "../../db/dbUtils";
  import { formatDate } from "../../format";
  import type { GraphChunk, GraphRawData } from "../graph/graphModel";
  import { toGraphData } from "../graph/graphModel";
  import { connectedEdgesOf, groupCommunities, sharedGroupNames, topByField } from "../graph/graphStats";
  import GraphCanvas from "../graph/GraphCanvas.svelte";
  import UsersIcon from "@lucide/svelte/icons/users";
  import MessageSquareIcon from "@lucide/svelte/icons/message-square";
  import AlertTriangleIcon from "@lucide/svelte/icons/alert-triangle";
  import XIcon from "@lucide/svelte/icons/x";
  import ChevronsLeftIcon from "@lucide/svelte/icons/chevrons-left";
  import ChevronsRightIcon from "@lucide/svelte/icons/chevrons-right";
  import Maximize2Icon from "@lucide/svelte/icons/maximize-2";
  import Minimize2Icon from "@lucide/svelte/icons/minimize-2";
  import RotateCcwIcon from "@lucide/svelte/icons/rotate-ccw";
  import DownloadIcon from "@lucide/svelte/icons/download";
  import {
    buildPoster,
    getPosterLayout,
    makePosterInput,
    posterToBlob,
    blobToBase64,
    type PosterRatio,
  } from "../graph/graphPoster";
  import {
    buildGraph,
    communityColor,
    type BuiltGraph,
    type GraphMode,
    type GraphSettings,
    type RawNode,
    type RelationGraphData,
  } from "../graph/graphModel";

  let { onOpenChat = () => {} }: { onOpenChat?: (username: string) => void } = $props();

  const DEFAULT_SETTINGS: GraphSettings = {
    mode: "people",
    nodeLimit: 100,
    minCommon: 1,
    friendsOnly: false,
    intimacySize: true,
    intimacyPull: true,
    groupLevelSize: true,
    groupLevelPull: true,
    groupFilterMode: "all",
    groupFilter: [],
    // 外观
    showArrows: false,
    labelOpacity: 0.9,
    nodeScale: 1,
    edgeWidth: 1,
    motion: true,
    // 力度
    forceCentripetal: 1,
    forceRepulsion: 1,
    forceAttraction: 1,
    forceEdgeLength: 1,
  };
  const RANGES: Record<GraphMode, { min: number; max: number; label: string }> = {
    people: { min: 1, max: 10, label: "连线阈值 · 共同群" },
    groups: { min: 1, max: 30, label: "连线阈值 · 共同成员" },
  };

  let settings = $state<GraphSettings>({ ...DEFAULT_SETTINGS });
  let graphData = $state<RelationGraphData | null>(null);
  let selectedId = $state<string | null>(null);
  let loading = $state(false);
  let refreshing = $state(false);
  let error = $state("");
  let progressText = $state("");
  let pickerOpen = $state(false);
  let unlistenProgress: UnlistenFn | null = null;
  let sawFinal = false;

  const graph: BuiltGraph = $derived(buildGraph(graphData, settings));
  const selected = $derived(graph.nodes.find((n) => n.id === selectedId) ?? null);
  const isPeople = $derived(settings.mode === "people");
  const range = $derived(RANGES[settings.mode]);
  const hasNodes = $derived(graph.nodes.some((n) => n.kind !== "self"));
  const groupOptions = $derived(graphData?.groups ?? []);
  const personNodes = $derived(graph.nodes.filter((n) => n.kind === "person"));
  const groupNodes = $derived(graph.nodes.filter((n) => n.kind === "group"));
  const friendCount = $derived(personNodes.filter((n) => n.isFriend).length);
  const personTopByMsg = $derived(topByField(personNodes, (n) => n.intimacy ?? 0, 6));
  const personTopByGroups = $derived(topByField(personNodes, (n) => n.groupCount ?? 0, 6));
  const groupTopByMsg = $derived(topByField(groupNodes, (n) => n.msgCount ?? 0, 6));
  const groupTopByShared = $derived(topByField(groupNodes, (n) => n.sharedCount ?? 0, 6));
  const groupTopByMembers = $derived(topByField(groupNodes, (n) => n.memberCount ?? 0, 6));
  /** 圈子概览：按成员数降序（self 不参与） */
  const communities = $derived(groupCommunities(graph.nodes));
  const totalMsg = $derived(
    graphData?.summary?.total_messages ??
      personNodes.reduce((a, n) => a + (n.intimacy ?? 0), 0),
  );
  const totalMembers = $derived(groupNodes.reduce((a, n) => a + (n.memberCount ?? 0), 0));
  /** 真实通讯录总数（排除群聊/公众号/本机其他账号） */
  const contactBookTotal = $derived(
    graphData?.summary?.contact_book_total ?? personNodes.length,
  );
  const contactBookFriends = $derived(
    graphData?.summary?.contact_book_friends ?? friendCount,
  );
  /** 群友上限滑杆最高值 = 好友数量（最多可展示与好友等量的群友） */
  const groupMateLimitMax = $derived(Math.max(contactBookFriends, 10));
  /** 用户是否手动调整过节点上限（调整后不再被默认值覆盖） */
  let nodeLimitTouched = $state(false);
  /**
   * 默认节点数 = 群友上限的 3/5：数据加载完成（好友数可知）后自动应用；
   * 用户手动拖过滑杆则保持用户选择，刷新数据不重置。
   */
  $effect(() => {
    if (nodeLimitTouched) return;
    if (!contactBookFriends) return;
    const target = Math.max(10, Math.round(contactBookFriends * 3 / 5));
    if (settings.nodeLimit !== target) {
      settings = { ...settings, nodeLimit: target };
      nodeLimitDraft = target;
    }
  });
  let railOpen = $state(true);
  /** 图谱全屏（覆盖整个窗口，隐藏右侧洞察栏，画布最大化） */
  let graphFullscreen = $state(false);
  /** 数值滑杆草稿（拖动中先显示，停顿后写回 settings，避免拖动时反复重建力导向） */
  const styleDrafts = $state<Record<string, number>>({});
  function styleDraft(key: string, value: number) {
    styleDrafts[key] = value;
    clearTimeout(sliderTimer);
    sliderTimer = setTimeout(() => {
      settings = { ...settings, [key]: value };
    }, 150);
  }

  // 滑杆防抖：拖动时只更新显示值，停顿 150ms 后才重建图谱，
  // 避免拖动过程反复重建力导向仿真（主要 CPU 开销来源）
  // svelte-ignore state_referenced_locally —— 草稿值为初始化快照，防抖提交时再读 settings 最新值
  let nodeLimitDraft = $state(settings.nodeLimit);
  // svelte-ignore state_referenced_locally —— 同上
  let minCommonDraft = $state(settings.minCommon);
  let sliderTimer: ReturnType<typeof setTimeout> | undefined;
  function onSliderInput(field: "nodeLimit" | "minCommon", value: number) {
    if (field === "nodeLimit") {
      nodeLimitTouched = true;
      nodeLimitDraft = value;
    } else minCommonDraft = value;
    clearTimeout(sliderTimer);
    sliderTimer = setTimeout(() => {
      settings = { ...settings, [field]: value };
    }, 150);
  }
  /** 图谱全屏切换：覆盖整个窗口，右侧洞察栏自动隐藏让画布最大化 */
  function toggleGraphFullscreen() {
    graphFullscreen = !graphFullscreen;
  }

  // ─── 图谱导出：高清分享海报（PNG / JPEG × 1:1 / 3:4）+ SVG 矢量图 ───
  type GraphExportFormat = "png" | "jpeg" | "svg";
  interface GraphCanvasApi {
    renderGraphLayer: (
      width: number,
      height: number,
      scale?: number,
      nodeScale?: number,
      withAvatars?: boolean,
    ) => Promise<HTMLCanvasElement>;
    renderGraphSvg: (width: number, height: number, nodeScale?: number) => Promise<string>;
  }
  const POSTER_OPTIONS: Array<{
    id: string;
    label: string;
    format: GraphExportFormat;
    ratio: PosterRatio;
  }> = [
    { id: "png-11", label: "PNG · 1:1 朋友圈", format: "png", ratio: "1:1" },
    { id: "png-34", label: "PNG · 3:4 竖版", format: "png", ratio: "3:4" },
    { id: "jpeg-11", label: "JPEG · 1:1 朋友圈", format: "jpeg", ratio: "1:1" },
    { id: "jpeg-34", label: "JPEG · 3:4 竖版", format: "jpeg", ratio: "3:4" },
    { id: "svg", label: "SVG 矢量图", format: "svg", ratio: "1:1" },
  ];
  let graphCanvasRef = $state<GraphCanvasApi | undefined>();
  let exportOpen = $state(false);
  let exporting = $state(false);
  /** 海报导出提示（4.5 秒自动消失，收敛自本地 showExportMsg，T-293） */
  const exportMsgState = createMsg(4500);

  async function doExport(opt: { format: GraphExportFormat; ratio: PosterRatio }) {
    exportOpen = false;
    if (!graphCanvasRef || exporting) return;
    exporting = true;
    try {
      // SVG 矢量图：直接由布局坐标生成矢量元素，任意缩放都清晰
      if (opt.format === "svg") {
        const { save } = await import("@tauri-apps/plugin-dialog");
        const path = await save({
          title: "导出社交关系图谱矢量图",
          defaultPath: `社交关系图谱_矢量_${Date.now()}.svg`,
          filters: [{ name: "SVG 矢量图", extensions: ["svg"] }],
        });
        if (!path) return;
        const svg = await graphCanvasRef.renderGraphSvg(1600, 1000, 1.3);
        await writeFile(path, utf8ToBase64(svg));
        exportMsgState.show(`已导出 → ${path}`);
        return;
      }

      // 1) 选择保存位置（先选路径，取消则不做任何事）
      const layout = getPosterLayout(opt.ratio);
      const { save } = await import("@tauri-apps/plugin-dialog");
      const ext = opt.format === "jpeg" ? "jpg" : opt.format;
      const filterName = opt.format === "jpeg" ? "JPEG 图片" : "PNG 图片";
      const path = await save({
        title: "导出社交关系图谱海报",
        defaultPath: `社交关系图谱_${opt.ratio === "1:1" ? "朋友圈" : "竖版"}_${Date.now()}.${ext}`,
        filters: [{ name: filterName, extensions: [ext] }],
      });
      if (!path) return; // 用户取消

      // 2) 高清渲染：尽可能大（上限 8192px / 8K），内存不足时自动降级重试
      const MAX_SIDE = 8192;
      const baseScale = Math.min(7.5, MAX_SIDE / Math.max(layout.width, layout.height));
      const now = new Date();
      const timeStr = formatDate(now, { showYear: true });
      const dateStr = timeStr.slice(0, 10);
      const mkInput = (graphLayer: HTMLCanvasElement, scale: number) => ({
        graphLayer,
        scale,
        ...makePosterInput({
          ratio: opt.ratio,
          isPeople,
          dateStr,
          timeStr,
          contactBookFriends,
          personCount: personNodes.length,
          groupCount: groupNodes.length,
          edgesCount: graph.edges.length,
          communityCount: graph.communityCount,
          totalGroups: graphData?.summary?.total_groups ?? 0,
        }),
      });

      let poster: HTMLCanvasElement | null = null;
      let blob: Blob | null = null;
      let lastErr: unknown = null;
      for (const fb of [1, 0.55, 0.3]) {
        try {
          const scale = baseScale * fb;
          const layerScale = Math.min(scale, MAX_SIDE / Math.max(layout.graphW, layout.graphH));
          const graphLayer = await graphCanvasRef.renderGraphLayer(
            layout.graphW,
            layout.graphH,
            layerScale,
            1.3,
            true,
          );
          poster = buildPoster(mkInput(graphLayer, scale));
          blob = await posterToBlob(poster, opt.format);
          break;
        } catch (e) {
          lastErr = e;
        }
      }
      if (!poster || !blob) throw lastErr ?? new Error("海报渲染失败");

      // 3) 生成文件内容并落盘
      const b64 = (await blobToBase64(blob)).split(",")[1];
      await writeFile(path, b64);
      exportMsgState.show(`已导出 → ${path}`);
    } catch (e: unknown) {
      exportMsgState.show(`导出失败：${errText(e)}`, false);
      console.error("[graph export] 失败:", e);
    } finally {
      exporting = false;
    }
  }
  $effect(() => {
    if (!graphFullscreen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") graphFullscreen = false;
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });
  $effect(() => {
    nodeLimitDraft = settings.nodeLimit;
    minCommonDraft = settings.minCommon;
  });

  // 好友数量变化后若上限低于当前值，自动钳制（仅在拿到真实好友数据时，
  // 避免数据加载前 fallback 上限 10 把滑杆值误钳小）
  $effect(() => {
    if (graphData?.summary?.contact_book_friends && settings.nodeLimit > groupMateLimitMax) {
      settings.nodeLimit = groupMateLimitMax;
    }
  });

  function applyData(r: GraphRawData) {
    graphData = toGraphData(r);
  }

  /** 合并扫描阶段增量：按 id 去重（最终以 finalData 覆盖） */
  function mergeChunk(p: GraphChunk) {
    if (!graphData) return;
    const newNodes: RawNode[] = Array.isArray(p?.nodes) ? p.nodes : [];
    if (newNodes.length === 0) return;
    const byId = new Map(graphData.persons.map((n) => [n.id, n]));
    for (const n of newNodes) {
      if ((n.kind as string) === "group" || (n.kind as string) === "self" || (n.kind as string) === "official") continue;
      const ex = byId.get(n.id);
      if (ex) {
        ex.msg_count = Math.max(ex.msg_count, Number(n.msg_count) || 0);
        ex.last_ts = Math.max(ex.last_ts, Number(n.last_ts) || 0);
      } else {
        byId.set(n.id, { ...n, group_codes: n.group_codes ?? [], is_friend: n.is_friend ?? false });
      }
    }
    graphData = { ...graphData, persons: [...byId.values()] };
  }

  /** 后台刷新：不阻塞已渲染的图谱，完成后替换数据 */
  async function backgroundRefresh() {
    if (refreshing) return;
    refreshing = true;
    try {
      const r = await getRelationshipGraph({ limit: 1000 });
      graphModuleCache = { data: r };
      applyData(r);
    } catch (e: unknown) {
      console.error("[graph] 后台刷新失败（保留上次图谱）", e);
    } finally {
      refreshing = false;
    }
  }

  async function load(force = false) {
    error = "";
    selectedId = null;
    progressText = "";
    sawFinal = false;
    loading = false;
    try {
      if (force) {
        // 强制重建：展示加载态
        loading = true;
        const r = await getRelationshipGraph({ limit: 1000 });
        graphModuleCache = { data: r };
        applyData(r);
        graphSessionRefreshed = true;
        return;
      }
      if (graphModuleCache) {
        // 模块缓存：切换页签/重建组件时秒开
        applyData(graphModuleCache.data);
      } else {
        // 先秒开磁盘缓存（上次结果），没有才全量加载
        const cached = await getRelationshipGraphCached();
        if (cached) {
          applyData(cached);
          graphModuleCache = { data: cached };
        } else {
          loading = true;
          const r = await getRelationshipGraph({ limit: 1000 });
          graphModuleCache = { data: r };
          if (!sawFinal) applyData(r);
          graphSessionRefreshed = true;
          return;
        }
      }
      // 有缓存：立即渲染，后台刷新一次（本会话只刷一次）
      if (!graphSessionRefreshed) {
        graphSessionRefreshed = true;
        backgroundRefresh();
      }
    } catch (e: unknown) {
      error = errText(e);
      graphData = null;
    } finally {
      loading = false;
    }
  }

  async function refresh() {
    refreshing = true;
    try {
      const r = await getRelationshipGraph({ limit: 1000 });
      graphModuleCache = { data: r };
      applyData(r);
    } catch (e: unknown) {
      error = errText(e);
    } finally {
      refreshing = false;
    }
  }

  function patch(next: Partial<GraphSettings>) {
    settings = { ...settings, ...next };
  }
  /** 恢复全部默认参数（节点/阈值/开关/外观/力度），不清除已加载的图谱数据 */
  function resetParams() {
    settings = { ...DEFAULT_SETTINGS };
    nodeLimitDraft = DEFAULT_SETTINGS.nodeLimit;
    minCommonDraft = DEFAULT_SETTINGS.minCommon;
    // 恢复默认后重新应用「节点数 = 群友上限 3/5」的默认值
    nodeLimitTouched = false;
    for (const k of Object.keys(styleDrafts)) delete styleDrafts[k];
  }

  function toggleGroup(code: string) {
    const set = new Set(settings.groupFilter);
    if (set.has(code)) set.delete(code);
    else set.add(code);
    patch({ groupFilter: [...set] });
  }

  onMount(async () => {
    load();
    try {
      unlistenProgress = await listen<GraphChunk & {
        phase?: string;
        percent?: number;
        finalData?: GraphRawData;
        message?: string;
        done?: number;
        total?: number;
      }>("wechat-graph-progress", (event) => {
        const p = event.payload;
        if (p?.phase === "chunk") {
          // 后台刷新时跳过增量合并：已渲染缓存图谱，只需进度提示，
          // 避免每个 chunk 都重建图模型与力导向仿真（CPU 大头）
          if (!refreshing) mergeChunk(p);
          progressText = `正在聚合消息统计… 已组装 ${graphData?.persons.length ?? 0} 个节点（${p.percent ?? 0}%）`;
        } else if (p?.phase === "final") {
          sawFinal = true;
          if (p.finalData) applyData(p.finalData);
          progressText = "";
        } else if (p?.phase === "scan" || p?.phase === "days" || p?.phase === "build") {
          progressText = p.message ?? `正在聚合消息统计… ${p.done ?? 0}/${p.total ?? 0}`;
        }
      });
    } catch {
      /* 进度监听失败不影响图谱 */
    }
  });

  onDestroy(() => {
    unlistenProgress?.();
    clearTimeout(sliderTimer);
  });
</script>

<div class="rg-root" class:rg-fullscreen={graphFullscreen}>
  <div class="rg-hd">
    <div>
      <div class="rg-title">社交关系图谱</div>
      <div class="rg-sub">群友圈子 · 群聊网络 · 社区着色（数据源：本地解密微信库）</div>
    </div>
    {#if graphFullscreen && graphData}
      <div class="rg-fs-stats">
        <span>{isPeople ? `好友 ${contactBookFriends} · 展示 ${personNodes.length}` : `群 ${groupNodes.length}`}</span>
        <span>{graph.edges.length} 连线</span>
        <span>{graph.communityCount} 圈子</span>
      </div>
    {/if}
    <div class="rg-ctl">
      <div class="rg-seg">
          <WechatHoverButton class={isPeople ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'} onclick={() => patch({ mode: "people" })}><UsersIcon class="size-3.5" />群友圈子</WechatHoverButton>
          <WechatHoverButton class={!isPeople ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'} onclick={() => patch({ mode: "groups" })}><MessageSquareIcon class="size-3.5" />群聊网络</WechatHoverButton>
        </div>
        <WechatHoverButton text={refreshing ? "刷新中…" : "刷新"} onclick={refresh} disabled={refreshing || loading} title="重新扫描群成员，重建关系网" class="!px-3 !py-1 !text-xs" />
        <WechatHoverButton
          onclick={toggleGraphFullscreen}
          title={graphFullscreen ? "退出全屏（Esc）" : "图谱全屏"}
          class="!px-3 !py-1 !text-xs"
        >
          {#if graphFullscreen}<Minimize2Icon class="size-3.5" />{:else}<Maximize2Icon class="size-3.5" />{/if}
          {graphFullscreen ? "退出全屏" : "全屏"}
        </WechatHoverButton>
        <div class="rg-export">
          <WechatHoverButton
            onclick={() => (exportOpen = !exportOpen)}
            disabled={!hasNodes || exporting || loading}
            title="导出高清分享海报（PNG / JPEG / PDF，可直接发朋友圈）"
            class="!px-3 !py-1 !text-xs"
          >
            {#if exporting}
              <span class="wc-loading-inline"></span>
            {:else}
              <DownloadIcon class="size-3.5" />
            {/if}
            {exporting ? "导出中…" : "导出"}
          </WechatHoverButton>
          {#if exportOpen}
            <div class="rg-export-menu">
              <div class="rg-export-menu-title">高清海报 · 可直接发朋友圈</div>
              {#each POSTER_OPTIONS as opt (opt.id)}
                <button class="rg-export-item" onclick={() => doExport(opt)}>{opt.label}</button>
              {/each}
            </div>
          {/if}
        </div>
    </div>
  </div>

  {#if exportMsgState.state.text}
    <div class="rg-export-msg" class:rg-export-msg-err={!exportMsgState.state.ok}>{exportMsgState.state.text}</div>
  {/if}

  {#if graphData}
    <div class="rg-chips">
      {#if isPeople}
        <span class="rg-chip rg-chip-key">通讯录 {contactBookTotal} 人</span>
        <span class="rg-chip">好友 {contactBookFriends} · 群成员 {graphData?.summary?.contact_book_members ?? 0}</span>
        <span class="rg-chip">图谱展示 {personNodes.length} 位</span>
        <span class="rg-chip">{graph.edges.length} 连线</span>
        <span class="rg-chip">{graph.communityCount} 个圈子</span>
        <span class="rg-chip">共同群 ≥ {settings.minCommon}</span>
      {:else}
        <span class="rg-chip rg-chip-key">群总数 {graphData?.summary?.total_groups ?? groupNodes.length}</span>
        <span class="rg-chip">图谱展示 {groupNodes.length} 个</span>
        <span class="rg-chip">{totalMembers} 群成员</span>
        <span class="rg-chip">{graph.edges.length} 连线</span>
        <span class="rg-chip">{graph.communityCount} 个圈子</span>
        <span class="rg-chip">共同成员 ≥ {settings.minCommon}</span>
      {/if}
      <span class="rg-chip rg-chip-muted">扫描群 {graphData.scannedGroups} · 消息 {fmtCount(totalMsg)}</span>
      {#if refreshing}
        <span class="rg-chip rg-chip-refreshing">
          <span class="wc-loading-inline"></span>
          <span class="rg-refresh-text">后台更新图谱{progressText ? ` · ${progressText.replace(/^正在聚合消息统计…\s*/, "")}` : ""}</span>
        </span>
      {/if}
    </div>
  {/if}

  <div class="rg-body">
    <div class="rg-stage">
      {#if error}
        <div class="rg-error"><AlertTriangleIcon class="size-3.5" /> {error}</div>
      {:else if loading && !graphData}
        <div class="rg-state"><span class="wc-loading-inline"></span> {progressText || "正在聚合消息统计…"}</div>
      {:else if !hasNodes}
        <div class="rg-state">当前条件下没有可显示的节点，试试降低阈值或调整过滤。</div>
      {:else}
        <GraphCanvas
          bind:this={graphCanvasRef}
          graph={graph}
          selectedId={selectedId}
          onSelect={(n) => (selectedId = n && n.kind !== "self" ? n.id : null)}
          {settings}
        />
        {#if graphData}
          <div class="rg-legend">
            <span class="rg-legend-title">图例</span>
            <span class="rg-legend-item"><i class="rg-legend-dot" style="background:{communityColor(0)}"></i>颜色 = 圈子</span>
            <span class="rg-legend-item"><i class="rg-legend-line"></i>连线 = {isPeople ? "共同群数" : "共同成员数"}</span>
            <span class="rg-legend-item"><i class="rg-legend-bar"></i>节点大小 = {isPeople ? "消息量 / 共同群" : "命中成员数"}</span>
          </div>
        {/if}
      {/if}
    </div>

    {#if railOpen && !graphFullscreen}
      <aside class="rg-rail">
        <div class="rg-rail-hd">
          <span>洞察</span>
          <button class="rg-rail-fold" onclick={() => (railOpen = false)} title="收起洞察"><ChevronsRightIcon class="size-3.5" /></button>
        </div>

        {#if selected}
          {@const sel = selected}
          <div class="rg-detail">
            <div class="rg-detail-hd">
              <span class="rg-detail-dot" style:background={communityColor(sel.community)}></span>
              <span class="rg-detail-name">{sel.label}</span>
              <button class="rg-detail-close" onclick={() => (selectedId = null)} title="关闭"><XIcon class="size-3.5" /></button>
            </div>
            <div class="rg-detail-rows">
              {#if sel.kind === "person"}
                <div><span>关系</span><b>{sel.isFriend ? "好友" : "群友"}</b></div>
                <div><span>共同群</span><b>{sel.groupCount ?? 0}</b></div>
                <div>
                  <span>消息量</span><b>{fmtCount(sel.intimacy ?? 0)}</b>
                  {#if rankOf(personNodes, sel.id, (n) => n.intimacy ?? 0) > 0}<i class="rg-rank">亲密度 #{rankOf(personNodes, sel.id, (n) => n.intimacy ?? 0)}</i>{/if}
                </div>
                <div><span>活跃天数</span><b>{sel.activeDays ?? 0} 天</b></div>
                <div><span>最近联系</span><b>{relTime(sel.lastTs)}</b></div>
              {:else if sel.kind === "group"}
                <div><span>群成员</span><b>{sel.memberCount ?? 0} 人</b></div>
                <div><span>命中成员</span><b>{sel.sharedCount ?? 0} 位</b></div>
                <div><span>消息量</span><b>{fmtCount(sel.msgCount ?? 0)}</b></div>
                <div><span>活跃天数</span><b>{sel.activeDays ?? 0} 天</b></div>
                <div><span>最近活跃</span><b>{relTime(sel.lastTs)}</b></div>
              {/if}
              <div>
                <span>所属圈子</span>
                {#if sel.community < 0}
                  <b>未分组</b>
                {:else}
                  <b>#{sel.community + 1} · {communities.find((c) => c.id === sel.community)?.members.length ?? 0} 位</b>
                {/if}
              </div>
            </div>
            {#if sel.kind === "person" && (sel.groupCodes?.length ?? 0) > 0}
              <div class="rg-detail-sub">共同群{(sel.groupCodes?.length ?? 0) > 6 ? `（前 6 / ${sel.groupCodes!.length}）` : ""}</div>
              <div class="rg-groups">
                {#each sharedGroupNames(sel, graphData?.groupNames, 6) as gname, gi (gi)}
                  <span class="rg-group-chip">{gname}</span>
                {/each}
              </div>
            {:else if sel.kind === "group" && (sel.sharedMembers?.length ?? 0) > 0}
              <div class="rg-detail-sub">共同成员（按消息量）</div>
              <div class="rg-members">
                {#each sel.sharedMembers!.slice(0, 6) as m (m.username)}
                  <button
                    type="button"
                    class="rg-member"
                    title="在「群友圈子」中查看"
                    onclick={() => { patch({ mode: "people" }); selectedId = m.username; }}
                  >
                    <span class="rg-member-name">{m.name}</span>
                    {#if m.is_friend}<span class="rg-member-tag">好友</span>{/if}
                    <span class="rg-member-val">{fmtCount(m.msg_count)}</span>
                  </button>
                {/each}
              </div>
            {/if}
            {#if sel.kind !== "self"}
              <WechatHoverButton text="打开聊天" onclick={() => onOpenChat(sel.id)} class="!px-3 !py-1 !text-xs" />
            {/if}
            {#if connectedEdgesOf(graph, sel.id).length > 0}
              <div class="rg-detail-sub">关联（按强度）</div>
              <div class="rg-edges">
                {#each connectedEdgesOf(graph, sel.id) as ce (String(ce.edge.source) + String(ce.edge.target))}
                  <button type="button" class="rg-edge" onclick={() => ce.other && (selectedId = ce.other.id)}>
                    <span class="rg-edge-name">{ce.other?.label ?? "?"}</span>
                    <span class="rg-edge-weight">{fmtCount(ce.edge.weight)}</span>
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        {/if}

        <div class="rg-insights">
          {#if isPeople}
            <div class="rg-insi-hd">亲密度榜</div>
            <div class="rg-insi-list">
              {#each personTopByMsg as p, i (p.id)}
                <button type="button" class="rg-insi-row" class:rg-insi-on={selectedId === p.id} onclick={() => (selectedId = p.id)}>
                  <span class="rg-insi-rank">{i + 1}</span>
                  <span class="rg-insi-name">{p.label}</span>
                  <span class="rg-insi-val">{fmtCount(p.intimacy ?? 0)}</span>
                </button>
              {/each}
              {#if personTopByMsg.length === 0}<div class="rg-insi-empty">暂无数据</div>{/if}
            </div>
            <div class="rg-insi-hd">共同群榜</div>
            <div class="rg-insi-list">
              {#each personTopByGroups as p, i (p.id)}
                <button type="button" class="rg-insi-row" class:rg-insi-on={selectedId === p.id} onclick={() => (selectedId = p.id)}>
                  <span class="rg-insi-rank">{i + 1}</span>
                  <span class="rg-insi-name">{p.label}</span>
                  <span class="rg-insi-val">{p.groupCount ?? 0} 群</span>
                </button>
              {/each}
              {#if personTopByGroups.length === 0}<div class="rg-insi-empty">暂无数据</div>{/if}
            </div>
          {:else}
            <div class="rg-insi-hd">活跃群榜</div>
            <div class="rg-insi-list">
              {#each groupTopByMsg as g, i (g.id)}
                <button type="button" class="rg-insi-row" class:rg-insi-on={selectedId === g.id} onclick={() => (selectedId = g.id)}>
                  <span class="rg-insi-rank">{i + 1}</span>
                  <span class="rg-insi-name">{g.label}</span>
                  <span class="rg-insi-val">{fmtCount(g.msgCount ?? 0)}</span>
                </button>
              {/each}
              {#if groupTopByMsg.length === 0}<div class="rg-insi-empty">暂无数据</div>{/if}
            </div>
            <div class="rg-insi-hd">命中榜</div>
            <div class="rg-insi-list">
              {#each groupTopByShared as g, i (g.id)}
                <button type="button" class="rg-insi-row" class:rg-insi-on={selectedId === g.id} onclick={() => (selectedId = g.id)}>
                  <span class="rg-insi-rank">{i + 1}</span>
                  <span class="rg-insi-name">{g.label}</span>
                  <span class="rg-insi-val">{g.sharedCount ?? 0} 人</span>
                </button>
              {/each}
              {#if groupTopByShared.length === 0}<div class="rg-insi-empty">暂无数据</div>{/if}
            </div>
            <div class="rg-insi-hd">规模榜</div>
            <div class="rg-insi-list">
              {#each groupTopByMembers as g, i (g.id)}
                <button type="button" class="rg-insi-row" class:rg-insi-on={selectedId === g.id} onclick={() => (selectedId = g.id)}>
                  <span class="rg-insi-rank">{i + 1}</span>
                  <span class="rg-insi-name">{g.label}</span>
                  <span class="rg-insi-val">{g.memberCount ?? 0} 人</span>
                </button>
              {/each}
              {#if groupTopByMembers.length === 0}<div class="rg-insi-empty">暂无数据</div>{/if}
            </div>
          {/if}

          <div class="rg-insi-hd">圈子概览</div>
          <div class="rg-insi-list">
            {#each communities.slice(0, 6) as c (c.id)}
              <button
                type="button"
                class="rg-insi-row rg-insi-community"
                onclick={() => c.members[0] && (selectedId = c.members[0].id)}
              >
                <span class="rg-insi-dot" style="background:{communityColor(c.id)}"></span>
                <span class="rg-insi-name">圈子 #{c.id + 1} · {c.members.length} 位</span>
                <span class="rg-insi-val">{c.members[0]?.label ?? ""}</span>
              </button>
            {/each}
            {#if communities.length === 0}<div class="rg-insi-empty">暂无圈子</div>{/if}
          </div>
        </div>
      </aside>
    {:else if !graphFullscreen}
      <button class="rg-rail-unfold" onclick={() => (railOpen = true)} title="展开洞察"><ChevronsLeftIcon class="size-3.5" /></button>
    {/if}
  </div>

  {#snippet toggle(label: string, checked: boolean, onchange: (v: boolean) => void)}
    <button
      type="button"
      class="rg-toggle"
      class:on={checked}
      role="switch"
      aria-checked={checked}
      onclick={() => onchange(!checked)}
    >
      <span>{label}</span>
      <span class="rg-toggle-track"><span class="rg-toggle-knob"></span></span>
    </button>
  {/snippet}

  <div class="rg-controls">
    <div class="rg-ctl-toolbar">
      <span class="rg-ctl-toolbar-title">图谱参数</span>
      <button type="button" class="rg-reset-btn" onclick={resetParams} title="恢复所有参数为默认值">
        <RotateCcwIcon class="size-3" /> 恢复默认
      </button>
    </div>
    <!-- ① 数据：节点规模与关系筛选 -->
    <div class="rg-ctl-block">
      <div class="rg-ctl-block-title">数据</div>
      <div class="rg-ctl-block-body">
        <label class="rg-slider rg-slider-wide">
          <span class="rg-slider-top"><span>群友上限</span><b>{nodeLimitDraft} / {groupMateLimitMax}</b></span>
          <input
            type="range"
            min={10}
            max={groupMateLimitMax}
            step={1}
            value={nodeLimitDraft}
            oninput={(e) => onSliderInput("nodeLimit", Number((e.currentTarget as HTMLInputElement).value))}
          />
        </label>
        <label class="rg-slider rg-slider-wide">
          <span class="rg-slider-top"><span>{range.label}</span><b>≥ {minCommonDraft}</b></span>
          <input
            type="range"
            min={range.min}
            max={range.max}
            step={1}
            value={minCommonDraft}
            oninput={(e) => onSliderInput("minCommon", Number((e.currentTarget as HTMLInputElement).value))}
          />
        </label>
        {#if isPeople}
          {@render toggle("仅显示好友", settings.friendsOnly, (v) => patch({ friendsOnly: v }))}
          {@render toggle("消息量决定大小", settings.intimacySize, (v) => patch({ intimacySize: v }))}
          {@render toggle("消息量决定亲疏", settings.intimacyPull, (v) => patch({ intimacyPull: v }))}
        {:else}
          {@render toggle("命中数决定大小", settings.groupLevelSize, (v) => patch({ groupLevelSize: v }))}
          {@render toggle("命中数决定亲疏", settings.groupLevelPull, (v) => patch({ groupLevelPull: v }))}
        {/if}
        <WechatHoverButton
          text={settings.groupFilterMode === "all"
            ? "全部群参与"
            : settings.groupFilterMode === "whitelist"
              ? `白名单 ${settings.groupFilter.length}`
              : `黑名单 ${settings.groupFilter.length}`}
          onclick={() => (pickerOpen = true)}
          title="选择参与计算的群聊"
          class="!px-3 !py-1 !text-xs"
        />
      </div>
    </div>
    <!-- ② 外观：画布视觉参数 -->
    <div class="rg-ctl-block">
      <div class="rg-ctl-block-title">外观</div>
      <div class="rg-ctl-block-body">
        <div class="rg-ctl-switches">
          {@render toggle("箭头", settings.showArrows, (v) => patch({ showArrows: v }))}
          {@render toggle("播放动画", settings.motion, (v) => patch({ motion: v }))}
        </div>
        <label class="rg-slider">
          <span class="rg-slider-top"><span>文本透明度</span><b>{Math.round((styleDrafts.labelOpacity ?? settings.labelOpacity) * 100)}%</b></span>
          <input type="range" min={0.2} max={1} step={0.05} value={styleDrafts.labelOpacity ?? settings.labelOpacity}
            oninput={(e) => styleDraft("labelOpacity", Number((e.currentTarget as HTMLInputElement).value))} />
        </label>
        <label class="rg-slider">
          <span class="rg-slider-top"><span>节点大小</span><b>{(styleDrafts.nodeScale ?? settings.nodeScale).toFixed(2)}×</b></span>
          <input type="range" min={0.6} max={1.8} step={0.05} value={styleDrafts.nodeScale ?? settings.nodeScale}
            oninput={(e) => styleDraft("nodeScale", Number((e.currentTarget as HTMLInputElement).value))} />
        </label>
        <label class="rg-slider">
          <span class="rg-slider-top"><span>连线粗细</span><b>{(styleDrafts.edgeWidth ?? settings.edgeWidth).toFixed(1)}</b></span>
          <input type="range" min={0.5} max={3} step={0.1} value={styleDrafts.edgeWidth ?? settings.edgeWidth}
            oninput={(e) => styleDraft("edgeWidth", Number((e.currentTarget as HTMLInputElement).value))} />
        </label>
      </div>
    </div>
    <!-- ③ 力度：力导向布局参数 -->
    <div class="rg-ctl-block">
      <div class="rg-ctl-block-title">力度</div>
      <div class="rg-ctl-block-body">
        <label class="rg-slider">
          <span class="rg-slider-top"><span>图谱向心力</span><b>{(styleDrafts.forceCentripetal ?? settings.forceCentripetal).toFixed(2)}×</b></span>
          <input type="range" min={0} max={3} step={0.05} value={styleDrafts.forceCentripetal ?? settings.forceCentripetal}
            oninput={(e) => styleDraft("forceCentripetal", Number((e.currentTarget as HTMLInputElement).value))} />
        </label>
        <label class="rg-slider">
          <span class="rg-slider-top"><span>节点间排斥力</span><b>{(styleDrafts.forceRepulsion ?? settings.forceRepulsion).toFixed(1)}×</b></span>
          <input type="range" min={0.2} max={8} step={0.1} value={styleDrafts.forceRepulsion ?? settings.forceRepulsion}
            oninput={(e) => styleDraft("forceRepulsion", Number((e.currentTarget as HTMLInputElement).value))} />
        </label>
        <label class="rg-slider">
          <span class="rg-slider-top"><span>相连节点吸引力</span><b>{(styleDrafts.forceAttraction ?? settings.forceAttraction).toFixed(2)}×</b></span>
          <input type="range" min={0.2} max={3} step={0.05} value={styleDrafts.forceAttraction ?? settings.forceAttraction}
            oninput={(e) => styleDraft("forceAttraction", Number((e.currentTarget as HTMLInputElement).value))} />
        </label>
        <label class="rg-slider">
          <span class="rg-slider-top"><span>连线长度</span><b>{(styleDrafts.forceEdgeLength ?? settings.forceEdgeLength).toFixed(2)}×</b></span>
          <input type="range" min={0.5} max={2} step={0.05} value={styleDrafts.forceEdgeLength ?? settings.forceEdgeLength}
            oninput={(e) => styleDraft("forceEdgeLength", Number((e.currentTarget as HTMLInputElement).value))} />
        </label>
      </div>
    </div>
  </div>
  </div>

{#if pickerOpen}
  <div
    class="rg-mask"
    role="button"
    tabindex="-1"
    aria-label="关闭群过滤"
    onclick={() => (pickerOpen = false)}
    onkeydown={(e) => {
      if (e.key === "Escape" || e.key === "Enter" || e.key === " ") pickerOpen = false;
    }}
  >
    <div
      class="rg-picker"
      role="dialog"
      aria-label="群过滤"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <div class="rg-picker-hd">
        <b>群过滤</b>
        <div class="rg-picker-modes">
          <WechatHoverButton text="全部" onclick={() => patch({ groupFilterMode: "all" })} class={settings.groupFilterMode === "all" ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'} />
          <WechatHoverButton text="白名单" onclick={() => patch({ groupFilterMode: "whitelist" })} class={settings.groupFilterMode === "whitelist" ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'} />
          <WechatHoverButton text="黑名单" onclick={() => patch({ groupFilterMode: "blacklist" })} class={settings.groupFilterMode === "blacklist" ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'} />
        </div>
      <button class="rg-picker-close" onclick={() => (pickerOpen = false)}><XIcon class="size-3.5" /></button>
      </div>
      <div class="rg-picker-list">
        {#each groupOptions as g (g.id)}
          <label class="rg-pick-row">
            <input type="checkbox" checked={settings.groupFilter.includes(g.id)} onchange={() => toggleGroup(g.id)} />
            <span class="rg-pick-name">{g.label || g.id}</span>
            <span class="rg-pick-meta">{g.member_count} 人 · 命中 {g.shared_count}</span>
          </label>
        {/each}
        {#if groupOptions.length === 0}
          <div class="rg-picker-empty">暂无群聊数据</div>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .rg-root {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    padding: 16px 20px;
    gap: 10px;
    box-sizing: border-box;
  }
  .rg-root.rg-fullscreen {
    position: fixed;
    inset: 0;
    z-index: 9999;
    width: 100vw;
    height: 100vh;
    padding: 12px 18px 14px;
    background: var(--wc-bg, #0d1015);
  }
  /* 全屏：隐藏副标题与统计芯片行，画布最大化 */
  .rg-root.rg-fullscreen .rg-sub,
  .rg-root.rg-fullscreen .rg-chips {
    display: none;
  }
  .rg-root.rg-fullscreen .rg-hd {
    min-height: 34px;
  }
  .rg-fs-stats {
    display: none;
    align-items: center;
    gap: 10px;
    font-size: 11.5px;
    color: var(--wc-muted);
    white-space: nowrap;
    overflow: hidden;
  }
  .rg-fs-stats span {
    padding: 3px 10px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--wc-bg2) 70%, transparent);
    border: 1px solid var(--wc-border-light);
  }
  .rg-root.rg-fullscreen .rg-fs-stats {
    display: flex;
  }
  /* 全屏：控制区压缩为两行紧凑条 */
  .rg-root.rg-fullscreen .rg-controls {
    display: grid;
    grid-template-columns: minmax(300px, 1.25fr) minmax(220px, 1fr) minmax(220px, 1fr);
    gap: 6px;
    margin: 0;
    width: 100%;
    max-width: none;
    padding: 6px 8px;
    border-radius: 9px;
  }
  .rg-root.rg-fullscreen .rg-ctl-toolbar {
    grid-column: 1 / -1;
    width: 100%;
    padding: 0 2px 2px;
  }
  .rg-root.rg-fullscreen .rg-ctl-block {
    padding: 4px 8px;
    border-radius: 7px;
    min-width: 0;
    flex: 1 1 0;
  }
  .rg-root.rg-fullscreen .rg-ctl-block:first-child { flex: 1.25 1 0; }
  .rg-root.rg-fullscreen .rg-ctl-block-body {
    gap: 4px 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .rg-root.rg-fullscreen .rg-slider {
    min-width: 88px;
    max-width: none;
    gap: 2px;
  }
  .rg-root.rg-fullscreen .rg-slider-top {
    font-size: 10.5px;
  }
  .rg-root.rg-fullscreen .rg-slider input {
    height: 3px;
  }
  .rg-root.rg-fullscreen .rg-toggle {
    font-size: 11px;
    gap: 5px;
  }
  .rg-root.rg-fullscreen .rg-ctl-switches {
    gap: 6px;
  }
  .rg-root.rg-fullscreen .rg-ctl-block-title {
    font-size: 9.5px;
    letter-spacing: 0.1em;
  }
  .rg-root.rg-fullscreen .rg-ctl-toolbar {
    flex: none;
  }
  .rg-root.rg-fullscreen .rg-ctl-toolbar-title {
    display: none;
  }
  .rg-root.rg-fullscreen .rg-reset-btn {
    height: 22px;
    padding: 0 7px;
    font-size: 10.5px;
  }
  .rg-root.rg-fullscreen .rg-rail,
  .rg-root.rg-fullscreen .rg-rail-unfold {
    display: none;
  }
  .rg-hd {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-shrink: 0;
  }
  .rg-title { font-size: 16px; font-weight: 700; color: var(--wc-text); }
  .rg-sub { font-size: 11.5px; color: var(--wc-muted); }
  .rg-ctl { display: flex; gap: 8px; align-items: center; }
  .rg-export {
    position: relative;
    display: inline-flex;
  }
  .rg-export-menu {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 40;
    min-width: 188px;
    display: flex;
    flex-direction: column;
    padding: 4px;
    border-radius: 9px;
    border: 1px solid var(--wc-border);
    background: var(--wc-card);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.18);
  }
  .rg-export-menu-title {
    font-size: 11px;
    color: var(--wc-muted);
    padding: 5px 10px 4px;
    border-bottom: 1px solid var(--wc-border-light);
    margin-bottom: 3px;
    white-space: nowrap;
  }
  .rg-export-item {
    border: none;
    background: transparent;
    color: var(--wc-text);
    font-size: 12.5px;
    text-align: left;
    padding: 7px 10px;
    border-radius: 6px;
    cursor: pointer;
    white-space: nowrap;
  }
  .rg-export-item:hover {
    background: var(--wc-item-hover);
    color: var(--wc-theme, #576b95);
  }
  .rg-export-msg {
    margin-top: 2px;
    padding: 6px 10px;
    font-size: 11.5px;
    color: var(--wc-text);
    background: rgba(7, 193, 96, 0.1);
    border: 1px solid rgba(7, 193, 96, 0.28);
    border-radius: 6px;
    word-break: break-all;
  }
  .rg-export-msg.rg-export-msg-err {
    color: #c0392b;
    background: rgba(192, 57, 43, 0.08);
    border-color: rgba(192, 57, 43, 0.24);
  }
  .rg-seg {
    display: inline-flex;
    gap: 2px;
    padding: 3px;
    border-radius: 9px;
    background: var(--wc-bg2);
    border: 1px solid var(--wc-border-light);
  }
  .rg-seg button {
    display: inline-flex; align-items: center; gap: 4px; white-space: nowrap;
    border: none;
    background: transparent;
    color: var(--wc-text2);
    font-size: 12px;
    padding: 5px 10px;
    border-radius: 7px;
    cursor: pointer;
  }
  .rg-seg button.on { background: var(--wc-theme, #576b95); color: #fff; font-weight: 600; }
  .rg-chips { display: flex; gap: 8px; flex-wrap: wrap; flex-shrink: 0; }
  .rg-chip {
    font-size: 11.5px;
    padding: 3px 10px;
    border-radius: 999px;
    background: var(--wc-bg2);
    border: 1px solid var(--wc-border-light);
    color: var(--wc-text2);
  }
  .rg-body { flex: 1; min-height: 0; display: flex; gap: 12px; position: relative; }
  .rg-stage { flex: 1; min-width: 0; position: relative; }
  .rg-state {
    position: absolute;
    inset: 0;
    z-index: 2;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--wc-muted);
    font-size: 13px;
    background: var(--wc-card);
    border: 1px solid var(--wc-border-light);
    border-radius: 10px;
  }
  .rg-error {
    position: absolute;
    top: 12px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 3;
    font-size: 12px;
    color: #c0392b;
    background: rgba(192, 57, 43, 0.08);
    border: 1px solid rgba(192, 57, 43, 0.2);
    padding: 8px 14px;
    border-radius: 8px;
    max-width: 90%;
  }
  .rg-detail {
    width: auto;
    flex-shrink: 0;
    border: 1px solid var(--wc-border-light);
    border-radius: 10px;
    background: var(--wc-card);
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .rg-detail-hd { display: flex; align-items: center; gap: 8px; }
  .rg-detail-dot { width: 10px; height: 10px; border-radius: 50%; flex-shrink: 0; }
  .rg-detail-name {
    font-size: 14px;
    font-weight: 700;
    color: var(--wc-text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
  }
  .rg-detail-close {
    border: none;
    background: transparent;
    color: var(--wc-muted);
    cursor: pointer;
    font-size: 13px;
  }
  .rg-detail-rows { display: flex; flex-direction: column; gap: 6px; font-size: 12px; }
  .rg-detail-rows > div {
    display: flex;
    justify-content: space-between;
    color: var(--wc-text2);
  }
  .rg-detail-rows b { color: var(--wc-text); }
  .rg-detail-sub { font-size: 11.5px; font-weight: 600; color: var(--wc-muted); }
  .rg-edges { display: flex; flex-direction: column; gap: 5px; }
  .rg-edge {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 5px 8px;
    border: none;
    border-radius: 6px;
    background: var(--wc-bg2);
    color: var(--wc-text);
    font-size: 12px;
    cursor: pointer;
    text-align: left;
  }
  .rg-edge:hover { background: var(--wc-item-hover); }
  .rg-edge-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rg-edge-weight { font-size: 11.5px; color: var(--wc-theme, #576b95); font-weight: 600; }

  /* ─── 洞察侧栏 ─── */
  .rg-rail {
    width: 300px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
    overflow-y: auto;
    scrollbar-width: thin;
    padding-right: 2px;
  }
  .rg-rail-hd {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 13px;
    font-weight: 700;
    color: var(--wc-text);
    letter-spacing: 0.08em;
    flex-shrink: 0;
  }
  .rg-rail-fold {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border: 1px solid var(--wc-border-light);
    border-radius: 7px;
    background: var(--wc-bg2);
    color: var(--wc-muted);
    cursor: pointer;
  }
  .rg-rail-fold:hover { color: var(--wc-text); border-color: var(--wc-border); }
  .rg-rail-unfold {
    position: absolute;
    top: 12px;
    right: 0;
    z-index: 3;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 30px;
    border: 1px solid var(--wc-border-light);
    border-left: none;
    border-radius: 0 8px 8px 0;
    background: var(--wc-card);
    color: var(--wc-muted);
    cursor: pointer;
  }
  .rg-rail-unfold:hover { color: var(--wc-theme, #576b95); }
  .rg-insights { display: flex; flex-direction: column; gap: 8px; }
  .rg-insi-hd {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.12em;
    color: var(--wc-muted);
    margin-top: 4px;
  }
  .rg-insi-list {
    display: flex;
    flex-direction: column;
    gap: 3px;
    border: 1px solid var(--wc-border-light);
    border-radius: 10px;
    background: var(--wc-card);
    padding: 4px;
  }
  .rg-insi-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 5px 7px;
    border: none;
    border-radius: 7px;
    background: transparent;
    color: var(--wc-text2);
    font-size: 12px;
    text-align: left;
    cursor: pointer;
  }
  .rg-insi-row:hover { background: var(--wc-item-hover); color: var(--wc-text); }
  .rg-insi-on { background: var(--wc-item-active); color: var(--wc-text); }
  .rg-insi-rank {
    width: 16px;
    flex-shrink: 0;
    font-size: 11px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    color: var(--wc-muted);
    text-align: center;
  }
  .rg-insi-row:nth-child(-n + 3) .rg-insi-rank { color: var(--wc-theme, #576b95); }
  .rg-insi-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rg-insi-val {
    flex-shrink: 0;
    font-size: 11.5px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--wc-text2);
  }
  .rg-insi-empty { padding: 10px 8px; font-size: 11.5px; color: var(--wc-muted); text-align: center; }
  .rg-insi-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .rg-insi-community .rg-insi-val { color: var(--wc-muted); font-weight: 500; }

  /* ─── 详情增强 ─── */
  .rg-rank {
    margin-left: auto;
    font-style: normal;
    font-size: 10.5px;
    font-weight: 600;
    color: var(--wc-theme, #576b95);
  }
  .rg-groups { display: flex; flex-wrap: wrap; gap: 5px; }
  .rg-group-chip {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--wc-bg2);
    border: 1px solid var(--wc-border-light);
    color: var(--wc-text2);
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rg-members { display: flex; flex-direction: column; gap: 3px; }
  .rg-member {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 4px 6px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--wc-text2);
    font-size: 12px;
    text-align: left;
    cursor: pointer;
  }
  .rg-member:hover { background: var(--wc-item-hover); color: var(--wc-text); }
  .rg-member-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rg-member-tag {
    flex-shrink: 0;
    font-size: 10px;
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--wc-theme, #576b95);
    color: #fff;
  }
  .rg-member-val {
    flex-shrink: 0;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    color: var(--wc-muted);
  }

  /* ─── 图例 ─── */
  .rg-legend {
    position: absolute;
    left: 12px;
    bottom: 12px;
    z-index: 2;
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 9px 12px;
    border: 1px solid var(--wc-border-light);
    border-radius: 9px;
    background: color-mix(in srgb, var(--wc-card) 88%, transparent);
    backdrop-filter: blur(4px);
    font-size: 11px;
    color: var(--wc-text2);
    pointer-events: none;
    max-width: 250px;
  }
  .rg-legend-title { font-size: 10.5px; font-weight: 700; letter-spacing: 0.1em; color: var(--wc-muted); }
  .rg-legend-item { display: inline-flex; align-items: center; gap: 7px; }
  .rg-legend-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .rg-legend-line { width: 14px; height: 2px; border-radius: 1px; background: var(--wc-border); flex-shrink: 0; }
  .rg-legend-bar {
    width: 14px;
    height: 8px;
    border-radius: 3px;
    background: color-mix(in srgb, var(--wc-theme, #576b95) 35%, transparent);
    flex-shrink: 0;
  }

  .rg-chip-key { border-color: var(--wc-theme, #576b95); color: var(--wc-theme, #576b95); font-weight: 700; }
  .rg-chip-muted { opacity: 0.72; }
  .rg-chip-refreshing {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    max-width: 340px;
    border-color: var(--wc-theme, #576b95);
    color: var(--wc-theme, #576b95);
    font-weight: 600;
  }
  .rg-refresh-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rg-controls {
    display: flex;
    gap: 14px;
    flex-wrap: wrap;
    flex-shrink: 0;
    position: relative;
    padding: 10px 12px;
    border: 1px solid var(--wc-border-light);
    border-radius: 10px;
    background: var(--wc-card);
  }
  /* ─── 控制区工具栏：标题 + 恢复默认 ─── */
  .rg-ctl-toolbar {
    flex: 1 1 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 0 2px;
  }
  .rg-ctl-toolbar-title {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.14em;
    color: var(--wc-muted);
    text-transform: uppercase;
  }
  .rg-reset-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 9px;
    border: 1px solid var(--wc-border-light);
    border-radius: 6px;
    background: var(--wc-bg2);
    color: var(--wc-text2);
    font-size: 11px;
    cursor: pointer;
    transition: color 0.14s ease, border-color 0.14s ease;
  }
  .rg-reset-btn:hover {
    color: var(--wc-theme, #576b95);
    border-color: color-mix(in srgb, var(--wc-theme, #576b95) 48%, var(--wc-border));
  }
  /* ─── 三段式控制区：数据 / 外观 / 力度 ─── */
  .rg-ctl-block {
    flex: 1 1 0;
    min-width: 180px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 10px;
    border: 1px solid var(--wc-border-light);
    border-radius: 9px;
    background: color-mix(in srgb, var(--wc-bg2) 55%, transparent);
  }
  .rg-ctl-block:first-child { flex: 1.35 1 0; }
  .rg-ctl-block-title {
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.14em;
    color: var(--wc-muted);
    text-transform: uppercase;
  }
  .rg-ctl-block-body {
    flex: 1;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px 12px;
  }
  .rg-ctl-switches { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .rg-slider-wide { min-width: 150px; }
  .rg-slider {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 120px;
    max-width: 190px;
    flex: 1;
  }
  .rg-slider-top { display: flex; justify-content: space-between; font-size: 11.5px; color: var(--wc-muted); }
  .rg-slider-top b { color: var(--wc-text); }
  .rg-slider input {
    width: 100%;
    accent-color: var(--wc-theme, #576b95);
  }
  .rg-toggle {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    border: none;
    background: transparent;
    color: var(--wc-text2);
    font-size: 12px;
    cursor: pointer;
  }
  .rg-toggle-track {
    width: 34px;
    height: 18px;
    border-radius: 999px;
    background: var(--wc-bg2);
    border: 1px solid var(--wc-border);
    position: relative;
    transition: background 0.15s;
  }
  .rg-toggle.on .rg-toggle-track { background: var(--wc-theme, #576b95); border-color: var(--wc-theme, #576b95); }
  .rg-toggle-knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #fff;
    transition: transform 0.15s;
  }
  .rg-toggle.on .rg-toggle-knob { transform: translateX(16px); }
  .rg-mask {
    position: fixed;
    inset: 0;
    z-index: 120;
    background: rgba(0, 0, 0, 0.5);
    display: grid;
    place-items: center;
    padding: 24px;
  }
  .rg-picker {
    width: min(460px, 92vw);
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    background: var(--wc-card);
    border: 1px solid var(--wc-border);
    border-radius: 12px;
    overflow: hidden;
  }
  .rg-picker-hd {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--wc-border-light);
  }
  .rg-picker-hd b { color: var(--wc-text); }
  .rg-picker-modes { display: flex; gap: 4px; margin-left: auto; }
  .rg-picker-modes button {
    border: 1px solid var(--wc-border);
    background: transparent;
    color: var(--wc-text2);
    font-size: 11.5px;
    padding: 3px 8px;
    border-radius: 6px;
    cursor: pointer;
  }
  .rg-picker-modes button.on { background: var(--wc-theme, #576b95); color: #fff; }
  .rg-picker-close { border: none; background: transparent; color: var(--wc-muted); cursor: pointer; }
  .rg-picker-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .rg-pick-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 8px;
    border-radius: 7px;
    cursor: pointer;
  }
  .rg-pick-row:hover { background: var(--wc-item-hover); }
  .rg-pick-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--wc-text); font-size: 12px; }
  .rg-pick-meta { font-size: 11.5px; color: var(--wc-muted); }
  .rg-picker-empty { padding: 24px; text-align: center; color: var(--wc-muted); font-size: 12px; }
</style>
