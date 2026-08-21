/* ============================================================
 * 知识库 — Wiki 图谱纯函数（布局 / 过滤）
 * 自 WikiPanel.svelte 下沉：径向树布局与 glob 通配匹配，
 * 不依赖组件状态，可独立单测。
 * ============================================================ */
import type { WikiGraph } from './kbTypes';

/** 径向树布局参数（原 graphParams 中参与布局的字段） */
export interface RadialLayoutParams {
  forceRepulsion: number;
  forceAttraction: number;
  forceEdgeLength: number;
  forceCentripetal: number;
  nodeScale: number;
}

/** 径向树布局：返回 nodeId → 画布坐标 */
export function radialTreeLayout(
  g: WikiGraph,
  w: number,
  h: number,
  params: RadialLayoutParams,
): Record<number, { x: number; y: number }> {
  const cx = w / 2, cy = h / 2;
  const n = g.nodes.length;
  if (n === 1) {
    const only = g.nodes[0].id;
    return { [only]: { x: cx, y: cy } };
  }
  // 无向邻接表
  const adj = new Map<number, number[]>();
  for (const e of g.edges) {
    let a = adj.get(e.from); if (!a) { a = []; adj.set(e.from, a); } a.push(e.to);
    let b = adj.get(e.to); if (!b) { b = []; adj.set(e.to, b); } b.push(e.from);
  }
  // 根节点：连接度最高者
  let root = g.nodes[0];
  for (const nd of g.nodes) {
    if (nd.inDegree + nd.outDegree > root.inDegree + root.outDegree) root = nd;
  }
  // BFS 构建父子树（环上多余边仅保留布局归属，连线照常绘制）
  const parent = new Map<number, number | null>([[root.id, null]]);
  const children = new Map<number, number[]>();
  const depth = new Map<number, number>([[root.id, 0]]);
  for (const nd of g.nodes) children.set(nd.id, []);
  const queue: number[] = [root.id];
  while (queue.length) {
    const cur = queue.shift()!;
    for (const nx of adj.get(cur) ?? []) {
      if (parent.has(nx)) continue;
      parent.set(nx, cur);
      children.get(cur)!.push(nx);
      depth.set(nx, (depth.get(cur) ?? 0) + 1);
      queue.push(nx);
    }
  }
  // 孤岛节点：直接挂到根节点下
  for (const nd of g.nodes) {
    if (!parent.has(nd.id)) {
      parent.set(nd.id, root.id);
      children.get(root.id)!.push(nd.id);
      depth.set(nd.id, 1);
    }
  }
  // 子树叶子数（决定各节点占用的角度楔形大小）
  const leaves = new Map<number, number>();
  function countLeaves(id: number): number {
    const ch = children.get(id)!;
    if (ch.length === 0) { leaves.set(id, 1); return 1; }
    const s = ch.reduce((acc, c) => acc + countLeaves(c), 0);
    leaves.set(id, s);
    return s;
  }
  countLeaves(root.id);
  // 楔形分配：每个节点的角度 = 自己楔形的中线；子节点按叶子数等分父楔形
  const angle = new Map<number, number>();
  function assign(node: number, start: number, end: number) {
    const mid = (start + end) / 2;
    angle.set(node, mid);
    const ch = children.get(node)!;
    if (ch.length === 0) return;
    const total = leaves.get(node)!;
    let a = start;
    for (const c of ch) {
      const share = (leaves.get(c) ?? 1) / Math.max(1, total);
      const ce = a + (end - start) * share;
      assign(c, a, ce);
      a = ce;
    }
    relaxSiblings(ch, start, end, mid);
  }
  // 同级松弛：排斥力让兄弟节点在父楔形内分散开，吸引力向父节点方向聚拢
  function relaxSiblings(ch: number[], start: number, end: number, mid: number) {
    if (ch.length < 2) return;
    const margin = 0.02;
    const rep = Math.max(0.2, params.forceRepulsion / 2000);
    const attr = Math.max(0.001, params.forceAttraction * 0.5);
    // 兄弟数量越多迭代越少（保证大节点组不卡顿）
    const maxIter = ch.length >= 24 ? 24 : ch.length >= 10 ? 40 : 60;
    for (let iter = 0; iter < maxIter; iter++) {
      const delta = new Map<number, number>();
      for (const c of ch) delta.set(c, 0);
      for (let i = 0; i < ch.length; i++) {
        for (let j = i + 1; j < ch.length; j++) {
          const a = angle.get(ch[i])!, b = angle.get(ch[j])!;
          let d = b - a;
          if (d < 0) d += Math.PI * 2;
          const minSep = 0.055;
          if (d < minSep) {
            const push = ((minSep - d) / 2) * rep;
            delta.set(ch[i], delta.get(ch[i])! - push);
            delta.set(ch[j], delta.get(ch[j])! + push);
          }
        }
      }
      for (const c of ch) {
        delta.set(c, delta.get(c)! + (mid - angle.get(c)!) * attr);
      }
      const temp = 0.004 * (1 - iter / maxIter) + 0.0002;
      for (const c of ch) {
        const dv = delta.get(c)!;
        const step = Math.max(-temp, Math.min(temp, dv));
        const na = Math.max(start + margin, Math.min(end - margin, angle.get(c)! + step));
        angle.set(c, na);
      }
    }
  }
  assign(root.id, 0, Math.PI * 2);
  // 半径：环间距 = 连线长度 × 收缩系数；并保证不小于节点直径 + 间隙，避免父子圆重叠
  const maxDepth = Math.max(1, ...depth.values());
  const avail = (Math.min(w, h) / 2 - 48) * Math.max(0.5, Math.min(2, params.forceEdgeLength))
    * Math.max(0.6, 1 - params.forceCentripetal * 2);
  const minRing = 26 * params.nodeScale + 30;
  const ringSpacing = Math.max(minRing, avail / maxDepth);
  const pos: Record<number, { x: number; y: number }> = {};
  for (const nd of g.nodes) {
    const d = depth.get(nd.id) ?? 1;
    const r = d * ringSpacing;
    const a = angle.get(nd.id) ?? 0;
    pos[nd.id] = { x: cx + r * Math.cos(a), y: cy + r * Math.sin(a) };
  }
  return pos;
}

/** glob 通配匹配（* 匹配任意字符，大小写不敏感） */
export function matchGlob(title: string, pattern: string): boolean {
  const re = new RegExp('^' + pattern.split('*').map((s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('.*') + '$', 'i');
  return re.test(title);
}
