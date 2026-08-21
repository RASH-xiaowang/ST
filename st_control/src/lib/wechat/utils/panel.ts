/* ============================================================
 * 微信数据管理模块 — WeChatPanel 纯函数
 * 自 WeChatPanel.svelte 下沉：无状态、可单测的展示/计算辅助。
 * ============================================================ */
import type { ContactItem, FavoriteEntry, GeneralCategory, MomentEntry, ResourceFile, ResourceFilesOverview, StaticEmoticonCategory, StaticEmoticonFile, WeChatMessage, WeChatSession } from '../types';
import { cellText } from './format';
import { isKefuSession, isMiniAppKefuSession } from './misc';
import { filterByAnyKeyword } from '../../utils/filter';

/** 静态表情文件名（去 .png 后缀）→ 路径 映射（自 WeChatPanel 下沉） */
export function buildStaticEmoticonMap(
  categories: StaticEmoticonCategory[],
): Map<string, string> {
  const map = new Map<string, string>();
  for (const cat of categories) {
    for (const f of cat.files ?? []) {
      const key = f.name.replace(/\.png$/i, '');
      if (!map.has(key)) map.set(key, f.path);
    }
  }
  return map;
}

/** 静态表情分类过滤（自 WeChatPanel filteredStaticEmoticons 下沉） */
export function filterStaticEmoticons(
  categories: StaticEmoticonCategory[],
  cat: string,
  search: string,
): { category: string; label: string; file: StaticEmoticonFile }[] {
  const list: { category: string; label: string; file: StaticEmoticonFile }[] = [];
  for (const c of categories) {
    if (cat !== 'all' && c.category !== cat) continue;
    for (const f of c.files ?? []) {
      if (search) {
        const q = search.toLowerCase();
        const name = f.name.replace(/\.png$/i, '').toLowerCase();
        if (!name.includes(q) && !c.label.includes(q)) continue;
      }
      list.push({ category: c.category, label: c.label, file: f });
    }
  }
  return list;
}

/** 资源文件列表：分类过滤 + 关键词过滤 + modify_time 降序（自 WeChatPanel shownFiles 下沉） */
export function filterSortResourceFiles(
  data: ResourceFilesOverview,
  cat: string,
  search: string,
): ResourceFile[] {
  const all = [
    ...(data.images ?? []),
    ...(data.videos ?? []),
    ...(data.files ?? []),
  ];
  const byCat = cat === 'all' ? all : all.filter((f) => f.category === cat);
  return filterByAnyKeyword(byCat, search, (f) => f.file_name || '', (f) => f.md5 || '').sort(
    (a, b) => (b.modify_time ?? 0) - (a.modify_time ?? 0),
  );
}

/** 设置分类行内搜索：行命中（cellText 大小写不敏感）或分类 label/table 命中；
 * 空关键词返回原数组引用（自 WeChatPanel settingsFilteredCats 下沉） */
export function filterSettingsCats(
  data: GeneralCategory[],
  search: string,
): GeneralCategory[] {
  const q = search.trim().toLowerCase();
  if (!q) return data;
  return data
    .map((cat) => {
      const rows = (cat.rows ?? []).filter((r) =>
        r.some((c) => cellText(c).toLowerCase().includes(q)));
      return { ...cat, rows, count: rows.length };
    })
    .filter((cat) =>
      (cat.label || '').toLowerCase().includes(q) ||
      (cat.table || '').toLowerCase().includes(q) ||
      cat.count > 0);
}

/** 会话图片消息条目（图片查看器数据源） */
export interface SessionImageItem {
  src: string;
  time: string;
  local_id: number;
  sender_username?: string;
  is_group?: boolean;
}

/** 收集会话已加载的图片消息（src 非空，含实时内嵌与懒加载缓存；自 WeChatPanel 下沉） */
export function collectSessionImages(
  messages: WeChatMessage[],
  imageCache: Record<string, string>,
  sessionKey: string | null,
): SessionImageItem[] {
  const imgs: SessionImageItem[] = [];
  if (!sessionKey) return imgs;
  for (const m of messages) {
    if (m.type !== 3) continue;
    const src = m.image_url || imageCache[sessionKey + ':' + m.local_id];
    if (src) {
      imgs.push({
        src,
        time: m.time || '',
        local_id: m.local_id,
        sender_username: m.sender_username ?? '',
        is_group: !!m.is_group,
      });
    }
  }
  return imgs;
}

/** 图片查看器缩放档位（自 WeChatPanel 下沉） */
export const VIEWER_ZOOM_STEPS = [1, 1.5, 2, 3, 4] as const;

/**
 * 缩放档位索引推进（自 WeChatPanel cycleZoom/onViewerWheel 下沉）：
 * cycle 模式循环推进（按钮）；clamp 模式按方向在范围内移动（滚轮）；
 * 当前值不在档位表时从 0 起算。
 */
export function zoomStepIndex(
  steps: readonly number[],
  current: number,
  dir: 1 | -1,
  mode: 'cycle' | 'clamp',
): number {
  const idx = steps.indexOf(current);
  const base = idx < 0 ? 0 : idx;
  if (mode === 'cycle') return (base + 1) % steps.length;
  return Math.max(0, Math.min(steps.length - 1, base + dir));
}

/** 裁剪 Record：键数超过 max 时删除最先插入的键（就地修改，返回 void） */
export function trimRecord(rec: Record<string, unknown>, max: number): void {
  const keys = Object.keys(rec);
  if (keys.length <= max) return;
  const drop = keys.length - max;
  for (const k of keys.slice(0, drop)) delete rec[k];
}

/** 日历热力色：按计数线性映射到主题色透明度 */
export function calHeat(count: number): string {
  if (!count) return 'var(--wc-bg2)';
  const a = Math.min(0.9, 0.15 + count / 30);
  return `color-mix(in srgb, var(--wc-theme) ${Math.round(a * 100)}%, transparent)`;
}

/** 朋友圈 tid 比较（新 → 旧）。
 * 注意不能用字符串比较——tid 常为负数（如 -3463300…），
 * 字典序会把更负（更旧）的排到前面，导致懒加载后顺序颠倒。 */
export function cmpTid(a: string, b: string): number {
  try {
    const x = BigInt(a);
    const y = BigInt(b);
    return x < y ? 1 : x > y ? -1 : 0;
  } catch {
    // 非数值 tid 兜底：按长度+字典序降序
    if (a.length !== b.length) return b.length - a.length;
    return a < b ? 1 : a > b ? -1 : 0;
  }
}

/** 会话已编辑消息去重键：username:localId */
export function editKey(username: string | null, localId: number): string {
  return `${username ?? ''}:${localId}`;
}

/** 会话是否匹配搜索关键词（名称或 username 子串，大小写不敏感） */
export function sessionMatchesKeyword(s: WeChatSession, q: string): boolean {
  const keyword = q.toLowerCase();
  return (
    !keyword ||
    (s.name || '').toLowerCase().includes(keyword) ||
    (s.username || '').toLowerCase().includes(keyword)
  );
}

/** 会话列表关键词匹配：名称/摘要/username 子串，大小写不敏感 */
export function sessionKeywordMatch(s: WeChatSession, q: string): boolean {
  const keyword = q.toLowerCase();
  return (
    !keyword ||
    (s.name || '').toLowerCase().includes(keyword) ||
    (s.summary || '').toLowerCase().includes(keyword) ||
    (s.username || '').toLowerCase().includes(keyword)
  );
}

/** 会话列表主体过滤：排除公众号/客服，再按关键词匹配（空关键词返回全量） */
export function filterMainSessions(
  sessions: WeChatSession[],
  q: string,
): WeChatSession[] {
  const base = sessions.filter(
    (s) =>
      !s.is_official &&
      !isKefuSession(s.username) &&
      !isMiniAppKefuSession(s.username),
  );
  return q ? base.filter((s) => sessionKeywordMatch(s, q)) : base;
}

/** 图片体检会话统计条目（MissingImagesData.chats 元素） */
export interface CheckupChat {
  username: string;
  name?: string;
  missing: number;
  total_images: number;
  [key: string]: unknown;
}

/** 图片体检排序模式 */
export type CheckupSort = 'missing' | 'total' | 'name';

/** 图片体检会话列表：关键词/仅缺失过滤 + 三路排序（缺失/总量/名称-zh） */
export function filterSortCheckupChats(
  chats: CheckupChat[],
  opts: { q: string; onlyMissing: boolean; sort: CheckupSort },
): CheckupChat[] {
  const q = opts.q.trim().toLowerCase();
  let out = chats;
  if (q) {
    out = out.filter(
      (c) =>
        String(c.name || '').toLowerCase().includes(q) ||
        String(c.username || '').toLowerCase().includes(q),
    );
  }
  if (opts.onlyMissing) out = out.filter((c) => (c.missing ?? 0) > 0);
  return [...out].sort((a, b) => {
    if (opts.sort === 'total') return (b.total_images ?? 0) - (a.total_images ?? 0);
    if (opts.sort === 'name') {
      return String(a.name || a.username || '').localeCompare(
        String(b.name || b.username || ''),
        'zh',
      );
    }
    return (b.missing ?? 0) - (a.missing ?? 0) || (b.total_images ?? 0) - (a.total_images ?? 0);
  });
}

/** 收藏条目过滤：按类型（'all' 或 type_label）与关键词（标题/描述/来源） */
export function filterFavoriteItems(
  items: FavoriteEntry[],
  opts: { type: string; q: string },
): FavoriteEntry[] {
  let base = items;
  if (opts.type !== 'all') base = base.filter((f) => f.type_label === opts.type);
  if (opts.q) {
    const q = opts.q.toLowerCase();
    base = base.filter(
      (f) =>
        (f.title || '').toLowerCase().includes(q) ||
        (f.desc || '').toLowerCase().includes(q) ||
        (f.source || '').toLowerCase().includes(q),
    );
  }
  return base;
}

/** 选择记录 → 有效正数 id 数组（过滤非法/非正数项） */
export function selectedIdsFromRecord(sel: Record<string, boolean>): number[] {
  return Object.keys(sel)
    .filter((k) => sel[k])
    .map((k) => Number(k))
    .filter((n) => Number.isFinite(n) && n > 0);
}

// 关键词过滤已上移至共享层 src/lib/utils/filter.ts，此处 re-export 保持既有调用点不变
export { filterByAnyKeyword, filterByKeyword } from '../../utils/filter';

/** 群监控规则（匹配条件；hits 计数由调用方维护） */
export type MonitorRule = {
  id: number;
  kind: 'keyword' | 'regex' | 'sender' | 'media';
  value: string;
  enabled: boolean;
};

/** 匹配消息命中哪些监控规则（纯函数，不含 hits 计数副作用） */
export function matchMonitors(
  m: { content?: unknown; sender_username?: unknown; sender?: unknown; media_type?: unknown },
  rules: MonitorRule[],
): number[] {
  const content = String(m?.content ?? '');
  const sender = String(m?.sender_username ?? m?.sender ?? '');
  const media = String(m?.media_type ?? '');
  const hits: number[] = [];
  for (const mon of rules) {
    if (!mon.enabled) continue;
    let ok = false;
    switch (mon.kind) {
      case 'keyword':
        ok = content.toLowerCase().includes(mon.value.toLowerCase());
        break;
      case 'regex':
        try {
          ok = new RegExp(mon.value, 'i').test(content);
        } catch {
          ok = false;
        }
        break;
      case 'sender':
        ok = sender.toLowerCase().includes(mon.value.toLowerCase());
        break;
      case 'media':
        ok = media === mon.value;
        break;
    }
    if (ok) hits.push(mon.id);
  }
  return hits;
}

/** 朋友圈增量合并：已存在条目按 tid 更新，新条目置顶，最后 tid 降序。
 * 返回合并后的条目与新增数量（供调用方做 UI 副作用，如预载头像/提示）。 */
export function mergeMoments(
  existing: MomentEntry[],
  incoming: MomentEntry[],
): { items: MomentEntry[]; fresh: MomentEntry[] } {
  const byTid = new Map(
    incoming.filter((m) => m.tid).map((m) => [m.tid, m] as const),
  );
  let next = existing.map((m) => byTid.get(m.tid) ?? m);
  const known = new Set(next.map((m) => m.tid));
  const fresh = incoming.filter((m) => m.tid && !known.has(m.tid));
  if (fresh.length > 0) next = [...fresh, ...next];
  next.sort((a, b) => cmpTid(String(a.tid), String(b.tid)));
  return { items: next, fresh };
}

/** 通讯录按拼音首字母分组：'#'（无首字母）置底，其余按 localeCompare 排序 */
export function groupContactsByInitial(
  contacts: ContactItem[],
): [string, ContactItem[]][] {
  const groups = new Map<string, ContactItem[]>();
  for (const c of contacts) {
    const k = c.initial || '#';
    if (!groups.has(k)) groups.set(k, []);
    groups.get(k)!.push(c);
  }
  return [...groups.entries()].sort((a, b) => {
    if (a[0] === '#') return 1;
    if (b[0] === '#') return -1;
    return a[0].localeCompare(b[0]);
  });
}

/**
 * 群成员按所在群聊分组（通讯录「群成员」分类）。
 * 无归属的成员归入「未归属群聊」并置底，其余按群名中文排序。
 */
export function groupMembersByRoom(
  contacts: ContactItem[],
): [string, ContactItem[]][] {
  const groups = new Map<string, ContactItem[]>();
  for (const c of contacts) {
    const k = c.group_name?.trim() || '未归属群聊';
    if (!groups.has(k)) groups.set(k, []);
    groups.get(k)!.push(c);
  }
  return [...groups.entries()].sort((a, b) => {
    if (a[0] === '未归属群聊') return 1;
    if (b[0] === '未归属群聊') return -1;
    return a[0].localeCompare(b[0], 'zh');
  });
}

// ─── 朋友圈日期分组 ───

/** 一天的朋友圈动态 */
export interface MomentDayGroup {
  /** 分组标签：今天 / 昨天 / YYYY-MM-DD（未知时间归入「未知时间」） */
  label: string;
  /** 排序键：日期字符串或 'zzz'（未知置底） */
  dateKey: string;
  items: MomentEntry[];
}

const pad2 = (n: number) => String(n).padStart(2, '0');

/** Unix 秒 → 本地日期 "YYYY-MM-DD" */
function localDateKey(ts: number): string {
  if (!ts || !Number.isFinite(ts)) return '';
  const d = new Date(ts * 1000);
  if (isNaN(d.getTime())) return '';
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
}

/**
 * 朋友圈按日期分组（保持原有顺序）：今天 / 昨天 / 更早按日期，
 * 未知时间的动态归入「未知时间」并置底。用于时间线卡片化的
 * 日期分隔条渲染。
 */
export function groupMomentsByDate(items: MomentEntry[]): MomentDayGroup[] {
  const now = new Date();
  const today = `${now.getFullYear()}-${pad2(now.getMonth() + 1)}-${pad2(now.getDate())}`;
  const yest = new Date(now.getTime() - 86400_000);
  const yesterday = `${yest.getFullYear()}-${pad2(yest.getMonth() + 1)}-${pad2(yest.getDate())}`;
  const groups: MomentDayGroup[] = [];
  for (const m of items) {
    const key = localDateKey(m.ts) || 'zzz';
    let label: string;
    if (key === 'zzz') label = '未知时间';
    else if (key === today) label = '今天';
    else if (key === yesterday) label = '昨天';
    else label = key;
    const last = groups[groups.length - 1];
    if (last && last.dateKey === key) {
      last.items.push(m);
    } else {
      groups.push({ label, dateKey: key, items: [m] });
    }
  }
  return groups;
}
