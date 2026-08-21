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
  RecommendItem,
  ReprocessResult,
  RetrievedChunk,
  SearchLogItem,
  WikiDir,
  WikiGenerateInput,
  WikiGraph,
  WikiPageDetail,
  WikiPageInput,
  WikiPageItem,
  UploadDocumentResult,
  UpdateChunkResult,
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

/** 上传文档输入（data 为原始文件字节数组） */
export type KbUploadDocInput = {
  kbId: number;
  dirId?: number | null;
  title: string;
  fileType: string;
  data: number[];
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
  data: number[];
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
  // ── 知识库 / 目录 / 文档 ──
  list: (userId: number) => invoke<KbSummary[]>('kb_list', { userId }),
  create: (name: string, description: string | null) => invoke<KbSummary>('kb_create', { name, description }),
  update: (kbId: number, name: string, description: string | null) =>
    invoke<void>('kb_update', { kbId, name, description }),
  remove: (kbId: number) => invoke<void>('kb_delete', { kbId }),
  setPin: (kbId: number, pinned: boolean) => invoke<void>('kb_set_pin', { kbId, pinned }),
  listDirs: (kbId: number) => invoke<DirNode[]>('kb_list_dirs', { kbId }),
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
  uploadNewVersion: (args: { input: KbNewVersionInput }) =>
    invoke<{ docId: number; versionId: number; jobId: number; title: string }>('kb_upload_new_version', args),
  batchDownload: (docIds: number[]) => invoke<BatchDownloadResult>('kb_batch_download', { docIds }),
  downloadDocument: (docId: number) => invoke<DownloadDocumentResult>('kb_download_document', { docId }),
  fetchUrl: (args: { input: KbFetchUrlInput }) => invoke<FetchUrlResult>('kb_fetch_url', args),
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
  createSession: (kbId: number | null, title: string) => invoke<number>('kb_qa_create_session', { kbId, title }),
  deleteSession: (sessionId: number) => invoke<void>('kb_qa_delete_session', { sessionId }),
  listMessages: (sessionId: number) => invoke<QaMessageItem[]>('kb_qa_list_messages', { sessionId }),
  ragStream: (input: KbRagInput, onChunk: (frame: string) => void): Promise<void> => {
    const channel = new Channel<string>();
    channel.onmessage = (m: string) => onChunk(m);
    return invoke<void>('kb_rag_stream', { input, onChunk: channel });
  },
  ragStreamWithChannel: (input: KbRagInput, onChunk: Channel<string>): Promise<void> =>
    invoke<void>('kb_rag_stream', { input, onChunk }),

  // ── 统计 / 分析 / 任务 ──
  getStats: () => invoke<KbStats>('kb_get_stats'),
  getAnalytics: () => invoke<AnalyticsResult>('kb_get_analytics'),
  getAnalyticsSettings: () => invoke<AnalyticsSetting[]>('kb_get_analytics_settings'),
  setAnalyticsSettings: (input: { key: string; label: string; visible: boolean }) =>
    invoke<void>('kb_set_analytics_settings', { input }),
  listJobs: (kbId: number | null, limit: number) => invoke<JobItem[]>('kb_list_jobs', { kbId, limit }),
  getJobLogs: (jobId: number) => invoke<JobLogItem[]>('kb_get_job_logs', { jobId }),
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
  wikiExtractAll: (kbId: number) => invoke<{ submitted: number }>('kb_wiki_extract_all', { kbId }),
  wikiGenerate: (input: WikiGenerateInput) => invoke<{ submitted: number }>('kb_wiki_generate', { input }),
  wikiGraph: (kbId: number) => invoke<WikiGraph>('kb_wiki_graph', { kbId }),
  wikiDirs: (kbId: number) => invoke<WikiDir[]>('kb_wiki_dirs', { kbId }),
};
