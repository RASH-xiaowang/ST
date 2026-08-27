/* ============================================================
 * 知识库 — Wiki 面板纯函数
 * 自 WikiPanel.svelte 下沉：状态文案、图谱节点颜色、节点提示，
 * 不依赖组件状态，可独立单测。
 * ============================================================ */
import { nodeColor, nodeTypeName } from './graphStyle';
import { communityColor } from './wikiGraphModel';
import type { WikiGraphNode } from './kbTypes';

/** Wiki 页面状态 → 中文标签（未知状态原样返回） */
export function statusLabel(s: string): string {
  const map: Record<string, string> = { draft: '草稿', published: '已发布', archived: '已归档', ready: '就绪' };
  return map[s] ?? s;
}

/** 图谱节点配色：按社区着色优先，否则走类型/状态配色 */
export function wikiNodeColor(opts: {
  status: string;
  community: number;
  colorByCommunity: boolean;
  colorGroups: { query: string; color: string }[];
  label: string;
  docTitle: string | null;
  dirName: string | null;
}): string {
  if (opts.colorByCommunity) return communityColor(opts.community);
  const pseudo: WikiGraphNode = {
    id: 0, pageId: 0, title: opts.label, docId: null,
    docTitle: opts.docTitle, dirName: opts.dirName, inDegree: 0, outDegree: 0, status: opts.status,
  };
  return nodeColor(opts.status, pseudo, opts.colorGroups);
}

/** 图谱节点悬浮提示：类型 · 状态 · 来源 · 出入度 */
export function wikiNodeTooltip(opts: {
  label: string;
  status: string;
  docTitle: string | null;
  dirName: string | null;
  inDegree: number;
  outDegree: number;
}): string {
  const pseudo: WikiGraphNode = {
    id: 0, pageId: 0, title: opts.label, docId: null,
    docTitle: opts.docTitle, dirName: opts.dirName, inDegree: opts.inDegree, outDegree: opts.outDegree, status: opts.status,
  };
  const parts: string[] = [nodeTypeName(pseudo)];
  parts.push(opts.status === 'missing' ? '尚未创建' : opts.status === 'draft' ? '草稿' : '已创建');
  if (opts.docTitle) parts.push('来源：' + opts.docTitle);
  parts.push(`入链 ${opts.inDegree} · 出链 ${opts.outDegree}`);
  return parts.join(' · ');
}
