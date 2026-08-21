/* ============================================================
 * 大模型对话 — 上下文裁剪纯函数
 * 自 GlobalChatTab.svelte 下沉：发送前滑动窗口裁剪，
 * 界面/历史保留全量，仅请求体截断。
 * ============================================================ */
import type { ChatMessage } from './types';

/** 裁剪结果：保留的消息 + 是否发生过裁剪（供调用方注入上下文说明） */
export interface TrimResult {
  messages: ChatMessage[];
  trimmed: boolean;
}

/**
 * 长对话上下文管理：发送给模型前做滑动窗口裁剪。
 *
 * 策略（保证不脱离当前对话主题）：
 * - 始终保留第一条用户消息（对话主题锚点），裁剪从中间开始；
 * - 按完整「轮次」（user 消息 + 其后的 assistant 回复）整轮丢弃，
 *   绝不把一问一答拆散，避免模型看到孤立的回复；
 * - 预算：最多 maxMessages 条 / maxChars 字符；仍超限时保留最近 minKeep 条。
 */
export function trimContext(
  all: ChatMessage[],
  maxMessages = 40,
  maxChars = 120_000,
  minKeep = 6,
): TrimResult {
  if (all.length === 0) {
    return { messages: [], trimmed: false };
  }
  // 主题锚点：第一条用户消息始终保留（若历史以 assistant 开头则跳过）
  const first = all[0];
  const anchor: ChatMessage[] =
    first.role === 'user' ? [first] : [];

  // 找下一轮起点：从 from 起第一个 role==='user' 的位置（一轮 = user + 后续 assistant）
  const nextTurnStart = (msgs: ChatMessage[], from: number): number => {
    for (let i = from; i < msgs.length; i++) {
      if (msgs[i].role === 'user') return i;
    }
    return -1;
  };

  // 从头部整轮丢弃一轮：返回丢弃后的数组（至少保留 minKeep 条）
  const dropOneTurn = (msgs: ChatMessage[]): ChatMessage[] => {
    const cut = nextTurnStart(msgs, 1);
    const remove = cut > 0 ? cut : 1;
    if (msgs.length - remove < minKeep) return msgs;
    return msgs.slice(remove);
  };

  const totalChars = (msgs: ChatMessage[]) =>
    msgs.reduce(
      (s, m) => s + (typeof m.content === 'string' ? m.content.length : 0),
      0,
    );

  let msgs = all.slice(anchor.length);
  // 条数预算：整轮裁剪
  while (anchor.length + msgs.length > maxMessages) {
    const next = dropOneTurn(msgs);
    if (next.length === msgs.length) break; // 已到最小保留，无法再裁
    msgs = next;
  }
  // 字符预算：整轮裁剪
  while (
    msgs.length > minKeep &&
    totalChars(anchor) + totalChars(msgs) > maxChars
  ) {
    const next = dropOneTurn(msgs);
    if (next.length === msgs.length) break;
    msgs = next;
  }

  const kept = [...anchor, ...msgs];
  return { messages: kept, trimmed: kept.length < all.length };
}

/** 上下文被裁剪时的系统说明（注入请求首部，防止模型脱离当前对话主题） */
export const TRIMMED_CONTEXT_NOTE =
  '注意：以下是当前会话的延续，更早的对话内容因长度限制已被省略。' +
  '请基于保留的对话内容紧扣当前主题继续回答；' +
  '若用户提到被省略部分的内容，请说明"更早的对话已超出上下文窗口"而不是编造。';
