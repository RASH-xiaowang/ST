/* ============================================================
 * 微信数据管理模块 — 会话类型识别纯函数
 * 自 HookManager.svelte 下沉：群聊/公众号/单聊分类。
 * 不依赖组件状态，可独立单测。
 * ============================================================ */

/** 会话类型（'all' 为过滤 UI 概念） */
export type SessionKind = 'all' | 'group' | 'private' | 'official';

/** 群聊识别：@chatroom / @im.chatroom 后缀 */
export function isGroup(u: string): boolean {
  return u.endsWith('@chatroom') || u.includes('@im.chatroom');
}

/** 公众号识别：gh_ 前缀 / @gh 后缀 */
export function isOfficial(u: string): boolean {
  return u.startsWith('gh_') || u.endsWith('@gh');
}

/** 会话分类：群聊 → 公众号 → 单聊 */
export function kindOf(u: string): SessionKind {
  if (isGroup(u)) return 'group';
  if (isOfficial(u)) return 'official';
  return 'private';
}
