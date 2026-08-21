/* ============================================================
 * 全局搜索 — 共享类型
 * ============================================================ */

/** 通讯录命中（GlobalSearch 实际消费的字段） */
export interface ContactHit {
  username: string;
  display_name?: string;
  nick_name?: string;
  remark?: string;
  alias?: string;
  description?: string;
  [key: string]: unknown;
}

/** 平台事件（query_events 返回条目） */
export interface SearchEvent {
  event_type?: string;
  source?: string;
  title?: string;
  detail?: string;
  level?: string;
  time?: string;
  [key: string]: unknown;
}

/** 微信消息搜索命中 */
export interface WechatSearchHit {
  name?: string;
  username: string;
  time?: string;
  text?: string;
  local_id: number;
  snippet?: string;
  ts?: number;
  create_time?: number;
  [key: string]: unknown;
}

/** 微信消息搜索结果 */
export interface WechatSearchResult {
  hits: WechatSearchHit[];
  indexed?: boolean;
  [key: string]: unknown;
}
