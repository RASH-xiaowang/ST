/* ============================================================
 * 知识库 — Wiki 图谱着色/分类纯函数
 * 自 WikiPanel.svelte 下沉：节点类型归类、节点/连线配色、
 * 颜色组命中与 slug 化，不依赖组件状态，可独立单测。
 * ============================================================ */
import type { WikiGraphNode } from './kbTypes';

/** 连线类型 → 颜色 */
export const EDGE_COLORS: Record<string, string> = {
  related: '#5b8ff9',
  backlink: '#a0d9ff',  // 反向链接（由正向 [[引用]] 自动生成，颜色较浅以区分）
  reference: '#5ad8a6',
  child_of: '#f6bd16',
  generated: '#b37feb',
  entity: '#4fd1c5',   // 隐含关系：共享实体
  center: '#6366f1',   // 中心节点连接线（紫色，半透明）
};

export const EDGE_COLOR_FALLBACK = '#7d8899';

/** 图谱节点类型（按目录归类，用于图例与着色） */
export const ENTITY_DIRS = ['实体', '人物', '组织', '地点', '产品', '事件', '日期', '作品', '资源', '类别', '操作'];

export const NODE_TYPE_COLORS: Record<string, string> = {
  摘要: '#52c41a', 实体: '#5b8ff9', 概念: '#7cc0ff', 综合: '#52c41a', 对比: '#5b8ff9', 页面: '#8d99ae',
};

/** 连线颜色（未知类型回退灰） */
export function edgeColor(t: string): string {
  return EDGE_COLORS[t] ?? EDGE_COLOR_FALLBACK;
}

/** 颜色 → slug（用于 SVG marker id） */
export function colorSlug(c: string): string {
  return c.replace('#', '').toLowerCase();
}

/** 颜色组命中：节点标题/文档标题包含查询词 */
export function nodeMatches(nd: WikiGraphNode, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return false;
  return nd.title.toLowerCase().includes(q) || (nd.docTitle ?? '').toLowerCase().includes(q);
}

/** 图谱节点类型（按目录归类） */
export function nodeTypeName(nd: WikiGraphNode): string {
  const d = (nd.dirName ?? '').trim();
  if (ENTITY_DIRS.includes(d)) return '实体';
  if (d === '概念') return '概念';
  if (d === '摘要') return '摘要';
  if (d === '综合') return '综合';
  if (d === '对比') return '对比';
  return d || '页面';
}

/** 节点颜色：首个命中的颜色组优先；否则按类型/状态着色 */
export function nodeColor(
  status: string,
  nd: WikiGraphNode,
  colorGroups: { query: string; color: string }[] = [],
): string {
  for (const g of colorGroups) {
    if (nodeMatches(nd, g.query)) return g.color;
  }
  const tc = NODE_TYPE_COLORS[nodeTypeName(nd)];
  if (tc) return tc;
  if (status === 'draft') return '#f6bd16';
  if (status === 'missing') return '#8d99ae';
  return '#5b8ff9';
}
