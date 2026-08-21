/** 消息类型 */
export type MessageType = 'command' | 'response' | 'event' | 'heartbeat' | 'error';

/** 消息来源 */
export type AppSource = 'st_control' | 'st_agent';

/** 协议消息 */
export interface ProtocolMessage {
  type: MessageType;
  id: string;
  timestamp: number;
  source: AppSource;
  target: AppSource;
  method?: string;
  payload?: unknown;
  correlationId?: string;
  error?: { code: string; message: string };
}

/** 连接状态 */
export type ConnectionState =
  | 'disconnected'
  | 'connecting'
  | 'connected'
  | 'reconnecting'
  | 'error';
