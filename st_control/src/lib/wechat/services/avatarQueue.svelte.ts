/* ============================================================
 * 微信数据管理模块 — 头像加载队列
 * O(1) 访问戳 LRU + 并发受限加载，防止内存泄漏与 IPC 风暴。
 * 自 WeChatPanel.svelte 下沉，行为保持原样（含失败冷却重试语义）。
 * 注意：本文件使用 $state rune，扩展名必须是 .svelte.ts。
 * ============================================================ */
import { getUserAvatar } from './ipc';
import { logError } from '../utils';

const MAX_AVATAR_CACHE = 2000;
const MAX_AVATAR_CONCURRENCY = 6;
/** 头像加载失败/空结果后的冷却时长，避免 IPC 风暴 */
const AVATAR_RETRY_MS = 20000;

/** key(用户名) → data URL；'' = 占位/上次失败，undefined = 未加载 */
export const avatarCache = $state<Record<string, string>>({});

/** key → 单调访问戳（普通 Map，不驱动 UI，仅用于低频 LRU 淘汰） */
const avatarOrder = new Map<string, number>();
const avatarInflight = new Set<string>();
/** 头像加载失败/空结果的时间戳，用于冷却后重试（新消息到达会再次触发） */
const avatarFailedAt = new Map<string, number>();
let avatarStamp = 0;
let avatarQueue: string[] = [];
let avatarActive = 0;

function touchAvatarKey(key: string) {
  avatarOrder.set(key, ++avatarStamp);
}

function evictAvatarIfNeeded() {
  if (avatarOrder.size <= MAX_AVATAR_CACHE) return;
  // 低频淘汰：一次性按访问戳升序移除最旧的一批，避免每次 touch 重建数组
  const sorted = [...avatarOrder.entries()].sort((a, b) => a[1] - b[1]);
  const drop = sorted.length - MAX_AVATAR_CACHE;
  // Svelte 5 $state 深响应：delete 即可驱动重渲染，无需整对象重建
  for (let i = 0; i < drop; i++) {
    const k = sorted[i][0];
    delete avatarCache[k];
    avatarOrder.delete(k);
  }
}

/** 加入头像加载队列（已缓存/排队/冷却期内自动去重） */
export function enqueueAvatar(u: string) {
  if (!u) return;
  const cached = avatarCache[u];
  if (cached !== undefined && cached !== '') { touchAvatarKey(u); return; }
  if (avatarInflight.has(u)) return; // 排队/加载中统一去重
  // 空占位 = 上次未取到：冷却期内不重试，避免 IPC 风暴；
  // 冷却后（或新消息/刷新再次触发）自动补拉，头像数据到位即可显示。
  if (cached === '') {
    const failedAt = avatarFailedAt.get(u) ?? 0;
    if (Date.now() - failedAt < AVATAR_RETRY_MS) return;
  }
  avatarInflight.add(u);
  avatarQueue.push(u);
  drainAvatarQueue();
}

function drainAvatarQueue() {
  while (avatarActive < MAX_AVATAR_CONCURRENCY && avatarQueue.length) {
    const u = avatarQueue.shift()!;
    avatarActive++;
    avatarCache[u] = ''; // 占位：先展示兜底字母
    touchAvatarKey(u);
    evictAvatarIfNeeded();
    loadUserAvatar(u).finally(() => {
      avatarActive--;
      avatarInflight.delete(u);
      drainAvatarQueue();
    });
  }
}

/** 批量预加载头像（会话/消息列表变化时调用） */
export function preloadAvatars(usernames: string[]) {
  for (const u of usernames) if (u) enqueueAvatar(u);
}

async function loadUserAvatar(username: string) {
  try {
    const r = await getUserAvatar(username);
    if (r && r.data && avatarCache[username] !== r.data) {
      // Svelte 5 $state 深响应：直接赋值属性即可触发重渲染，省去全量拷贝
      avatarCache[username] = r.data;
      avatarFailedAt.delete(username);
      touchAvatarKey(username);
    } else {
      avatarFailedAt.set(username, Date.now());
    }
  } catch (e) {
    avatarFailedAt.set(username, Date.now());
    logError('loadUserAvatar', e);
  }
}

