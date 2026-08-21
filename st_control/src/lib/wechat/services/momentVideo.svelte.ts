/* ============================================================
 * 微信数据管理模块 — 朋友圈视频播放器状态
 * 自 WeChatPanel.svelte 下沉：IPC 解密 → HTTP 播放，
 * 行为保持原样（含关闭清理与播放错误处理）。
 * 注意：本文件使用 $state rune，扩展名必须是 .svelte.ts。
 * ============================================================ */
import { getMomentVideo } from './ipc';
import { logError } from '../utils';
import { mediaApi } from './mediaApi.svelte';
import type { MomentEntry } from '../types';

/** 朋友圈视频播放器状态（$state 可变对象，属性级变更驱动重渲染） */
export const momentVideo = $state({
  open: false,
  src: '',
  title: '',
  error: '',
});

/** 点击朋友圈视频：IPC 解密 → HTTP 播放 */
export async function playMomentVideo(m: MomentEntry, idx: number) {
  const v = m?.videos?.[idx];
  if (!v?.url) return;
  momentVideo.title = m.text || '朋友圈视频';
  momentVideo.error = '';
  momentVideo.src = '';
  momentVideo.open = true;
  try {
    const r = await getMomentVideo({ url: v.url, key: v.key || '' });
    if (!momentVideo.open) return;
    if (r?.kind === 'data' && r.file_key && mediaApi.videoBase) {
      momentVideo.src = `${mediaApi.videoBase}/${encodeURIComponent(r.file_key)}` +
        (mediaApi.token ? `?access_token=${encodeURIComponent(mediaApi.token)}` : '');
    } else {
      momentVideo.error = r?.error || '视频解密失败';
    }
  } catch (e) {
    logError('get_moment_video', e);
    if (momentVideo.open) momentVideo.error = '视频加载失败';
  }
}

/** 关闭朋友圈视频播放器 */
export function closeMomentVideo() {
  momentVideo.open = false;
  momentVideo.src = '';
  momentVideo.error = '';
}

/** 视频元素加载失败：清空源并显示错误（模板 onerror 内联逻辑下沉） */
export function handleVideoError() {
  momentVideo.error = '视频播放失败（文件可能已失效）';
  momentVideo.src = '';
}
