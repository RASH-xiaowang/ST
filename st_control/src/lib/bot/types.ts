// 消息通道（ClawBot / iLink）类型定义

export type BotStatus = 'connecting' | 'online' | 'expiring' | 'expired' | 'error' | 'disabled';
// 消息通道平台：微信（ClawBot iLink）+ QQ 官方机器人（J-23 起
// 企业微信 / 钉钉 / OneBot 已移除，专注维护这两条通道）
export type BotPlatform = 'wechat' | 'qqbot';

export interface BotAccount {
  id: number;
  botId: string;
  name: string;
  ownerId: string;
  platform: BotPlatform;
  targetId: string;
  configJson: string;
  baseUrl: string;
  cdnBaseUrl: string;
  status: BotStatus;
  connectedAt: string | null;
  expiresAt: string | null;
  lastActiveAt: string | null;
  lastError: string;
  createdAt: string;
}

export interface BotLog {
  id: number;
  accountId: number;
  direction: 'in' | 'out';
  msgType: string;
  peer: string;
  content: string;
  localPath: string;
  status: string;
  error: string;
  createdAt: string;
}

export interface AccountContact {
  peer: string;
  lastText: string;
  lastTs: number;
}

/** QQ 官方机器人：网关自动收集到的 openid 目标（用户 / 群） */
export interface QqbotContact {
  id: number;
  kind: 'private' | 'group';
  openid: string;
  display: string;
  lastContent: string;
  lastSeenAt: string;
}

export interface BotStatusSummary {
  total: number;
  online: number;
  expired: number;
  error: number;
}

export interface QrView {
  sessionId: string;
  imageDataUrl: string;
  rawUrl: string;
}

export const STATUS_META: Record<BotStatus, { label: string; cls: string; dot: string }> = {
  connecting: { label: '连接中', cls: 'bg-sky-500/15 text-sky-400 border-sky-500/30', dot: 'bg-sky-400' },
  online: { label: '在线', cls: 'bg-emerald-500/15 text-emerald-400 border-emerald-500/30', dot: 'bg-emerald-400' },
  expiring: { label: '即将过期', cls: 'bg-amber-500/15 text-amber-400 border-amber-500/30', dot: 'bg-amber-400' },
  expired: { label: '已过期', cls: 'bg-rose-500/15 text-rose-400 border-rose-500/30', dot: 'bg-rose-400' },
  error: { label: '异常', cls: 'bg-rose-500/15 text-rose-400 border-rose-500/30', dot: 'bg-rose-400' },
  disabled: { label: '未绑定', cls: 'bg-muted text-muted-foreground border-border', dot: 'bg-muted-foreground' },
};

export const PLATFORM_META: Record<
  BotPlatform,
  { label: string; short: string; desc: string; badge: string }
> = {
  wechat: {
    label: '微信',
    short: '微信',
    desc: 'ClawBot 官方 iLink，扫码绑定，双向收发，长期有效',
    badge: 'bg-emerald-500/15 text-emerald-500 border-emerald-500/30',
  },
  qqbot: {
    label: 'QQ官方',
    short: 'QQ官方',
    desc: 'QQ 官方机器人（AppID + Secret → 官方开放平台 API），主动消息需 24 小时互动窗口',
    badge: 'bg-cyan-500/15 text-cyan-500 border-cyan-500/30',
  },
};

export const MSG_TYPE_LABELS: Record<string, string> = {
  text: '文本', image: '图片', voice: '语音', file: '文件', video: '视频', media: '媒体',
};

/** 解析 "YYYY-MM-DD HH:MM:SS" → Date */
export function parseDbTime(s: string | null): Date | null {
  if (!s) return null;
  const m = /^(\d{4})-(\d{2})-(\d{2})[ T](\d{2}):(\d{2}):(\d{2})/.exec(s);
  if (!m) return null;
  return new Date(+m[1], +m[2] - 1, +m[3], +m[4], +m[5], +m[6]);
}

/** 剩余时间文案（无过期时间 = 长期有效，凭服务端会话状态自动维持） */
export function countdown(expiresAt: string | null): { text: string; urgent: boolean } {
  const end = parseDbTime(expiresAt);
  if (!end) return { text: '长期有效', urgent: false };
  const ms = end.getTime() - Date.now();
  if (ms <= 0) return { text: '已过期', urgent: true };
  const h = Math.floor(ms / 3_600_000);
  const m = Math.floor((ms % 3_600_000) / 60_000);
  if (h > 0) return { text: `${h} 小时 ${m} 分`, urgent: h < 12 };
  return { text: `${m} 分钟`, urgent: true };
}
