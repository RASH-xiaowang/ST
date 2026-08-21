/* ============================================================
 * 微信数据管理模块 — 格式化 / 图标 / 展示辅助函数
 * 全部为无副作用纯函数；自 WeChatPanel.svelte 下沉，行为保持原样。
 * ============================================================ */

export { errText } from '../../format';

/**
 * 会话/消息时间分割线。
 * 注意：语义与 utils/index.ts 的 formatDividerTime 不同（此处今天只显示 HH:mm，
 * 且没有“昨天”分支），两者勿合并。
 */
export function formatDividerTime(ts: number | string | Date | undefined): string {
  if (!ts) return '';
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return String(ts);
  const now = new Date();
  const isToday = d.toDateString() === now.toDateString();
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  const hh = String(d.getHours()).padStart(2, '0');
  const mm = String(d.getMinutes()).padStart(2, '0');
  if (isToday) return `${hh}:${mm}`;
  return `${y}-${m}-${day} ${hh}:${mm}`;
}

/**
 * 从用户名取头像首字母。
 * 注意：保持 WeChatPanel 原语义（含前导空格场景），勿与 utils/index.ts 版本混用。
 */
export function avatarLetter(name: string): string {
  const c = name?.trim().charAt(0);
  return (c && /[\u4e00-\u9fff]/.test(c)) ? c : (name?.charAt(0)?.toUpperCase() || '?');
}

/** 用户名 → 固定头像底色（哈希取色，同名恒定） */
export function colorFromName(name: string): string {
  const colors: string[] = ['#f44336','#e91e63','#9c27b0','#673ab7','#3f51b5','#2196f3','#009688','#4caf50','#ff9800','#795548','#607d8b','#ff5722'];
  let h = 0;
  for (let i = 0; i < (name || '?').length; i++) h = ((h << 5) - h) + (name || '?').charCodeAt(i);
  return colors[Math.abs(h) % colors.length];
}

/** 根据表情描述查找本地静态表情图片路径（未命中返回 null） */
export function resolveStaticEmojiPath(description: unknown, emojiMap: Map<string, string>): string | null {
  if (!description) return null;
  return emojiMap.get(String(description).trim()) ?? null;
}

/** 语音时长（秒）→ `m:ss` 文案 */
export function fmtDur(sec: number | null | undefined): string {
  const s = Math.max(0, Math.round(sec || 0));
  const m = Math.floor(s / 60);
  const r = s % 60;
  return `${m}:${r.toString().padStart(2, '0')}`;
}

/** 统一线性图标渲染：24 视口、1.6 stroke、随 currentColor */
export function iconSvg(paths: string, size = 16): string {
  return `<svg viewBox="0 0 24 24" width="${size}" height="${size}" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths}</svg>`;
}

/** 内置线性图标路径表（Feather 风格子集） */
export const ICON_PATHS = {
  file: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="8" y1="13" x2="16" y2="13"/><line x1="8" y1="17" x2="14" y2="17"/>',
  image: '<rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/>',
  music: '<path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/>',
  video: '<polygon points="23 7 16 12 23 17 23 7"/><rect x="1" y="5" width="15" height="14" rx="2"/>',
  archive: '<rect x="2" y="3" width="20" height="5" rx="1"/><path d="M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8"/><path d="M10 12h4"/>',
  sheet: '<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M3 9h18M3 15h18M9 3v18M15 3v18"/>',
  present: '<path d="M2 3h20"/><path d="M4 3v14h16V3"/><path d="M9 21h6M12 17v4"/>',
  gear: '<circle cx="12" cy="12" r="3"/><path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42"/>',
  link: '<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>',
  pin: '<path d="M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0 1 18 0z"/><circle cx="12" cy="10" r="3"/>',
  mic: '<path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" y1="19" x2="12" y2="22"/>',
  note: '<path d="M16 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V8z"/><path d="M15 3v4a1 1 0 0 0 1 1h4"/><path d="M8 13h8M8 17h5"/>',
  chat: '<path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>',
  clip: '<path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48"/>',
  film: '<rect x="2" y="2" width="20" height="20" rx="2.18"/><line x1="7" y1="2" x2="7" y2="22"/><line x1="17" y1="2" x2="17" y2="22"/><line x1="2" y1="12" x2="22" y2="12"/><line x1="2" y1="7" x2="7" y2="7"/><line x1="2" y1="17" x2="7" y2="17"/><line x1="17" y1="17" x2="22" y2="17"/><line x1="17" y1="7" x2="22" y2="7"/>',
  app: '<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M3 9h18M3 15h18M9 3v18"/>',
  card: '<rect x="1" y="4" width="22" height="16" rx="2"/><line x1="1" y1="10" x2="23" y2="10"/>',
  gift: '<polyline points="20 12 20 22 4 22 4 12"/><rect x="2" y="7" width="20" height="5"/><line x1="12" y1="22" x2="12" y2="7"/><path d="M12 7H7.5a2.5 2.5 0 0 1 0-5C11 2 12 7 12 7z"/><path d="M12 7h4.5a2.5 2.5 0 0 0 0-5C13 2 12 7 12 7z"/>',
  users: '<path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M23 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/>',
  rewind: '<polyline points="1 4 1 10 7 10"/><path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10"/>',
  dot: '<circle cx="12" cy="12" r="9"/>',
  corner: '<polyline points="15 14 20 9 15 4"/><path d="M4 20v-7a4 4 0 0 1 4-4h12"/>',
  download: '<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/>',
  lock: '<rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>',
  monitor: '<rect x="2" y="3" width="20" height="14" rx="2"/><line x1="8" y1="21" x2="16" y2="21"/><line x1="12" y1="17" x2="12" y2="21"/>',
  search: '<circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>',
} as const;

/** 文件扩展名 → 文件图标 SVG */
export function fileIcon(ext: string): string {
  const e = (ext || '').toLowerCase();
  if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'heic'].includes(e)) return iconSvg(ICON_PATHS.image);
  if (['mp3', 'wav', 'm4a', 'flac', 'aac', 'silk'].includes(e)) return iconSvg(ICON_PATHS.music);
  if (['mp4', 'mov', 'avi', 'mkv', 'm4v'].includes(e)) return iconSvg(ICON_PATHS.video);
  if (['zip', 'rar', '7z', 'tar', 'gz'].includes(e)) return iconSvg(ICON_PATHS.archive);
  if (['doc', 'docx', 'wps'].includes(e)) return iconSvg(ICON_PATHS.file);
  if (['xls', 'xlsx', 'csv'].includes(e)) return iconSvg(ICON_PATHS.sheet);
  if (['ppt', 'pptx'].includes(e)) return iconSvg(ICON_PATHS.present);
  if (['pdf'].includes(e)) return iconSvg(ICON_PATHS.file);
  if (['apk', 'exe', 'msi'].includes(e)) return iconSvg(ICON_PATHS.gear);
  return iconSvg(ICON_PATHS.file);
}

/** 收藏类型标签 → 图标 SVG */
export function favIcon(typeLabel: string): string {
  const map: Record<string, string> = {
    '文本': ICON_PATHS.file, '图片': ICON_PATHS.image, '语音': ICON_PATHS.mic, '视频': ICON_PATHS.video,
    '链接': ICON_PATHS.link, '位置': ICON_PATHS.pin, '文件': ICON_PATHS.file, '笔记': ICON_PATHS.note,
    '聊天记录': ICON_PATHS.chat,
  };
  return iconSvg(map[typeLabel] || ICON_PATHS.clip);
}

/** 转账/红包状态文案 → 微信卡片状态类 */
export function payStateClass(direction: string): string {
  const d = direction || '';
  if (d.includes('过期')) return 'wc-pay-overdue';
  if (d.includes('退还')) return 'wc-pay-returned';
  if (d.includes('已收') || d.includes('已领') || d.includes('接收')) return 'wc-pay-received';
  return '';
}

/** 转账状态文案（区分我发起 / 我收到），与后端 media::transfer_status_label 保持一致 */
export function transferStatusLabel(isSelf: boolean, paysubtype: string): string {
  const ps = String(paysubtype || '');
  if (ps === '3' || ps === '8') return isSelf ? '已被接收' : '已收款';
  if (ps === '4' || ps === '9') return '已退还';
  if (ps === '5' || ps === '10') return '已过期退回';
  if (ps === '7') return '待领取';
  if (ps === '1') return isSelf ? '等待对方领取' : '待收款';
  return '';
}

/** 红包 paysubtype → 状态文字 */
export function redPacketLabel(ps: string): string {
  if (ps === '3' || ps === '8') return '已领取';
  if (ps === '4') return '已退还';
  if (ps === '5') return '已过期退回';
  return '';
}

/** 聊天记录转发预览条目（name + 文本内容） */
export interface ChatlogPreviewItem {
  name?: string;
  text?: unknown;
}

/** 聊天记录转发预览行（最多 3 条） */
export function chatlogPreview(items: readonly ChatlogPreviewItem[] | null | undefined): string[] {
  return (items ?? []).slice(0, 3).map((it) => {
    const name = it.name || '';
    const text = String(it.text || '').replace(/\s+/g, ' ').trim();
    return name ? `${name}: ${text}` : text;
  });
}

/** 文件大小（字节）→ 可读文案 */
export function fmtFileSize(n: number | null | undefined): string {
  if (!n) return '';
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

/** 收藏文件大小（字节）→ 可读文案（保留原独立语义，勿与 fmtFileSize 直接合并） */
export function favFileSize(n: number): string {
  if (!n) return '';
  if (n >= 1024 * 1024) return (n / 1024 / 1024).toFixed(1) + ' MB';
  if (n >= 1024) return (n / 1024).toFixed(1) + ' KB';
  return n + ' B';
}

/** 任意单元格值 → 展示文本 */
export function cellText(v: unknown): string {
  if (v === null || v === undefined) return '';
  if (typeof v === 'object') return JSON.stringify(v);
  return String(v);
}

/** 智能单元格文本：时间戳列格式化为日期时间 */
export function cellTextSmart(v: unknown, colName: string): string {
  if (v === null || v === undefined) return '';
  if (typeof v === 'object') return JSON.stringify(v);
  const name = (colName || '').toLowerCase();
  if ((name.includes('time') || name.includes('date')) && typeof v === 'number') {
    const d = v > 100000000000 ? new Date(v) : v > 1000000000 ? new Date(v * 1000) : null;
    if (d && !Number.isNaN(d.getTime())) {
      const p = (n: number) => String(n).padStart(2, '0');
      return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
    }
  }
  return String(v);
}
