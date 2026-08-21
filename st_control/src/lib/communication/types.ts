/** 协议消息 */
export interface ProtocolMessage {
  type: 'command' | 'response' | 'event' | 'heartbeat' | 'error';
  id: string;
  timestamp: number;
  source: string;
  target: string;
  method?: string;
  payload?: unknown;
  correlationId?: string;
  error?: { code: string; message: string };
}

/** 已连接的 Agent 信息 */
export interface AgentInfo {
  id: string;
  name: string;
  connectedAt: string;
  lastHeartbeat: string;
  remoteAddr: string;
}

/** 服务器状态 */
export interface ServerStateData {
  status: 'stopped' | 'starting' | 'running' | 'stopping' | 'error';
  port: number;
  agentCount: number;
  messageCount: number;
}
