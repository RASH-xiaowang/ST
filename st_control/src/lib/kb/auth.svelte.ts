// 知识库登录态（前端）
// 通过 kb_login / kb_logout / kb_current_user 维护当前用户，
// 全局共享，供所有 KB IPC 调用注入真实 userId。
// 注意：此文件必须是 .svelte.ts 扩展名，因为使用了 $state rune（Svelte 5 要求 runes 只能出现在 .svelte / .svelte.js / .svelte.ts 中）。
import { invoke } from '@tauri-apps/api/core';
import type { CurrentUser } from './kbTypes';
import { lsGet, lsRemove, lsSet } from '../storage';

function load(): CurrentUser | null {
  try {
    const raw = lsGet('kb_current_user');
    return raw ? (JSON.parse(raw) as CurrentUser) : null;
  } catch {
    return null;
  }
}

function save(u: CurrentUser | null) {
  if (u) lsSet('kb_current_user', JSON.stringify(u));
  else lsRemove('kb_current_user');
}

export const kbUser = $state<{ user: CurrentUser | null }>({ user: load() });

export async function refreshKbUser() {
  try {
    const u = await invoke<CurrentUser | null>('kb_current_user');
    kbUser.user = u ?? null;
    save(kbUser.user);
  } catch {
    kbUser.user = null;
  }
  return kbUser.user;
}

