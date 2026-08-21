/* ============================================================
 * 微信数据管理模块 — 杂项纯函数
 * 自 WeChatPanel.svelte 下沉：文件类型/小程序 URL/缺失图统计/会话识别。
 * 不依赖组件状态，可独立单测。
 * ============================================================ */

/** 文件类型 → 图标底色分类（决定彩色瓦片颜色） */
export function extTone(ext: string | null | undefined): string {
  const e = (ext || '').toLowerCase();
  if (['doc', 'docx', 'wps', 'txt', 'md', 'pdf'].includes(e)) return 'doc';
  if (['xls', 'xlsx', 'csv'].includes(e)) return 'sheet';
  if (['ppt', 'pptx'].includes(e)) return 'slide';
  if (['zip', 'rar', '7z', 'tar', 'gz'].includes(e)) return 'zip';
  if (['mp3', 'wav', 'm4a', 'flac', 'aac', 'silk'].includes(e)) return 'audio';
  if (['mp4', 'mov', 'avi', 'mkv', 'm4v'].includes(e)) return 'video';
  if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'heic'].includes(e)) return 'image';
  if (['apk', 'exe', 'msi'].includes(e)) return 'app';
  return 'file';
}

/** 从 pagepath 的 url= 参数中解码出真实网页链接（如 docs.qq.com） */
export function miniAppPageUrl(r: { pagepath?: unknown } | null | undefined): string {
  const pp = String(r?.pagepath || '');
  const m = pp.match(/[?&]url=([^&\s]+)/);
  if (!m) return '';
  try {
    const decoded = decodeURIComponent(m[1]);
    return /^https?:\/\//i.test(decoded) ? decoded : '';
  } catch {
    return '';
  }
}

/** 缺失图统计占比（% 字符串，total 为 0 时返回 0.0） */
export function checkupPct(n: number, total: number): string {
  if (!total) return '0.0';
  return ((n / total) * 100).toFixed(1);
}

/** 单个会话缺失图占比（% 字符串，total 为 0 时返回 0.0） */
export function checkupRatePct(missing: number, total: number): string {
  if (!total) return '0.0';
  return ((missing / total) * 100).toFixed(1);
}

/** 统计存在缺失图片的会话数（missing 缺省按 0 计） */
export function countMissingChats(chats: { missing?: number }[]): number {
  return chats.filter((c) => (c.missing ?? 0) > 0).length;
}

/** 客服会话识别：客服消息（@kefu.openim / @weclaw / 品牌服务） */
export function isKefuSession(u: string): boolean {
  const s = (u || '').toLowerCase();
  return s.includes('@kefu.openim')
    || s.includes('@weclaw')
    || s === 'brandservicesessionholder'
    || s === 'brandsessionholder';
}

/** 小程序客服消息识别（@openim / opencustomerservicemsg） */
export function isMiniAppKefuSession(u: string): boolean {
  const s = (u || '').toLowerCase();
  return s.includes('@openim') || s.includes('opencustomerservicemsg');
}
