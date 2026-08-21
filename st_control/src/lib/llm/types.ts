// 大模型管理 — 前端类型定义（与后端 src/tauri/src/llm/types.rs 保持一致）

export type ProviderType = "openai" | "azure" | "ollama" | "custom";

export const PROVIDER_TYPE_LABELS: Record<ProviderType, string> = {
  openai: "OpenAI 兼容",
  azure: "Azure OpenAI",
  ollama: "Ollama 本地",
  custom: "自定义兼容",
};

export interface ProviderConfig {
  id: string;
  name: string;
  provider_type: ProviderType;
  base_url: string;
  api_key: string;
  organization?: string;
  /** Azure OpenAI 的 api-version（如 2024-02-15-preview），仅 azure 类型使用 */
  azure_api_version?: string;
  default_model: string;
  models: string[];
  enabled: boolean;
  /** 输入价格：每 100 万 token（USD） */
  input_price_per_1m: number;
  /** 输出价格：每 100 万 token（USD） */
  output_price_per_1m: number;
  /** 每月 token 配额上限（null 表示不限制） */
  monthly_token_limit: number | null;
  /** 每月成本配额上限 USD（null 表示不限制） */
  monthly_cost_limit: number | null;
  extra_headers: Record<string, string>;
  /** 单个模型的能力元数据（类型 / 标签）。键为模型 id。 */
  model_meta?: Record<string, ModelMeta>;
  created_at: string;
  updated_at: string;
}

export interface LlmConfig {
  providers: ProviderConfig[];
  default_provider_id: string | null;
  /** 最后一次全局调用的 提供方 id（重启后恢复会话用） */
  last_chat_provider_id?: string | null;
  /** 最后一次全局调用的 模型 id */
  last_chat_model?: string | null;
}

export type ChatRole = "system" | "user" | "assistant";

/** 多模态内容片段 */
export interface ContentPart {
  type: "text" | "image_url" | "file";
  text?: string;
  image_url?: { url: string };
  name?: string;
  mime?: string;
  /** 不含 `data:` 前缀的 base64 原文（用于二进制文件透传） */
  data?: string;
  /** 持久化到本地的附件路径（用于聊天记录中恢复显示） */
  file_path?: string;
}

/** 图表规范（LLM 输出 JSON：柱状/折线/饼图，字段宽松可选） */
export interface ChartSpec {
  type?: "bar" | "line" | "pie";
  title?: string;
  labels?: unknown[];
  series?: Array<{ name?: unknown; data?: unknown[] }>;
  data?: Array<{ label?: unknown; name?: unknown; x?: unknown; value?: unknown; y?: unknown }>;
  [key: string]: unknown;
}

export interface ChatMessage {
  role: ChatRole;
  content: string;
  /** 多模态片段；存在时优先以其构造请求体 */
  parts?: ContentPart[];
  /** 该条助手消息关联的工具调用步骤（代理模式，随对话持久化） */
  tool_steps?: AgentStepRecord[];
}

export interface ChatRequest {
  provider_id?: string | null;
  model?: string | null;
  /** 关联的 AI 角色 ID（由 Agent 模块定义），后端会据此注入系统提示词 */
  role_id?: string | null;
  messages: ChatMessage[];
  max_tokens?: number | null;
  temperature?: number | null;
  top_p?: number | null;
  presence_penalty?: number | null;
  frequency_penalty?: number | null;
}

// ─── 代理模式（工具调用，DeepSeek Harness 能力迁移） ───

/** 工具目录条目（get_agent_tools 返回） */
export interface AgentToolInfo {
  name: string;
  description: string;
  requires_approval: boolean;
}

// ─── 动态插件（DSH 插件模型迁移） ───

/** 插件工具定义（JavaScript 实现，前端 WebView 执行） */
export interface PluginToolDef {
  name: string;
  description: string;
  parameters?: Record<string, unknown>;
  requires_approval?: boolean;
  /** JS 函数体：async function(args, ctx) { ... }；ctx 提供 fetch/log */
  code: string;
}

/** 版本记录（不可变历史） */
export interface PluginVersion {
  version: number;
  saved_at: string;
}

/** 动态插件 */
export interface AgentPlugin {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  tools: PluginToolDef[];
  versions: PluginVersion[];
  created_at: string;
  updated_at: string;
}

/** 工具调用步骤记录（随对话持久化的形态；字段与 tool_done 事件对齐） */
export interface AgentStepRecord {
  id: string;
  name: string;
  args: string;
  status: "running" | "ok" | "err";
  result?: string;
  duration_ms?: number;
  /** 是否为用户手动重试（仅查看，不回传模型） */
  retried?: boolean;
}

/** 代理流式事件（chat_agent_stream 通过 Channel 推送） */
export type AgentStreamEvent =
  | { type: "tool_start"; id: string; name: string; arguments: string }
  | {
      type: "tool_done";
      id: string;
      name: string;
      ok: boolean;
      result: string;
      duration_ms: number;
    }
  | { type: "delta"; content: string }
  | {
      type: "done";
      content: string;
      model: string;
      prompt_tokens: number;
      completion_tokens: number;
      total_tokens: number;
      cost: number;
    }
  | { type: "error"; message: string };

// ============================================================
// AI 角色（跨模块，由 Agent 模块定义，经「全局调用」检索复用）
// 字段与 st_agent/src-tauri/src/role_store.rs 的 AiRole 保持一致。
// ============================================================
export interface AiRole {
  id: string;
  name: string;
  emoji: string;
  description: string;
  enabled: boolean;
  system_prompt: string;
  preferred_provider_name?: string | null;
  preferred_model?: string | null;
  temperature: number;
  max_tokens: number;
  top_p: number;
  presence_penalty: number;
  frequency_penalty: number;
  behavior_constraints: string[];
  capabilities: string[];
  response_language: string;
  knowledge_context: string;
  created_at: string;
  updated_at: string;
}

/** 图像生成结果 */
export interface ImageGenResult {
  provider_id: string;
  provider_name: string;
  model: string;
  urls: string[];
}

/** 视频生成结果 */
export interface VideoGenResult {
  provider_id: string;
  provider_name: string;
  model: string;
  urls: string[];
}

/** 语音合成（TTS）请求 */
export interface SpeechRequest {
  provider_id?: string | null;
  model?: string | null;
  /** 待合成文本 */
  input: string;
  /** 音色（如 alloy / echo / nova ...） */
  voice?: string | null;
  /** 返回音频格式（mp3 / wav / opus / aac / flac） */
  response_format?: string | null;
  /** 语速倍率（0.5 ~ 2.0，默认 1.0） */
  speed?: number | null;
}

/** 语音合成结果：音频以 base64 返回 */
export interface SpeechResult {
  provider_id: string;
  provider_name: string;
  model: string;
  /** 音频字节的 base64 编码（不含 `data:` 前缀） */
  audio_data: string;
  /** 音频格式（mp3 / wav / ogg ...） */
  format: string;
  /** 实际使用的音色 */
  voice: string;
}

/** 文本嵌入请求 */
export interface EmbeddingRequest {
  provider_id?: string | null;
  model?: string | null;
  /** 单行文本；含换行时按行拆分为多条输入 */
  input: string;
}

/** 文本嵌入结果 */
export interface EmbeddingResult {
  provider_id: string;
  provider_name: string;
  model: string;
  /** 每条输入对应的向量（二维） */
  embeddings: number[][];
  /** 向量维度 */
  dimensions: number;
  prompt_tokens: number;
  total_tokens: number;
}

/** 重排序单项结果 */
export interface RerankItem {
  /** 在原始 documents 中的下标 */
  index: number;
  /** 对应文档文本 */
  document: string;
  /** 相关性得分 */
  score: number;
}

/** 重排序请求 */
export interface RerankRequest {
  provider_id?: string | null;
  model?: string | null;
  query: string;
  documents: string[];
  top_n?: number | null;
}

/** 重排序结果 */
export interface RerankResult {
  provider_id: string;
  provider_name: string;
  model: string;
  results: RerankItem[];
}

/** 模型类型列表（用于全局调用路由） */
export type LlmModelType = "对话" | "生图" | "视频" | "语音" | "嵌入" | "重排序";

export interface ChatResult {
  content: string;
  model: string;
  provider_id: string;
  provider_name: string;
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  /** 本次调用估算成本（USD） */
  cost: number;
}

/** 单个模型的元数据（类型 + 标签），用于切换模型时展示其能力 */
export interface ModelMeta {
  /** 模型类型（对话 / 生图 / 视频 / 语音 / 嵌入 / 重排序 等），单选 */
  model_type?: string | null;
  /** 模型能力标签（视觉 / MoE / 推理 / Tools / FIM / Math / Coder 等），可多选 */
  tags?: string[];
  /** 推理等级选择（DSH reasoningEfforts：off / high / max 等；空 = 未声明） */
  reasoning_efforts?: string[];
  /** 上下文窗口（token；上下文仪表容量显示） */
  context_window?: number | null;
}

/** 流式响应的单帧 */
export type ChatChunk =
  | { type: "delta"; content: string }
  | {
      type: "done";
      content: string;
      model: string;
      prompt_tokens: number;
      completion_tokens: number;
      total_tokens: number;
      cost: number;
    }
  | { type: "error"; message: string };

export interface TestResult {
  ok: boolean;
  latency_ms: number;
  model: string | null;
  error: string | null;
}

export interface ProviderUsage {
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  cost: number;
  call_count: number;
}

export interface LlmUsage {
  // "YYYY-MM" -> (providerId -> ProviderUsage)
  months: Record<string, Record<string, ProviderUsage>>;
}

export interface UsageSummaryItem {
  id: string;
  name: string;
  enabled: boolean;
  usage: ProviderUsage;
  monthly_token_limit: number | null;
  monthly_cost_limit: number | null;
  token_ratio: number;
  cost_ratio: number;
}
