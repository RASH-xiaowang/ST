<!--
  社交关系图谱 — Canvas 力导向渲染（移植自 WeQ GraphCanvas）
  能力：d3-force 布局（链接距离/强度、斥力、碰撞、居中）、社区着色 + 头像剪裁、
  悬停聚焦/淡化、拖拽节点、平移、滚轮缩放、点击选中、悬浮提示。
-->
<script lang="ts">
  import {
    forceCenter,
    forceCollide,
    forceLink,
    forceManyBody,
    forceSimulation,
    type Simulation,
  } from "d3-force";
  import { communityColor, SELF_ID, type BuiltGraph, type GEdge, type GNode, type GraphSettings } from "./graphModel";
  import { fetchImageDataUrl } from "../services/ipc";

  // 头像 data URL 会话级缓存（后端下载结果复用，避免每次导出重复请求）
  const avatarDataCache = new Map<string, Promise<string | null>>();

  async function mapLimit<T, R>(items: T[], limit: number, fn: (t: T) => Promise<R>): Promise<R[]> {
    const results: R[] = new Array(items.length);
    let i = 0;
    const workers = Array.from({ length: Math.min(limit, items.length) }, async () => {
      while (i < items.length) {
        const idx = i++;
        results[idx] = await fn(items[idx]);
      }
    });
    await Promise.all(workers);
    return results;
  }

  let {
    graph,
    selectedId = null,
    onSelect = () => {},
    settings,
  }: {
    graph: BuiltGraph;
    selectedId?: string | null;
    onSelect?: (node: GNode | null) => void;
    settings: GraphSettings;
  } = $props();

  let wrapEl: HTMLDivElement | undefined = $state();
  let canvasEl: HTMLCanvasElement | undefined = $state();
  let tooltipEl: HTMLDivElement | undefined = $state();

  // 仅用于绘制/指针逻辑，不参与模板响应式；若为 $state 会在重建仿真的
  // $effect 里“读→写”自身形成无限循环（effect_update_depth_exceeded）。
  let simulation: Simulation<GNode, unknown> | null = null;
  let hoverNode = $state<GNode | null>(null);

  // 视图与尺寸仅在 draw/仿真逻辑中使用，不参与模板响应式（避免 effect 互相触发循环）
  const view = { scale: 0.85, x: 0, y: 0 };
  const size = { w: 0, h: 0, dpr: 1 };
  const imgCache = new Map<string, HTMLImageElement>();
  let drawRaf = 0;
  let hoverRef: GNode | null = null;
  const selectedRef = $derived(selectedId);
  let dragNode: GNode | null = null;
  let pan: { x: number; y: number } | null = null;
  let moved = 0;
  /** self 节点固定在世界坐标原点（画布中心经 centerView 映射后即为可视中心） */
  let selfNode: GNode | null = null;
  /** 面板是否在视口内（切走页面/折叠时暂停） */
  let inView = $state(true);
  /** 窗口是否可见（最小化时暂停） */
  let pageVisible = $state(true);
  const visible = $derived(inView && pageVisible);
  let lastTickDraw = 0;

  function scheduleDraw() {
    if (drawRaf || !visible || document.hidden) return;
    drawRaf = requestAnimationFrame(() => {
      drawRaf = 0;
      draw();
    });
  }

  /** 暂停：停止仿真并取消排队帧（面板不可见时调用） */
  function pauseSimulation() {
    simulation?.stop();
    if (drawRaf) {
      cancelAnimationFrame(drawRaf);
      drawRaf = 0;
    }
  }

  function getImage(url: string): HTMLImageElement | null {
    if (!url) return null;
    const cached = imgCache.get(url);
    if (cached) return cached.complete && cached.naturalWidth > 0 ? cached : null;
    const img = new Image();
    img.referrerPolicy = "no-referrer";
    img.onload = scheduleDraw;
    img.src = url;
    imgCache.set(url, img);
    return null;
  }

  /** 加载一张图片并等待完成（导出前预载头像，避免首帧缺图） */
  function loadImageOnce(url: string): Promise<HTMLImageElement | null> {
    return new Promise((res) => {
      const cached = imgCache.get(url);
      if (cached && cached.complete && cached.naturalWidth > 0) return res(cached);
      const img = new Image();
      img.referrerPolicy = "no-referrer";
      img.onload = () => {
        imgCache.set(url, img);
        res(img);
      };
      img.onerror = () => res(null);
      img.src = url;
    });
  }

  /** 后端下载头像并缓存为 data URL（失败返回 null 并缓存失败结果） */
  function ensureAvatarData(url: string): Promise<string | null> {
    // data URL（如「我」的本地头像）无需下载，直接可用且不会污染画布
    if (url.startsWith("data:")) return Promise.resolve(url);
    let p = avatarDataCache.get(url);
    if (!p) {
      p = fetchImageDataUrl(url)
        .then((d) => (d && d.startsWith("data:") ? d : null))
        .catch(() => null);
      avatarDataCache.set(url, p);
    }
    return p;
  }

  /** 为有头像的节点批量获取 data URL（并发 6，避免瞬时打爆 CDN） */
  async function loadAvatarOverrides(nodes: GNode[]): Promise<Map<string, string>> {
    const map = new Map<string, string>();
    const urls = [...new Set(nodes.map((n) => n.avatarUrl).filter((u): u is string => !!u))];
    const results = await mapLimit(urls, 6, async (u) => ({ u, d: await ensureAvatarData(u) }));
    for (const { u, d } of results) if (d) map.set(u, d);
    return map;
  }

  function centerView() {
    // 「我」固定在世界原点 (0,0)，画布变换为 translate(view) + scale(s)，
    // 因此世界原点映射到屏幕 (view.x, view.y)。要让它恒在画布正中，
    // view 必须直接等于画布中心坐标，与缩放比无关。
    view.x = size.w / 2;
    view.y = size.h / 2;
  }

  function screenToWorld(sx: number, sy: number) {
    return { x: (sx - view.x) / view.scale, y: (sy - view.y) / view.scale };
  }

  function nodeAt(sx: number, sy: number): GNode | null {
    if (!simulation) return null;
    const { x, y } = screenToWorld(sx, sy);
    const nodes = simulation.nodes();
    const self = selfNode;
    for (let i = nodes.length - 1; i >= 0; i--) {
      const n = nodes[i];
      if (n.kind === "self") continue;
      const dx = n.x - x;
      const dy = n.y - y;
      if (dx * dx + dy * dy <= (n.radius + 2) * (n.radius + 2)) return n;
    }
    if (self) {
      const dx = 0 - x;
      const dy = 0 - y;
      if (dx * dx + dy * dy <= (self.radius + 2) * (self.radius + 2)) return self;
    }
    return null;
  }

  function draw() {
    if (!canvasEl || !simulation) return;
    const ctx = canvasEl.getContext("2d");
    if (!ctx) return;
    paint(ctx, size.w, size.h, size.dpr, view, false);
  }

  /**
   * 把当前力导向布局绘制到任意 canvas 上下文（屏幕绘制与导出共用同一套逻辑）。
   * `skipAvatars`：导出时跳过头像图片。头像来自微信 CDN 等跨域地址，
   * 直接 drawImage 会让画布变成 "tainted"，导致 toBlob/toDataURL 抛 SecurityError。
   */
  function paint(
    ctx: CanvasRenderingContext2D,
    w: number,
    h: number,
    dpr: number,
    v: { scale: number; x: number; y: number },
    skipAvatars = false,
    nodeScale = 1,
    avatarOverrides?: Map<string, string>,
    showLabels = true,
  ) {
    if (!simulation) return;
    const labelFill = "#c8d0da";
    const labelHalo = "rgba(8,10,13,0.85)";

    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    ctx.translate(v.x, v.y);
    ctx.scale(v.scale, v.scale);

    const nodes = simulation.nodes();
    const index = new Map(nodes.map((n) => [n.id, n]));
    const focus = hoverRef?.id ?? selectedRef ?? null;
    const focusNeighbours = new Set<string>();
    if (focus) {
      for (const e of graph.edges) {
        const es = e.source;
        const et = e.target;
        const s = typeof es === "object" ? es.id : es;
        const t = typeof et === "object" ? et.id : et;
        if (s === focus) focusNeighbours.add(t);
        else if (t === focus) focusNeighbours.add(s);
      }
    }

    // 边绘制批处理：按 (active, isSelfEdge, 线宽档) 分组，
    // 组内合并为单条 path 一次 stroke——数千条边时 stroke 调用
    // 从 O(E) 降到 O(档位数)，是画布大图的主要性能优化。
    const edgeGroups = new Map<string, { sx0: number; sy0: number; tx0: number; ty0: number }[]>();
    const arrowGroups = new Map<string, { ux: number; uy: number; tipX: number; tipY: number }[]>();
    const groupKey = (active: boolean, isSelfEdge: boolean, width: number) =>
      `${active ? "a" : "n"}${isSelfEdge ? "s" : "e"}${Math.round(width * 4) / 4}`;
    for (const e of graph.edges) {
      const es = e.source;
      const et = e.target;
      const s = typeof es === "object" ? es : index.get(es);
      const t = typeof et === "object" ? et : index.get(et);
      if (!s || !t) continue;
      const active = focus && (s.id === focus || t.id === focus);
      const isSelfEdge = s.id === SELF_ID || t.id === SELF_ID;
      // self 恒为世界原点：与它相连的边端点固定指向中心，避免 self 漂移后线错位
      const sx0 = s.kind === "self" ? 0 : s.x;
      const sy0 = s.kind === "self" ? 0 : s.y;
      const tx0 = t.kind === "self" ? 0 : t.x;
      const ty0 = t.kind === "self" ? 0 : t.y;
      const width = active
        ? Math.min(1 + e.weight * 0.5, 4) * settings.edgeWidth
        : isSelfEdge
          ? 0.5 * settings.edgeWidth
          : Math.min(0.6 + e.weight * 0.25, 2.4) * settings.edgeWidth;
      const key = groupKey(!!active, isSelfEdge, width);
      let g = edgeGroups.get(key);
      if (!g) {
        g = [];
        edgeGroups.set(key, g);
      }
      g.push({ sx0, sy0, tx0, ty0 });
      if (settings.showArrows) {
        const dx = tx0 - sx0;
        const dy = ty0 - sy0;
        const len = Math.hypot(dx, dy);
        if (len > 1e-6) {
          const ux = dx / len;
          const uy = dy / len;
          const tipX = tx0 - ux * (t.radius * nodeScale + 2);
          const tipY = ty0 - uy * (t.radius * nodeScale + 2);
          let ag = arrowGroups.get(key);
          if (!ag) {
            ag = [];
            arrowGroups.set(key, ag);
          }
          ag.push({ ux, uy, tipX, tipY });
        }
      }
    }
    for (const [key, list] of edgeGroups) {
      const active = key.startsWith("a");
      const isSelfEdge = key[1] === "s";
      ctx.strokeStyle = active
        ? "rgba(0,153,255,0.55)"
        : isSelfEdge
          ? "rgba(120,140,165,0.07)"
          : "rgba(120,140,165,0.18)";
      ctx.lineWidth = Number(key.slice(2));
      ctx.beginPath();
      for (const g of list) {
        ctx.moveTo(g.sx0, g.sy0);
        ctx.lineTo(g.tx0, g.ty0);
      }
      ctx.stroke();
    }
    for (const [key, list] of arrowGroups) {
      const active = key.startsWith("a");
      const isSelfEdge = key[1] === "s";
      ctx.fillStyle = active
        ? "rgba(0,153,255,0.7)"
        : isSelfEdge
          ? "rgba(120,140,165,0.16)"
          : "rgba(120,140,165,0.34)";
      ctx.beginPath();
      const arrowLen = 7 * settings.edgeWidth;
      for (const a of list) {
        const bx = -a.uy;
        const by = a.ux;
        ctx.moveTo(a.tipX, a.tipY);
        ctx.lineTo(a.tipX - a.ux * arrowLen + bx * arrowLen * 0.5, a.tipY - a.uy * arrowLen + by * arrowLen * 0.5);
        ctx.lineTo(a.tipX - a.ux * arrowLen - bx * arrowLen * 0.5, a.tipY - a.uy * arrowLen - by * arrowLen * 0.5);
        ctx.closePath();
      }
      ctx.fill();
    }

    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    for (const n of nodes) {
      const isSelfNode = n.kind === "self";
      const color = isSelfNode ? "#0a7fd0" : communityColor(n.community);
      const dim = focus && n.id !== focus && !focusNeighbours.has(n.id);
      const r = (n.radius ?? 8) * nodeScale;
      const nx = isSelfNode ? 0 : n.x;
      const ny = isSelfNode ? 0 : n.y;

      ctx.globalAlpha = dim ? 0.35 : 1;
      ctx.beginPath();
      ctx.arc(nx, ny, r, 0, Math.PI * 2);
      ctx.fillStyle = color;
      ctx.fill();

      // 导出时只允许绘制 data URL 头像（避免跨域图片污染画布）；
      // 屏幕上则沿用原始头像地址。
      let avatarUrl: string | null = null;
      if (n.avatarUrl && !skipAvatars) {
        avatarUrl = avatarOverrides
          ? avatarOverrides.get(n.avatarUrl) ?? null
          : n.avatarUrl;
      }
      const img = avatarUrl ? getImage(avatarUrl) : null;
      if (img) {
        ctx.save();
        ctx.beginPath();
        ctx.arc(nx, ny, r - 1.5, 0, Math.PI * 2);
        ctx.clip();
        ctx.drawImage(img, nx - r, ny - r, r * 2, r * 2);
        ctx.restore();
      } else if (showLabels) {
        ctx.fillStyle = "#ffffff";
        ctx.font = `600 ${Math.round(r)}px var(--font-sans, sans-serif)`;
        ctx.fillText((n.label || "?").slice(0, 1), nx, ny + 1);
      }

      const isFocus = n.id === focus;
      ctx.lineWidth = isFocus || isSelfNode ? 3 : 1.5;
      ctx.strokeStyle = isFocus ? "#0099ff" : isSelfNode ? "#0a7fd0" : color;
      ctx.globalAlpha = dim ? 0.4 : 1;
      ctx.beginPath();
      ctx.arc(nx, ny, r, 0, Math.PI * 2);
      ctx.stroke();

      if (showLabels && !dim && (isSelfNode || isFocus || focusNeighbours.has(n.id) || r >= 22 || v.scale >= 1.1)) {
        ctx.globalAlpha = settings.labelOpacity;
        const fontPx = Math.max(10, 11 / v.scale);
        ctx.font = `500 ${fontPx}px var(--font-sans, sans-serif)`;
        const label = n.label.length > 12 ? `${n.label.slice(0, 12)}…` : n.label;
        ctx.lineWidth = 3 / v.scale;
        ctx.strokeStyle = labelHalo;
        ctx.strokeText(label, nx, ny + r + fontPx * 0.9);
        ctx.fillStyle = labelFill;
        ctx.fillText(label, nx, ny + r + fontPx * 0.9);
      }
    }
    ctx.globalAlpha = 1;
  }

  /**
   * 渲染图谱图层（深色卡片底、自动取景整图、跳过跨域头像）。
   * 供海报/分享图导出复用：返回未污染的离屏 canvas，由调用方排版组合。
   */
  export async function renderGraphLayer(
    width: number,
    height: number,
    scale = 3,
    nodeScale = 1.3,
    withAvatars = true,
  ): Promise<HTMLCanvasElement> {
    if (!canvasEl || !simulation) throw new Error("图谱尚未就绪");
    const nodes = simulation.nodes();
    // 导出头像：后端下载为 data URL（不污染画布），失败节点回退为彩色圆
    const overrides = withAvatars ? await loadAvatarOverrides(nodes) : new Map<string, string>();
    if (overrides.size > 0) {
      await Promise.all([...overrides.values()].map((u) => loadImageOnce(u)));
    }
    // 世界坐标包围盒（self 恒在原点）
    let minX = Infinity;
    let maxX = -Infinity;
    let minY = Infinity;
    let maxY = -Infinity;
    for (const n of nodes) {
      const x = n.kind === "self" ? 0 : n.x ?? 0;
      const y = n.kind === "self" ? 0 : n.y ?? 0;
      const r = (n.radius ?? 8) * nodeScale + 4;
      minX = Math.min(minX, x - r);
      maxX = Math.max(maxX, x + r);
      minY = Math.min(minY, y - r);
      maxY = Math.max(maxY, y + r);
    }
    if (![minX, maxX, minY, maxY].every(Number.isFinite)) {
      // 布局尚未完成（节点坐标 NaN）时退回默认范围
      minX = -200;
      maxX = 200;
      minY = -200;
      maxY = 200;
    }
    const w = width || 800;
    const h = height || 600;
    const pad = Math.max(30, Math.min(w, h) * 0.04);
    const bw = Math.max(maxX - minX, 1);
    const bh = Math.max(maxY - minY, 1);
    // 整图适配导出画布，放大倍数上限 2.5（节点尽量大、更清晰）
    const fitScale = Math.min((w - pad * 2) / bw, (h - pad * 2) / bh);
    const viewScale = Math.max(0.1, Math.min(fitScale, 2.5));
    // 导出分辨率上限 8192px（8K），尽可能清晰
    const outScale = Math.max(1, Math.min(scale, 8192 / Math.max(w, h)));
    const off = document.createElement("canvas");
    off.width = Math.max(1, Math.round(w * outScale));
    off.height = Math.max(1, Math.round(h * outScale));
    const octx = off.getContext("2d");
    if (!octx) throw new Error("无法创建导出画布");
    // 深色背景（与图谱画布一致，避免透明底）
    octx.fillStyle = "#0b0e13";
    octx.fillRect(0, 0, off.width, off.height);
    const cx = (minX + maxX) / 2;
    const cy = (minY + maxY) / 2;
    const v = {
      scale: viewScale,
      x: w / 2 - cx * viewScale,
      y: h / 2 - cy * viewScale,
    };
    paint(octx, w, h, outScale, v, !withAvatars, nodeScale, overrides, false);
    return off;
  }

  /**
   * 生成图谱矢量图（SVG）：节点/连线/标签全部为矢量元素，
   * 任意缩放都清晰；同样跳过跨域头像。
   */
  export async function renderGraphSvg(width: number, height: number, nodeScale = 1.3): Promise<string> {
    if (!simulation) throw new Error("图谱尚未就绪");
    const nodes = simulation.nodes();
    const overrides = await loadAvatarOverrides(nodes);
    let minX = Infinity;
    let maxX = -Infinity;
    let minY = Infinity;
    let maxY = -Infinity;
    for (const n of nodes) {
      const x = n.kind === "self" ? 0 : n.x ?? 0;
      const y = n.kind === "self" ? 0 : n.y ?? 0;
      const r = (n.radius ?? 8) * nodeScale + 4;
      minX = Math.min(minX, x - r);
      maxX = Math.max(maxX, x + r);
      minY = Math.min(minY, y - r);
      maxY = Math.max(maxY, y + r);
    }
    if (![minX, maxX, minY, maxY].every(Number.isFinite)) {
      minX = -200;
      maxX = 200;
      minY = -200;
      maxY = 200;
    }
    const w = width || 1200;
    const h = height || 720;
    const pad = Math.max(24, Math.min(w, h) * 0.04);
    const bw = Math.max(maxX - minX, 1);
    const bh = Math.max(maxY - minY, 1);
    const s = Math.min((w - pad * 2) / bw, (h - pad * 2) / bh);
    const cx = (minX + maxX) / 2;
    const cy = (minY + maxY) / 2;
    const tx = w / 2 - cx * s;
    const ty = h / 2 - cy * s;
    const esc = (v: string) =>
      v.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
    const parts: string[] = [];
    parts.push(
      `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}" ` +
        `font-family="-apple-system,'PingFang SC','Microsoft YaHei','Segoe UI',sans-serif">`,
    );
    parts.push(`<rect width="${w}" height="${h}" fill="#0b0e13"/>`);
    // 头像剪裁路径
    const clipDefs: string[] = [];
    nodes.forEach((n, i) => {
      if (!n.avatarUrl || !overrides.has(n.avatarUrl)) return;
      const isSelf = n.kind === "self";
      const r = (n.radius ?? 8) * nodeScale - 1.5;
      const nx = isSelf ? 0 : n.x;
      const ny = isSelf ? 0 : n.y;
      clipDefs.push(
        `<clipPath id="ava${i}"><circle cx="${nx.toFixed(2)}" cy="${ny.toFixed(2)}" r="${r.toFixed(2)}"/></clipPath>`,
      );
    });
    parts.push(`<defs>${clipDefs.join("")}</defs>`);
    parts.push(`<g transform="translate(${tx.toFixed(2)} ${ty.toFixed(2)}) scale(${s.toFixed(4)})">`);

    const index = new Map(nodes.map((n) => [n.id, n]));
    for (const e of graph.edges) {
      const es = e.source;
      const et = e.target;
      const s2 = typeof es === "object" ? es : index.get(es);
      const t2 = typeof et === "object" ? et : index.get(et);
      if (!s2 || !t2) continue;
      const isSelfEdge = s2.id === SELF_ID || t2.id === SELF_ID;
      const width2 = isSelfEdge
        ? 0.5 * settings.edgeWidth
        : Math.min(0.6 + (e.weight ?? 1) * 0.25, 2.4) * settings.edgeWidth;
      const stroke = isSelfEdge ? "rgba(120,140,165,0.16)" : "rgba(120,140,165,0.22)";
      const x1 = s2.kind === "self" ? 0 : s2.x;
      const y1 = s2.kind === "self" ? 0 : s2.y;
      const x2 = t2.kind === "self" ? 0 : t2.x;
      const y2 = t2.kind === "self" ? 0 : t2.y;
      parts.push(
        `<line x1="${x1.toFixed(2)}" y1="${y1.toFixed(2)}" x2="${x2.toFixed(2)}" y2="${y2.toFixed(2)}" ` +
          `stroke="${stroke}" stroke-width="${width2.toFixed(2)}"/>`,
      );
    }

    nodes.forEach((n, i) => {
      const isSelf = n.kind === "self";
      const r = (n.radius ?? 8) * nodeScale;
      const nx = isSelf ? 0 : n.x;
      const ny = isSelf ? 0 : n.y;
      const color = isSelf ? "#0a7fd0" : communityColor(n.community);
      parts.push(
        `<circle cx="${nx.toFixed(2)}" cy="${ny.toFixed(2)}" r="${r.toFixed(2)}" fill="${color}" ` +
          `stroke="${isSelf ? "#0a7fd0" : color}" stroke-width="1.5"/>`,
      );
      const dataUrl = n.avatarUrl ? overrides.get(n.avatarUrl) : undefined;
      if (dataUrl) {
        parts.push(
          `<image href="${esc(dataUrl)}" x="${(nx - r).toFixed(2)}" y="${(ny - r).toFixed(2)}" ` +
            `width="${(r * 2).toFixed(2)}" height="${(r * 2).toFixed(2)}" ` +
            `clip-path="url(#ava${i})" preserveAspectRatio="xMidYMid slice"/>`,
        );
      }
    });
    parts.push("</g></svg>");
    return parts.join("");
  }

  // 力导向仿真：graph 重建时保留旧节点位置，避免整图重排
  $effect(() => {
    const prev = simulation;
    let freshLayout = false;
    if (prev) {
      const prevNodes = new Map(prev.nodes().map((n) => [n.id, n]));
      let same = 0;
      for (const n of graph.nodes) {
        if (prevNodes.has(n.id)) same++;
      }
      // 节点集合大幅变化（模式切换/群过滤大改）时采用全新布局并重新居中，
      // 避免新旧坐标混合导致力导向能量爆开；小幅过滤/阈值调整仍保留旧位置。
      freshLayout = same / Math.max(graph.nodes.length, 1) < 0.5;
      if (freshLayout) {
        // 预热布局：按社区分组环形摆放（self 恒在原点）。
        // 初始位置接近力导向稳态，收敛 tick 数从数百降到数十，
        // 显著缩短打开/切换后的布局演化期（主要 CPU 开销来源）。
        const self = graph.nodes.find((n) => n.kind === "self");
        const others = graph.nodes.filter((n) => n.kind !== "self");
        const byComm = new Map<number, GNode[]>();
        for (const o of others) {
          const c = o.community ?? 0;
          let list = byComm.get(c);
          if (!list) {
            list = [];
            byComm.set(c, list);
          }
          list.push(o);
        }
        const commGroups = [...byComm.entries()];
        commGroups.sort((a, b) => b[1].length - a[1].length);
        const groupCount = Math.max(commGroups.length, 1);
        // 半径随分组数增长（约 sqrt(N) 保证密度），社区环错开 60° 起始角
        const baseR = 70 + Math.sqrt(Math.max(graph.nodes.length, 1)) * 6;
        commGroups.forEach(([, list], gi) => {
          const angle0 = (gi / groupCount) * Math.PI * 2 + (gi % 2) * 0.6;
          const ring = baseR * (1 + 0.35 * (gi % 3));
          list.forEach((o, i) => {
            const a = angle0 + (i / Math.max(list.length, 1)) * 1.15;
            o.x = Math.cos(a) * ring;
            o.y = Math.sin(a) * ring;
            o.vx = 0;
            o.vy = 0;
          });
        });
        if (self) {
          self.x = 0;
          self.y = 0;
          self.vx = 0;
          self.vy = 0;
        }
      } else {
        for (const n of graph.nodes) {
          const p = prevNodes.get(n.id);
          if (p && p.x != null && p.y != null) {
            n.x = p.x;
            n.y = p.y;
            n.vx = p.vx;
            n.vy = p.vy;
          }
        }
      }
      prev.stop();
    }

    const degree = new Map<string, number>();
    const bump = (id: string) => degree.set(id, (degree.get(id) ?? 0) + 1);
    for (const e of graph.edges) {
      const es = e.source;
      const et = e.target;
      bump(typeof es === "object" ? es.id : es);
      bump(typeof et === "object" ? et.id : et);
    }
    const linkStrength = (e: GEdge) => {
      // 相连节点吸引力：相对倍率（默认 1 = 原布局），对所有边（含「我」的枢纽边）统一缩放
      if (e.strength != null) return e.strength * settings.forceAttraction;
      const s = typeof e.source === "object" ? e.source.id : e.source;
      const t = typeof e.target === "object" ? e.target.id : e.target;
      return (1 / Math.min(degree.get(s) ?? 1, degree.get(t) ?? 1)) * settings.forceAttraction;
    };

    // 预热环布局已接近稳态：初始 alpha 降低 + 衰减加快，
    // 布局稳定时间从数百 tick 减半，且不牺牲最终质量
    const sim = forceSimulation<GNode>(graph.nodes)
      .alpha(freshLayout ? 0.6 : 0.3)
      .alphaDecay(0.035)
      .force("link", forceLink<GNode, GEdge>(graph.edges).id((d) => d.id).distance((d) => d.dist).strength(linkStrength))
      .force("charge", forceManyBody<GNode>().strength(-300 * settings.forceRepulsion))
      .force("collide", forceCollide<GNode>().radius((d) => d.radius + 3).iterations(1))
      // 「我」锚定中心：center 力以 self（世界原点）为圆心聚拢，保证整图围绕「我」
      .force("center", forceCenter(0, 0).strength(0.1 * settings.forceCentripetal));
    // 「我」钉在世界原点：力导向布局围绕中心展开，self 不再被推开
    selfNode = graph.nodes.find((n) => n.kind === "self") ?? null;
    if (selfNode) {
      selfNode.x = 0;
      selfNode.y = 0;
      selfNode.vx = 0;
      selfNode.vy = 0;
      selfNode.fx = 0;
      selfNode.fy = 0;
    }
    lastTickDraw = 0;
    sim.on("tick", () => {
      // 仿真期间限流绘制：拖拽时 60fps（跟手），平时 30fps（省 CPU）
      const now = performance.now();
      const budget = dragNode ? 16 : 33;
      if (now - lastTickDraw >= budget) {
        lastTickDraw = now;
        scheduleDraw();
      }
    });
    simulation = sim;
    if (!visible || document.hidden) sim.stop();
    // 播放动画：关闭时冻结布局（仍可拖拽/平移，但力导向不再自动演化）
    if (!settings.motion) sim.stop();
    if (freshLayout && size.w > 0) {
      centerView();
      draw();
    }

    return () => {
      sim.stop();
      if (drawRaf) {
        cancelAnimationFrame(drawRaf);
        drawRaf = 0;
      }
    };
  });

  // 尺寸自适应（DPR + ResizeObserver）
  $effect(() => {
    const wrap = wrapEl;
    const canvas = canvasEl;
    if (!wrap || !canvas) return;
    const applySize = () => {
      const rect = wrap.getBoundingClientRect();
      if (rect.width < 2 || rect.height < 2) return; // 隐藏/折叠时不重建画布
      // 画布 DPR 上限 1.5：高 DPI 屏上减少每帧像素量
      const dpr = Math.min(window.devicePixelRatio || 1, 1.5);
      size.w = rect.width;
      size.h = rect.height;
      size.dpr = dpr;
      canvas.width = Math.max(1, Math.round(rect.width * dpr));
      canvas.height = Math.max(1, Math.round(rect.height * dpr));
      canvas.style.width = `${rect.width}px`;
      canvas.style.height = `${rect.height}px`;
      simulation?.force("center", forceCenter(0, 0));
      if (visible && settings.motion) simulation?.alpha(0.3).restart();
      // 画布尺寸变化（含首次挂载）后，让「我」（世界原点）始终位于新画布正中
      centerView();
    };
    applySize();
    const ro = new ResizeObserver(applySize);
    ro.observe(wrap);
    return () => ro.disconnect();
  });

  // 可见性监听：面板切走 / 窗口最小化时暂停仿真，回来再继续
  $effect(() => {
    const el = wrapEl;
    if (!el || typeof IntersectionObserver === "undefined") return;
    const obs = new IntersectionObserver((entries) => {
      inView = entries.some((e) => e.isIntersecting);
    });
    obs.observe(el);
    return () => obs.disconnect();
  });

  $effect(() => {
    const onVis = () => {
      pageVisible = !document.hidden;
    };
    document.addEventListener("visibilitychange", onVis);
    return () => document.removeEventListener("visibilitychange", onVis);
  });

  $effect(() => {
    if (visible && !document.hidden && settings.motion) {
      if (simulation) {
        simulation.alpha(0.3).restart();
        scheduleDraw();
      }
    } else {
      pauseSimulation();
    }
  });

  function onPointerDown(e: PointerEvent) {
    if (!wrapEl) return;
    const rect = wrapEl.getBoundingClientRect();
    const sx = e.clientX - rect.left;
    const sy = e.clientY - rect.top;
    (e.target as Element).setPointerCapture?.(e.pointerId);
    moved = 0;
    const hit = nodeAt(sx, sy);
    if (hit) {
      dragNode = hit;
      const w = screenToWorld(sx, sy);
      hit.fx = w.x;
      hit.fy = w.y;
      simulation?.alphaTarget(0.3).restart();
    } else {
      pan = { x: e.clientX, y: e.clientY };
    }
  }

  function onPointerMove(e: PointerEvent) {
    if (!wrapEl) return;
    const rect = wrapEl.getBoundingClientRect();
    const sx = e.clientX - rect.left;
    const sy = e.clientY - rect.top;
    if (dragNode) {
      moved += Math.abs(e.movementX) + Math.abs(e.movementY);
      const w = screenToWorld(sx, sy);
      dragNode.fx = w.x;
      dragNode.fy = w.y;
      return;
    }
    if (pan) {
      moved += Math.abs(e.movementX) + Math.abs(e.movementY);
      view.x += e.clientX - pan.x;
      view.y += e.clientY - pan.y;
      pan = { x: e.clientX, y: e.clientY };
      scheduleDraw();
      return;
    }
    const hit = nodeAt(sx, sy);
    if (hit !== hoverRef) {
      hoverRef = hit;
      hoverNode = hit;
      scheduleDraw();
    }
    if (hit && tooltipEl) {
      tooltipEl.style.left = `${sx + 14}px`;
      tooltipEl.style.top = `${sy + 14}px`;
    }
    if (wrapEl) wrapEl.style.cursor = hit ? "pointer" : "grab";
  }

  function endPointer() {
    const node = dragNode;
    if (node) {
      if (moved < 4) onSelect(node);
      node.fx = null;
      node.fy = null;
      simulation?.alphaTarget(0);
    } else if (pan && moved < 4) {
      onSelect(null);
    }
    dragNode = null;
    pan = null;
  }

  function onWheel(e: WheelEvent) {
    if (!wrapEl) return;
    const rect = wrapEl.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;
    const factor = e.deltaY < 0 ? 1.12 : 1 / 1.12;
    const next = Math.min(Math.max(view.scale * factor, 0.2), 4);
    const wx = (mx - view.x) / view.scale;
    const wy = (my - view.y) / view.scale;
    view.scale = next;
    view.x = mx - wx * next;
    view.y = my - wy * next;
    // 滚轮事件高频触发：走 rAF 合并，避免每事件同步全量重绘阻塞主线程
    scheduleDraw();
  }
</script>

<div
  class="gx-canvas-wrap"
  bind:this={wrapEl}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={endPointer}
  onpointerleave={endPointer}
  onwheel={onWheel}
  role="img"
  aria-label="社交关系图谱"
>
  <canvas bind:this={canvasEl} class="gx-canvas"></canvas>
  <div class="gx-tooltip" bind:this={tooltipEl} style:display={hoverNode ? "block" : "none"}>
    {#if hoverNode}
      <strong>{hoverNode.label}</strong>
      {#if hoverNode.kind === "person"}
        <span>
          {hoverNode.isFriend ? "好友" : "群友"} · 共 {hoverNode.groupCount ?? 0} 群
          {hoverNode.intimacy ? ` · 消息 ${hoverNode.intimacy}` : ""}
        </span>
      {:else if hoverNode.kind === "group"}
        <span>{hoverNode.memberCount ?? 0} 人 · 命中 {hoverNode.sharedCount ?? 0} 位</span>
      {:else}
        <span>我</span>
      {/if}
    {/if}
  </div>
</div>

<style>
  .gx-canvas-wrap {
    position: absolute;
    inset: 0;
    overflow: hidden;
    cursor: grab;
    border: 1px solid var(--wc-border-light);
    border-radius: 10px;
    background: color-mix(in srgb, var(--wc-card) 55%, black 35%);
  }
  .gx-canvas {
    display: block;
  }
  .gx-tooltip {
    position: absolute;
    pointer-events: none;
    z-index: 5;
    display: none;
    max-width: 240px;
    padding: 7px 10px;
    border-radius: 8px;
    background: color-mix(in srgb, var(--wc-card) 92%, black 8%);
    border: 1px solid var(--wc-border);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
    font-size: 12px;
    line-height: 1.5;
    color: var(--wc-text);
  }
  .gx-tooltip strong {
    display: block;
    margin-bottom: 2px;
  }
  .gx-tooltip span {
    color: var(--wc-muted);
    font-size: 11.5px;
  }
</style>
