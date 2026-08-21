/* ============================================================
 * 微信数据管理模块 — 实时消息映射纯函数
 * 自 WeChatPanel.svelte 下沉：WeChatMessagePayload → WeChatMessage
 * 展示结构（方向判定、群聊发送者、通知类型、转账状态文案重算）。
 * ============================================================ */
import type { WeChatMessage, WeChatMessagePayload } from '../types';
import { formatDividerTime, transferStatusLabel } from './format';

/** 将实时推送的 WeChatMessage 映射为与 ChatMessage 一致的展示结构 */
export function toRealtimeMsg(m: WeChatMessagePayload, selfUsername = ''): WeChatMessage {
  const isGroup = !!m.is_group;
  const senderName = isGroup ? (m.sender || m.sender_username || '') : '';
  const ts = Math.floor((m.timestamp ?? 0) / 1_000_000);
  const msg = {
    local_id: m.local_id ?? 0,
    server_id: 0,
    sort_seq: m.sort_seq ?? 0,
    ts,
    time: m.time ?? formatDividerTime(ts),
    divider: formatDividerTime(ts),
    // 后端实时消息携带 is_send 方向字段（私聊/群聊均适用）；
    // 优先用 sender 精确比对（单聊摘要回退偶发 is_send 缺失/错判时，
    // 只要 sender_username 与本机 wxid 一致就一定是自己发的消息）；
    // 其次信任后端 is_send，都缺失时默认对方。
    is_self:
      !!selfUsername && !!m.sender_username && m.sender_username === selfUsername
        ? true
        : m.is_send !== undefined && m.is_send !== null
          ? !!m.is_send
          : false,
    type: m.msg_type ?? 1,
    type_label: '',
    text: m.content ?? '',
    sender_username: m.sender_username ?? '',
    sender_name: senderName,
    is_notice: (m.msg_type ?? 0) === 10000 || (m.msg_type ?? 0) === 10002,
    rich: (m.rich ? { ...m.rich } : null) as WeChatMessage['rich'],
    image_url: m.image_url ?? null,
  };
  // 实时推送的转账卡片统一按方向重算状态文案：
  // 后端实时路径的 direction 是方向无关的通用标签（如“待收款”），
  // 需要换成“我发出→等待对方领取/已被接收，我收到→待收款/已收款”。
  if (msg.rich?.type === 'transfer' && msg.rich?.paysubtype) {
    msg.rich.direction = transferStatusLabel(msg.is_self, String(msg.rich.paysubtype));
  }
  return msg;
}
