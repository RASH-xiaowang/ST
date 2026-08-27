/**
 * 微信 API 可视化控制台 — 类型定义
 * 从 viewapi 项目迁入，适配 Svelte 5 + TypeScript
 */

/** API 统一响应结构 */
export interface ApiResponse<T = unknown> {
  ok: boolean;
  status: number;
  data: {
    ret: number;
    msg: string;
    data: T;
  } & Record<string, unknown>;
}

/** 全局状态 */
export interface ConsoleState {
  token: string;
  tokenStatus: TokenStatus;
  appId: string;
  uuid: string;
  currentTargetWxid: string;
  currentTargetDisplayName: string;
  loginNickName: string;
}

export type TokenStatus = 'empty' | 'draft' | 'checking' | 'valid_locked' | 'invalid';

/** 登录二维码响应 */
export interface LoginQrCodeData {
  qrData: string;
  qrUrl: string;
  qrImgBase64: string;
  qrCode: string;  // 与 qrImgBase64 同义，部分接口返回此字段
  appId: string;
  uuid: string;
}

/** 登录轮询响应 */
export interface CheckLoginData {
  uuid: string;
  headImgUrl: string;
  nickName: string;
  expiredTime: number;
  status: number; // 0未扫码 1已扫码未登录 2登录成功 4已取消
  loginInfo?: {
    wxid: string;
    nickName: string;
    [key: string]: unknown;
  };
  url?: string;
}

/** 个人资料 */
export interface ProfileData {
  nickName: string;
  wxid: string;
  alias: string;
  sex: number;
  country: string;
  province: string;
  city: string;
  signature: string;
  headImgUrl: string;
  mobile: string;
  region: string;
  [key: string]: unknown;
}

/** 联系人简要信息 */
export interface BriefContactInfo {
  userName: string;
  nickName: string;
  remark: string;
  headImgUrl: string;
  [key: string]: unknown;
}

/** 联系人详细信息 */
export interface DetailContactInfo extends BriefContactInfo {
  sex: number;
  country: string;
  province: string;
  city: string;
  signature: string;
  bgImgUrl: string;
  [key: string]: unknown;
}

/** 通讯录列表 */
export interface ContactsListData {
  friends: string[];
  chatrooms: string[];
  ghs: string[];
}

/** 设备记录 */
export interface SafetyDevice {
  name: string;
  type: string;
  lastTime: string;
  [key: string]: unknown;
}

/** 调用日志条目 */
export interface ApiLogEntry {
  at: string;
  path: string;
  method: string;
  fullUrl: string;
  requestHeaders: Record<string, string>;
  requestBodyRaw: unknown;
  requestBody: unknown;
  responseHttpStatus: number | null;
  responseBody: unknown;
  error: string | null;
  durationMs: number | null;
}

/** 朋友圈动态 */
export interface SnsItem {
  snsId: string;
  userName: string;
  nickName: string;
  content: string;
  createTime: number;
  type: number;
  imgUrls: string[];
  xml?: string;
  [key: string]: unknown;
}

/** 标签 */
export interface LabelItem {
  labelId: number;
  labelName: string;
  [key: string]: unknown;
}

/** 收藏夹条目 */
export interface FavorItem {
  favId: number;
  type: number;
  flag: number;
  content: string;
  [key: string]: unknown;
}

/**
 * 本地通讯录缓存 schema（§8 版本化）
 * 分键存储，不与 API 令牌桶混用
 */
export interface ContactsCacheSchema {
  version: 1;
  savedAt: number;          // 毫秒时间戳
  appId: string;            // 写入缓存时的会话设备标识
  friendsIds: string[];
  chatroomIds: string[];
  ghIds: string[];
  details: DetailContactInfo[];  // 按 userName 小写去重
}

/** 侧栏导航模块 ID */
export type ModuleId =
  | 'login'
  | 'profile'
  | 'contacts'
  | 'messages'
  | 'sns'
  | 'labels'
  | 'favorites'
  | 'group'
  | 'finder'
  | 'webhook'
  | 'api-logs';
