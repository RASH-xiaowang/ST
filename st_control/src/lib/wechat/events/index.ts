// ============================================================
// 微信消息事件总线
// ============================================================
// 职责：
//   1. 统一接收 Tauri Event 与 WebSocket 双通道消息
//   2. 基于 ack_id 做去重（滑动窗口 5000 条）
//   3. 支持批量消息拆包，逐条回调业务层
//   4. 批量 ACK：累积 50ms 或 32 个 ack_id 后统一回传
//   5. WebSocket 连接健康检查与自动回退
// ============================================================

import { listen, type Event as TauriEvent, type UnlistenFn } from '@tauri-apps/api/event';
import type { WeChatMessagePayload, MonitorStatus } from '../types';
import { ackWechatMessage, getMonitorStatus, resyncWechatMessages } from '../services/ipc';

export interface WechatEventBusOptions {
  /** WebSocket 端口，0 表示不使用 WebSocket */
  wsPort?: number;
  /** 收到消息后调用 */
  onMessage: (payload: WeChatMessagePayload) => void;
  /** 状态变化后调用 */
  onStatus?: (status: MonitorStatus) => void;
  /** 看门狗检测到后端监控异常（死亡/假死）时回调，业务层应重启监控 */
  onNeedRestart?: () => void;
  /** 去重窗口大小，默认 5000 */
  dedupWindow?: number;
  /** 批量 ACK 窗口 ms，默认 50 */
  ackBatchMs?: number;
  /** 批量 ACK 最大数量，默认 32 */
  ackBatchSize?: number;
  /** 心跳超时 ms，超过该时长无任何消息/心跳则触发健康检查，默认 75000 */
  heartbeatTimeoutMs?: number;
}

export interface WechatEventBus {
  /** 启动事件总线；幂等，可重复调用 */
  start: () => Promise<void>;
  destroy: () => void;
  /** 手动触发 WebSocket 重连 */
  reconnectWs: () => Promise<void>;
  /** 按本地最大 ack_id 从后端补拉断线期间遗漏的消息 */
  resync: () => Promise<void>;
}

interface BatchEnvelope {
  batch: true;
  messages: WeChatMessagePayload[];
  ack_ids: string[];
}

export function createWechatEventBus(options: WechatEventBusOptions): WechatEventBus {
  const {
    wsPort: initialWsPort = 0,
    onMessage,
    onStatus,
    onNeedRestart,
    dedupWindow = 5000,
    ackBatchMs = 50,
    ackBatchSize = 32,
    heartbeatTimeoutMs = 75000,
  } = options;

  let wsPort = initialWsPort;
  let unlistenMessage: UnlistenFn | null = null;
  let unlistenStatus: UnlistenFn | null = null;
  let ws: WebSocket | null = null;
  let wsConnecting = false;
  let lastReconnectAt = 0;
  const MIN_RECONNECT_INTERVAL_MS = 3000;
  let healthTimer: ReturnType<typeof setInterval> | null = null;
  let watchdogTimer: ReturnType<typeof setInterval> | null = null;
  let ackTimer: ReturnType<typeof setTimeout> | null = null;
  let pendingAcks: string[] = [];
  let started = false;
  /** 已见的最大数值 ack_id（补推水位线） */
  let lastAckSeq = 0;
  /** 最近一次收到消息或状态心跳的时间戳 */
  let lastEventAt = Date.now();
  /** 防止看门狗并发触发多次重启 */
  let restartInFlight = false;

  // 去重窗口：使用 Set 存储最近 N 个 ack_id
  const seenAckIds = new Set<string>();
  const ackQueue: string[] = [];

  function recordAckId(ackId: string) {
    if (seenAckIds.has(ackId)) return;
    seenAckIds.add(ackId);
    ackQueue.push(ackId);
    while (ackQueue.length > dedupWindow) {
      const old = ackQueue.shift();
      if (old) seenAckIds.delete(old);
    }
    // 维护数值水位线（ack_id 由后端单调递增分配）
    const n = Number(ackId);
    if (Number.isFinite(n) && n > lastAckSeq) lastAckSeq = n;
  }

  function isDuplicate(ackId: string): boolean {
    return seenAckIds.has(ackId);
  }

  /** 重置去重窗口与水位线：后端监控重启后 ack_id 从 1 重新分配，
   *  不重置会把新会话消息误判为重复而丢弃 */
  function resetDedup() {
    seenAckIds.clear();
    ackQueue.length = 0;
    lastAckSeq = 0;
  }

  function sendAcksImmediately(ackIds: string[]) {
    if (ackIds.length === 0) return;
    // 优先使用 WebSocket 批量 ACK，减少 IPC 开销
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ ack_ids: ackIds }));
    } else {
      // 回退到单条 IPC ACK（保持兼容性）
      for (const ackId of ackIds) {
        // 收敛到类型化封装（Tauri 会把 camelCase 参数自动转成 Rust
        // snake_case，裸 invoke 也能工作；此处统一走服务层保证类型提示）
        ackWechatMessage(ackId).catch((err) => {
          console.warn('[wechat:bus] ACK 失败:', err);
        });
      }
    }
  }

  function flushAcks() {
    if (ackTimer) {
      clearTimeout(ackTimer);
      ackTimer = null;
    }
    if (pendingAcks.length === 0) return;
    const batch = pendingAcks.splice(0, pendingAcks.length);
    sendAcksImmediately(batch);
  }

  function scheduleAck(ackId: string) {
    if (pendingAcks.includes(ackId)) return;
    pendingAcks.push(ackId);
    if (pendingAcks.length >= ackBatchSize) {
      flushAcks();
    } else if (!ackTimer) {
      ackTimer = setTimeout(() => flushAcks(), ackBatchMs);
    }
  }

  function handleSingleMessage(payload: WeChatMessagePayload) {
    lastEventAt = Date.now();
    const ackId = payload.ack_id;
    if (!ackId || isDuplicate(ackId)) return;
    recordAckId(ackId);
    scheduleAck(ackId);
    onMessage(payload);
  }

  function handleRawText(text: string) {
    try {
      const envelope = JSON.parse(text) as WeChatMessagePayload | BatchEnvelope;
      // 批量消息
      if ('batch' in envelope && envelope.batch === true) {
        const batch = envelope as BatchEnvelope;
        for (let i = 0; i < batch.messages.length; i++) {
          const msg = batch.messages[i];
          // 优先使用 batch.ack_ids 中的 ack_id，缺失则回退到 msg.ack_id
          if (batch.ack_ids[i] && !msg.ack_id) {
            msg.ack_id = batch.ack_ids[i];
          }
          handleSingleMessage(msg);
        }
        return;
      }
      // 单条消息
      handleSingleMessage(envelope as WeChatMessagePayload);
    } catch (e) {
      console.warn('[wechat:bus] 消息解析失败:', e, text.slice(0, 200));
    }
  }

  async function connectWebSocket() {
    if (wsPort === 0 || ws?.readyState === WebSocket.OPEN || wsConnecting) return;
    const now = Date.now();
    if (now - lastReconnectAt < MIN_RECONNECT_INTERVAL_MS) return;
    lastReconnectAt = now;
    wsConnecting = true;
    try {
      const socket = new WebSocket(`ws://127.0.0.1:${wsPort}`);
      ws = socket;
      socket.onopen = () => {
        console.info('[wechat:bus] WebSocket 已连接');
        wsConnecting = false;
      };
      socket.onmessage = (ev) => {
        if (typeof ev.data === 'string') {
          handleRawText(ev.data);
        }
      };
      socket.onerror = (err) => {
        console.warn('[wechat:bus] WebSocket 错误:', err);
      };
      socket.onclose = () => {
        console.warn('[wechat:bus] WebSocket 已关闭');
        wsConnecting = false;
        if (ws === socket) ws = null;
      };
    } catch (e) {
      console.warn('[wechat:bus] WebSocket 连接失败:', e);
      wsConnecting = false;
      ws = null;
    }
  }

  function ensureWebSocket() {
    if (wsPort === 0) return;
    if (!ws || ws.readyState === WebSocket.CLOSED || ws.readyState === WebSocket.CLOSING) {
      connectWebSocket().catch((err) => {
        console.warn('[wechat:bus] WebSocket 重连失败:', err);
      });
    }
  }

  /** 从后端补拉断线/隐藏期间遗漏的消息（按 ack_id 水位线增量） */
  async function resync() {
    if (!started) return;
    try {
      // 收敛到类型化封装（参数命名与返回类型由服务层统一声明）
      const missed = await resyncWechatMessages(String(lastAckSeq));
      if (Array.isArray(missed) && missed.length > 0) {
        console.info(`[wechat:bus] 补拉 ${missed.length} 条遗漏消息`);
        for (const text of missed) handleRawText(text);
      }
    } catch (e) {
      console.warn('[wechat:bus] 补拉遗漏消息失败:', e);
    }
  }

  function handleStatus(status: MonitorStatus) {
    lastEventAt = Date.now();
    // 监控重启后 ack_id 序列归零，必须重置去重窗口避免误杀新消息
    if (status.status === 'started') {
      resetDedup();
    }
    // 背压告警：立即冲刷积压 ACK，并补拉可能被跳过的消息
    if (status.status === 'backpressure') {
      flushAcks();
      resync();
    }
    // 后端监控任务异常退出：交给业务层重启
    if (status.status === 'monitor_exited' || status.status === 'listener_error') {
      onNeedRestart?.();
    }
    if (status.ws_port && status.ws_port > 0 && status.ws_port !== wsPort) {
      wsPort = status.ws_port;
      // 端口变更时关闭旧连接并重建
      if (ws) {
        ws.close();
        ws = null;
      }
      connectWebSocket();
    }
    onStatus?.(status);
  }

  /** 页面从隐藏恢复可见：flush ACK、重连 WS、补拉遗漏消息 */
  function handleVisibilityChange() {
    if (document.visibilityState === 'visible' && started) {
      flushAcks();
      ensureWebSocket();
      resync();
    }
  }

  /** 健康看门狗：超过 heartbeatTimeoutMs 无任何消息/心跳时主动探活，
   *  后端报告已停止则触发重启回调 */
  async function watchdogTick() {
    if (!started || restartInFlight) return;
    if (Date.now() - lastEventAt < heartbeatTimeoutMs) return;
    try {
      const status = await getMonitorStatus();
      lastEventAt = Date.now();
      if (!status?.running) {
        restartInFlight = true;
        try {
          onNeedRestart?.();
        } finally {
          // 60s 内不重复触发，给重启留出时间
          setTimeout(() => { restartInFlight = false; }, 60000);
        }
      } else {
        // 后端存活但事件通道静默（如 listen 丢失）：补拉一次兜底
        resync();
      }
    } catch (e) {
      console.warn('[wechat:bus] 看门狗探活失败:', e);
    }
  }

  async function start() {
    if (started) return;
    started = true;

    unlistenMessage = await listen<string>('wechat-message', (event: TauriEvent<string>) => {
      handleRawText(event.payload);
    });
    unlistenStatus = await listen<MonitorStatus>('wechat-status', (event: TauriEvent<MonitorStatus>) => {
      handleStatus(event.payload);
    });

    if (wsPort > 0) {
      await connectWebSocket();
    }

    // 每 5s 健康检查：WebSocket 断开时自动重连
    healthTimer = setInterval(() => {
      ensureWebSocket();
    }, 5000);

    // 每 10s 看门狗：长时间无消息/心跳时探活并视情况重启
    watchdogTimer = setInterval(() => {
      watchdogTick();
    }, 10000);

    // 页面隐藏→恢复时补拉遗漏消息（隐藏期间定时器被节流，ACK/重连可能停滞）
    document.addEventListener('visibilitychange', handleVisibilityChange);

    // 启动后补拉一次：覆盖 listen 注册前已推送的消息
    resync();
  }

  return {
    start,
    resync,
    destroy: () => {
      started = false;
      unlistenMessage?.();
      unlistenStatus?.();
      if (healthTimer) clearInterval(healthTimer);
      if (watchdogTimer) clearInterval(watchdogTimer);
      document.removeEventListener('visibilitychange', handleVisibilityChange);
      flushAcks();
      if (ws) {
        ws.close();
        ws = null;
      }
    },
    reconnectWs: async () => {
      wsConnecting = false;
      if (ws) {
        ws.close();
        ws = null;
      }
      lastReconnectAt = 0;
      await connectWebSocket();
    },
  };
}
