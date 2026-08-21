/**
 * ST Agent WebSocket 客户端
 *
 * 连接策略：
 * - 启动时自动连接 st_control
 * - 每 5s 发送应用层 heartbeat
 * - 服务端每 10s 发送 WebSocket PING → 浏览器自动 PONG（更新 last_any_activity）
 * - 健康检测：仅监控，不主动触发重连（让 onclose 统一处理）
 * - 断线后指数退避重连
 */

import { writable, type Writable } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';
import type { ProtocolMessage, ConnectionState } from './types';

export const connectionState: Writable<ConnectionState> = writable('disconnected');
export const lastMessage: Writable<ProtocolMessage | null> = writable(null);
export const messageHistory: Writable<ProtocolMessage[]> = writable([]);

// ============================================================
const HOST = '127.0.0.1';
const PORT = 9786;
const HB_SEND_MS = 5000;   // 每 5s 发心跳

/** Agent 主机名（由 App.svelte 加载后设置） */
let agentHostname = '';

/** 设置主机名（供 App.svelte 初始化时调用） */
export function setHostname(name: string) { agentHostname = name; }

let ws: WebSocket | null = null;
let hbTimer: ReturnType<typeof setInterval> | null = null;
let rcTimer: ReturnType<typeof setTimeout> | null = null;
let rcAttempts = 0;
let intentionalClose = false;
/** 是否正在重连过程中（防止 onclose 期间重复调度） */
let reconnecting = false;

function setState(s: ConnectionState) { connectionState.set(s); }

function notify(title: string, msg: string) {
  window.dispatchEvent(new CustomEvent('st-notification', { detail: { title, message: msg, timestamp: Date.now() } }));
}

// ---------- 重连 ----------
function scheduleReconnect() {
  if (intentionalClose || reconnecting) return;
  reconnecting = true;

  clearRcTimer();
  rcAttempts++;
  const delay = Math.min(1000 * Math.pow(2, rcAttempts - 1), 30000) + Math.random() * 1000;

  setState('reconnecting');

  rcTimer = setTimeout(() => {
    reconnecting = false;
    doConnect();
  }, delay);
}

function clearRcTimer() {
  if (rcTimer) { clearTimeout(rcTimer); rcTimer = null; }
}

function clearAll() {
  clearRcTimer(); stopHb(); reconnecting = false;
}

// ---------- 心跳 ----------
function startHb() {
  stopHb();
  hbTimer = setInterval(() => {
    if (ws?.readyState === WebSocket.OPEN) {
      try {
        ws.send(JSON.stringify({
          type: 'heartbeat', id: crypto.randomUUID(), timestamp: Date.now(),
          source: 'st_agent', target: 'st_control',
          payload: { time: new Date().toISOString(), status: 'alive', agentName: agentHostname },
        } as ProtocolMessage));
      } catch (_) { /* 静默失败，onclose 会处理 */ }
    }
  }, HB_SEND_MS);
}

function stopHb() { if (hbTimer) { clearInterval(hbTimer); hbTimer = null; } }

// ---------- 连接 ----------
function doConnect() {
  if (ws) {
    try { ws.close(1000); } catch {}
    ws = null;
  }
  clearAll();
  intentionalClose = false;

  setState('connecting');
  const url = `ws://${HOST}:${PORT}`;

  try {
    const sock = new WebSocket(url);
    // 先赋值给 ws，这样 onclose 能访问到
    ws = sock;

    sock.onopen = () => {
      console.log(`[Agent] 已连接 ${url}`);
      rcAttempts = 0; reconnecting = false;
      setState('connected');
      startHb();
      notify('连接成功', `已接入 Control (${HOST}:${PORT})`);

      // 发送握手命令通告 Agent 名称
      if (agentHostname) {
        sock.send(JSON.stringify({
          type: 'command', id: crypto.randomUUID(), timestamp: Date.now(),
          source: 'st_agent', target: 'st_control',
          method: 'agent.handshake',
          payload: { agentName: agentHostname },
        } as ProtocolMessage));
      }

      // 初次连接发送系统请求
      sock.send(JSON.stringify({
        type: 'command', id: crypto.randomUUID(), timestamp: Date.now(),
        source: 'st_agent', target: 'st_control', method: 'system.info',
      } as ProtocolMessage));
    };

    sock.onclose = (ev) => {
      console.log(`[Agent] 断开 code=${ev.code} reason="${ev.reason || ''}" rcAttempts=${rcAttempts}`);
      clearAll();
      setState('disconnected');

      // 1000=正常关闭（stop() 主动调用） 3001=保留（暂未用）
      if (!intentionalClose) {
        if (rcAttempts < 2) notify('连接断开', `code=${ev.code}`);
        scheduleReconnect();
      }
    };

    sock.onerror = () => {
      console.warn('[Agent] onerror（onclose 后续触发）');
    };

    sock.onmessage = (ev) => {
      try {
        const msg: ProtocolMessage = JSON.parse(ev.data);
        // 心跳回执忽略不记录
        if (msg.type === 'heartbeat') return;

        lastMessage.set(msg);
        messageHistory.update(list => [msg, ...list].slice(0, 200));

        if (msg.type === 'command') {
          // 自动回复确认
          try {
            sock.send(JSON.stringify({
              type: 'response', id: crypto.randomUUID(), timestamp: Date.now(),
              source: 'st_agent', target: msg.source,
              method: msg.method, correlationId: msg.id,
              payload: { status: 'received' },
            } as ProtocolMessage));
          } catch (_) {}
          // 保存任务到磁盘
          if (msg.method && msg.method !== 'agent.handshake' && msg.method !== 'system.info') {
            invoke('ipc_save_task', {
              taskId: msg.id,
              method: msg.method,
              payload: msg.payload || {},
            }).then((path) => {
              console.log(`[Agent] 任务已保存: ${path}`);
              // 刷新任务存储路径信息（更新文件数量）
              getTaskPath().then(info => {
                window.dispatchEvent(new CustomEvent('st-task-refresh', { detail: info }));
              }).catch(err => {
                console.error('[Agent] 刷新任务路径信息失败:', err);
              });
            }).catch((err) => {
              console.error('[Agent] 任务保存失败:', err);
            });
          }
          // 通知 UI
          window.dispatchEvent(new CustomEvent('st-notification', {
            detail: { title: '收到任务', message: `方法: ${msg.method || '-'}`, timestamp: Date.now() },
          }));
        }
      } catch (_) {}
    };
  } catch (err) {
    console.error('[Agent] new WebSocket 失败:', err);
    setState('error');
    if (!intentionalClose) scheduleReconnect();
  }
}

/** 对外：启动自动连接 */
export function start() { doConnect(); }

/** 对外：断开 */
export function stop() {
  intentionalClose = true; reconnecting = false;
  clearAll(); rcAttempts = 0;
  if (ws) { try { ws.close(1000, 'Agent 停止'); } catch {} ws = null; }
  setState('disconnected');
}

/** 对外：发送 */
export function send(msg: ProtocolMessage): boolean {
  if (ws?.readyState === WebSocket.OPEN) {
    try { ws.send(JSON.stringify(msg)); return true; } catch {}
  }
  return false;
}

// ============================================================
// 任务路径管理 IPC
// ============================================================

export interface TaskPathInfo {
  path: string;
  exists: boolean;
  is_dir: boolean;
  item_count: number;
}

/** 获取当前任务存储路径 */
export async function getTaskPath(): Promise<TaskPathInfo> {
  return invoke<TaskPathInfo>('ipc_get_task_path');
}

/** 设置任务存储路径（自动迁移数据） */
export async function setTaskPath(path: string): Promise<TaskPathInfo> {
  return invoke<TaskPathInfo>('ipc_set_task_path', { path });
}

/** 获取 Agent 主机名 */
export async function getHostname(): Promise<string> {
  return invoke<string>('ipc_get_hostname');
}
