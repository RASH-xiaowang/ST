/**
 * st_control 服务器管理模块
 * 
 * 通过 Tauri IPC 与 Rust 后端通信，监控 WebSocket 服务器状态
 * 服务器自动启动，前端仅做状态展示
 */

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { writable, type Writable } from 'svelte/store';
import type { ServerStateData, AgentInfo } from './types';

/** 服务器状态 store */
export const serverStatus: Writable<ServerStateData> = writable({
  status: 'stopped',
  port: 9786,
  agentCount: 0,
  messageCount: 0,
});

/** 已连接 Agent 列表 */
export const agents: Writable<AgentInfo[]> = writable([]);

/** 事件记录 */
export const eventLog: Writable<Array<{ time: string; event: string; detail: string }>> = writable([]);

/** 服务器是否已就绪 */
let eventListenersInitialized = false;

/**
 * 初始化事件监听（接收来自 Rust 后端的服务器事件）
 */
export async function initEventListeners(): Promise<void> {
  if (eventListenersInitialized) return;
  eventListenersInitialized = true;

  // 监听服务器事件
  await listen<string>('server-event', (event) => {
    try {
      const data = JSON.parse(event.payload);
      if (!data || typeof data !== 'object') return;
      const eventType: string = typeof data.event === 'string' ? data.event : 'unknown';

      if (eventType === 'agent_connected') {
        const c = data.client;
        if (!c || typeof c !== 'object' || typeof c.id !== 'string' || typeof c.name !== 'string') return;
        const client: AgentInfo = {
          id: String(c.id).slice(0, 64),
          name: String(c.name).slice(0, 128),
          connectedAt: typeof c.connectedAt === 'string' ? c.connectedAt : '',
          lastHeartbeat: typeof c.lastHeartbeat === 'string' ? c.lastHeartbeat : '',
          remoteAddr: typeof c.remoteAddr === 'string' ? c.remoteAddr : '',
        };
        agents.update((list) => {
          if (!list.find((a) => a.id === client.id)) {
            return [...list, client];
          }
          return list;
        });
        addEventLog('agent_connected', `Agent 已连接: ${client.name}`);
      } else if (eventType === 'agent_disconnected') {
        const clientId = typeof data.client_id === 'string' ? data.client_id : '';
        if (!clientId) return;
        agents.update((list) => list.filter((a) => a.id !== clientId));
        addEventLog('agent_disconnected', `Agent 已断开: ${clientId.slice(0, 8)}...`);
      } else if (eventType === 'agent_name_updated') {
        const clientId = typeof data.client_id === 'string' ? data.client_id : '';
        const newName = typeof data.name === 'string' ? data.name : '';
        if (!clientId || !newName) return;
        agents.update((list) => list.map((a) => a.id === clientId ? { ...a, name: newName } : a));
        addEventLog('agent_name_updated', `Agent 名称更新: ${newName}`);
      } else if (eventType === 'message_received') {
        // 更新消息计数
        serverStatus.update((s) => ({ ...s, messageCount: s.messageCount + 1 }));
      }
    } catch {
      addEventLog('event', event.payload);
    }
  });

  // 首次拉取状态
  await refreshServerStatus();
}

/**
 * 刷新服务器状态
 */
export async function refreshServerStatus(): Promise<void> {
  try {
    const state = await invoke<ServerStateData>('get_server_status');
    serverStatus.set(state);
  } catch (err) {
    console.error('获取服务器状态失败:', err);
  }
}

function addEventLog(event: string, detail: string) {
  eventLog.update((list) =>
    [{ time: new Date().toLocaleTimeString(), event, detail }, ...list].slice(0, 100)
  );
}
