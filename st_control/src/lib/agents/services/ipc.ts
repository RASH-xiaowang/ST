// 智能体 — Tauri IPC 封装层
// 组件层统一通过本模块调用后端，避免直接 invoke。
import { invoke, Channel } from '@tauri-apps/api/core';
import type { AgentInput } from '../agentForm';

export interface AgentItem {
  id: number;
  name: string;
  description: string;
  roleId: string;
  providerId: string;
  model: string;
  kbId: number | null;
  temperature: number;
  maxTokens: number;
  topP: number;
  createdAt: string;
  updatedAt: string;
}

export const agentApi = {
  list: () => invoke<AgentItem[]>('agent_list'),
  create: (input: AgentInput) => invoke<number>('agent_create', { input }),
  update: (id: number, input: AgentInput) => invoke<void>('agent_update', { id, input }),
  remove: (id: number) => invoke<void>('agent_delete', { id }),
  sendCommand: (agentId: string | number, method: string, payload: unknown) =>
    invoke<string>('send_command_to_agent', { args: { agentId, method, payload } }),
  chatStream: (agentId: number, query: string, onChunk: (frame: string) => void): Promise<void> => {
    const channel = new Channel<string>();
    channel.onmessage = (m: string) => onChunk(m);
    return invoke<void>('agent_chat_stream', { input: { agentId, query }, onChunk: channel });
  },
};
