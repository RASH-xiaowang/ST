/* ============================================================
 * 微信数据管理模块 — IPC 服务层
 * 集中封装所有微信数据 IPC 调用，对外提供类型安全的函数接口
 * 组件层应通过本模块调用后端，而非直接 invoke('xxx')
 * ============================================================ */
import { invoke } from '@tauri-apps/api/core';
import type { WechatSearchResult } from '../../search/types';
import type { GraphRawData } from '../graph/graphModel';
import type {
  MonitorStatus,
  WeChatSession,
  MessagePage,
  ContactBook,
  FavoritesData,
  MediaResult,
  MomentEntry,
  SessionMessageTypeStat,
  MomentsInsight,
  AnnualSummaryData,
  AutoDbKeyResult,
  AutoKeysResult,
  ApiSettings,
  AskWechatResult,
  ChatEditStatus,
  ContactItem,
  DailySummaryRecord,
  DailySummaryFormats,
  DailySummaryTask,
  AutoImgKeyResult,
  DecodeImagesResult,
  DecryptAllResult,
  DetectedAccount,
  GenerateKeysResult,
  MessageRawRowResult,
  MissingImagesData,
  FavoriteDetail,
  PrivacyScanResult,
  RecordsCsvResult,
  SessionEditedItem,
  SttConfigInput,
  SttDownloadResult,
  SttStatus,
  SwitchAccountResult,
  WechatArchiveResult,
  WechatImportResult,
  ContactPageResult,
  CdnImageStatus,
  DailyCountsResult,
  GroupMembersResult,
  GeneralCategoryCsvResult,
  OfficialAccount,
  ResolvedFile,
  SearchIndexBuildResult,
  SearchIndexStatus,
  TranscribeResult,
  WechatAccountStatus,
  WechatBackupCreateResult,
  WechatBackupListResult,
  WechatBackupRestoreResult,
  VerifyDatabaseKeyResult,
  VerifyImageKeyResult,
  WechatKeysInfo,
  WechatConfigResult,
  MomentsPage,
  EmoticonOverview,
  StaticEmoticonCategory,
  ExportResult,
  ResourceFilesOverview,
  GeneralCategory,
  RecordListResult,
  WechatStorageStats,
  WechatDataOverview,
  RevokedMessage,
} from '../types';

// ─── 实时消息监控 ───

export function getMonitorStatus(): Promise<MonitorStatus> {
  return invoke<MonitorStatus>('get_wechat_monitor_status');
}
/** 从后端补拉断线/隐藏期间遗漏的消息（按 ack_id 水位线增量） */
export function resyncWechatMessages(sinceAckId: string): Promise<string[]> {
  // Tauri 参数必须是 camelCase（后端 Rust 参数 since_ack_id → sinceAckId），
  // 此前误用 snake_case 导致补拉请求参数静默丢失、始终拿不到遗漏消息
  return invoke<string[]>('resync_wechat_messages', { sinceAckId });
}

export function ackWechatMessage(ackId: string): Promise<void> {
  // 同理：后端 ack_id → ackId；此前传 ack_id 导致每条 ACK 都报
  // "missing required key ackId"、路由器永远重传（端到端延迟告警）
  return invoke('ack_wechat_message', { ackId });
}

export function startMonitor(): Promise<void> {
  return invoke('start_wechat_monitor');
}

export function stopMonitor(): Promise<void> {
  return invoke('stop_wechat_monitor');
}

// ─── 会话 ───

export function getSessionList(): Promise<WeChatSession[]> {
  return invoke('get_session_list');
}

export function refreshWechatSessions(): Promise<WeChatSession[]> {
  return invoke('refresh_wechat_sessions');
}

// ─── 消息 ───

export function getConversationMessages(
  args: { username: string; page?: number; pageSize?: number; beforeSortSeq?: number | null },
): Promise<MessagePage> {
  return invoke('get_conversation_messages', args);
}

/** 会话消息构成统计（各消息类型条数，聊天头部画像） */
export function getSessionMessageStats(username: string): Promise<SessionMessageTypeStat[]> {
  return invoke('get_session_message_stats', { username });
}

export function exportSessionMessages(
  username: string,
  format: string,
  count: number,
  path?: string,
): Promise<ExportResult> {
  return invoke('export_session_messages', { username, format, count, path });
}

export function batchExportSessions(
  usernames: string[],
  format: string,
  dir?: string,
): Promise<{ sessions: number; total_messages: number; dir: string }> {
  return invoke('batch_export_sessions', { usernames, format, dir });
}

export function deleteConversationMessages(
  username: string,
): Promise<{ deleted: number }> {
  return invoke('delete_conversation_messages', { username });
}

export function clearSessionDraft(
  username: string,
): Promise<{ username: string; updated: number }> {
  return invoke('clear_session_draft', { username });
}

export function clearAllSessionDrafts(): Promise<{
  updated: number;
  drafts: { username: string; draft: string }[];
}> {
  return invoke('clear_all_session_drafts');
}

// ─── 通讯录 ───

export function getContacts(): Promise<ContactBook> {
  return invoke('get_contacts');
}

export function exportContactsCsv(path?: string): Promise<ExportResult> {
  return invoke('export_contacts_csv', path ? { path } : {});
}

// ─── 朋友圈 ───

export function getMoments(offset = 0, limit = 6, authorUsername?: string): Promise<MomentsPage> {
  return invoke('get_moments_page', { offset, limit, authorUsername: authorUsername ?? null });
}

/** 朋友圈洞察：作者活跃榜 / 月度热力 / 媒体构成 */
export function getMomentsInsights(): Promise<MomentsInsight> {
  return invoke('get_moments_insights');
}

export function refreshWechatMoments(
  offset?: number,
  limit?: number,
  authorUsername?: string,
): Promise<{ items: MomentEntry[]; total: number; has_more: boolean }> {
  return invoke('refresh_wechat_moments', { offset, limit, authorUsername: authorUsername ?? null });
}

/**
 * 导出朋友圈：format = csv / json / txt / html；
 * authorUsername 非空时只导出该作者（对应「正在看某位好友」的过滤态）；
 * html 格式会把全部图片/视频资源下载到 `<html名>_media/` 并相对引用。
 */
export function exportMoments(opts: {
  format?: 'csv' | 'json' | 'txt' | 'html';
  authorUsername?: string | null;
  path?: string;
} = {}): Promise<ExportResult> {
  return invoke('export_moments', {
    format: opts.format ?? 'csv',
    authorUsername: opts.authorUsername ?? null,
    path: opts.path ?? null,
  });
}

// ─── 收藏 ───

export function getFavorites(): Promise<FavoritesData> {
  return invoke('get_favorites', { page: 0 });
}

export function deleteFavoriteItems(localIds: number[]): Promise<{ deleted: number }> {
  return invoke('delete_favorite_items', { localIds });
}

export function exportFavoritesCsv(path?: string): Promise<ExportResult> {
  return invoke('export_favorites_csv', path ? { path } : {});
}

// ─── 表情 ───

export function getEmoticons(): Promise<EmoticonOverview> {
  return invoke('get_emoticons');
}

export function getStaticEmoticons(): Promise<StaticEmoticonCategory[]> {
  return invoke('get_static_emoticons');
}

// ─── 文件 ───

export function getResourceFiles(): Promise<ResourceFilesOverview> {
  return invoke('get_resource_files');
}

// ─── 通用设置 ───

export function getGeneralSettings(): Promise<GeneralCategory[]> {
  return invoke('get_general_settings');
}

// ─── 头像 ───

export function getUserAvatar(
  username: string,
): Promise<{ kind: 'data' | 'url' | 'none'; data?: string; url?: string }> {
  return invoke('get_user_avatar', { username });
}

// ─── 数据库状态 ───

export function getWechatDbStatus(): Promise<string[]> {
  return invoke('get_wechat_db_status');
}

// ─── 微信配置 ───

export function getWechatConfig(): Promise<WechatConfigResult> {
  return invoke('get_wechat_config');
}

export function saveWechatConfig(config: Record<string, unknown>): Promise<void> {
  return invoke('save_wechat_config', { config });
}

export function getWechatKeysInfo(fileName?: string): Promise<WechatKeysInfo> {
  return invoke('get_wechat_keys_info', fileName ? { fileName } : {});
}

// ─── 全自动密钥获取（对标 WeFlow：wx_key.dll Hook + 图片模板校验） ───

export function autoGetDbKey(timeoutMs?: number): Promise<AutoDbKeyResult> {
  return invoke('auto_get_db_key', { timeoutMs });
}

/** 4.1.10.31+ 调试器方案：临时重启微信并断点提取 master key（需重新扫码登录一次） */
export function autoGetDbKeyV2(timeoutMs?: number): Promise<AutoDbKeyResult> {
  return invoke('auto_get_db_key_v2', { timeoutMs });
}

export function autoGetImageKey(baseDir?: string, wxid?: string): Promise<AutoImgKeyResult> {
  return invoke('auto_get_image_key', { baseDir, wxid });
}

export function autoGetWechatKeys(timeoutMs?: number): Promise<AutoKeysResult> {
  return invoke('auto_get_wechat_keys', { timeoutMs });
}

export function verifyDatabaseKey(dbPath: string, encKeyHex: string): Promise<VerifyDatabaseKeyResult> {
  return invoke('verify_database_key', { dbPath, encKeyHex });
}

export function generateKeysFile(args: {
  dbDir: string;
  keysFile: string;
  encKeyHex: string;
  keyFormat: string | null;
}): Promise<GenerateKeysResult> {
  return invoke('generate_keys_file', args);
}

export function decryptAllDatabases(args: {
  keysFile: string;
  dbDir: string;
  decryptedDir: string;
}): Promise<DecryptAllResult> {
  return invoke('decrypt_all_databases', args);
}

export function verifyImageKey(args: {
  dbDir: string;
  aesKeyHex: string;
  xorKeyStr: string;
}): Promise<VerifyImageKeyResult> {
  return invoke('verify_image_key', args);
}

export function decodeAllImages(args: {
  dbDir: string;
  outputDir: string;
  aesKeyHex: string;
  xorKeyStr: string;
}): Promise<DecodeImagesResult> {
  return invoke('decode_all_images', args);
}

export function detectWechatAccounts(): Promise<DetectedAccount[]> {
  return invoke('detect_wechat_accounts');
}

// ─── 年度总结 ───

export function getAnnualAvailableYears(): Promise<{ years: string[] }> {
  return invoke('get_annual_available_years');
}

export function getAnnualSummary(year: string | number): Promise<AnnualSummaryData> {
  return invoke('get_annual_summary', { year });
}

// ─── AI 问答 ───

export function askWechat(
  question: string,
  limit?: number,
  history?: { question: string; answer: string }[]
): Promise<AskWechatResult> {
  return invoke('ask_wechat', { question, limit, history });
}

// ─── 备份 / 归档 ───

export function createWechatBackup(args: { passphrase: string; outputDir: string }): Promise<WechatBackupCreateResult> {
  return invoke('create_wechat_backup', args);
}

export function deleteWechatBackup(path: string): Promise<void> {
  return invoke('delete_wechat_backup', { path });
}

export function listWechatBackups(dir: string): Promise<WechatBackupListResult> {
  return invoke('list_wechat_backups', { dir });
}

export function restoreWechatBackup(args: { path: string; passphrase: string }): Promise<WechatBackupRestoreResult> {
  return invoke('restore_wechat_backup', args);
}

export function importWechatBackup(args: { source: string }): Promise<WechatImportResult> {
  return invoke('import_wechat_backup', args);
}

export function exportWechatArchive(args: {
  outputDir: string | null;
  includeResources: boolean;
}): Promise<WechatArchiveResult> {
  return invoke('export_wechat_archive', args);
}

// ─── 每日总结 ───

export function listDailySummaryTasks(): Promise<DailySummaryTask[]> {
  return invoke('list_daily_summary_tasks');
}

export function saveDailySummaryTask(task: { task: DailySummaryTask }): Promise<DailySummaryTask> {
  return invoke('save_daily_summary_task', { task });
}

export function deleteDailySummaryTask(id: number): Promise<void> {
  return invoke('delete_daily_summary_task', { id });
}

export function toggleDailySummaryTask(id: number, enabled: boolean): Promise<void> {
  return invoke('toggle_daily_summary_task', { id, enabled });
}

export function runDailySummaryTask(id: number): Promise<void> {
  return invoke('run_daily_summary_task', { id });
}

export function runDailySummaryRange(args: {
  taskId: number;
  startDate: string;
  endDate: string;
}): Promise<void> {
  return invoke('run_daily_summary_range', args);
}

export function listDailySummaryRecords(taskId: number): Promise<DailySummaryRecord[]> {
  return invoke('list_daily_summary_records', { taskId });
}

export function deleteDailySummaryRecord(id: number): Promise<void> {
  return invoke('delete_daily_summary_record', { id });
}

export function getDailySummaryFormats(): Promise<DailySummaryFormats> {
  return invoke('get_daily_summary_formats');
}

export function getGroupMembers(groupUsername: string): Promise<GroupMembersResult> {
  return invoke('get_group_members', { groupUsername });
}

// ─── 通用记录导出 ───

export function exportWechatRecordsCsv(args: { kind: string }): Promise<RecordsCsvResult> {
  return invoke('export_wechat_records_csv', args);
}

/** 记录列表查询参数（limit/offset 分页 + q 关键字） */
export type WechatRecordListQuery = {
  limit: number;
  offset: number;
  q: string | null;
};

export function listWechatRevokes(args: WechatRecordListQuery): Promise<RecordListResult> {
  return invoke('list_wechat_revokes', args);
}

export function listWechatTransfers(args: WechatRecordListQuery): Promise<RecordListResult> {
  return invoke('list_wechat_transfers', args);
}

export function listWechatRedEnvelopes(args: WechatRecordListQuery): Promise<RecordListResult> {
  return invoke('list_wechat_red_envelopes', args);
}

export function listWechatFinder(args: WechatRecordListQuery): Promise<RecordListResult> {
  return invoke('list_wechat_finder', args);
}

export function listWechatMiniPrograms(args: WechatRecordListQuery): Promise<RecordListResult> {
  return invoke('list_wechat_mini_programs', args);
}

export function listWechatFriendVerifications(
  args: WechatRecordListQuery,
): Promise<RecordListResult> {
  return invoke('list_wechat_friend_verifications', args);
}

// ─── 隐私扫描 / 关系图谱 ───

export function scanPrivacyRisks(): Promise<PrivacyScanResult> {
  return invoke('scan_privacy_risks_cmd');
}

export function getRelationshipGraph(args: { limit?: number }): Promise<GraphRawData> {
  return invoke('get_relationship_graph', args);
}

/** 读取上次成功构建的关系图谱缓存（无缓存返回 null） */
export function getRelationshipGraphCached(): Promise<GraphRawData | null> {
  return invoke('get_relationship_graph_cached');
}

/** 写入导出文件（内容 base64，支持二进制） */
export function writeFile(path: string, contentB64: string): Promise<void> {
  return invoke('write_file', { path, contentB64 });
}

/** 后端下载远程图片并转为 data URL（用于导出图嵌入头像，绕过浏览器 CORS） */
export function fetchImageDataUrl(url: string): Promise<string> {
  return invoke('fetch_image_data_url', { url });
}

// ─── CDN 图片 / API 设置 ───

export function getCdnImageStatus(): Promise<CdnImageStatus> {
  return invoke('get_cdn_image_status');
}

export function setCdnImageEnabled(enabled: boolean): Promise<void> {
  return invoke('set_cdn_image_enabled', { enabled });
}

export function setCdnImageLocalDecrypt(localDecrypt: boolean): Promise<void> {
  return invoke('set_cdn_image_local_decrypt', { localDecrypt });
}

export function getWechatMissingImages(): Promise<MissingImagesData> {
  return invoke('get_wechat_missing_images');
}

export function exportWechatMissingImagesCsv(): Promise<ExportResult> {
  return invoke('export_wechat_missing_images_csv');
}

export function getWechatAccountStatus(): Promise<WechatAccountStatus> {
  return invoke('get_wechat_account_status');
}

/** 一键切换到当前登录微信账号并重新获取密钥（后台 hook/调试器方案，耗时较长） */
export function switchWechatAccountToLive(timeoutMs?: number): Promise<SwitchAccountResult> {
  return invoke('switch_wechat_account_to_live', { timeoutMs });
}

/** 本地离线语音转写（whisper.cpp）：状态 */
export function getLocalSttStatus(): Promise<SttStatus> {
  return invoke('get_local_stt_status');
}

/** 本地离线语音转写：保存配置（启用/模型路径/语言） */
export function setLocalSttConfig(config: SttConfigInput): Promise<SttStatus> {
  return invoke('set_local_stt_config', { config });
}

/** 本地离线语音转写：下载 Whisper GGML 模型（tiny/base/small） */
export function downloadLocalSttModel(size?: string): Promise<SttDownloadResult> {
  return invoke('download_local_stt_model', { size });
}

export function applyApiSettings(settings?: Record<string, unknown>): Promise<void> {
  return invoke('apply_api_settings', settings ? { settings } : {});
}

// ─── 聊天编辑 / 消息检索 ───

export function listSessionEditedMessages(args: { username: string }): Promise<{ items: SessionEditedItem[] }> {
  return invoke('list_session_edited_messages', args);
}

export function getMessageRawRow(args: { username: string; localId: number }): Promise<MessageRawRowResult> {
  return invoke('get_message_raw_row', args);
}

export function updateMessageRawFields(args: {
  username: string;
  localId: number;
  edits: Record<string, unknown>;
  unsafeEdit: boolean;
}): Promise<void> {
  return invoke('update_message_raw_fields', args);
}

export function editChatMessage(args: { username: string; localId: number; newText: string }): Promise<void> {
  return invoke('edit_chat_message', args);
}

export function resetEditedMessage(args: { username: string; localId: number }): Promise<void> {
  return invoke('reset_edited_message', args);
}

export function searchWechatMessages(args: { query: string; limit?: number }): Promise<WechatSearchResult> {
  return invoke('search_wechat_messages', args);
}

export function buildWechatSearchIndex(force: boolean): Promise<SearchIndexBuildResult> {
  return invoke('build_wechat_search_index', { force });
}

export function getWechatSearchIndexStatus(): Promise<SearchIndexStatus> {
  return invoke('get_wechat_search_index_status');
}

export function getOfficialAccounts(): Promise<OfficialAccount[]> {
  return invoke('get_official_accounts');
}

export function getContactProfile(username: string): Promise<ContactItem> {
  return invoke('get_contact_profile', { username });
}

export function getContactsByCategory(args: { category: string; offset?: number; limit?: number; query?: string }): Promise<ContactPageResult> {
  return invoke('get_contacts_by_category', {
    category: args.category,
    offset: args.offset ?? 0,
    limit: args.limit ?? 200,
    query: args.query ?? null,
  });
}

// ─── 媒体 / 附件 ───

/** 消息图片查询参数（size 缺省即高清原图） */
export type MessageMediaQuery = {
  username: string;
  localId: number;
  size?: 'thumb' | 'hd' | null;
};

export function getMessageImage(args: MessageMediaQuery): Promise<MediaResult> {
  return invoke('get_message_image', args);
}

export function getMessageVoice(args: { username: string; localId: number }): Promise<MediaResult> {
  return invoke('get_message_voice', args);
}

export function getMomentImage(args: { url: string; key: string; token?: string }): Promise<MediaResult> {
  return invoke('get_moment_image', args);
}

export function getMomentVideo(args: { url: string; key: string }): Promise<MediaResult> {
  return invoke('get_moment_video', args);
}

export function getFavoriteDetail(localId: number): Promise<FavoriteDetail> {
  return invoke('get_favorite_detail', { localId });
}

export function getFavoriteImage(imageMd5: string): Promise<MediaResult> {
  return invoke('get_favorite_image', { imageMd5 });
}

export function getFavoriteVoice(args: { serverId: number }): Promise<MediaResult> {
  return invoke('get_favorite_voice', args);
}

export function resolveWechatFile(username: string, localId: number): Promise<ResolvedFile> {
  return invoke('resolve_wechat_file', { username, localId });
}

export function transcribeMessageVoice(args: {
  username: string | null | undefined;
  localId: number;
}): Promise<TranscribeResult> {
  return invoke('transcribe_message_voice', args);
}

export function ocrIngestResource(args: {
  senderUsername: string;
  sessionType: string;
  timestamp: string;
  username: string;
  mediaUrl: string;
}): Promise<number> {
  return invoke('ocr_ingest_resource', args);
}

export function openWechatFolder(path: string): Promise<void> {
  return invoke('open_wechat_folder', { path });
}

export function openWechatAttachFolder(args: { username: string }): Promise<void> {
  return invoke('open_wechat_attach_folder', args);
}

export function openWechatPath(args: { path: string }): Promise<void> {
  return invoke('open_wechat_path', args);
}

export function openWechatProtocol(url: string): Promise<void> {
  return invoke('open_wechat_protocol', { url });
}

export function exportGeneralCategoryCsv(args: { table: string }): Promise<GeneralCategoryCsvResult> {
  return invoke('export_general_category_csv', args);
}

export function getApiSettings(): Promise<ApiSettings> {
  return invoke('get_api_settings');
}

export function setDbConfig(key: string, value: string): Promise<void> {
  return invoke('set_db_config', { key, value });
}

export function getDbConfig(): Promise<{ key: string; value: string }[]> {
  return invoke('get_db_config');
}

export function getChatDailyCounts(args: {
  username: string | null | undefined;
  year: number;
  month: number;
}): Promise<DailyCountsResult> {
  return invoke('get_chat_daily_counts', args);
}

export function getChatEditStatus(args: { username: string; localId: number }): Promise<ChatEditStatus> {
  return invoke('get_chat_edit_status', args);
}

// ─── 原图 Hook（img_helper.dll，参考 WeFlow） ───

export interface ImgHookStatus {
  supported: boolean;
  enabled: boolean;
  hooked: boolean;
  pid: number | null;
  whitelist: string[];
  error: string;
  dll_ok: boolean;
}

export function imgHookStart(whitelist: string[]): Promise<ImgHookStatus> {
  return invoke<ImgHookStatus>('img_hook_start', { whitelist });
}

export function imgHookStop(): Promise<ImgHookStatus> {
  return invoke<ImgHookStatus>('img_hook_stop');
}

export function imgHookSetWhitelist(whitelist: string[]): Promise<ImgHookStatus> {
  return invoke<ImgHookStatus>('img_hook_set_whitelist', { whitelist });
}

export function imgHookStatus(): Promise<ImgHookStatus> {
  return invoke<ImgHookStatus>('img_hook_status');
}
export function getWechatStorageStats(): Promise<WechatStorageStats> {
  return invoke<WechatStorageStats>('get_wechat_storage_stats');
}
export function getWechatDataOverview(): Promise<WechatDataOverview> {
  return invoke<WechatDataOverview>('get_wechat_data_overview');
}
export function getWechatRevokedMessages(limit?: number): Promise<RevokedMessage[]> {
  return invoke<RevokedMessage[]>('get_wechat_revoked_messages', { limit });
}