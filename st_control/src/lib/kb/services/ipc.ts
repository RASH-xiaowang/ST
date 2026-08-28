// 知识库 — Tauri IPC 封装层
// 组件层统一通过本模块调用后端，避免直接 invoke。
import { invoke, Channel } from '@tauri-apps/api/core';
import type {
  AnalyticsSetting,
  AnalyticsResult,
  BatchDownloadResult,
  CurrentUser,
  DirNode,
  DocItem,
  DocView,
  DownloadDocumentResult,
  FetchUrlResult,
  HighlightSegment,
  JobItem,
  JobLogItem,
  HousekeepingResult,
  KbStats,
  KbSummary,
  KbVersion,
  ModelInfo,
  ModelSettingsResult,
  QaMessageItem,
  QaSessionItem,
  RagAnswer,
  RecommendItem,
  ReprocessResult,
  RetrievedChunk,
  RoleItem,
  SearchLogItem,
  WikiDir,
  WikiVersionItem,
  WikiGenerateInput,
  WikiGraph,
  WikiPageDetail,
  WikiPageInput,
  WikiPageItem,
  UploadDocumentResult,
  UpdateChunkResult,
  UserItem,
  MemberItem,
} from '../kbTypes';

/** RAG 检索输入（与后端 SearchInput camelCase 对应） */
export type KbSearchInput = {
  userId?: number;
  kbId?: number | null;
  query: string;
  topK?: number;
  mode?: string;
  providerId?: string | null;
  model?: string | null;
};

/** 上传文档输入（data 为原始文件字节数组，dataBase64 为 base64 编码，二选一） */
export type KbUploadDocInput = {
  kbId: number;
  dirId?: number | null;
  title: string;
  fileType: string;
  data?: number[];
  /** base64 编码的文件内容（大文件推荐，避免 JSON 数组的巨大内存开销） */
  dataBase64?: string;
  embeddingProvider?: string | null;
  embeddingModel?: string | null;
  chunkStrategy?: string;
  chunkSize?: number;
  chunkOverlap?: number;
};

/** 上传新版本输入 */
export type KbNewVersionInput = {
  docId: number;
  fileType: string;
  data?: number[];
  /** base64 编码的文件内容（大文件推荐） */
  dataBase64?: string;
  note?: string | null;
  embeddingProvider?: string | null;
  embeddingModel?: string | null;
  chunkStrategy?: string;
  chunkSize?: number;
  chunkOverlap?: number;
};

/** 网页抓取输入 */
export type KbFetchUrlInput = {
  url: string;
  kbId: number;
  dirId?: number | null;
  embeddingProvider?: string | null;
  embeddingModel?: string | null;
};

/** RAG 检索片段覆盖（人工编辑，跳过自动检索） */
export type KbRagChunkOverride = {
  chunkId: number;
  content: string;
};

/** RAG 流式输入（与后端 RagInput camelCase 对应） */
export type KbRagInput = {
  userId?: number;
  kbId?: number | null;
  query: string;
  providerId?: string | null;
  model?: string | null;
  topK?: number;
  mode?: string;
  sessionId?: number | null;
  chunks?: KbRagChunkOverride[];
};

export const kbApi = {
  // ── 通用 invoke（供未封装的命令使用） ──
  invoke: <T>(cmd: string, args?: Record<string, unknown>) => invoke<T>(cmd, args),

  // ── 知识库 / 目录 / 文档 ──
  list: (userId: number) => invoke<KbSummary[]>('kb_list', { userId }),
  create: (name: string, description: string | null) => invoke<KbSummary>('kb_create', { name, description }),
  update: (kbId: number, name: string, description: string | null) =>
    invoke<void>('kb_update', { kbId, name, description }),
  remove: (kbId: number) => invoke<void>('kb_delete', { kbId }),
  setPin: (kbId: number, pinned: boolean) => invoke<void>('kb_set_pin', { kbId, pinned }),
  listDirs: (kbId: number) => invoke<DirNode[]>('kb_list_dirs', { kbId }),
  createDir: (kbId: number, parentId: number | null, name: string) =>
    invoke<number>('kb_create_dir', { kbId, parentId, name }),
  renameDir: (dirId: number, name: string) => invoke<void>('kb_rename_dir', { dirId, name }),
  deleteDir: (dirId: number) => invoke<void>('kb_delete_dir', { dirId }),
  listDocuments: (params: {
    kbId: number;
    page: number;
    pageSize: number;
    status?: string | null;
    category?: string | null;
    keyword?: string | null;
    tag?: string | null;
    dirId?: number | null;
  }) =>
    invoke<{ items: DocItem[]; total: number }>('kb_list_documents', params),
  getDocument: (docId: number) => invoke<DocView>('kb_get_document', { docId }),
  deleteDocument: (docId: number) => invoke<void>('kb_delete_document', { docId }),
  renameDocument: (docId: number, title: string) => invoke<void>('kb_rename_document', { docId, title }),
  moveDoc: (docId: number, targetDirId: number | null) => invoke<void>('kb_move_doc', { docId, targetDirId }),
  setDocTags: (docId: number, tags: string[]) => invoke<void>('kb_set_doc_tags', { docId, tags }),
  listTags: (kbId: number) => invoke<{ tag: string; count: number }[]>('kb_list_tags', { kbId }),
  uploadDocument: (args: { input: KbUploadDocInput }) => invoke<UploadDocumentResult>('kb_upload_document', args),
  multimodalAnalyze: (docId: number) => invoke<{ doc_id: number; summary: string; status: string }>('kb_multimodal_analyze', { docId }),
  uploadNewVersion: (args: { input: KbNewVersionInput }) =>
    invoke<{ docId: number; versionId: number; jobId: number; title: string }>('kb_upload_new_version', args),
  batchDownload: (docIds: number[]) => invoke<BatchDownloadResult>('kb_batch_download', { docIds }),
  downloadDocument: (docId: number) => invoke<DownloadDocumentResult>('kb_download_document', { docId }),
  fetchUrl: (args: { input: KbFetchUrlInput }) => invoke<FetchUrlResult>('kb_fetch_url', args),
  batchFetchUrl: (urls: string[], kbId: number, dirId?: number | null) =>
    invoke<{ ok: number; err: number; errors: string[] }>('kb_batch_fetch_url', { urls, kbId, dirId: dirId ?? null }),
  reprocessDocument: (args: {
    docId: number;
    embeddingProvider?: string | null;
    embeddingModel?: string | null;
    chunkStrategy?: string;
    chunkSize?: number;
    chunkOverlap?: number;
  }) => invoke<ReprocessResult>('kb_reprocess_document', args),
  updateChunk: (chunkId: number, content: string) => invoke<UpdateChunkResult>('kb_update_chunk', { chunkId, content }),
  listVersions: (docId: number) => invoke<KbVersion[]>('kb_list_versions', { docId }),
  versionDiff: (docId: number, fromVersionId: number, toVersionId: number) =>
    invoke<{ fromVersionNo: number; toVersionNo: number; added: string[]; removed: string[] } | null>(
      'kb_version_diff',
      { docId, fromVersionId, toVersionId },
    ),
  restoreVersion: (versionId: number) => invoke<void>('kb_restore_version', { versionId }),

  // ── 模型 / 检索 / 问答 ──
  listModels: () => invoke<ModelInfo[]>('kb_list_models'),
  getDefaultModel: () => invoke<[string, string]>('kb_get_default_model'),
  getDefaultChatModel: () => invoke<[string, string]>('kb_get_default_chat_model'),
  getModelSettings: () => invoke<ModelSettingsResult>('kb_get_model_settings'),
  setModelSettings: (role: string, providerId: string, model: string) =>
    invoke<void>('kb_set_model_settings', { role, providerId, model }),
  search: (params: { input: KbSearchInput }) => invoke<RetrievedChunk[]>('kb_search', params),
  searchHistory: (limit: number) => invoke<SearchLogItem[]>('kb_search_history', { limit }),
  recommendQuestions: (kbId: number | null, limit: number) =>
    invoke<RecommendItem[]>('kb_recommend_questions', { kbId, limit }),
  listSessions: () => invoke<QaSessionItem[]>('kb_qa_list_sessions'),
  login: (username?: string, password?: string) =>
    invoke<CurrentUser>('kb_login', { username: username ?? null, password: password ?? null }),
  logout: () => invoke<void>('kb_logout'),
  createSession: (kbId: number | null, title: string) => invoke<number>('kb_qa_create_session', { kbId, title }),
  deleteSession: (sessionId: number) => invoke<void>('kb_qa_delete_session', { sessionId }),
  listMessages: (sessionId: number) => invoke<QaMessageItem[]>('kb_qa_list_messages', { sessionId }),
  /** RAG 流式问答：返回 ragId 用于精准取消 */
  ragStream: (input: KbRagInput, onChunk: (frame: string) => void): Promise<{ ragId?: number } | void> => {
    const channel = new Channel<string>();
    channel.onmessage = (m: string) => onChunk(m);
    return invoke<{ ragId?: number }>('kb_rag_stream', { input, onChunk: channel });
  },
  ragStreamWithChannel: (input: KbRagInput, onChunk: Channel<string>): Promise<{ ragId?: number } | void> =>
    invoke<{ ragId?: number }>('kb_rag_stream', { input, onChunk }),
  /** 请求取消指定 RAG 流式生成（用户点击「停止生成」）；传入 ragId 精准取消，不传则取消最新活跃请求 */
  ragCancel: (ragId?: number) => invoke<void>('kb_rag_cancel', { ragId: ragId ?? null }),
  /** RAG 非流式问答（返回完整答案 + 引用上下文） */
  rag: (input: KbRagInput) => invoke<RagAnswer>('kb_rag', { input }),
  /** 文本高亮：返回命中/未命中分段 */
  highlight: (content: string, query: string) => invoke<HighlightSegment[]>('kb_highlight', { content, query }),

  // ── 分块设置 ──
  getChunkSettings: () => invoke<{ strategy: string; size: number; overlap: number; vectorScanCap?: number }>('kb_get_chunk_settings'),
  setChunkSettings: (args: { strategy: string; size: number; overlap: number; vectorScanCap?: number | null }) => invoke<void>('kb_set_chunk_settings', args),

  // ── 埋点 ──
  trackEvent: (input: { eventType: string; kbId?: number | null; docId?: number | null; pageId?: number | null; sessionId?: number | null; detail?: string | null }) =>
    invoke<void>('kb_track_event', { input }),

  // ── 统计 / 分析 / 任务 ──
  getStats: () => invoke<KbStats>('kb_get_stats'),
  getAnalytics: () => invoke<AnalyticsResult>('kb_get_analytics'),
  getAnalyticsSettings: () => invoke<AnalyticsSetting[]>('kb_get_analytics_settings'),
  setAnalyticsSettings: (input: { key: string; label: string; visible: boolean }) =>
    invoke<void>('kb_set_analytics_settings', { input }),
  getRagSystemPrompt: () => invoke<string>('kb_get_rag_system_prompt'),
  setRagSystemPrompt: (prompt: string) => invoke<void>('kb_set_rag_system_prompt', { prompt }),
  testModel: (providerId: string, model: string, modelType: string) =>
    invoke<{ ok: boolean; providerId: string; model: string; latencyMs: number; note?: string }>('kb_test_model', { providerId, model, modelType }),
  listJobs: (kbId: number | null, limit: number) => invoke<{ items: JobItem[]; total: number; counts?: { pending: number; processing: number; done: number; failed: number } }>('kb_list_jobs', { kbId, limit }),
  getJobLogs: (jobId: number) => invoke<JobLogItem[]>('kb_get_job_logs', { jobId }),
  clearActivity: (scope: 'jobs' | 'logs' | 'history') => invoke<{ jobs?: number; logs?: number; history?: number }>('kb_clear_activity', { scope }),
  stopProcessing: (kbId: number | null) => invoke<{ stopped: number }>('kb_stop_processing', { kbId }),
  retryJob: (jobId: number) => invoke<{ retried: boolean; jobId: number }>('kb_retry_job', { jobId }),
  retryFailedJobs: (kbId: number | null) => invoke<{ retried: number }>('kb_retry_failed_jobs', { kbId }),
  housekeeping: () => invoke<HousekeepingResult>('kb_housekeeping'),

  // ── Wiki ──
  wikiListPages: (kbId: number) => invoke<WikiPageItem[]>('kb_wiki_list_pages', { kbId }),
  wikiSearch: (kbId: number, query: string, limit: number) =>
    invoke<WikiPageItem[]>('kb_wiki_search', { kbId, query, limit }),
  wikiGetPage: (pageId: number) => invoke<WikiPageDetail>('kb_wiki_get_page', { pageId }),
  wikiCreatePage: (input: WikiPageInput) => invoke<number>('kb_wiki_create_page', { input }),
  wikiUpdatePage: (pageId: number, input: WikiPageInput) => invoke<void>('kb_wiki_update_page', { pageId, input }),
  wikiDeletePage: (pageId: number) => invoke<void>('kb_wiki_delete_page', { pageId }),
  wikiExtract: (pageId: number) => invoke<{ submitted: number }>('kb_wiki_extract', { pageId }),
  wikiExtractAll: (kbId: number, force?: boolean) => invoke<{ submitted: number; force: boolean }>('kb_wiki_extract_all', { kbId, force }),
  wikiGenerate: (input: WikiGenerateInput) => invoke<{ submitted: number }>('kb_wiki_generate', { input }),
  wikiGraph: (kbId: number) => invoke<WikiGraph>('kb_wiki_graph', { kbId }),
  wikiDirs: (kbId: number) => invoke<WikiDir[]>('kb_wiki_dirs', { kbId }),
  wikiListVersions: (pageId: number) => invoke<WikiVersionItem[]>('kb_wiki_list_versions', { pageId }),
  wikiRestoreVersion: (pageId: number, versionId: number) => invoke<void>('kb_wiki_restore_version', { pageId, versionId }),

  // ── 用户 / 成员管理 ──
  currentUser: () => invoke<CurrentUser | null>('kb_current_user'),
  listUsers: () => invoke<UserItem[]>('kb_list_users'),
  createUser: (args: { username: string; displayName?: string | null; password: string }) => invoke<number>('kb_create_user', args),
  changePassword: (args: { oldPassword: string; newPassword: string }) => invoke<void>('kb_change_password', args),
  deleteUser: (userId: number) => invoke<void>('kb_delete_user', { userId }),
  resetPassword: (args: { userId: number; newPassword: string }) => invoke<void>('kb_reset_password', args),
  setAdmin: (args: { userId: number; isAdmin: boolean }) => invoke<void>('kb_set_admin', args),
  listRoles: () => invoke<RoleItem[]>('kb_list_roles'),
  listMembers: (kbId: number) => invoke<MemberItem[]>('kb_list_members', { kbId }),
  addMember: (kbId: number, userId: number, role: string) =>
    invoke<void>('kb_add_member', { kbId, userId, role }),
  removeMember: (kbId: number, userId: number) =>
    invoke<void>('kb_remove_member', { kbId, userId }),
  updateMemberRole: (kbId: number, userId: number, role: string) =>
    invoke<void>('kb_update_member_role', { kbId, userId, role }),

  // ── 备份管理 ──
  backup: () => invoke<string>('kb_backup'),
  listBackups: () => invoke<[string, number][]>('kb_list_backups'),
  cleanupBackups: (keep: number) => invoke<number>('kb_cleanup_backups', { keep }),

  // ── 审计日志 ──
  listAuditLogs: (limit: number) => invoke<Record<string, unknown>[]>('kb_list_audit_logs', { limit }),

  // ── FAQ 管理 ──
  faqList: (kbId: number) => invoke<Record<string, unknown>[]>('kb_faq_list', { kbId }),
  faqImport: (kbId: number, entries: Array<{ question: string; answer: string; category?: string }>) =>
    invoke<{ imported: number }>('kb_faq_import', { kbId, entries }),
  faqDelete: (kbId: number, entryId: number) => invoke<void>('kb_faq_delete', { kbId, entryId }),

  // ── ACL 权限管理 ──
  getAcl: (kbId: number, scope?: string, docId?: number, dirId?: number) =>
    invoke<Record<string, unknown>[]>('kb_get_acl', { kbId, scope: scope ?? null, docId: docId ?? null, dirId: dirId ?? null }),
  setAcl: (input: { scope: string; docId?: number; dirId?: number; kbId: number; granteeType: string; userId?: number; roleId?: number; effect: string }) =>
    invoke<void>('kb_set_acl', { input }),
  deleteAcl: (input: { scope: string; docId?: number; dirId?: number; kbId: number; granteeType: string; userId?: number; roleId?: number }) =>
    invoke<void>('kb_acl_delete', { input }),

  // ── 导出 ──
  exportKb: (kbId: number) => invoke<{ dataBase64: string; fileName: string; sizeBytes: number }>('kb_export', { kbId }),
  importKb: (dataBase64: string, newName?: string) => invoke<{ kbId: number; name: string; documents: number; wikiPages: number }>('kb_import', { dataBase64, newName }),
};
