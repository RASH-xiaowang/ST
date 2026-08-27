// 大模型管理 — Tauri IPC 封装层
// 所有与后端 llm 模块的交互都通过本文件，避免散落的 invoke 调用。
import { invoke, Channel } from "@tauri-apps/api/core";
import type {
  LlmConfig,
  ProviderConfig,
  TestResult,
  ChatRequest,
  ChatResult,
  ChatChunk,
  ChatMessage,
  LlmUsage,
  UsageSummaryItem,
  ProviderType,
  ImageGenResult,
  VideoGenResult,
  SpeechRequest,
  SpeechResult,
  EmbeddingRequest,
  EmbeddingResult,
  RerankRequest,
  RerankResult,
  AgentStreamEvent,
  AgentToolInfo,
  AgentPlugin,
  AiRole,
} from "../types";

export const llmApi = {
  // ─── 配置 ───
  getConfig: () => invoke<LlmConfig>("get_llm_config"),
  getConfigPath: () => invoke<string>("get_llm_config_path"),
  upsertProvider: (provider: ProviderConfig) =>
    invoke<ProviderConfig>("upsert_llm_provider", { provider }),
  deleteProvider: (id: string) => invoke<void>("delete_llm_provider", { id }),
  setDefaultProvider: (id: string) =>
    invoke<void>("set_llm_default_provider", { id }),

  // ─── 连接测试 ───
  testConnection: (id: string) => invoke<TestResult>("test_llm_connection", { id }),

  // ─── 图像生成 ───
  generateImage: (req: {
    provider_id?: string | null;
    model?: string | null;
    prompt: string;
    n?: number;
    size?: string;
  }) => invoke<ImageGenResult>("generate_image", { request: req }),

  // ─── 视频生成 ───
  generateVideo: (req: {
    provider_id?: string | null;
    model?: string | null;
    prompt: string;
    n?: number;
  }) => invoke<VideoGenResult>("generate_video", { request: req }),

  // ─── 语音合成（TTS） ───
  generateSpeech: (req: SpeechRequest) =>
    invoke<SpeechResult>("create_speech", { request: req }),

  /** 语音对话转写（STT）：本地 Whisper 优先，云端 /audio/transcriptions 兜底 */
  transcribeVoiceAudio: (audio: Uint8Array, ext?: string) =>
    invoke<{ text: string; engine: string }>("transcribe_voice_audio", {
      audio,
      ext: ext ?? "wav",
    }),

  /** 系统离线语音合成（Windows SAPI）：返回 base64 WAV，零配置兜底。
   *  rate 为 SAPI 语速（-10 ~ 10，负值更慢；缺省 -2 更接近自然语速） */
  synthesizeNativeSpeech: (text: string, rate?: number) =>
    invoke<SpeechResult>("synthesize_native_speech", { text, rate: rate ?? -2 }),

  // ─── AI 角色（跨模块外部调用接口，读取 Agent 模块定义的角色）───
  getAiRoles: (): Promise<AiRole[]> => {
    try {
      return invoke<AiRole[]>("get_ai_roles");
    } catch {
      return Promise.resolve([]);
    }
  },
  getAiRole: (id: string): Promise<AiRole | null> =>
    invoke<AiRole | null>("get_ai_role", { id }),
  saveAiRole: (role: AiRole): Promise<AiRole> =>
    invoke<AiRole>("save_ai_role", { role }),
  deleteAiRole: (id: string): Promise<boolean> =>
    invoke<boolean>("delete_ai_role", { id }),

  // ─── 文本嵌入 ───
  generateEmbedding: (req: EmbeddingRequest) =>
    invoke<EmbeddingResult>("create_embedding", { request: req }),

  // ─── 重排序 ───
  rerank: (req: RerankRequest) =>
    invoke<RerankResult>("rerank", { request: req }),

  // ─── 模型管理 ───
  listModels: (id: string) => invoke<string[]>("list_llm_models", { id }),
  addModel: (id: string, model: string) =>
    invoke<ProviderConfig>("add_llm_model", { id, model }),
  removeModel: (id: string, model: string) =>
    invoke<ProviderConfig>("remove_llm_model", { id, model }),
  removeModels: (id: string, models: string[]) =>
    invoke<ProviderConfig>("remove_llm_models", { id, models }),
  setDefaultModel: (id: string, model: string) =>
    invoke<ProviderConfig>("set_llm_default_model", { id, model }),

  // ─── 全局调用 ───
  chat: (request: ChatRequest) => invoke<ChatResult>("chat_with_llm", { request }),
  chatStream: (
    request: ChatRequest,
    onChunk: (chunk: ChatChunk) => void,
  ): Promise<void> => {
    const channel = new Channel<string>();
    channel.onmessage = (msg: string) => {
      try {
        onChunk(JSON.parse(msg) as ChatChunk);
      } catch {
        /* 忽略无法解析的帧 */
      }
    };
    return invoke<void>("chat_with_llm_stream", { request, onChunk: channel });
  },

  // ─── 代理模式（工具调用，DeepSeek Harness 能力迁移） ───
  chatAgentStream: (
    request: ChatRequest,
    onEvent: (ev: AgentStreamEvent) => void,
  ): Promise<void> => {
    const channel = new Channel<string>();
    channel.onmessage = (msg: string) => {
      try {
        onEvent(JSON.parse(msg) as AgentStreamEvent);
      } catch {
        /* 忽略无法解析的帧 */
      }
    };
    return invoke<void>("chat_agent_stream", { request, onChunk: channel });
  },
  getAgentTools: () => invoke<AgentToolInfo[]>("get_agent_tools"),
  approveAgentTool: (id: string) => invoke<boolean>("approve_agent_tool", { id }),
  rejectAgentTool: (id: string) => invoke<boolean>("reject_agent_tool", { id }),
  /** 会话内记住批准：同一 (提供方, 模型, 工具) 有效期内不再弹审批 */
  trustAgentTool: (providerId: string, model: string, tool: string) =>
    invoke<void>("trust_agent_tool", { providerId, model, tool }),
  /** 清空某会话的信任记录（清空对话时调用） */
  clearAgentTrust: (providerId: string, model: string) =>
    invoke<void>("clear_agent_trust", { providerId, model }),
  /** 保存某条助手消息（按助手序号）的工具调用步骤 */
  saveAgentToolSteps: (
    providerId: string,
    model: string,
    assistantIdx: number,
    steps: unknown[],
  ) =>
    invoke<void>("save_agent_tool_steps", {
      providerId,
      model,
      assistantIdx,
      steps,
    }),
  /** 读取某会话全部工具调用步骤 */
  getAgentToolSteps: (providerId: string, model: string) =>
    invoke<Array<[number, unknown[]]>>("get_agent_tool_steps", {
      providerId,
      model,
    }),

  // ─── 动态插件（DSH 插件模型迁移） ───
  listAgentPlugins: () => invoke<AgentPlugin[]>("list_agent_plugins"),
  saveAgentPlugin: (plugin: AgentPlugin) =>
    invoke<AgentPlugin>("save_agent_plugin", { plugin }),
  deleteAgentPlugin: (id: string) => invoke<void>("delete_agent_plugin", { id }),
  setAgentPluginEnabled: (id: string, enabled: boolean) =>
    invoke<AgentPlugin>("set_agent_plugin_enabled", { id, enabled }),
  submitAgentToolResult: (id: string, ok: boolean, result: string) =>
    invoke<boolean>("submit_agent_tool_result", { id, ok, result }),
  getChatHistory: (providerId: string, model: string) =>
    invoke<ChatMessage[]>("get_llm_chat_history", { providerId, model }),

  /** 记住最后一次聊天的 提供方/模型，重启后恢复会话 */
  setLastChat: (providerId: string, model: string) =>
    invoke<void>("set_last_chat", { providerId, model }),

  /** 设置单个模型的能力元数据（类型 / 标签），便于切换模型时识别能力 */
  setModelMeta: (
    providerId: string, 
    model: string, 
    modelType: string | null, 
    tags: string[],
    inputModalities: string[] = [],
    outputModalities: string[] = [],
    reasoning: boolean = false,
    toolUse: boolean = false,
    streaming: boolean = false,
    webSearch: boolean = false,
    structuredOutput: boolean = false,
    promptCache: boolean = false,
    multimodal: boolean = false,
    maxOutputTokens: number | null = null,
    requestsPerMinute: number | null = null,
    tokensPerMinuteUnit: string = "1000",
    tokensPerMinute: number | null = null,
    contextWindow: number | null = null,
  ) =>
    invoke<ProviderConfig>("set_llm_model_meta", {
      id: providerId,
      model,
      modelType,
      tags,
      inputModalities: inputModalities,
      outputModalities: outputModalities,
      reasoning,
      toolUse: toolUse,
      streaming,
      webSearch: webSearch,
      structuredOutput: structuredOutput,
      promptCache: promptCache,
      multimodal: multimodal,
      maxOutputTokens: maxOutputTokens,
      requestsPerMinute: requestsPerMinute,
      tokensPerMinute: tokensPerMinute !== null ? tokensPerMinute * Number(tokensPerMinuteUnit) : null,
      contextWindow: contextWindow,
    }),
  appendChatMessages: (providerId: string, model: string, messages: ChatMessage[]) =>
    invoke<void>("append_llm_chat_messages", { providerId, model, messages }),
  clearChatHistory: (providerId: string, model: string) =>
    invoke<number>("clear_llm_chat_history", { providerId, model }),

  // ─── 附件持久化 ───
  saveUploadedFile: (fileName: string, fileData: Uint8Array) =>
    invoke<string>("save_uploaded_file", { fileName, fileData }),

  // 从图片地址（远程 URL 或 data: URL）下载并保存到资源目录，返回绝对路径
  saveResourceFromUrl: (url: string, fileName?: string) =>
    invoke<string>("save_resource_from_url", { url, fileName }),

  // ─── 流量与成本 ───
  getUsage: () => invoke<LlmUsage>("get_llm_usage"),
  resetUsage: () => invoke<void>("reset_llm_usage"),
  getUsageSummary: () => invoke<UsageSummaryItem[]>("get_llm_usage_summary"),
  getProviderTypes: () => invoke<ProviderType[]>("get_llm_provider_types"),
};
