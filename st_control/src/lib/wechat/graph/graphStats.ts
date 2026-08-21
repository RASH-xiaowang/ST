/* ============================================================
 * 微信关系图谱 — 统计派生纯函数
 * 自 RelationshipGraph.svelte 下沉：Top-N 榜单、圈子分组、
 * 相连边解析与共同群名称。
 * ============================================================ */
import type { BuiltGraph, GEdge, GNode } from './graphModel';

/** 按数值字段取 Top-N（字段缺失按 0 计；返回新数组） */
export function topByField<T>(
  nodes: T[],
  get: (n: T) => number,
  count: number,
): T[] {
  return [...nodes].sort((a, b) => get(b) - get(a)).slice(0, count);
}

/** 圈子概览：按成员数降序分组（self 不参与、community < 0 排除） */
export function groupCommunities(
  nodes: GNode[],
): { id: number; members: GNode[] }[] {
  const map = new Map<number, GNode[]>();
  for (const n of nodes) {
    if (n.kind === 'self' || n.community < 0) continue;
    const arr = map.get(n.community) ?? [];
    arr.push(n);
    map.set(n.community, arr);
  }
  return [...map.entries()]
    .map(([id, members]) => ({ id, members }))
    .sort((a, b) => b.members.length - a.members.length);
}

/** 与指定节点相连的边（按权重降序，含对端节点解析；默认取前 12） */
export function connectedEdgesOf(
  graph: BuiltGraph,
  nodeId: string,
  limit = 12,
): { edge: GEdge; other: GNode | undefined }[] {
  return graph.edges
    .filter((e) => e.source === nodeId || e.target === nodeId)
    .map((e) => ({
      edge: e,
      other: graph.nodes.find((x) => x.id === (e.source === nodeId ? e.target : e.source)),
    }))
    .sort((a, b) => b.edge.weight - a.edge.weight)
    .slice(0, limit);
}

/** 共同群名称（详情展示，群名缺失时回退 code） */
export function sharedGroupNames(
  n: GNode,
  groupNames: Record<string, string> | undefined,
  limit = 6,
): string[] {
  return (n.groupCodes ?? [])
    .map((c) => groupNames?.[c] || c)
    .slice(0, limit);
}
