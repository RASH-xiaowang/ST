/* ============================================================
 * 微信数据管理模块 — 朋友圈图片懒加载
 * 自 WeChatPanel.svelte 下沉：下载 + ISAAC-64 解密，并发受限 + LRU，
 * 行为保持原样（含缩略图预加载与查看器原图异步补拉）。
 * 注意：本文件使用 $state rune，扩展名必须是 .svelte.ts。
 * ============================================================ */
import { getMomentImage } from './ipc';
import { logError } from '../utils';

const MAX_MOMENT_IMG_CONCURRENCY = 4;
const MAX_MOMENT_IMG_CACHE = 400;

/** 朋友圈图片所需字段（MomentMedia 或视频封面等部分对象） */
type MomentImageLike = {
  thumb?: string;
  url?: string;
  key?: string;
  thumb_token?: string;
  url_token?: string;
};

/**
 * 朋友圈图片加载状态（$state 可变对象：
 * svelte-check 对 .svelte.ts 的 rune 重赋值检查有局限，属性级变更即可驱动重渲染）
 */
export const momentMedia = $state({
  /** key（`${key || '-'}:${url}`）→ data URL；'' = 加载失败，undefined = 未加载 */
  imgCache: {} as Record<string, string>,
});

const inflight = new Set<string>();
let queue: { key: string; url: string; key_: string; token: string }[] = [];
let active = 0;

/** 朋友圈图片缓存 key（原图 URL 与 key 组合去重） */
export function momentImgKey(url: string, key: string) {
  return `${key || '-'}:${url}`;
}

/** 加入朋友圈图片加载队列（已缓存/排队中自动去重） */
export function enqueueMomentImage(img: MomentImageLike) {
  const url = img?.thumb || img?.url;
  const key_ = img?.key || '';
  if (!url) return;
  const key = momentImgKey(url, key_);
  if (momentMedia.imgCache[key] !== undefined || inflight.has(key)) return;
  inflight.add(key);
  queue.push({ key, url, key_, token: img?.thumb_token || img?.url_token || '' });
  drainQueue();
}

/** 获取已解码的朋友圈图片（网格缩略图），未加载返回空串 */
export function momentImgSrc(img: MomentImageLike | null | undefined): string {
  if (!img) return '';
  const u = img.thumb || img.url || '';
  return momentMedia.imgCache[momentImgKey(u, img.key || '')] || '';
}

/** 异步拉取朋友圈原图（/0）：写入缓存并返回 data URL，失败返回空串 */
export async function loadMomentOriginal(img: MomentImageLike): Promise<string> {
  const url = img?.url;
  if (!url) return '';
  const k = momentImgKey(url, img.key || '');
  const cached = momentMedia.imgCache[k];
  if (cached) return cached;
  try {
    const r = await getMomentImage({
      url,
      key: img.key || '',
      token: img.url_token || img.thumb_token || '',
    });
    const data = r?.kind === 'data' && r.data ? r.data : '';
    momentMedia.imgCache[k] = data;
    return data;
  } catch (e) {
    logError('get_moment_image', e);
    return '';
  }
}

function trimCache() {
  const keys = Object.keys(momentMedia.imgCache);
  if (keys.length <= MAX_MOMENT_IMG_CACHE) return;
  const drop = keys.slice(0, keys.length - MAX_MOMENT_IMG_CACHE);
  const next: Record<string, string> = {};
  for (const k of keys.slice(drop.length)) next[k] = momentMedia.imgCache[k];
  // 整体重建：与下沉前行为一致（Svelte 5 $state 响应替换）
  momentMedia.imgCache = next;
}

function drainQueue() {
  while (active < MAX_MOMENT_IMG_CONCURRENCY && queue.length) {
    const job = queue.shift()!;
    active++;
    getMomentImage({ url: job.url, key: job.key_, token: job.token })
      .then((r) => {
        const data = r && r.kind === 'data' && r.data ? r.data : '';
        momentMedia.imgCache[job.key] = data;
        trimCache();
      })
      .catch((e) => {
        momentMedia.imgCache[job.key] = '';
        logError('get_moment_image', e);
      })
      .finally(() => {
        active--;
        inflight.delete(job.key);
        drainQueue();
      });
  }
}
