/* ============================================================
 * 微信数据管理模块 — 消息虚拟滚动纯计算
 * 自 WeChatPanel.svelte 下沉：估算高度、前缀和、二分定位、可见条数。
 * 不依赖组件状态，可独立单测。
 * ============================================================ */

/** 文本行高 */
export const MSG_LINE_H = 20;
/** 14px 中文字号在气泡内容宽内每行约 22 字 */
export const MSG_CHARS_PER_LINE = 22;
/** 文本消息基础高（留白 + 首行） */
export const MSG_TEXT_BASE = 38;
/** 图片消息 */
export const MSG_IMG_H = 240;
/** 系统/撤回消息 */
export const MSG_NOTICE_H = 30;
/** 消息间距（.wc-msg margin-bottom） */
export const MSG_ITEM_GAP = 14;
import type { WeChatMessage } from '../types';

/** 普通消息最小高 */
export const MSG_MIN_H = 54;

/** 估算单条消息高度（px）：通知/图片/富媒体/文本各自分支 */
export function estimateMsgHeight(m: WeChatMessage): number {
  if (m.is_notice) return MSG_NOTICE_H + 20;
  if (m.type === 3) return MSG_IMG_H + MSG_ITEM_GAP;
  if (m.rich) {
    const base = (() => {
      switch (m.rich.type) {
        case 'newsfeed': return Math.min(320, 120 + (m.rich.items?.length ?? 1) * 54);
        case 'file': return 84;
        case 'link': {
          const hasCover = !!(m.rich?.thumb && /mp\.weixin\.qq\.com/i.test(m.rich?.url || ''));
          return (hasCover ? 224 : 96) + ((m.rich?.articles?.length ?? 0) * 64);
        }
        case 'miniapp': case 'channels': case 'chatlog': case 'transfer': return 96;
        case 'quote': return 80;
        case 'emoji': return 120;
        case 'voice': case 'video': return 50;
        default: return 70;
      }
    })();
    return base + MSG_ITEM_GAP;
  }
  const text = m.text || '';
  const lines = Math.max(1, Math.ceil(text.length / MSG_CHARS_PER_LINE));
  let h = MSG_TEXT_BASE + lines * MSG_LINE_H;
  // 群聊发送者名行
  if (m.sender_name) h += 16;
  return Math.max(MSG_MIN_H, h + MSG_ITEM_GAP);
}

/** 前缀和：p[i] = 前 i 条总高；返回 [prefix, total] */
export function computePrefixSums(heights: number[]): { prefix: number[]; total: number } {
  const prefix = new Array<number>(heights.length);
  let acc = 0;
  for (let i = 0; i < heights.length; i++) { prefix[i] = acc; acc += heights[i]; }
  return { prefix, total: acc };
}

/** 二分查找：第一个前缀和 > target 的索引（target px 落在哪条消息上） */
export function upperBoundPrefix(prefix: number[], target: number): number {
  let lo = 0, hi = prefix.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (prefix[mid] <= target) lo = mid + 1; else hi = mid;
  }
  return lo;
}

/** 可视窗口应渲染的条数（按平均估算高度推算） */
export function estimateVisibleCount(count: number, totalEst: number, viewH: number): number {
  if (!count) return 8;
  const avg = Math.max(24, totalEst / count);
  return Math.max(8, Math.ceil(viewH / avg));
}

/** 可视窗口上下各多渲染的条数（windowed rendering 缓冲） */
export const MSG_VIRTUAL_BUFFER = 24;

/** 虚拟滚动可视窗口：start/end 为消息索引区间，topPad/bottomPad 为占位高度（px） */
export interface MsgVisRange {
  start: number;
  end: number;
  topPad: number;
  bottomPad: number;
}

/**
 * 计算可视窗口（窗口化渲染）：贴底模式覆盖到末尾；否则按滚动位置二分定位。
 * 与原 WeChatPanel visRange 派生逻辑逐项等价。
 */
export function computeVisRange(
  count: number,
  totalEst: number,
  viewH: number,
  prefix: number[],
  scrollTop: number,
  stickToBottom: boolean,
  buffer = MSG_VIRTUAL_BUFFER,
): MsgVisRange {
  if (!count) return { start: 0, end: 0, topPad: 0, bottomPad: 0 };
  const vis = estimateVisibleCount(count, totalEst, viewH);
  if (stickToBottom) {
    // 贴底模式：窗口覆盖到最后一条，保证最新消息渲染在底部、吸底精确
    const start = Math.max(0, count - vis - buffer * 2);
    return { start, end: count, topPad: prefix[start] ?? 0, bottomPad: 0 };
  }
  const idx = upperBoundPrefix(prefix, scrollTop);
  const start = Math.max(0, idx - buffer);
  const end = Math.min(count, idx + vis + buffer);
  return {
    start,
    end,
    topPad: prefix[start] ?? 0,
    bottomPad: totalEst - (prefix[end] ?? totalEst),
  };
}

/** 裁剪消息窗口（内存上限）：返回裁剪后的数组与移除高度（px） */
export function trimMessageWindow(
  messages: WeChatMessage[],
  estH: number[],
  maxKeep: number,
): { messages: WeChatMessage[]; estH: number[]; removedH: number } {
  if (messages.length <= maxKeep) return { messages, estH, removedH: 0 };
  const keep = messages.length - maxKeep;
  let removedH = 0;
  for (let i = 0; i < keep; i++) removedH += estH[i];
  return { messages: messages.slice(keep), estH: estH.slice(keep), removedH };
}

/** 两条消息是否需时间分隔条（时间间隔 > thresholdMs；无前一条视为需分隔） */
export function shouldShowDivider(
  prev: WeChatMessage | undefined,
  cur: WeChatMessage | undefined,
  thresholdMs = 300,
): boolean {
  if (!cur) return false;
  if (!prev) return true;
  return Math.abs((cur.ts ?? 0) - (prev.ts ?? 0)) > thresholdMs;
}
