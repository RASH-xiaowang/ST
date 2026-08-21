/* ============================================================
 * 微信数据管理模块 — 共享类型定义
 * 聚合微信数据板块的所有公共 TS 类型，供组件/服务/事件层共用
 * ============================================================ */

// ─── 通用 ───

export type WeChatTab = 'chats' | 'contacts' | 'moments' | 'favorites' | 'emoticons' | 'bizchats' | 'files' | 'settings';

export interface LatencyHistogram {
  /** 延迟分桶：[<50ms, <200ms, <500ms, <1000ms, >=1000ms] */
  buckets: [number, number, number, number, number];
  sum_ms: number;
  count: number;
}

export interface MonitorStatus {
  running: boolean;
  status: string;
  ws_port?: number;
  pending_acks?: number;
  /** 已发送消息总数 */
  sent_total?: number;
  /** 批量发送次数 */
  sent_batch_count?: number;
  /** WebSocket 回退发送总数 */
  sent_ws_count?: number;
  /** 端到端延迟直方图 */
  latency?: LatencyHistogram;
}

// ─── 会话 ───

export interface SessionEntry {
  username: string;
  name?: string;
  timestamp?: number;
  summary?: string;
  raw_summary?: string;
  draft?: string;
  ts?: number;
  sort_ts?: number;
  pinned?: boolean;
  is_hidden?: boolean;
  time?: string;
  full_time?: string;
  unread?: number;
  unread_count?: number;
  msg_type?: number;
  is_official?: boolean;
  is_group?: boolean;
  avatar_url?: string;
  sender?: string;
  sender_name?: string;
  content?: string;
}

// ─── 消息 ───

export interface ChatMessage {
  local_id: number;
  msg_type: number;
  type_label: string;
  text: string;
  time: string;
  sender_username: string;
  sender_name: string;
  is_self: boolean;
  status?: number;
  rich?: RichMedia;
  [key: string]: unknown;
}

/** 富媒体条目（newsfeed 子条目等） */
export interface RichMediaItem {
  url?: string;
  title?: string;
  digest?: string;
  cover?: string;
  name?: string;
  text?: unknown;
  [key: string]: unknown;
}

/** 通讯录条目（get_contacts_by_category 返回，WeChatPanel 通讯录分页使用） */
export interface ContactItem {
  username: string;
  display_name?: string;
  nick_name?: string;
  remark?: string;
  alias?: string;
  category?: string;
  local_type_label?: string;
  member_count?: number;
  initial?: string;
  description?: string;
  group_name?: string;
  group_username?: string;
  owner?: string;
  owner_name?: string;
  [key: string]: unknown;
}

/** 微信配置（get_wechat_config 返回结构） */
export interface WechatConfigData {
  db_dir?: string;
  db_enc_key?: string;
  keys_file?: string;
  decrypted_dir?: string;
  decoded_image_dir?: string;
  image_aes_key?: string | null;
  image_xor_key?: number;
  api_enabled?: boolean;
  api_port?: number;
  api_token?: string;
  wechat_process?: string;
  wechat_root?: string | null;
  [key: string]: unknown;
}

export interface WechatConfigResult {
  configPath?: string;
  config?: WechatConfigData;
  raw?: WechatConfigData;
  [key: string]: unknown;
}

/** 微信账号检测结果（detect_wechat_accounts 返回条目） */
export interface DetectedAccount {
  db_dir?: string;
  wxid?: string;
  last_active?: number;
  [key: string]: unknown;
}

/** 自动获取数据库密钥结果 */
export interface AutoDbKeyResult {
  key?: string;
  db_dir?: string;
  valid?: number;
  total?: number;
  errors?: string[];
  [key: string]: unknown;
}

/** 自动获取图片密钥结果 */
export interface AutoImgKeyResult {
  aes_key?: string;
  xor_key?: number | null;
  verified?: boolean;
  [key: string]: unknown;
}

/** 数据库密钥校验结果（verify_database_key 返回） */
export interface VerifyDatabaseKeyResult {
  valid: boolean;
  format: string | null;
  aes_ok?: boolean;
  hmac_ok?: boolean;
  [key: string]: unknown;
}

/** 密钥文件生成结果（generate_keys_file 返回） */
export interface GenerateKeysResult {
  valid: number;
  total: number;
  [key: string]: unknown;
}

/** 全自动获取密钥结果（auto_get_wechat_keys 返回） */
export interface AutoKeysResult {
  db_key?: AutoDbKeyResult;
  image_key?: AutoImgKeyResult;
  [key: string]: unknown;
}

/** 一键切换登录账号结果（switch_wechat_account_to_live 返回） */
export interface SwitchAccountResult {
  switched?: boolean;
  live_account?: string;
  db_key_error?: string;
  monitor_error?: string;
  [key: string]: unknown;
}

/** 数据库解密结果（decrypt_all_databases 返回） */
export interface DecryptAllResult {
  decrypted: number;
  total: number;
  wal_patched: number;
  errors?: string[];
  [key: string]: unknown;
}

/** 图片密钥校验结果（verify_image_key 返回） */
export interface VerifyImageKeyResult {
  valid: boolean;
  format: string;
  total_cached: number;
  [key: string]: unknown;
}

/** 图片解码结果（decode_all_images 返回） */
export interface DecodeImagesResult {
  decoded: number;
  total: number;
  errors?: string[];
  [key: string]: unknown;
}

/** 本地 STT 状态（get_local_stt_status 返回，对应 Rust `status_json`） */
export interface SttStatus {
  enabled: boolean;
  model_path: string;
  model_exists: boolean;
  model_size_bytes: number;
  model_loaded: boolean;
  language: string;
  translate: boolean;
  model_size: string;
  default_model_dir?: string;
  languages?: Array<{ value: string; label: string }>;
  available_models?: Array<{ value: string; label: string }>;
  [key: string]: unknown;
}

/** 本地 STT 配置输入（set_local_stt_config 参数） */
export interface SttConfigInput {
  enabled: boolean;
  model_path: string;
  language: string;
  translate: boolean;
  model_size: string;
  [key: string]: unknown;
}

/** 本地 STT 模型下载结果（download_local_stt_model 返回） */
export interface SttDownloadResult {
  path: string;
  size_bytes: number;
  model_loaded: boolean;
  load_error?: string | null;
  status?: SttStatus;
  [key: string]: unknown;
}

/** 微信密钥信息（get_wechat_keys_info 返回） */
export interface WechatKeysInfo {
  keyFormat?: string;
  keyCount?: number;
  [key: string]: unknown;
}

/** CDN 原图自动获取状态 */
export interface CdnImageStatus {
  enabled?: boolean;
  localDecrypt?: boolean;
  [key: string]: unknown;
}

/** 微信文件解析结果（resolve_wechat_file 返回） */
export interface ResolvedFile {
  path?: string;
  dir?: string;
  found?: boolean;
  [key: string]: unknown;
}

/** 账号归档导出结果（export_wechat_archive 返回） */
export interface WechatArchiveResult {
  path: string;
  filename: string;
  file_count: number;
  total_bytes: number;
}

/** 微信备份导入结果（import_wechat_backup 返回） */
export interface WechatImportResult {
  imported: number;
  target: string;
}

export interface RichMedia {
  type: string;
  emoji_url?: string;
  description?: string;
  // ── 模板实际访问的已知字段（按卡片类型分支） ──
  items?: RichMediaItem[];
  top_cover?: string;
  name?: string;
  title?: string;
  url?: string;
  thumb?: string;
  cover?: string;
  file_ext?: string;
  file_size?: number;
  duration?: number;
  icon?: string;
  des?: string;
  source?: string;
  ref_content?: string;
  ref_name?: string;
  pay_memo?: string;
  fee_desc?: string;
  paysubtype?: string;
  direction?: string;
  amount?: string;
  label?: string;
  nickname?: string;
  username?: string;
  poiname?: string;
  pagepath?: unknown;
  articles?: RichMediaItem[];
  [key: string]: unknown;
}

export interface MessagePage {
  messages: WeChatMessage[];
  has_more: boolean;
  next_cursor: number;
  total?: number;
  page?: number;
  page_size?: number;
  chat_name?: string;
  self_username?: string;
}

/** 聊天面板消息（WeChatPanel 展示结构；与 ChatMessage 的 msg_type 字段命名差异） */
export interface WeChatMessage {
  local_id: number;
  server_id?: number;
  sort_seq?: number;
  type: number;
  type_label?: string;
  text: string;
  time: string;
  ts?: number;
  divider?: string;
  is_self: boolean;
  sender_username: string;
  sender_name?: string;
  is_notice?: boolean;
  is_group?: boolean;
  username?: string;
  image_url?: string | null;
  rich?: RichMedia | null;
  [key: string]: unknown;
}

/** 聊天面板会话（含置顶/草稿等面板状态字段） */
export interface WeChatSession extends SessionEntry {
  pinned?: boolean;
  draft?: string;
  sort_ts?: number;
  ts?: number;
  [key: string]: unknown;
}

// ─── 通讯录 ───

export interface ContactEntry {
  username: string;
  nickname: string;
  remark?: string;
  alias?: string;
  avatar?: string;
  label_ids?: string;
  [key: string]: unknown;
}

export interface ContactBook {
  contacts: ContactEntry[];
  labels?: { id: number; name: string }[];
  stats?: unknown;
  [key: string]: unknown;
}

// ─── 朋友圈 ───

export interface MomentLike {
  username: string;
  nickname: string;
}

export interface MomentComment {
  username: string;
  nickname: string;
  to_username: string;
  to_nickname: string;
  content: string;
  ts: number;
  time: string;
}

/** 朋友圈单张图片媒体（含 CDN 下载/解密所需参数） */
export interface MomentMedia {
  /** 缩略图 URL（150px，列表网格使用） */
  thumb: string;
  /** 缩略图下载 token */
  thumb_token: string;
  /** 原图 URL（/0，查看大图使用） */
  url: string;
  /** 原图下载 token */
  url_token: string;
  /** 解密 key（数字字符串；空 = 直链无需解密） */
  key: string;
  /** 图片内容 MD5 */
  md5: string;
}

/** IPC 媒体加载结果（消息图/朋友圈图/视频共用形态） */
export interface MediaResult {
  kind?: string;
  data?: string | null;
  file_key?: string;
  error?: string;
  mime?: string;
  base64?: string;
  [key: string]: unknown;
}

/** 语音转写结果（transcribe_message_voice 返回） */
export type TranscribeResult =
  | { kind: 'data'; data: string }
  | { kind: 'none'; data: string };

/** 会话已编辑消息条目（list_session_edited_messages 返回，对应 Rust edit_store 行） */
export interface SessionEditedItem {
  db: string;
  table_name: string;
  local_id: number;
  edit_count: number;
  last_edited_at: number;
}

/** 消息编辑状态（get_chat_edit_status 返回） */
export interface ChatEditStatus {
  modified: boolean;
  edit_count?: number;
  first_edited_at?: number;
  last_edited_at?: number;
  original_msg_json?: string;
}

/** 消息原始行（get_message_raw_row 返回） */
export interface MessageRawRowResult {
  row: Record<string, unknown>;
  db: string;
  table: string;
}

/** 微信消息搜索索引状态（get_wechat_search_index_status 返回） */
export interface SearchIndexStatus {
  exists: boolean;
  rows: number;
  built_at: string | null;
}

/** 搜索索引构建结果（build_wechat_search_index 返回） */
export interface SearchIndexBuildResult {
  status?: string;
  rows: number;
  message?: string;
  built_at?: string | null;
}

/** 单会话每日消息数（get_chat_daily_counts 返回） */
export interface DailyCountsResult {
  counts: Record<string, number>;
  year: number;
  month: number;
}

/** 通讯录分页（get_contacts_by_category 返回，对应 Rust `ContactPage`） */
export interface ContactPageResult {
  contacts: ContactItem[];
  total: number;
  has_more: boolean;
}

/** 图片体检（缺失图检查）结果 */
export interface MissingImagesData {
  total_images: number;
  local_ok: number;
  cdn_possible: number;
  missing: number;
  scanned_at?: string;
  chats: Array<{
    username: string;
    name?: string;
    missing: number;
    total_images: number;
    [key: string]: unknown;
  }>;
  [key: string]: unknown;
}

/** 每日总结任务 */
export interface DailySummaryTask {
  id?: number;
  name?: string;
  enabled?: boolean;
  provider_id?: string;
  model?: string;
  target_users?: string[];
  target_groups?: string[];
  format_key?: string;
  group_name?: string;
  group_username?: string;
  format?: string;
  custom_prompt?: string;
  schedule_time?: string;
  last_error?: string;
  last_run_at?: number;
  last_status?: string;
  [key: string]: unknown;
}

/** 每日总结记录 */
export interface DailySummaryRecord {
  id?: number;
  task_id?: number;
  status: 'done' | 'error' | string;
  summary_date?: string;
  summary?: string;
  error?: string;
  message_sample?: string;
  message_count?: number;
  char_count?: number;
  provider_id?: string;
  model?: string;
  duration_ms?: number;
  total_tokens?: number;
  prompt_tokens?: number;
  completion_tokens?: number;
  [key: string]: unknown;
}

/** 提供方选项（下拉列表用） */
export interface ProviderOption {
  id: string;
  name: string;
  models: string[];
  default_model: string;
}

/** 每日总结格式（get_daily_summary_formats 返回） */
export interface DailySummaryFormats {
  formats: { key: string; label: string }[];
  [key: string]: unknown;
}

/** 年度总结单项（榜单 / 高频短语 / 表情等，对应 Rust `TopItem`） */
export interface AnnualTopItem {
  key: string;
  name: string;
  count: number;
}

/** 年度总结消息类型项（对应 Rust `kind_counts` 的 JSON 项） */
export interface AnnualKindCount {
  kind: string;
  label: string;
  count: number;
}

/** 年度总结首末消息（对应 Rust `MomentItem`） */
export interface AnnualMomentItem {
  ts: number;
  time: string;
  date: string;
  username: string;
  name: string;
  text: string;
}

/** 周活跃热力图（对应 Rust `heatmap` JSON） */
export interface AnnualHeatmap {
  weekdayLabels: string[];
  hourLabels: string[];
  matrix: number[][];
  total: number;
}

/** 年度总结数据（get_annual_summary 返回，字段与后端 `AnnualSummary` 一致） */
export interface AnnualSummaryData {
  year: number;
  total_messages: number;
  text_messages: number;
  active_days: number;
  total_chars: number;
  avg_chars: number;
  kind_counts: AnnualKindCount[];
  monthly_counts: number[];
  heatmap: AnnualHeatmap;
  top_contacts: AnnualTopItem[];
  top_groups: AnnualTopItem[];
  top_phrases: AnnualTopItem[];
  top_emojis: AnnualTopItem[];
  earliest: AnnualMomentItem | null;
  latest: AnnualMomentItem | null;
}

/** 朋友圈视频媒体 */
export interface MomentVideo {
  /** 视频文件 URL（snsvideodownload，已含 token 参数） */
  url: string;
  /** 封面 URL（vweixinthumb 图片 或 video.qq.com 视频本体） */
  thumb: string;
  /** 封面是否为图片（vweixinthumb 域 → 可直接解出封面 JPEG） */
  thumb_is_image: boolean;
  /** 视频解密 key（`<enc key>`） */
  key: string;
  /** 视频文件 MD5 */
  md5: string;
  /** 视频时长（秒） */
  duration: number;
  width: number;
  height: number;
}

export interface MomentEntry {
  tid: string;
  username: string;
  author: string;
  text: string;
  ts: number;
  time: string;
  media_count: number;
  media_desc: string;
  images: MomentMedia[];
  videos: MomentVideo[];
  location: string;
  link_title: string;
  is_self: boolean;
  likes: MomentLike[];
  comments: MomentComment[];
}

export interface MomentsPage {
  items: MomentEntry[];
  total: number;
  has_more: boolean;
}

// ─── 朋友圈洞察 ───

export interface MomentsAuthorStat {
  username: string;
  name: string;
  posts: number;
  last_ts: number;
}

export interface MomentsMonthStat {
  month: string;
  posts: number;
}

export interface MomentsInsight {
  total: number;
  with_images: number;
  with_videos: number;
  with_location: number;
  with_link: number;
  self_posts: number;
  top_authors: MomentsAuthorStat[];
  monthly: MomentsMonthStat[];
}

/** 会话消息构成统计条目（get_session_message_stats 返回） */
export interface SessionMessageTypeStat {
  type: number;
  label: string;
  count: number;
}

// ─── 收藏 ───

export interface FavoriteEntry {
  local_id: number;
  type?: number;
  type_label?: string;
  text?: string;
  timestamp?: number;
  title?: string;
  desc?: string;
  url?: string;
  ts?: number;
  time?: string;
  source?: string;
  sync_status?: number;
  server_id?: number;
  [key: string]: unknown;
}

/** 收藏列表数据（get_favorites 实际返回 { items, tags }） */
export interface FavoritesData {
  items: FavoriteEntry[];
  tags: string[];
  [key: string]: unknown;
}

/** 收藏详情（get_favorite_detail 返回，对应 Rust `parse_fav_detail` + 元数据） */
export interface FavoriteDetail {
  local_id: number;
  type: number;
  type_label: string;
  text: string;
  title: string;
  images: string[];
  ts: number;
  time: string;
  source: string;
  voice_server_id?: number;
  video?: { duration: number; md5: string };
  link?: { url: string; title: string };
  location?: { name: string; label: string };
  file?: { name: string; ext: string; size: number };
  items?: Array<{ type: string; text?: string; des?: string }>;
  [key: string]: unknown;
}

/** 本地 HTTP API 设置（get_api_settings 返回） */
export interface ApiSettings {
  enabled: boolean;
  port: number;
  token: string | null;
}

/** 微信账号一致性状态（get_wechat_account_status 返回） */
export interface WechatAccountStatus {
  analysis_account: string;
  live_account: string;
  live_account_mtime: string;
  mismatch: boolean;
  weixin_running: boolean;
}

/** 会话快照（get_session_snapshots 返回，对应 Rust SessionTable 查询行） */
export interface SessionSnapshot {
  username: string;
  last_timestamp: number;
  last_msg_locald_id: number;
  last_msg_type: number;
  last_msg_sender: string;
  last_sender_display_name: string;
  unread_count: number;
  summary: string;
  [key: string]: unknown;
}

/** 记录列表 CSV 导出结果（export_wechat_records_csv 返回） */
export interface RecordsCsvResult {
  csv: string;
}

/** AI 问答引用（ask_wechat 返回 citations 条目） */
export interface AskCitation {
  kind: string;
  kind_label: string;
  username: string;
  name: string;
  local_id: number;
  ts: number;
  time: string;
  snippet: string;
  [key: string]: unknown;
}

/** AI 问答统计表（ask_wechat 返回 stats 条目） */
export interface AskStatsTable {
  title: string;
  columns: string[];
  rows: string[][];
  summary: string;
  [key: string]: unknown;
}

/** AI 问答结果（ask_wechat 返回） */
export interface AskWechatResult {
  answer?: string;
  error?: string;
  citations?: AskCitation[];
  stats?: AskStatsTable[];
  steps?: string[];
  rounds?: number;
  plan?: unknown;
  llm_used?: boolean;
  elapsed_ms?: number;
  [key: string]: unknown;
}

/** 加密备份创建结果（create_wechat_backup 返回） */
export interface WechatBackupCreateResult {
  path: string;
  filename: string;
  size: number;
  file_count: number;
  created_at: string;
}

/** 加密备份恢复结果（restore_wechat_backup 返回） */
export interface WechatBackupRestoreResult {
  restored: boolean;
  imported: number | null;
  target: string | null;
}

/** 加密备份列表条目（list_wechat_backups 返回） */
export interface WechatBackupItem {
  name: string;
  path: string;
  size: number;
  modified: number;
}

/** 加密备份列表（list_wechat_backups 返回） */
export interface WechatBackupListResult {
  dir: string;
  items: WechatBackupItem[];
}

/** 隐私体检样本（scan_privacy_risks 返回） */
export interface PrivacySample {
  username: string;
  name: string;
  local_id: number;
  ts: number;
  time: string;
  snippet: string;
  [key: string]: unknown;
}

/** 隐私体检分类（scan_privacy_risks 返回） */
export interface PrivacyCategory {
  key: string;
  label: string;
  icon: string;
  count: number;
  samples: PrivacySample[];
  [key: string]: unknown;
}

/** 隐私体检 TOP 联系/群聊（scan_privacy_risks 返回） */
export interface PrivacyTopItem {
  username: string;
  name: string;
  count: number;
}

/** 隐私体检结果（scan_privacy_risks 返回） */
export interface PrivacyScanResult {
  scanned: { rows: number; sessions: number; elapsed_ms: number; budget: number };
  total_hits: number;
  categories: PrivacyCategory[];
  top_contacts: PrivacyTopItem[];
  top_groups: PrivacyTopItem[];
  [key: string]: unknown;
}

// ─── 表情 ───

export interface EmoticonPackage {
  package_id: string;
  name?: string;
  count?: number;
  [key: string]: unknown;
}

export interface EmoticonItem {
  md5: string;
  item_type: number;
  size_label?: string;
  raw?: Record<string, unknown>;
}

export interface EmoticonOverview {
  packages: EmoticonPackage[];
  custom: EmoticonItem[];
  store_files: EmoticonItem[];
}

export interface StaticEmoticonFile {
  name: string;
  path: string;
}

export interface StaticEmoticonCategory {
  category: string;
  label: string;
  files: StaticEmoticonFile[];
}

// ─── 公众号 ───

export interface BizChatGroup {
  username: string;
  name?: string;
  [key: string]: unknown;
}

/** 公众号/服务号条目（get_official_accounts 返回，对应 Rust `OfficialAccount`） */
export interface OfficialAccount {
  username: string;
  name: string;
  /** subscription(订阅号) / service(服务号) / enterprise(企业号) / unknown */
  official_kind: string;
  ts: number;
  time: string;
  summary: string;
  unread_count: number;
  pinned: boolean;
  /** “查看历史消息”网页链接 */
  history_url: string;
  [key: string]: unknown;
}

// ─── 文件 ───

/** 资源文件条目（get_resource_files 返回，对应 Rust `ResourceFile`） */
export interface ResourceFile {
  md5: string;
  file_name: string;
  file_size: number;
  size_label: string;
  modify_time: number;
  time: string;
  /** 分类：image / video / file */
  category: string;
  ext: string;
  path?: string | null;
  cover_path?: string | null;
  [key: string]: unknown;
}

/** 资源文件总览（get_resource_files 返回，对应 Rust `ResourceFilesOverview`） */
export interface ResourceFilesOverview {
  images: ResourceFile[];
  videos: ResourceFile[];
  files: ResourceFile[];
  total_size: number;
  total_size_label: string;
  images_total: number;
  videos_total: number;
  files_total: number;
}

// ─── 通用设置 ───

/** 通用设置分类（get_general_settings 返回，对应 Rust `GeneralCategory`） */
export interface GeneralCategory {
  key: string;
  label: string;
  table: string;
  columns: string[];
  column_labels: string[];
  rows: unknown[][];
  count: number;
  total: number;
}

/** 通用分类 CSV 导出结果（export_general_category_csv 返回） */
export interface GeneralCategoryCsvResult {
  csv: string;
}

/** 记录列表条目（revokes/transfers/redpackets 等通用字段） */
export interface RecordListItem {
  session_name?: string;
  msg_local_id?: number | string | null;
  batch_id?: string | null;
  msg_create_time?: number | string | null;
  pay_payer?: string;
  pay_receiver?: string;
  begin_transfer_time?: number | string | null;
  sender_user_name?: string;
  finder_username?: string;
  user_name?: string;
  user_name_?: string;
  last_update_time?: number | string | null;
  timestamp_?: number | string | null;
  [key: string]: unknown;
}

/** 记录分页结果（list_*_records 返回） */
export interface RecordListResult {
  items: RecordListItem[];
  total?: number;
  [key: string]: unknown;
}

/** 群成员列表（get_group_members 返回） */
export interface GroupMembersResult {
  members: { username: string; name: string }[];
  [key: string]: unknown;
}

// ─── 导出回调 ───

export interface ExportResult {
  path: string;
  filename: string;
  count: number;
  /** HTML 导出时成功落盘的图片/视频资源数 */
  media?: number;
  /** HTML 导出时下载失败的资源数 */
  media_failed?: number;
}

// ─── 事件载荷 ───

export interface WeChatMessagePayload {
  username: string;
  content?: string;
  summary?: string;
  timestamp?: number;
  msg_type?: number;
  media_type?: string;
  sender?: string;
  sender_name?: string;
  unread?: number;
  local_id?: number;
  sort_seq?: number;
  is_send?: boolean | null;
  is_group?: boolean;
  is_self?: boolean;
  image_url?: string | null;
  rich?: RichMedia | null;
  time?: string;
  sender_username?: string;
  /** 后端生成的去重/确认 ID */
  ack_id?: string;
  /** 消息来源通道：event（Tauri Event）或 websocket */
  channel?: 'event' | 'websocket';
  /** 后端 Unix 毫秒时间戳，用于延迟观测 */
  ts_backend?: number;
}

export interface MonitorStatusPayload {
  running: boolean;
  total_messages?: number;
  ws_port?: number;
  pending_acks?: number;
  status?: string;
  sent_total?: number;
  sent_batch_count?: number;
  sent_ws_count?: number;
  latency?: LatencyHistogram;
}

/** 微信密钥/解密操作进度事件（wechat-op-progress，对应后端 auto_key::emit_progress） */
export interface WeChatOpProgress {
  op: string;
  done: number;
  total: number;
  percent: number;
  message: string;
}

/** STT 模型下载进度事件（stt-download-progress） */
export interface SttDownloadProgress {
  filename: string;
  done: number;
  total: number;
  percent: number;
  finished: boolean;
}
/** 存储空间分析：分类统计（get_wechat_storage_stats） */
export interface StorageCategoryStat {
  label: string;
  count: number;
  size: number;
}

/** 存储空间分析：会话/发送者排行项 */
export interface StorageRankItem {
  username: string;
  /** 显示名（备注 > 昵称；空则前端回退 username） */
  name?: string;
  count: number;
  size: number;
}

/** 存储空间分析：大文件清单项 */
export interface StorageLargeFile {
  name: string;
  size: number;
  username: string;
  create_time: number;
}

/** 存储空间分析结果（对应 Rust WechatStorageStats） */
export interface WechatStorageStats {
  total_size: number;
  total_count: number;
  categories: StorageCategoryStat[];
  chats: StorageRankItem[];
  senders: StorageRankItem[];
  large_files: StorageLargeFile[];
}
/** 微信数据总览（get_wechat_data_overview） */
export interface WechatDataOverview {
  sessions: number;
  groups: number;
  contacts: number;
  official: number;
  moments: number;
  favorites: number;
  emoticons: number;
  revoked: number;
  storage: WechatStorageStats;
  /** 朋友圈活跃作者 Top 15 */
  moments_authors?: MomentsAuthorStat[];
}
/** 撤回消息记录（get_wechat_revoked_messages） */
export interface RevokedMessage {
  sender: string;
  type_label: string;
  content: string;
  create_time: number;
}