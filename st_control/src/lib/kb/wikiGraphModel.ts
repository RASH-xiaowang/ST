/* ============================================================
 * 知识库 — Wiki 图谱力导向模型（纯函数）
 * 仿微信「社交关系图谱」graphModel：把 Wiki 图谱数据 + 可见节点 +
 * 布局参数构建为 d3-force 力导向图（节点 / 边 / 社区）。
 *  - 社区检测：加权标签传播（与社交图谱一致），既用于初始聚类布局，
 *    也用于「按社区着色」模式。
 *  - 幽灵节点（status=missing，pageId 为负）与正常页面一同参与布局。
 * 不依赖组件状态，可独立单测。
 * ============================================================ */
import type { WikiGraph, WikiGraphEdge } from './kbTypes';

/** Wiki 图谱节点（力导向仿真就地修改位置） */
export interface WNode {
  /** String(pageId)：幽灵节点为负数，仍全局唯一 */
  id: string;
  pageId: number;
  label: string;
  docTitle: string | null;
  dirName: string | null;
  status: string;
  /** 入度 + 出度（后端统计，决定节点半径） */
  degree: number;
  inDegree: number;
  outDegree: number;
  weight: number;
  radius: number;
  /** 社区编号（-1 = 未分组/孤立） */
  community: number;
  x: number;
  y: number;
  vx: number;
  vy: number;
  fx: number | null;
  fy: number | null;
}

/** Wiki 图谱边（source/target 为节点 id 字符串） */
export interface WEdge {
  source: string;
  target: string;
  linkType: string;
  weight: number;
  /** 期望弹簧长度（越短 = 关系越强） */
  dist: number;
  /** 可选边强度（默认按两端连接度归一） */
  strength?: number;
}

export interface BuiltWikiGraph {
  nodes: WNode[];
  edges: WEdge[];
  communityCount: number;
}

/** 构建模型时使用的布局参数（与图谱画布外观参数解耦） */
export interface WikiGraphBuildParams {
  nodeScale: number;
  forceEdgeLength: number;
  /** 显示隐含关系（共享实体）边 */
  showImplicit: boolean;
}

/** 社区配色（与微信社交图谱一致，保证两处观感统一） */
export const COMMUNITY_COLORS = [
  '#0099ff',
  '#36c08f',
  '#f6a23c',
  '#ef6f6c',
  '#9b6dff',
  '#2bb6d6',
  '#e072b8',
  '#7e8bd9',
  '#5bbf6a',
  '#d98c4a',
];

export function communityColor(community: number): string {
  if (community < 0) return '#9aa0a6'; // 未分组（中性灰）
  return COMMUNITY_COLORS[community % COMMUNITY_COLORS.length];
}

/**
 * 把 Wiki 图谱数据构建为力导向图。
 * 保留可见节点（visibleIds 为调用方权威过滤结果，空集 = 无节点），
 * 边两端都必须可见；showImplicit=false 时丢弃共享实体边。
 * 新增：以知识库名为主节点（pageId=0），所有页面节点都连接到主节点，
 * 形成以库名为中心的约束布局，防止节点随意发散。
 */
export function buildWikiGraph(
  graph: WikiGraph | null,
  visibleIds: Set<number>,
  params: WikiGraphBuildParams,
  kbName?: string,
): BuiltWikiGraph {
  if (!graph || graph.nodes.length === 0) return { nodes: [], edges: [], communityCount: 0 };
  const nodes: WNode[] = [];

  // 添加知识库主节点（pageId=0，固定在中心）
  const centerNode: WNode = {
    id: '0',
    pageId: 0,
    label: kbName || '知识库',
    docTitle: null,
    dirName: null,
    status: 'center',
    degree: graph.nodes.length,
    inDegree: 0,
    outDegree: graph.nodes.length,
    weight: graph.nodes.length,
    radius: 18 * params.nodeScale,
    community: -2, // 特殊社区编号：中心节点
    x: 0,
    y: 0,
    vx: 0,
    vy: 0,
    fx: 0, // 固定在中心
    fy: 0,
  };
  nodes.push(centerNode);

  for (const nd of graph.nodes) {
    if (!visibleIds.has(nd.pageId)) continue;
    const degree = nd.inDegree + nd.outDegree;
    nodes.push({
      id: String(nd.pageId),
      pageId: nd.pageId,
      label: nd.title,
      docTitle: nd.docTitle,
      dirName: nd.dirName,
      status: nd.status,
      degree,
      inDegree: nd.inDegree,
      outDegree: nd.outDegree,
      weight: degree,
      radius: (7 + Math.min(degree, 6)) * params.nodeScale,
      community: 0,
      x: 0,
      y: 0,
      vx: 0,
      vy: 0,
      fx: null,
      fy: null,
    });
  }
  const index = new Set(nodes.map((n) => n.id));
  const edges: WEdge[] = [];

  // 添加所有页面节点到中心节点的边（向心力约束）
  for (const nd of nodes) {
    if (nd.pageId === 0) continue; // 跳过中心节点自身
    edges.push({
      source: '0',
      target: nd.id,
      linkType: 'center',
      weight: 1,
      dist: 180 * params.forceEdgeLength, // 中心到页面的距离
    });
  }

  // 添加原有的页面间边
  for (const e of graph.edges) {
    if (!params.showImplicit && e.linkType === 'entity') continue;
    const s = String(e.from);
    const t = String(e.to);
    if (!index.has(s) || !index.has(t)) continue;
    edges.push({
      source: s,
      target: t,
      linkType: e.linkType,
      weight: e.weight,
      dist: edgeDist(e, params.forceEdgeLength),
    });
  }
  const communityCount = detectCommunities(nodes, edges);
  return { nodes, edges, communityCount };
}

/** 边期望长度：显式链接更近，共享实体弱关系更远（权重越大越近） */
function edgeDist(e: WikiGraphEdge, forceEdgeLength: number): number {
  if (e.linkType === 'entity') {
    // 权重 = 共享实体数：实体越多的两页语义越近
    const d = (230 / Math.sqrt(Math.max(e.weight, 1))) * forceEdgeLength;
    return Math.max(46, Math.min(d, 320));
  }
  return 150 * forceEdgeLength;
}

/**
 * 加权标签传播社区检测：节点反复采纳「邻居标签总权重」最大的社区。
 * 与微信社交图谱 detectCommunities 一致；孤立节点标 -1（未分组）。
 */
export function detectCommunities(nodes: WNode[], edges: WEdge[]): number {
  if (nodes.length === 0) return 0;
  const adj = new Map<string, Array<{ id: string; w: number }>>();
  for (const n of nodes) adj.set(n.id, []);
  for (const e of edges) {
    const s = e.source;
    const t = e.target;
    adj.get(s)?.push({ id: t, w: e.weight });
    adj.get(t)?.push({ id: s, w: e.weight });
  }

  const label = new Map<string, number>();
  nodes.forEach((n, i) => label.set(n.id, i));

  for (let iter = 0; iter < 10; iter++) {
    let changed = false;
    for (const n of nodes) {
      const neighbours = adj.get(n.id);
      if (!neighbours || neighbours.length === 0) continue;
      const score = new Map<number, number>();
      for (const nb of neighbours) {
        const l = label.get(nb.id)!;
        score.set(l, (score.get(l) ?? 0) + nb.w);
      }
      let best = label.get(n.id)!;
      let bestScore = -1;
      for (const [l, s] of score) {
        if (s > bestScore || (s === bestScore && l < best)) {
          best = l;
          bestScore = s;
        }
      }
      if (best !== label.get(n.id)) {
        label.set(n.id, best);
        changed = true;
      }
    }
    if (!changed) break;
  }

  const remap = new Map<number, number>();
  let k = 0;
  for (const n of nodes) {
    // 孤立节点（无任何连线）不参与圈子划分，标为未分组
    const neighbours = adj.get(n.id);
    if (!neighbours || neighbours.length === 0) {
      n.community = -1;
      continue;
    }
    const l = label.get(n.id)!;
    if (!remap.has(l)) remap.set(l, k++);
    n.community = remap.get(l)!;
  }
  return k;
}

/**
 * 初始布局预热：按社区分组环形摆放（孤岛单独成环）。
 * 初始位置接近力导向稳态，收敛 tick 数大幅减少，
 * 显著缩短打开 / 切换后的布局演化期。
 */
export function seedPositions(nodes: WNode[]): void {
  if (nodes.length === 0) return;
  const byComm = new Map<number, WNode[]>();
  for (const n of nodes) {
    const c = n.community >= 0 ? n.community : -1;
    let list = byComm.get(c);
    if (!list) {
      list = [];
      byComm.set(c, list);
    }
    list.push(n);
  }
  const groups = [...byComm.entries()].sort((a, b) => b[1].length - a[1].length);
  const groupCount = Math.max(groups.length, 1);
  // 半径随节点数增长（约 sqrt(N) 保证密度），社区环错开 60° 起始角
  const baseR = 70 + Math.sqrt(Math.max(nodes.length, 1)) * 6;
  groups.forEach(([, list], gi) => {
    const angle0 = (gi / groupCount) * Math.PI * 2 + (gi % 2) * 0.6;
    const ring = baseR * (1 + 0.35 * (gi % 3));
    list.forEach((n, i) => {
      const a = angle0 + (i / Math.max(list.length, 1)) * 1.15;
      n.x = Math.cos(a) * ring;
      n.y = Math.sin(a) * ring;
      n.vx = 0;
      n.vy = 0;
    });
  });
}



