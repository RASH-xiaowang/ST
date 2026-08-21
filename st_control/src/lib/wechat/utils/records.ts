/* ============================================================
 * 微信数据管理模块 — 记录展示纯函数
 * 自 GeneralRecords.svelte 下沉：类型图标、转账/红包/直播状态映射。
 * ============================================================ */

/** 记录类型 → SVG path（未知回退 app） */
export const KIND_PATHS: Record<string, string> = {
  rewind: '<polyline points="1 4 1 10 7 10"/><path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10"/>',
  card: '<rect x="1" y="4" width="22" height="16" rx="2"/><line x1="1" y1="10" x2="23" y2="10"/>',
  gift: '<polyline points="20 12 20 22 4 22 4 12"/><rect x="2" y="7" width="20" height="5"/><line x1="12" y1="22" x2="12" y2="7"/><path d="M12 7H7.5a2.5 2.5 0 0 1 0-5C11 2 12 7 12 7z"/><path d="M12 7h4.5a2.5 2.5 0 0 0 0-5C13 2 12 7 12 7z"/>',
  film: '<rect x="2" y="2" width="20" height="20" rx="2.18"/><line x1="7" y1="2" x2="7" y2="22"/><line x1="17" y1="2" x2="17" y2="22"/><line x1="2" y1="12" x2="22" y2="12"/><line x1="2" y1="7" x2="7" y2="7"/><line x1="2" y1="17" x2="7" y2="17"/><line x1="17" y1="17" x2="22" y2="17"/><line x1="17" y1="7" x2="22" y2="7"/>',
  app: '<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M3 9h18M3 15h18M9 3v18"/>',
  users: '<path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M23 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/>',
};

/** 记录类型图标（SVG 字符串） */
export function kindIcon(kind: string, size = 18): string {
  return `<svg viewBox="0 0 24 24" width="${size}" height="${size}" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${KIND_PATHS[kind] || KIND_PATHS.app}</svg>`;
}

/** 转账子类型 → 中文标签 */
export function transferSubType(v: unknown): string {
  const map: Record<string, string> = {
    '1': '微信支付', '2': '群收款', '3': '转账', '4': '二维码收款',
    '5': '收款', '6': 'AA收款', '7': '面对面', '8': '公众号支付',
  };
  return map[String(v)] ?? `类型 ${v}`;
}

/** 红包状态 → 中文标签 */
export function hbStatus(v: unknown): string {
  const map: Record<string, string> = { '0': '未知', '1': '正常', '2': '已退回', '3': '已领完' };
  return map[String(v)] ?? `状态 ${v}`;
}

/** 直播状态 → 中文标签 */
export function liveStatus(v: unknown): string {
  const map: Record<string, string> = { '1': '直播中', '2': '已结束', '3': '预告' };
  return map[String(v)] ?? `状态 ${v}`;
}

/** 用户名截断（>32 字符 → 前 30 + …） */
export function shortUser(u: string | null | undefined): string {
  if (!u) return '—';
  return u.length > 32 ? `${u.slice(0, 30)}…` : u;
}
