/**
 * AI 角色定位 —— 前端 ↔ 后端 IPC 桥接。
 * 封装 role_store.rs 暴露的 Tauri 命令，供「AI 角色定位」界面直接调用，
 * 同时挂载到 window.__roleApi 作为跨模块/控制台调试的统一入口。
 */
import { invoke } from '@tauri-apps/api/core';
import type { AiRole } from '../components/roleTypes';

export interface RoleApi {
  /** 列出全部 AI 角色 */
  list(): Promise<AiRole[]>;
  /** 获取单个角色详情 */
  get(id: string): Promise<AiRole | null>;
  /** 新增角色 */
  create(role: AiRole): Promise<AiRole>;
  /** 新增或更新角色（按 id upsert） */
  update(role: AiRole): Promise<AiRole>;
  /** 删除角色 */
  remove(id: string): Promise<boolean>;
}

export const roleApi: RoleApi = {
  list: () => invoke<AiRole[]>('role_list'),
  get: (id: string) => invoke<AiRole | null>('role_get', { id }),
  create: (role: AiRole) => invoke<AiRole>('role_save', { role }),
  update: (role: AiRole) => invoke<AiRole>('role_save', { role }),
  remove: (id: string) => invoke<boolean>('role_delete', { id }),
};

declare global {
  interface Window {
    __roleApi: RoleApi;
  }
}

// 挂到全局，便于控制台调试与跨模块调用
if (typeof window !== 'undefined') {
  window.__roleApi = roleApi;
}
