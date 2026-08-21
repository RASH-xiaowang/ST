// ============================================================
// 社交关系图谱 — 图模型（移植自 WeQ「群友圈子 / 群聊网络」）
// 纯函数：把后端关系数据 + 控制面板设置构建为力导向图节点/边/社区。
//  - people（群友圈子）：节点 = 与我共同的联系人，边 = 共同群数量
//  - groups（群聊网络）：节点 = 群聊，边 = 共同成员数量（共现）
//  - 加权标签传播社区检测 + 饱和指数半径 + 以「我」为枢纽的拉力模型
// ============================================================

import { clamp } from '../utils';

export type GraphMode = "people" | "groups";
export type GroupFilterMode = "all" | "whitelist" | "blacklist";

/** 后端返回的原始节点（st_control get_relationship_graph 扩展后的字段） */
export interface RawNode {
  id: string;
  label: string;
  kind: "contact" | "group" | "official";
  msg_count: number;
  active_days: number;
  last_ts: number;
  member_count: number;
  group_count: number;
  group_codes: string[];
  is_friend: boolean;
  shared_count: number;
  avatar_url: string;
  /** 群节点：共同成员明细（按消息量取前 N 名） */
  shared_members?: SharedMember[];
}

/** 群节点中的共同成员 */
export interface SharedMember {
  username: string;
  name: string;
  is_friend: boolean;
  msg_count: number;
}

export interface RelationGraphData {
  selfUin: string;
  /** 「我」的头像（data URL 或远程地址，可为空） */
  selfAvatar: string;
  /** 联系人 / 公众号节点（people 模式的顶点） */
  persons: RawNode[];
  /** 群聊节点（groups 模式的顶点） */
  groups: RawNode[];
  /** 群 code → 群名（展示「共同群」列表用） */
  groupNames: Record<string, string>;
  scannedGroups: number;
  /** 后端汇总（总数 / 好友等） */
  summary?: {
    total_contacts: number;
    total_groups: number;
    total_messages: number;
    /** 真实通讯录口径（与通讯录面板「全部」一致的六个可见分类合计） */
    contact_book_total: number;
    contact_book_friends: number;
    contact_book_members: number;
    contact_book_official: number;
    selected_contacts: number;
    selected_groups: number;
    top_relations: Array<{ username: string; name: string; msg_count: number; active_days: number }>;
  };
  builtAt: number;
}

export interface GraphSettings {
  mode: GraphMode;
  /** 最多绘制的节点数 */
  nodeLimit: number;
  /** 连线阈值：共同群（people）或共同成员（groups） */
  minCommon: number;
  /** people 模式：只显示好友 */
  friendsOnly: boolean;
  /** people 模式：消息量决定节点大小 */
  intimacySize: boolean;
  /** people 模式：消息量决定「我」的拉力（越近越亲） */
  intimacyPull: boolean;
  /** groups 模式：命中人数决定节点大小 */
  groupLevelSize: boolean;
  /** groups 模式：命中人数决定「我」的拉力 */
  groupLevelPull: boolean;
  groupFilterMode: GroupFilterMode;
  /** 白/黑名单选中的群 code */
  groupFilter: string[];
  // ─── 外观 ───
  /** 连线箭头 */
  showArrows: boolean;
  /** 节点标签透明度（0-1） */
  labelOpacity: number;
  /** 节点大小倍率（0.6-1.8） */
  nodeScale: number;
  /** 连线粗细（0.5-3） */
  edgeWidth: number;
  /** 播放布局动画（仿真运动） */
  motion: boolean;
  // ─── 力度 ───
  /** 图谱向心力（中心拉力强度） */
  forceCentripetal: number;
  /** 节点间排斥力强度 */
  forceRepulsion: number;
  /** 相连节点吸引力强度 */
  forceAttraction: number;
  /** 连线长度倍率（0.5-2） */
  forceEdgeLength: number;
}

export interface GNode {
  id: string;
  kind: "person" | "group" | "self";
  label: string;
  pinned?: boolean;
  avatarUrl: string | null;
  community: number;
  weight: number;
  radius: number;
  // people refs
  uin?: string;
  isFriend?: boolean;
  intimacy?: number;
  groupCount?: number;
  groupCodes?: string[];
  /** 活跃天数 / 最后联系时间（详情与榜单用） */
  activeDays?: number;
  lastTs?: number;
  /** 消息量（群节点榜单用） */
  msgCount?: number;
  /** 群节点：共同成员明细 */
  sharedMembers?: SharedMember[];
  // group refs
  code?: string;
  memberCount?: number;
  sharedCount?: number;
  myLevel?: number;
  // simulation state（力导向就地修改）
  x: number;
  y: number;
  vx: number;
  vy: number;
  fx: number | null;
  fy: number | null;
}

/** 边端点：字符串 ID（构建期）或节点对象（力导向模拟后就地替换） */
export type GEdgeEndpoint = string | GNode;

export interface GEdge {
  source: GEdgeEndpoint;
  target: GEdgeEndpoint;
  weight: number;
  /** 期望弹簧长度（越短 = 关系越强） */
  dist: number;
  /** 可选边强度（「我」的枢纽边较弱/较强） */
  strength?: number;
}

export interface BuiltGraph {
  nodes: GNode[];
  edges: GEdge[];
  communityCount: number;
}

/** 后端图谱原始数据（get_relationship_graph 返回） */
export interface GraphRawData {
  nodes?: RawNode[];
  self?: string;
  self_avatar?: string;
  group_names?: Record<string, string>;
  summary?: RelationGraphData['summary'];
  [key: string]: unknown;
}

/** 扫描阶段增量块（进度事件 payload） */
export interface GraphChunk {
  nodes?: RawNode[];
  [key: string]: unknown;
}

/** 好友基准权重（消息量为 0 时仍保持可辨识），非好友取较低值 */
const NON_FRIEND_WEIGHT = 80;
const FRIEND_BASE_WEIGHT = 100;

/** 合成「我」节点 id */
export const SELF_ID = "__self__";
const SELF_RADIUS = 26;

/** 消息量（亲密度代理）→ 权重：好友至少 100，非好友 80，再叠加消息量 */
export function personWeight(node: Pick<RawNode, "is_friend" | "msg_count">): number {
  const base = node.is_friend ? FRIEND_BASE_WEIGHT : NON_FRIEND_WEIGHT;
  return base + Math.min((node.msg_count || 0) / 10, 400);
}

/** 饱和指数半径：低值增长快、高值趋平，避免少数热点淹没其他人 */
function radiusByIntimacy(weight: number): number {
  const MIN_R = 10;
  const MAX_R = 24;
  const SCALE = 420;
  const t = 1 - Math.exp(-weight / SCALE);
  return MIN_R + t * (MAX_R - MIN_R);
}

function radiusBySqrt(value: number, divisor: number, min: number, max: number): number {
  const t = Math.min(Math.sqrt(Math.max(value, 0)) / divisor, 1);
  return min + t * (max - min);
}

function allowedGroupPredicate(settings: GraphSettings): (code: string) => boolean {
  if (settings.groupFilterMode === "all" || settings.groupFilter.length === 0) {
    return () => true;
  }
  const set = new Set(settings.groupFilter);
  return settings.groupFilterMode === "whitelist"
    ? (code) => set.has(code)
    : (code) => !set.has(code);
}

function makeSelfNode(data: RelationGraphData, label: string, nodeScale: number): GNode {
  return {
    id: SELF_ID,
    kind: "self",
    label,
    avatarUrl: data.selfAvatar || null,
    community: 0,
    weight: 0,
    radius: SELF_RADIUS * nodeScale,
    uin: data.selfUin,
    isFriend: true,
    x: 0,
    y: 0,
    vx: 0,
    vy: 0,
    fx: null,
    fy: null,
  };
}

/** people 模式：节点 = 与我共同的联系人，边 = 共同群数量 */
function buildPeople(data: RelationGraphData, settings: GraphSettings): BuiltGraph {
  const allow = allowedGroupPredicate(settings);

  const prepared: Array<{ raw: RawNode; groups: string[] }> = [];
  for (const raw of data.persons) {
    if (settings.friendsOnly && !raw.is_friend) continue;
    const groups = raw.group_codes.filter(allow);
    // 好友全量展示（即使没有共同群）；非好友需要有共同群才进入圈子
    if (!raw.is_friend && groups.length === 0) continue;
    prepared.push({ raw, groups });
  }

  // 排序：好友优先（内部按共同群数），其后是群友
  prepared.sort((a, b) => {
    if (a.raw.is_friend !== b.raw.is_friend) return a.raw.is_friend ? -1 : 1;
    return b.groups.length - a.groups.length;
  });
  // 群友上限控制图谱展示的节点总数（含好友与群友）：
  // 按“好友优先、共同群数多者优先”整体截取前 nodeLimit 个。
  const top = prepared.slice(0, settings.nodeLimit);

  const nodes: GNode[] = top.map((p) => {
    const weight = personWeight(p.raw);
    return {
      id: p.raw.id,
      kind: "person" as const,
      label: p.raw.label || p.raw.id,
      avatarUrl: p.raw.avatar_url || null,
      community: 0,
      weight,
      radius: settings.intimacySize
        ? radiusByIntimacy(weight) * settings.nodeScale
        : radiusBySqrt(p.groups.length, 6, 9, 18) * settings.nodeScale,
      uin: p.raw.id,
      isFriend: p.raw.is_friend,
      intimacy: p.raw.msg_count,
      groupCount: p.raw.group_count,
      groupCodes: p.groups,
      activeDays: p.raw.active_days,
      lastTs: p.raw.last_ts,
      msgCount: p.raw.msg_count,
      x: 0,
      y: 0,
      vx: 0,
      vy: 0,
      fx: null,
      fy: null,
    };
  });

  const sets = top.map((p) => new Set(p.groups));
  const edges: GEdge[] = [];
  for (let i = 0; i < top.length; i++) {
    const a = sets[i];
    for (let j = i + 1; j < top.length; j++) {
      const b = sets[j];
      const [small, big] = a.size < b.size ? [a, b] : [b, a];
      let common = 0;
      for (const g of small) if (big.has(g)) common++;
      if (common >= settings.minCommon) {
        edges.push({
          source: nodes[i].id,
          target: nodes[j].id,
          weight: common,
          dist: (200 / Math.sqrt(common)) * settings.forceEdgeLength,
        });
      }
    }
  }

  const communityCount = detectCommunities(nodes, edges);

  const self = makeSelfNode(data, "我", settings.nodeScale);
  for (let i = 0; i < top.length; i++) {
    const node = nodes[i];
    const shared = top[i].groups.length;
    // 无共同群的好友也保留一条「我」的边（weight=1），避免孤立漂移
    const weight = node.isFriend && shared === 0 ? 1 : shared;
    let dist: number;
    let strength: number;
    if (settings.intimacyPull) {
      const t = 1 - Math.exp(-node.weight / 600);
      dist = (340 - t * 260) * settings.forceEdgeLength;
      strength = 0.02 + t * 0.16;
    } else {
      dist = clamp(360 / Math.sqrt(weight), 60, 360) * settings.forceEdgeLength;
      strength = 0.006;
    }
    edges.push({ source: SELF_ID, target: node.id, weight, dist, strength });
  }
  nodes.push(self);

  return { nodes, edges, communityCount };
}

/** groups 模式：节点 = 群聊，边 = 共同成员数量（共现） */
function buildGroups(data: RelationGraphData, settings: GraphSettings): BuiltGraph {
  const allow = allowedGroupPredicate(settings);
  const groups = data.groups.filter((g) => allow(g.id));
  groups.sort((a, b) => b.shared_count - a.shared_count);
  const top = groups.slice(0, settings.nodeLimit);
  const allowedCodes = new Set(top.map((g) => g.id));

  const nodes: GNode[] = top.map((g) => ({
    id: g.id,
    kind: "group" as const,
    label: g.label || g.id,
    avatarUrl: g.avatar_url || null,
    community: 0,
    weight: g.shared_count,
    radius: settings.groupLevelSize
      ? radiusBySqrt(g.shared_count, 8, 10, 22) * settings.nodeScale
      : radiusBySqrt(g.shared_count, 8, 10, 22) * settings.nodeScale,
    code: g.id,
    memberCount: g.member_count,
    sharedCount: g.shared_count,
    activeDays: g.active_days,
    lastTs: g.last_ts,
    msgCount: g.msg_count,
    sharedMembers: g.shared_members ?? [],
    myLevel: 0,
    x: 0,
    y: 0,
    vx: 0,
    vy: 0,
    fx: null,
    fy: null,
  }));

  // 共现：对每个人，其所在群两两成边
  const pairCount = new Map<string, number>();
  for (const person of data.persons) {
    const codes = person.group_codes.filter((c) => allowedCodes.has(c));
    for (let i = 0; i < codes.length; i++) {
      for (let j = i + 1; j < codes.length; j++) {
        const key = codes[i] < codes[j] ? `${codes[i]}|${codes[j]}` : `${codes[j]}|${codes[i]}`;
        pairCount.set(key, (pairCount.get(key) ?? 0) + 1);
      }
    }
  }

  const edges: GEdge[] = [];
  for (const [key, weight] of pairCount) {
    if (weight < settings.minCommon) continue;
    const [source, target] = key.split("|");
    edges.push({ source, target, weight, dist: (220 / Math.sqrt(weight)) * settings.forceEdgeLength });
  }

  const communityCount = detectCommunities(nodes, edges);

  const self = makeSelfNode(data, "我", settings.nodeScale);
  for (const g of top) {
    let dist: number;
    let strength: number;
    if (settings.groupLevelPull) {
      const t = 1 - Math.exp(-Math.max(g.shared_count, 0) / 60);
      dist = (340 - t * 260) * settings.forceEdgeLength;
      strength = 0.02 + t * 0.16;
    } else {
      dist = clamp(380 / Math.sqrt(Math.max(g.shared_count, 1)), 64, 380) * settings.forceEdgeLength;
      strength = 0.006;
    }
    edges.push({ source: SELF_ID, target: g.id, weight: g.shared_count, dist, strength });
  }
  nodes.push(self);

  return { nodes, edges, communityCount };
}

export function buildGraph(
  data: RelationGraphData | null,
  settings: GraphSettings,
): BuiltGraph {
  if (!data) return { nodes: [], edges: [], communityCount: 0 };
  return settings.mode === "groups"
    ? buildGroups(data, settings)
    : buildPeople(data, settings);
}

/** 加权标签传播社区检测：节点反复采纳「邻居标签总权重」最大的社区 */
function detectCommunities(nodes: GNode[], edges: GEdge[]): number {
  if (nodes.length === 0) return 0;
  const adj = new Map<string, Array<{ id: string; w: number }>>();
  for (const n of nodes) adj.set(n.id, []);
  for (const e of edges) {
    const s = typeof e.source === "string" ? e.source : e.source.id;
    const t = typeof e.target === "string" ? e.target : e.target.id;
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

export const COMMUNITY_COLORS = [
  "#0099ff",
  "#36c08f",
  "#f6a23c",
  "#ef6f6c",
  "#9b6dff",
  "#2bb6d6",
  "#e072b8",
  "#7e8bd9",
  "#5bbf6a",
  "#d98c4a",
];

export function communityColor(community: number): string {
  if (community < 0) return "#9aa0a6"; // 未分组（中性灰）
  return COMMUNITY_COLORS[community % COMMUNITY_COLORS.length];
}

/**
 * GraphRawData → RelationGraphData。
 * 只排除当前账号本体；本机其他账号若出现在通讯录里是被添加的好友，应展示。
 */
export function toGraphData(r: GraphRawData): RelationGraphData {
  const raw: RawNode[] = Array.isArray(r?.nodes) ? r.nodes : [];
  const selfWxids = new Set<string>(r?.self ? [r.self] : []);
  return {
    selfUin: r?.self ?? "",
    selfAvatar: r?.self_avatar ?? "",
    persons: raw.filter(
      (n) => (n.kind === "contact" || n.kind === "official") && !selfWxids.has(n.id),
    ),
    groups: raw.filter((n) => n.kind === "group"),
    groupNames: r?.group_names ?? {},
    scannedGroups: r?.summary?.selected_groups ?? 0,
    summary: r?.summary,
    builtAt: 0,
  };
}
