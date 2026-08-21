/* ============================================================
 * 微信数据管理模块 — 消息图片加载队列
 * 自 WeChatPanel.svelte 下沉：URL 直链优先 → IPC base64 回退，
 * 并发受限 + LRU 淘汰 + 失败有界自动重试，行为保持原样。
 * 注意：本文件使用 $state rune，扩展名必须是 .svelte.ts。
 * ============================================================ */
import { getMessageImage } from './ipc';
import { logError } from '../utils';
import { messageImageUrl } from './mediaApi.svelte';

const MAX_IMAGE_CACHE = 120;
const MAX_IMAGE_CONCURRENCY = 4;
const MAX_BLOCKED_KEYS = 500;
const IMAGE_AUTO_RETRY_MAX = 4;
const IMAGE_AUTO_RETRY_MS = 12000;

/**
 * 消息图片加载状态（$state 可变对象：
 * svelte-check 对 .svelte.ts 的 rune 重赋值检查有局限，属性级变更即可驱动重渲染）
 */
export const imageQueueState = $state({
  /** key（`${username}:${local_id}`）→ data URL；'' = 加载失败，undefined = 未加载 */
  cache: {} as Record<string, string>,
  /** URL 直链失败过的 key 集合：阻断模板的 URL 兜底，避免坏图反复重试 */
  blocked: new Set<string>(),
  /** 失败原因（key → 后端/本地诊断信息），供失效占位符展示 */
  failedReasons: {} as Record<string, string>,
});

const inflight = new Set<string>();
let queue: Array<{ key: string; username: string; localId: number }> = [];
let active = 0;
const retryCounts = new Map<string, number>();
const retryTimers = new Map<string, number>();

function trimCache() {
  const { cache } = imageQueueState;
  const keys = Object.keys(cache);
  if (keys.length <= MAX_IMAGE_CACHE) return;
  const drop = keys.slice(0, keys.length - MAX_IMAGE_CACHE);
  const next: Record<string, string> = {};
  for (const k of keys.slice(drop.length)) next[k] = cache[k];
  // 整体重建：与下沉前行为一致（Svelte 5 $state 响应替换）
  imageQueueState.cache = next;
}

function trimBlockedSet() {
  if (imageQueueState.blocked.size <= MAX_BLOCKED_KEYS) return;
  const arr = [...imageQueueState.blocked];
  imageQueueState.blocked = new Set(arr.slice(arr.length - MAX_BLOCKED_KEYS));
}

/** 加入消息图片加载队列（已缓存/排队中自动去重） */
export function enqueueImage(username: string, localId: number) {
  const key = `${username}:${localId}`;
  const { cache } = imageQueueState;
  if (cache[key] !== undefined || inflight.has(key)) return;
  inflight.add(key);
  // 优先 URL 直链：浏览器自行加载并 HTTP 缓存（后端 Cache-Control immutable），
  // 已解码图片二次查看零请求、零 base64 传输、不显示"解密中"。
  const url = messageImageUrl(username, localId);
  if (url) {
    cache[key] = url;
    inflight.delete(key);
    return;
  }
  // 回退：IPC 获取 base64 data URL（并发受限队列）
  queue.push({ key, username, localId });
  drainQueue();
}

/** URL 直链加载失败：清除 URL 并回退 IPC base64 加载；IPC 失败则标记不可显示 */
export function onImageLoadError(username: string | null, localId: number) {
  if (!username) return;
  const key = `${username}:${localId}`;
  const { cache, blocked } = imageQueueState;
  const current = cache[key];
  if (current && current.startsWith('http://')) {
    // URL 直链失败：探测失败原因（状态码/网络错误），供占位符展示
    void probeUrlStatus(current, key);
    // 阻断该图 URL 兜底，回退 IPC base64 加载
    delete cache[key];
    blocked.add(key);
    // token/端口可能已变更：刷新一次 API 配置，后续 URL 直链使用新值
    void import('./mediaApi.svelte').then((m) => m.loadMediaConfig());
    if (!inflight.has(key)) {
      inflight.add(key);
      queue.push({ key, username, localId });
      drainQueue();
    }
  } else {
    cache[key] = '';
    blocked.add(key);
    scheduleAutoRetry(username, localId);
  }
  trimBlockedSet();
}

/** 探测图片服务直链失败的具体原因（HTTP 状态码 / 网络错误），记录到失败原因 */
async function probeUrlStatus(url: string, key: string) {
  try {
    const ctrl = new AbortController();
    const timer = window.setTimeout(() => ctrl.abort(), 4000);
    const resp = await fetch(url, { signal: ctrl.signal, cache: 'no-store' });
    window.clearTimeout(timer);
    ctrl.abort(); // 拿到状态头后停止 body 下载
    if (!imageQueueState.failedReasons[key]) {
      if (resp.ok) {
        imageQueueState.failedReasons[key] = '图片服务返回正常但浏览器解码失败（格式异常）';
      } else {
        const ct = resp.headers.get('content-type') || '未知内容';
        imageQueueState.failedReasons[key] = `图片服务返回 HTTP ${resp.status}（${ct}）`;
      }
    }
  } catch {
    // 网络错误 / 超时 / CORS：仅在没有更具体原因时记录
    if (!imageQueueState.failedReasons[key]) {
      imageQueueState.failedReasons[key] = '本地图片服务连接失败（端口未开或已变更），将回退本地解析';
    }
  }
}

/** 记录图片失败原因（占位符 title 展示；URL 直链失败时原因未知，留待 IPC 回退结果覆盖） */
export function recordImageFailure(username: string | null, localId: number, reason: string) {
  if (!username) return;
  imageQueueState.failedReasons[`${username}:${localId}`] = reason;
}

/** 图片解密失败后点击重试：清除失败标记并重新入队 */
export function retryImage(username: string | null, localId: number) {
  if (!username) return;
  const key = `${username}:${localId}`;
  const { cache, blocked } = imageQueueState;
  // 仅当该图处于"无法显示"状态（失败缓存 或 URL 直链被阻断）时允许重试
  if (cache[key] !== '' && !blocked.has(key)) return;
  delete cache[key];
  blocked.delete(key);
  delete imageQueueState.failedReasons[key];
  enqueueImage(username, localId);
}

/** 图片加载失败后的有界自动重试：等微信本地缓存/网络恢复后自动补显 */
function scheduleAutoRetry(username: string, localId: number) {
  const key = `${username}:${localId}`;
  if (retryTimers.has(key)) return;
  const n = (retryCounts.get(key) || 0) + 1;
  retryCounts.set(key, n);
  if (n > IMAGE_AUTO_RETRY_MAX) return;
  const timer = window.setTimeout(() => {
    retryTimers.delete(key);
    retryImage(username, localId);
  }, IMAGE_AUTO_RETRY_MS);
  retryTimers.set(key, timer);
}

/** 清空图片自动重试定时器（切换会话 / 组件销毁时调用） */
export function clearAutoRetries() {
  for (const t of retryTimers.values()) clearTimeout(t);
  retryTimers.clear();
  retryCounts.clear();
}

function drainQueue() {
  while (active < MAX_IMAGE_CONCURRENCY && queue.length) {
    const job = queue.shift()!;
    active++;
    const { cache } = imageQueueState;
    getMessageImage({ username: job.username, localId: job.localId })
      .then((r) => {
        const url = r && r.kind === 'data' && r.data ? r.data : '';
        if (cache[job.key] !== url) cache[job.key] = url;
        if (url) {
          // 成功补显后取消后续自动重试
          const t = retryTimers.get(job.key);
          if (t) { clearTimeout(t); retryTimers.delete(job.key); }
          retryCounts.delete(job.key);
          delete imageQueueState.failedReasons[job.key];
        } else {
          // 全链路失败：记录后端返回的具体原因（本地无文件/CDN 未取到/需开 Hook）
          const reason = r && typeof r.reason === 'string' && r.reason ? r.reason : '';
          if (reason) imageQueueState.failedReasons[job.key] = reason;
        }
        trimCache();
      })
      .catch((e) => {
        cache[job.key] = '';
        logError('get_message_image', e);
      })
      .finally(() => {
        active--;
        inflight.delete(job.key);
        drainQueue();
      });
  }
}
