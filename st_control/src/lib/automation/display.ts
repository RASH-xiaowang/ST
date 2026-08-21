/* ============================================================
 * 自动化 — 推送消息/任务展示纯函数
 * 自 AutomationPanel.svelte 下沉：消息类型分类、状态徽章、类型标签。
 * 不依赖组件状态，可独立单测。
 * ============================================================ */

/** 推送消息类型（不含 'all' 过滤项，后者是面板 UI 概念） */
export type MessageKind = 'text' | 'image' | 'video' | 'file';

/** 推送消息中参与分类的字段 */
export interface MessageLike {
  media_type?: string | null;
  msg_type?: number | null;
}

/** 实时推送消息（AutomationPanel 面板展示结构） */
export interface LiveMessage extends MessageLike {
  automationHit?: boolean;
  sender_username?: string;
  username?: string;
  content?: string;
  time?: string;
  timestamp?: number;
  ruleName?: string;
  [key: string]: unknown;
}

/** 消息类型标签 */
export const MESSAGE_KIND_LABELS: Record<MessageKind, string> = {
  text: '文本', image: '图片', video: '视频', file: '文件',
};

/** 任务状态元信息（label + Tailwind 类） */
export const STATUS_META: Record<string, { label: string; cls: string }> = {
  pending: { label: '待处理', cls: 'bg-amber-500/15 text-amber-400 border-amber-500/30' },
  claimed: { label: '已派发', cls: 'bg-sky-500/15 text-sky-400 border-sky-500/30' },
  processing: { label: '处理中', cls: 'bg-violet-500/15 text-violet-400 border-violet-500/30' },
  to_reply: { label: '待回复', cls: 'bg-orange-500/15 text-orange-400 border-orange-500/30' },
  replied: { label: '已回复', cls: 'bg-emerald-500/15 text-emerald-400 border-emerald-500/30' },
  done: { label: '已完成', cls: 'bg-emerald-500/15 text-emerald-400 border-emerald-500/30' },
  ignored: { label: '已忽略', cls: 'bg-muted text-muted-foreground border-border' },
};

/** 分类推送消息：media_type 优先，其次 msg_type 数字（3 图片 / 43 视频，其余非 1 为文件） */
export function classifyMessageType(m: MessageLike | null | undefined): MessageKind {
  const mt = m?.media_type;
  if (mt) {
    if (mt === 'image') return 'image';
    if (mt === 'video') return 'video';
    return 'file';
  }
  const num = m?.msg_type;
  if (num === 3) return 'image';
  if (num === 43) return 'video';
  if (num && num !== 1) return 'file';
  return 'text';
}

/** 消息类型 → 徽章配色类 */
export function kindColor(kind: MessageKind): string {
  if (kind === 'image') return 'bg-violet-500/15 text-violet-400';
  if (kind === 'video') return 'bg-rose-500/15 text-rose-400';
  if (kind === 'file') return 'bg-amber-500/15 text-amber-400';
  return 'bg-cyan-500/15 text-cyan-400';
}

/** 消息类型 → 显示标签 */
export function kindLabel(kind: MessageKind): string {
  return MESSAGE_KIND_LABELS[kind] ?? '其他';
}

/** 任务状态徽章 HTML（未知状态回退灰底 + 原文） */
export function statusBadge(s: string): string {
  const m = STATUS_META[s] ?? { label: s, cls: 'bg-muted text-muted-foreground border-border' };
  return `<span class="inline-flex items-center rounded-full border px-2 py-0.5 text-xs ${m.cls}">${m.label}</span>`;
}

/** 媒体类型标签（空 → 文本） */
export function mediaLabel(m: string | null): string {
  if (!m) return '文本';
  return m;
}
