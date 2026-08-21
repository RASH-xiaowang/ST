/* ============================================================
 * 知识库 — Wiki 图纯算法工具
 * 自 WikiPanel.svelte 下沉：邻居集合 / 连接度 / 边类型枚举 /
 * 可见节点过滤。
 * ============================================================ */
import { matchGlob } from './graphLayout';
import type { WikiGraph, WikiGraphEdge } from './kbTypes';

/** 图中指定节点的邻居 id 集合（含节点自身） */
export function graphNeighborSet(
  edges: WikiGraphEdge[],
  nodeId: number,
): Set<number> {
  const set = new Set<number>([nodeId]);
  for (const e of edges) {
    if (e.from === nodeId) set.add(e.to);
    if (e.to === nodeId) set.add(e.from);
  }
  return set;
}

/** 每个节点的总连接度（显式边数），用于「孤立节点」判断 */
export function nodeDegreeMap(edges: WikiGraphEdge[]): Record<number, number> {
  const m: Record<number, number> = {};
  for (const e of edges) {
    m[e.from] = (m[e.from] ?? 0) + 1;
    m[e.to] = (m[e.to] ?? 0) + 1;
  }
  return m;
}

/** 图中出现的边类型集合（去重、按首现序） */
export function edgeLinkTypes(graph: WikiGraph | null): string[] {
  return graph ? [...new Set(graph.edges.map((e) => e.linkType))] : [];
}

/** 可见节点过滤（WikiPanel graphVisible 派生下沉）。
 * createdOnly 仅保留非 missing；showOrphans=false 排除零连接度；
 * ignorePatterns 通配排除；query 大小写不敏感匹配标题/文档名；
 * localOnly 仅保留锚点及其邻居（复用 graphNeighborSet）。 */
export function visibleNodeIds(
  graph: WikiGraph | null,
  opts: {
    nodeDegree: Record<number, number>;
    ignorePatterns: string[];
    createdOnly: boolean;
    showOrphans: boolean;
    query: string;
    localOnly: boolean;
    anchorId: number | null;
  },
): Set<number> {
  if (!graph) return new Set<number>();
  const q = opts.query;
  let ids = new Set<number>();
  for (const nd of graph.nodes) {
    if (opts.createdOnly && nd.status === 'missing') continue;
    if (!opts.showOrphans && (opts.nodeDegree[nd.id] ?? 0) === 0) continue;
    if (opts.ignorePatterns.some((p) => matchGlob(nd.title, p))) continue;
    if (!q || nd.title.toLowerCase().includes(q) || (nd.docTitle ?? '').toLowerCase().includes(q)) {
      ids.add(nd.id);
    }
  }
  if (opts.localOnly && opts.anchorId !== null) {
    const keep = graphNeighborSet(graph.edges, opts.anchorId);
    ids = new Set([...ids].filter((id) => keep.has(id)));
  }
  return ids;
}
