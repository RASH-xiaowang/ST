<!--
  Wiki 知识图谱 — Canvas 力导向渲染（移植自微信「社交关系图谱」GraphCanvas）
  能力：d3-force 布局（链接距离/强度、斥力、碰撞、向心）、社区聚类预热、
  悬停聚焦/淡化、拖拽节点、平移、滚轮缩放、点击选中、双击打开、悬浮提示。
  与社交图谱共用同一套渲染观感：深色画布、描边圆节点、光晕标签、聚焦高亮。
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
  import { seedPositions, type BuiltWikiGraph, type WEdge, type WNode } from "./wikiGraphModel";

  /** 图谱画布设置（与 WikiPanel graphParams 外观/力度字段对应） */
  interface CanvasSettings {
    nodeScale: number;
    edgeWidth: number;
    edgeOpacity: number;
    showLabels: boolean;
    labelOpacity: number;
    motion: boolean;
    showArrows: boolean;
    forceCentripetal: number;
    forceRepulsion: number;
    forceAttraction: number;
    forceEdgeLength: number;
  }

  let {
    graph,
    selectedId = null,
    onSelect = () => {},
    onOpen = () => {},
    settings,
    nodeColor,
    edgeColor,
    edgeDash = () => false,
    tooltip = () => "",
    redrawKey = "",
  }: {
    graph: BuiltWikiGraph;
    selectedId?: number | null;
    onSelect?: (node: WNode | null) => void;
    onOpen?: (node: WNode) => void;
    settings: CanvasSettings;
    nodeColor: (node: WNode) => string;
    edgeColor: (edge: WEdge) => string;
    edgeDash?: (edge: WEdge) => boolean;
    tooltip?: (node: WNode) => string;
    /** 着色模式/颜色组等外观变化时递增，触发立即重绘 */
    redrawKey?: string;
  } = $props();

  let wrapEl: HTMLDivElement | undefined = $state();
  let canvasEl: HTMLCanvasElement | undefined = $state();
  let tooltipEl: HTMLDivElement | undefined = $state();

  // 仅用于绘制/指针逻辑，不参与模板响应式；若为 $state 会在重建仿真的
  // $effect 里“读→写”自身形成无限循环（effect_update_depth_exceeded）。
  let simulation: Simulation<WNode, unknown> | null = null;
  let hoverNode = $state<WNode | null>(null);

  // 视图与尺寸仅在 draw/仿真逻辑中使用，不参与模板响应式（避免 effect 互相触发循环）
  const view = { scale: 0.85, x: 0, y: 0 };
  const size = { w: 0, h: 0, dpr: 1 };
  let drawRaf = 0;
  let hoverRef: WNode | null = null;
  const selectedRef = $derived(selectedId);
  let dragNode: WNode | null = null;
  let pan: { x: number; y: number } | null = null;
  let moved = 0;
  /** 面板是否在视口内（切走页面/折叠时暂停） */
  let inView = $state(true);
  /** 窗口是否可见（最小化时暂停） */
  let pageVisible = $state(true);
  const visible = $derived(inView && pageVisible);
  let lastTickDraw = 0;

  // ── 力度参数映射：Wiki 滑杆语义 → d3-force 力度 ──
  // Wiki 面板持久化的力度字段沿用旧版量纲，这里折算为与社交图谱一致的
  // 力导向参数（默认值下等效于社交图谱：charge -300 / link×1 / center 0.1）。
  const REPULSION_BASE = 2600;  // forceRepulsion 默认 → charge -300
  const ATTRACTION_BASE = 0.04; // forceAttraction 默认 → link 强度倍率 1
  const CENTRIPETAL_BASE = 0.02; // forceCentripetal 默认 → center 强度 0.1

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

/**
 * d3-force 的 forceLink 会把边的 source/target 从 id 字符串替换为节点对象，
 * 绘制/算力时统一取 id（参数声明为联合类型，TS 可在 typeof 分支正确收窄）。
 */
function endId(ep: string | WNode): string {
  return typeof ep === "object" ? ep.id : ep;
}

  function centerView() {
    // 力导向围绕世界原点 (0,0) 聚拢，画布变换为 translate(view) + scale(s)，
    // 因此世界原点映射到屏幕 (view.x, view.y)。要让它恒在画布正中，
    // view 必须直接等于画布中心坐标，与缩放比无关。
    view.x = size.w / 2;
    view.y = size.h / 2;
  }

  function screenToWorld(sx: number, sy: number) {
    return { x: (sx - view.x) / view.scale, y: (sy - view.y) / view.scale };
  }

  function nodeAt(sx: number, sy: number): WNode | null {
    if (!simulation) return null;
    const { x, y } = screenToWorld(sx, sy);
    const nodes = simulation.nodes();
    for (let i = nodes.length - 1; i >= 0; i--) {
      const n = nodes[i];
      const dx = n.x - x;
      const dy = n.y - y;
      if (dx * dx + dy * dy <= (n.radius + 2) * (n.radius + 2)) return n;
    }
    return null;
  }

  function draw() {
    if (!canvasEl || !simulation) return;
    const ctx = canvasEl.getContext("2d");
    if (!ctx) return;
    paint(ctx, size.w, size.h, size.dpr, view);
  }

  /** #rrggbb → rgba()，用于给连线/节点填充叠加透明度 */
  function withAlpha(hex: string, a: number): string {
    const m = /^#([0-9a-f]{6})$/i.exec(hex);
    if (!m) return hex;
    const r = parseInt(m[1].slice(0, 2), 16);
    const g = parseInt(m[1].slice(2, 4), 16);
    const b = parseInt(m[1].slice(4, 6), 16);
    return `rgba(${r},${g},${b},${a})`;
  }

  /** 把当前力导向布局绘制到画布（屏幕绘制与缩放共用同一套逻辑） */
  function paint(
    ctx: CanvasRenderingContext2D,
    w: number,
    h: number,
    dpr: number,
    v: { scale: number; x: number; y: number },
  ) {
    if (!simulation || simulation.nodes().length === 0) return;
    const labelFill = "#c8d0da";
    const labelHalo = "rgba(8,10,13,0.85)";

    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    ctx.translate(v.x, v.y);
    ctx.scale(v.scale, v.scale);

    const nodes = simulation.nodes();
    const indexMap = new Map(nodes.map((n) => [n.id, n]));
    const focus = hoverRef?.id ?? (selectedRef != null ? String(selectedRef) : null);
    const focusNeighbours = new Set<string>();
    if (focus) {
      for (const e of graph.edges) {
        const es = endId(e.source);
        const et = endId(e.target);
        if (es === focus) focusNeighbours.add(et);
        else if (et === focus) focusNeighbours.add(es);
      }
    }

    // 边绘制批处理：按 (active, dash, 颜色, 线宽档) 分组，
    // 组内合并为单条 path 一次 stroke——数千条边时 stroke 调用
    // 从 O(E) 降到 O(档位数)，是画布大图的主要性能优化。
    const edgeGroups = new Map<string, { sx0: number; sy0: number; tx0: number; ty0: number }[]>();
    const arrowGroups = new Map<
      string,
      { ux: number; uy: number; tipX: number; tipY: number; active: boolean; dash: boolean }[]
    >();
    const groupKey = (active: boolean, dash: boolean, color: string, width: number) =>
      `${active ? "a" : "n"}${dash ? "d" : "s"}${color}${Math.round(width * 4) / 4}`;
    for (const e of graph.edges) {
      const es = endId(e.source);
      const et = endId(e.target);
      const s = indexMap.get(es);
      const t = indexMap.get(et);
      if (!s || !t) continue;
      const active = !!focus && (es === focus || et === focus);
      const width = active
        ? Math.min(1.2 + e.weight * 0.4, 3) * settings.edgeWidth
        : Math.max(0.5, Math.min(0.7 + e.weight * 0.25, 2.2) * settings.edgeWidth);
      const color = edgeColor(e);
      const dash = edgeDash(e);
      const key = groupKey(!!active, dash, color, width);
      let g = edgeGroups.get(key);
      if (!g) {
        g = [];
        edgeGroups.set(key, g);
      }
      g.push({ sx0: s.x, sy0: s.y, tx0: t.x, ty0: t.y });
      if (settings.showArrows) {
        const dx = t.x - s.x;
        const dy = t.y - s.y;
        const len = Math.hypot(dx, dy);
        if (len > 1e-6) {
          const ux = dx / len;
          const uy = dy / len;
          const tipX = t.x - ux * (t.radius + 2);
          const tipY = t.y - uy * (t.radius + 2);
          let ag = arrowGroups.get(key);
          if (!ag) {
            ag = [];
            arrowGroups.set(key, ag);
          }
          ag.push({ ux, uy, tipX, tipY, active: !!active, dash });
        }
      }
    }
    for (const [key, list] of edgeGroups) {
      const active = key.startsWith("a");
      const dash = key[1] === "d";
      const color = key.slice(2, 9);
      ctx.strokeStyle = active
        ? withAlpha(color, settings.edgeOpacity)
        : withAlpha(color, settings.edgeOpacity * 0.6);
      ctx.lineWidth = Number(key.slice(9));
      ctx.setLineDash(dash ? [6, 4] : []);
      ctx.beginPath();
      for (const g of list) {
        ctx.moveTo(g.sx0, g.sy0);
        ctx.lineTo(g.tx0, g.ty0);
      }
      ctx.stroke();
    }
    ctx.setLineDash([]);
    for (const [key, list] of arrowGroups) {
      const active = key.startsWith("a");
      const color = key.slice(2, 9);
      ctx.fillStyle = active
        ? withAlpha(color, settings.edgeOpacity)
        : withAlpha(color, settings.edgeOpacity * 0.55);
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
      const color = nodeColor(n);
      const isGhost = n.status === "missing";
      const dim = !!focus && n.id !== focus && !focusNeighbours.has(n.id);
      const r = n.radius;

      // 幽灵节点（尚未创建）：半透明填充 + 虚线描边，弱化存在感
      ctx.globalAlpha = dim ? 0.3 : isGhost ? 0.55 : 1;
      ctx.beginPath();
      ctx.arc(n.x, n.y, r, 0, Math.PI * 2);
      ctx.fillStyle = color;
      ctx.fill();

      const isFocus = n.id === focus;
      ctx.setLineDash(isGhost ? [4, 3] : []);
      ctx.lineWidth = isFocus ? 3 : 1.5;
      ctx.strokeStyle = isFocus ? "#ffffff" : color;
      ctx.globalAlpha = dim ? 0.35 : 1;
      ctx.beginPath();
      ctx.arc(n.x, n.y, r, 0, Math.PI * 2);
      ctx.stroke();
      ctx.setLineDash([]);

      if (
        settings.showLabels &&
        !dim &&
        (isFocus || focusNeighbours.has(n.id) || r >= 20 || v.scale >= 1.05)
      ) {
        ctx.globalAlpha = settings.labelOpacity;
        const fontPx = Math.max(10, 11 / v.scale);
        ctx.font = `500 ${fontPx}px var(--font-sans, sans-serif)`;
        const label = n.label.length > 14 ? `${n.label.slice(0, 14)}…` : n.label;
        ctx.lineWidth = 3 / v.scale;
        ctx.strokeStyle = labelHalo;
        ctx.strokeText(label, n.x, n.y + r + fontPx * 0.9);
        ctx.fillStyle = labelFill;
        ctx.fillText(label, n.x, n.y + r + fontPx * 0.9);
      }
    }
    ctx.globalAlpha = 1;
  }

  // 外观参数 / 着色模式变化时立即重绘（仿真关闭时也能即时生效）
  $effect(() => {
    void redrawKey;
    void settings.nodeScale;
    void settings.edgeWidth;
    void settings.edgeOpacity;
    void settings.showLabels;
    void settings.labelOpacity;
    void settings.showArrows;
    scheduleDraw();
  });

  // 选中节点变化时立即重绘（外部入口设置选中也能即时高亮）
  $effect(() => {
    void selectedRef;
    scheduleDraw();
  });

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
      // 节点集合大幅变化（筛选/模式切换大改）时采用全新布局并重新居中，
      // 避免新旧坐标混合导致力导向能量爆开；小幅过滤/阈值调整仍保留旧位置。
      freshLayout = same / Math.max(graph.nodes.length, 1) < 0.5;
      if (freshLayout) {
        // 预热布局：按社区分组环形摆放。初始位置接近力导向稳态，
        // 收敛 tick 数从数百降到数十，显著缩短布局演化期（主要 CPU 开销来源）。
        seedPositions(graph.nodes);
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
    } else if (graph.nodes.length > 0) {
      // 首次挂载：直接社区环预热，避免节点从原点挤成一团
      seedPositions(graph.nodes);
      freshLayout = true;
    }

    const degree = new Map<string, number>();
    const bump = (id: string) => degree.set(id, (degree.get(id) ?? 0) + 1);
    for (const e of graph.edges) {
      bump(e.source);
      bump(e.target);
    }
    const linkStrength = (e: WEdge) => {
      // 相连节点吸引力：默认值下等效社交图谱（倍率 1），对所有边统一缩放。
      // forceLink 初始化后 source/target 已是节点对象，取 id 再查度数。
      const s = endId(e.source);
      const t = endId(e.target);
      if (e.strength != null) return e.strength * (settings.forceAttraction / ATTRACTION_BASE);
      return (1 / Math.min(degree.get(s) ?? 1, degree.get(t) ?? 1))
        * (settings.forceAttraction / ATTRACTION_BASE);
    };

    const sim = forceSimulation<WNode>(graph.nodes)
      .alpha(freshLayout ? 0.6 : 0.3)
      .alphaDecay(0.035)
      .force("link", forceLink<WNode, WEdge>(graph.edges).id((d) => d.id).distance((d) => d.dist).strength(linkStrength))
      .force("charge", forceManyBody<WNode>().strength(-300 * (settings.forceRepulsion / REPULSION_BASE)))
      .force("collide", forceCollide<WNode>().radius((d) => d.radius + 3).iterations(1))
      .force("center", forceCenter(0, 0).strength(0.1 * (settings.forceCentripetal / CENTRIPETAL_BASE)));
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
      simulation?.force("center", forceCenter(0, 0).strength(0.1 * (settings.forceCentripetal / CENTRIPETAL_BASE)));
      if (visible && settings.motion) simulation?.alpha(0.3).restart();
      // 画布尺寸变化（含首次挂载）后，让世界原点始终位于新画布正中
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

  /** 指针离开画布：清除悬停，避免 tooltip 残留 */
  function onPointerLeave() {
    endPointer();
    if (hoverRef || hoverNode) {
      hoverRef = null;
      hoverNode = null;
      scheduleDraw();
    }
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

  function onDblClick(e: MouseEvent) {
    if (!wrapEl) return;
    const rect = wrapEl.getBoundingClientRect();
    const hit = nodeAt(e.clientX - rect.left, e.clientY - rect.top);
    if (hit) onOpen(hit);
  }

  /** 重置视图：恢复初始缩放并把世界原点放回画布正中 */
  export function resetView() {
    view.scale = 1;
    centerView();
    draw();
  }

  /** 重新布局：按社区重新预热摆放并重启力导向仿真（关闭动画时只重绘一帧） */
  export function relayout() {
    if (!simulation || graph.nodes.length === 0) return;
    seedPositions(simulation.nodes());
    if (settings.motion) {
      simulation.alpha(1).restart();
      scheduleDraw();
    } else {
      draw();
    }
  }
</script>

<div
  class="wk-canvas-wrap"
  bind:this={wrapEl}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={endPointer}
  onpointerleave={onPointerLeave}
  onwheel={onWheel}
  ondblclick={onDblClick}
  role="img"
  aria-label="Wiki 知识图谱"
>
  <canvas bind:this={canvasEl} class="wk-canvas"></canvas>
  <div class="wk-tooltip" bind:this={tooltipEl} style:display={hoverNode ? "block" : "none"}>
    {#if hoverNode}
      <strong>{hoverNode.label}</strong>
      {#if tooltip(hoverNode)}
        <span>{tooltip(hoverNode)}</span>
      {/if}
    {/if}
  </div>
</div>

<style>
  .wk-canvas-wrap {
    position: absolute;
    inset: 0;
    overflow: hidden;
    cursor: grab;
    touch-action: none;
    border: 1px solid var(--kb-border);
    border-radius: 10px;
    /* 深色底 + 中心微亮径向过渡：比纯色更柔和，节点/连线更容易辨认 */
    background:
      radial-gradient(110% 90% at 50% 38%, #17202c 0%, #11161f 46%, #0b0e14 78%, #090b10 100%);
  }
  .wk-canvas {
    display: block;
  }
  .wk-tooltip {
    position: absolute;
    pointer-events: none;
    z-index: 5;
    display: none;
    max-width: 240px;
    padding: 7px 10px;
    border-radius: 8px;
    background: color-mix(in srgb, var(--kb-surface-2) 92%, #000 8%);
    border: 1px solid var(--kb-border-strong);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
    font-size: 12px;
    line-height: 1.5;
    color: var(--kb-text);
  }
  .wk-tooltip strong {
    display: block;
    margin-bottom: 2px;
  }
  .wk-tooltip span {
    color: var(--kb-text-3);
    font-size: 11.5px;
  }
</style>



