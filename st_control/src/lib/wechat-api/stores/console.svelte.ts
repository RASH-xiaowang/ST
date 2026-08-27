/**
 * 微信 API 控制台 — 响应式状态管理
 * 使用 Svelte 5 runes 实现全局状态
 *
 * 通讯录缓存遵循 §8 版本化 schema：
 *   { version, savedAt, appId, friendsIds, chatroomIds, ghIds, details }
 *   - appId 不一致时拒绝恢复（硬规则）
 *   - 读取失败或 version 不匹配视为无缓存
 */

const STORAGE_KEY = 'wechat_api_console_v1';
const CONTACTS_CACHE_KEY = 'wechat_api_contacts_cache_v1';
const LOGIN_SNAPSHOT_KEY = 'wechat_console_login_snapshot_v1';
const WEBHOOK_URL_KEY = 'wechat_console_webhook_recv_url_v1';

import type { ConsoleState, TokenStatus, DetailContactInfo, ContactsCacheSchema } from '../types';

// ═══════════════════════════════════════════════════════════
// 主会话状态
// ═══════════════════════════════════════════════════════════
function loadState(): ConsoleState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return defaultState();
    const o = JSON.parse(raw);
    return {
      token: o.token ?? '',
      tokenStatus: o.tokenStatus ?? (o.token ? 'draft' : 'empty'),
      appId: o.appId ?? '',
      uuid: o.uuid ?? '',
      currentTargetWxid: o.currentTargetWxid ?? '',
      currentTargetDisplayName: o.currentTargetDisplayName ?? '',
      loginNickName: o.loginNickName ?? '',
    };
  } catch {
    return defaultState();
  }
}

function defaultState(): ConsoleState {
  return {
    token: '',
    tokenStatus: 'empty',
    appId: '',
    uuid: '',
    currentTargetWxid: '',
    currentTargetDisplayName: '',
    loginNickName: '',
  };
}

// ---------- 响应式状态 ----------
export const consoleState = $state(loadState());

function saveState() {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(consoleState));
  } catch {}
}

// ═══════════════════════════════════════════════════════════
// Token 操作
// ═══════════════════════════════════════════════════════════
export function setTokenDraft(value: string) {
  consoleState.token = String(value || '');
  const trimmed = consoleState.token.trim();
  if (!trimmed) {
    consoleState.tokenStatus = 'empty';
  } else if (consoleState.tokenStatus === 'valid_locked') {
    consoleState.tokenStatus = 'draft';
  } else if (consoleState.tokenStatus !== 'checking') {
    consoleState.tokenStatus = 'draft';
  }
  saveState();
}

export function setTokenStatus(status: TokenStatus) {
  consoleState.tokenStatus = status;
  saveState();
}

// ═══════════════════════════════════════════════════════════
// 登录信息
// ═══════════════════════════════════════════════════════════
export function setLoginInfo(appId: string, uuid: string, nickName: string) {
  consoleState.appId = appId;
  consoleState.uuid = uuid;
  consoleState.loginNickName = nickName;
  saveState();
}

export function clearLoginInfo() {
  consoleState.appId = '';
  consoleState.uuid = '';
  consoleState.loginNickName = '';
  saveState();
}

// ═══════════════════════════════════════════════════════════
// 锁定目标（§7）
// ═══════════════════════════════════════════════════════════
export function setTargetWxid(wxid: string, displayName: string) {
  consoleState.currentTargetWxid = wxid;
  consoleState.currentTargetDisplayName = displayName;
  saveState();
}

// ═══════════════════════════════════════════════════════════
// 展示名解析（§4）
// 优先级：remark > nickName > userName > id 本身
// 详情映射键须同时支持原始值与小写键
// ═══════════════════════════════════════════════════════════

/** §4 从详情行解析展示名 */
export function pickDisplayName(row: DetailContactInfo | undefined | null, idFallback: string): string {
  if (row && typeof row === 'object') {
    const remark = row.remark != null ? String(row.remark).trim() : '';
    if (remark) return remark;
    const nick = row.nickName != null ? String(row.nickName).trim() : '';
    if (nick) return nick;
    const un = row.userName != null ? String(row.userName).trim() : '';
    if (un) return un;
  }
  return String(idFallback || '').trim();
}

/** §3 标识归一：去首尾空白，全角 @ → 半角 @ */
export function normalizeWxid(id: string): string {
  return String(id || '').trim().replace(/\uFF20/g, '@');
}

/** §3 群聊判定：小写以 @chatroom 结尾 */
export function isChatroomId(id: string): boolean {
  return normalizeWxid(id).toLowerCase().endsWith('@chatroom');
}

/** §3 公众号判定：小写以 gh 开头 */
export function isGhId(id: string): boolean {
  return normalizeWxid(id).toLowerCase().startsWith('gh');
}

// ═══════════════════════════════════════════════════════════
// 通讯录缓存（§8 版本化 schema）
// 分键存储，不与 API 令牌桶混用
// ═══════════════════════════════════════════════════════════

/**
 * §8 写入通讯录缓存
 * @param friendsIds 归一后的好友 id 列表
 * @param chatroomIds 归一后的群聊 id 列表
 * @param ghIds 归一后的公众号 id 列表
 * @param details 详情映射导出的去重行对象数组
 */
export function saveContactsCache(
  friendsIds: string[],
  chatroomIds: string[],
  ghIds: string[],
  details: DetailContactInfo[],
) {
  // §8 按 userName 小写去重
  const seen = new Set<string>();
  const dedupedDetails: DetailContactInfo[] = [];
  for (const row of details) {
    if (!row) continue;
    const key = String(row.userName || '').toLowerCase().trim();
    if (!key || seen.has(key)) continue;
    seen.add(key);
    dedupedDetails.push(row);
  }

  const cache: ContactsCacheSchema = {
    version: 1,
    savedAt: Date.now(),
    appId: consoleState.appId,
    friendsIds,
    chatroomIds,
    ghIds,
    details: dedupedDetails,
  };

  try {
    localStorage.setItem(CONTACTS_CACHE_KEY, JSON.stringify(cache));
  } catch (e) {
    // §C.4 缓存写入失败不得静默吞掉
    console.warn('[contacts-cache] 写入失败:', e);
  }
}

/**
 * §8 恢复通讯录缓存
 * - version 不匹配 → 视为无缓存
 * - appId 不一致 → 拒绝恢复（硬规则）
 * @returns 缓存数据或 null
 */
export function loadContactsCache(): ContactsCacheSchema | null {
  try {
    const raw = localStorage.getItem(CONTACTS_CACHE_KEY);
    if (!raw) return null;
    const o = JSON.parse(raw) as ContactsCacheSchema;

    // §8 读取失败或 version 不匹配 → 视为无缓存
    if (!o || o.version !== 1) return null;

    // §8 appId 不一致拒绝恢复（硬规则）
    // 两者均非空且不相等时拒绝
    const currentAppId = (consoleState.appId || '').trim();
    const cacheAppId = (o.appId || '').trim();
    if (currentAppId && cacheAppId && currentAppId !== cacheAppId) {
      console.warn('[contacts-cache] appId 不匹配，拒绝恢复');
      return null;
    }

    return o;
  } catch {
    return null;
  }
}

/**
 * §11 展示名反查：仅 wxid → 从缓存 details 中解析展示名
 * 供消息发送目标 Toast 等使用
 */
export function lookupContactDisplayName(wxid: string): string {
  const w = normalizeWxid(wxid);
  if (!w) return '';
  const cache = loadContactsCache();
  if (!cache) return '';
  const wl = w.toLowerCase();

  // §4 详情映射键须同时支持原始值与小写键
  for (const row of cache.details) {
    if (!row) continue;
    const un = String(row.userName || '');
    if (un === w || un.toLowerCase() === wl) {
      return pickDisplayName(row, w);
    }
  }
  return '';
}

/** 清除通讯录缓存 */
export function clearContactsCache() {
  try {
    localStorage.removeItem(CONTACTS_CACHE_KEY);
  } catch {}
}

// ═══════════════════════════════════════════════════════════
// 登录快照
// ═══════════════════════════════════════════════════════════
export function saveLoginSnapshot(data: Record<string, unknown>) {
  try {
    localStorage.setItem(LOGIN_SNAPSHOT_KEY, JSON.stringify(data));
  } catch {}
}

export function loadLoginSnapshot(): Record<string, unknown> | null {
  try {
    const raw = localStorage.getItem(LOGIN_SNAPSHOT_KEY);
    return raw ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}

// ═══════════════════════════════════════════════════════════
// Webhook URL
// ═══════════════════════════════════════════════════════════
export function saveWebhookUrl(url: string) {
  try {
    localStorage.setItem(WEBHOOK_URL_KEY, url);
  } catch {}
}

export function loadWebhookUrl(): string {
  try {
    return localStorage.getItem(WEBHOOK_URL_KEY) || '';
  } catch {
    return '';
  }
}

// ═══════════════════════════════════════════════════════════
// 清理全部（§5）
// ═══════════════════════════════════════════════════════════
export function purgeAll() {
  try {
    localStorage.removeItem(STORAGE_KEY);
    localStorage.removeItem(CONTACTS_CACHE_KEY);
    localStorage.removeItem(LOGIN_SNAPSHOT_KEY);
    localStorage.removeItem(WEBHOOK_URL_KEY);
    localStorage.removeItem('wechat-console:profile-snapshot');
  } catch {}
  Object.assign(consoleState, defaultState());
}
