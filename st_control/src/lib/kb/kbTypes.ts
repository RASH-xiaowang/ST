// 知识库模块共享类型（抽离以避免组件间循环依赖导致白屏）
export interface KbSummary { id: number; name: string; description: string | null; owner_id: number | null; pinned: boolean; isSystem: boolean; docCount: number; created_at: string; }
export interface CurrentUser { id: number; username: string; displayName: string | null; isAdmin: boolean; }
export interface DirNode { id: number; kb_id: number; parent_id: number | null; name: string; depth: number; children: DirNode[]; }
export interface DocItem { id: number; title: string; fileType: string | null; status: string; processStatus: string | null; createdAt: string; updatedAt?: string; fileSize?: number | null; source?: string | null; tags: string[]; snippet?: string; }
export interface RetrievedChunk { chunk_id: number; doc_id: number; kb_id: number; content: string; page_no: number | null; section: string | null; score: number; source: string; doc_title: string; }
export interface RagContextItem { chunk_id: number; doc_id: number; kb_id: number; content: string; score: number; doc_title: string; section?: string | null; page_no?: number | null; }
export interface RagAnswer { answer: string; context: RagContextItem[]; model: string; provider: string; }
export interface HighlightSegment { text: string; hit: boolean; }
export interface ModelInfo { providerId: string; providerName: string; model: string; isDefault: boolean; modelType: string | null; }
export interface KbVersion { id: number; versionNo: number; note: string | null; createdBy: number; createdAt: string; }
export interface KbAclEntry { id: number; scope: string; docId: number | null; dirId: number | null; kbId: number | null; granteeType: string; userId: number | null; roleId: number | null; effect: string; createdBy: number; createdAt: string; }
export interface DocView { meta: { id: number; kbId: number; title: string; fileType: string | null; status: string; processStatus: string | null; createdAt: string; updatedAt: string }; content: string | null; chunks: Array<{ id: number; seq: number; content: string; tokens: number }>; }
export interface UploadTask { file: File; status: 'pending' | 'uploading' | 'done' | 'error'; msg: string; }

/** 文档上传结果（kb_upload_document 返回） */
export interface UploadDocumentResult {
  duplicateDocId?: number | null;
  duplicateTitle?: string | null;
  [key: string]: unknown;
}

/** 文档下载结果（kb_download_document 返回） */
export interface DownloadDocumentResult {
  dataBase64: string;
  fileName?: string;
  [key: string]: unknown;
}

/** 批量下载结果（kb_batch_download 返回） */
export interface BatchDownloadResult {
  dataBase64: string;
  fileName: string;
  count: number;
}

/** 分块更新结果（kb_update_chunk 返回） */
export interface UpdateChunkResult {
  chunkId: number;
  docId: number;
  embedded: number;
  content: string;
  warning?: string;
  [key: string]: unknown;
}

/** 文档重新处理结果（kb_reprocess_document 返回） */
export interface ReprocessResult {
  chunkCount: number;
  embedded: number;
  [key: string]: unknown;
}

/** 埋点统计结果（kb_get_analytics 返回） */
export interface AnalyticsResult {
  metrics: AnalyticsMetric[];
}

/** 卡死任务清理结果（kb_housekeeping 返回） */
export interface HousekeepingResult {
  jobs: number;
  docs: number;
}

/** 模型引用（providerId + model） */
export interface ModelRef {
  providerId: string;
  model: string;
}

/** 模型设置（kb_get_model_settings 返回） */
export interface ModelSettingsResult {
  inference?: ModelRef;
  parsing?: ModelRef;
  embedding?: ModelRef;
  rerank?: ModelRef;
  [key: string]: unknown;
}


/** 网页抓取结果（kb_fetch_url 返回） */
export interface FetchUrlResult {
  title?: string;
  [key: string]: unknown;
}
export interface UserItem { id: number; username: string; displayName: string | null; isAdmin: boolean; }
export interface RoleItem { id: number; name: string; description: string | null; }
export interface MemberItem { userId: number; username: string; displayName: string | null; role: string; }
export interface QaSessionItem { id: number; kbId: number | null; title: string | null; createdAt: string; updatedAt: string; }
export interface QaMessageItem { id: number; role: string; content: string | null; citations: string | null; createdAt: string; }
export interface SearchLogItem { id: number; kbId: number | null; query: string; mode: string; hitCount: number; createdAt: string; }
export interface JobItem { id: number; docId: number; docTitle: string; stage: string; progress: number; error: string | null; createdAt: string; updatedAt: string; }
export interface JobLogItem { id: number; level: string; message: string; detail: string | null; createdAt: string; }
// 全局统计（kb_get_stats，字段为 snake_case 与后端一致）
export interface KbStats {
  kb_count: number; doc_count: number; chunk_count: number; wiki_page_count: number;
  storage_bytes: number; storage_quota: number;
  doc_ready: number; doc_processing: number; doc_failed: number;
  job_pending: number; job_done: number; job_failed: number;
}

// ─── Wiki 页面（知识库 Wiki 模式）───
export interface WikiPageItem {
  id: number;
  kbId: number;
  dirId: number | null;
  docId: number | null;
  docTitle: string | null;
  title: string;
  slug: string;
  summary: string;
  status: string;
  outLinks: number;
  inLinks: number;
  entityCount: number;
  createdAt: string;
  updatedAt: string;
}
export interface WikiLinkInfo { pageId: number; title: string; slug: string; linkType: string; weight: number; snippet: string | null; }
export interface WikiEntity { id: number; name: string; entityType: string; description: string | null; }
export interface WikiPageDetail {
  id: number;
  kbId: number;
  docId: number | null;
  docTitle: string | null;
  title: string;
  slug: string;
  summary: string;
  contentMd: string;
  status: string;
  createdBy: number | null;
  createdAt: string;
  updatedAt: string;
  outLinks: WikiLinkInfo[];
  inLinks: WikiLinkInfo[];
  unresolved: string[];
  unlinkedMentions: WikiLinkInfo[];
  entities: WikiEntity[];
  extractStatus: string;
}
export interface WikiGraphNode { id: number; pageId: number; title: string; docId: number | null; docTitle: string | null; dirName: string | null; inDegree: number; outDegree: number; status: string; }
export interface WikiGraphEdge { from: number; to: number; linkType: string; weight: number; }
export interface WikiGraph { nodes: WikiGraphNode[]; edges: WikiGraphEdge[]; }
export interface WikiPageInput { kbId: number; docId?: number | null; title: string; summary?: string | null; contentMd?: string | null; }
export interface WikiGenerateInput { kbId: number; docId?: number | null; providerId?: string | null; model?: string | null; }

// ─── Wiki 目录（kb_wiki_dirs 返回）───
export interface WikiDir {
  id: number;
  parentId: number | null;
  name: string;
  count: number;
}

/** 目录树列表项（扁平目录前序展开后的行，depth 为层级深度） */
export interface WikiDirTreeItem {
  id: number;
  name: string;
  count: number;
  depth: number;
}

/** Wiki 页面版本记录 */
export interface WikiVersionItem {
  id: number;
  versionNo: number;
  title: string;
  summary: string;
  contentMd: string;
  note: string | null;
  createdAt: string;
}

// ─── 指标统计（埋点）───
export interface AnalyticsMetric {
  key: string;
  value: string;
  today: number;
  daily: string;
  yearly: string;
  series: Array<{ date: string; value: number }>;
  label?: string;
  visible?: boolean;
}
export interface AnalyticsSetting {
  key: string;
  label: string;
  visible: boolean;
}
export interface RecommendItem {
  type: 'faq' | 'query';
  question: string;
}
