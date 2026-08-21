/* ============================================================
 * 微信数据管理模块 — 本地 HTTP API 媒体配置
 * 自 WeChatPanel.svelte 下沉：管理 API 端口/令牌与媒体 URL 直链构造。
 * 注意：本文件使用 $state/$derived rune，扩展名必须是 .svelte.ts。
 * ============================================================ */
import { getApiSettings } from './ipc';

/**
 * 本地 HTTP API 媒体配置（$state 可变对象：
 * svelte-check 对 .svelte.ts 的 rune 重赋值检查有局限，属性级变更即可驱动重渲染）
 */
export const mediaApi = $state({
  /** HTTP API 图片直链基址（api_enabled 时使用，浏览器 HTTP 缓存避免重复 IPC/解密） */
  mediaBase: '',
  /** HTTP API 朋友圈视频直链基址 */
  videoBase: '',
  /** HTTP API 访问令牌 */
  token: '',
});

/** 读取 HTTP API 端口/令牌，用于图片 URL 直链 */
export async function loadMediaConfig(): Promise<void> {
  try {
    const s = await getApiSettings();
    if (s?.enabled && s?.port) {
      mediaApi.mediaBase = `http://127.0.0.1:${s.port}/api/v1/media`;
      mediaApi.videoBase = `http://127.0.0.1:${s.port}/api/v1/sns/video`;
      mediaApi.token = s?.token ?? '';
    } else {
      mediaApi.mediaBase = '';
      mediaApi.videoBase = '';
      mediaApi.token = '';
    }
  } catch {
    mediaApi.mediaBase = '';
    mediaApi.videoBase = '';
    mediaApi.token = '';
  }
}

/** 消息图片 URL 直链（自动附带 access_token，避免 401 被浏览器 ORB 拦截导致裂图） */
export function messageImageUrl(username: string, localId: number): string {
  if (!mediaApi.mediaBase) return '';
  const q = mediaApi.token ? `?access_token=${encodeURIComponent(mediaApi.token)}` : '';
  return `${mediaApi.mediaBase}/${encodeURIComponent(username)}/${localId}${q}`;
}

/** 本地 API 资源直链（自动附带 access_token，用于表情/文件等媒体资源） */
export function apiAssetUrl(apiPath: string): string {
  const root = mediaApi.mediaBase ? mediaApi.mediaBase.replace(/\/media$/, '') : '';
  if (!root) return '';
  const q = mediaApi.token ? `?access_token=${encodeURIComponent(mediaApi.token)}` : '';
  return `${root}${apiPath}${q}`;
}
