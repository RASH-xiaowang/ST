// ============================================================
// 大模型管理 — 类型定义
// 外部 API 大模型接入配置、模型管理、全局调用、流量与成本管控
// 所使用的全部数据结构。前后端共享同一套字段约定。
// ============================================================

use serde::{Deserialize, Serialize};

/// 外部模型提供方类型。
/// - openai：OpenAI 及所有 OpenAI 兼容网关（DeepSeek / 通义 / 月之暗面 / vLLM / LM Studio 等）
/// - azure：Azure OpenAI（使用 api-key 请求头）
/// - ollama：本地 Ollama（/v1 兼容接口，通常无需密钥）
/// - custom：自定义兼容 OpenAI 协议的接口
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ProviderType {
    #[default]
    OpenAI,
    Azure,
    Ollama,
    Xiaomi,
    Custom,
}

impl ProviderType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderType::OpenAI => "openai",
            ProviderType::Azure => "azure",
            ProviderType::Ollama => "ollama",
            ProviderType::Xiaomi => "xiaomi",
            ProviderType::Custom => "custom",
        }
    }
}

/// 单个外部模型提供方（大模型接入点）配置
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub provider_type: ProviderType,
    /// API Base URL，例如 https://api.openai.com/v1
    pub base_url: String,
    pub api_key: String,
    pub organization: Option<String>,
    /// Azure OpenAI 的 api-version（如 2024-02-15-preview），仅 azure 类型使用
    #[serde(default)]
    pub azure_api_version: Option<String>,
    /// 该提供方下默认使用的模型 id
    pub default_model: String,
    /// 已知模型 id 列表（手动添加或探测得到）
    pub models: Vec<String>,
    pub enabled: bool,
    /// 输入价格（每 100 万 token，单位 USD）
    pub input_price_per_1m: f64,
    /// 输出价格（每 100 万 token，单位 USD）
    pub output_price_per_1m: f64,
    /// 每月 token 配额上限（None 表示不限制）
    pub monthly_token_limit: Option<u64>,
    /// 每月成本配额上限 USD（None 表示不限制）
    pub monthly_cost_limit: Option<f64>,
    /// 额外的自定义请求头（部分网关需要）
    #[serde(default)]
    pub extra_headers: std::collections::HashMap<String, String>,
    /// 单个模型的能力元数据（类型 / 标签）。键为模型 id。
    #[serde(default)]
    pub model_meta: std::collections::HashMap<String, ModelMeta>,
    /// 部署级默认推理等级（DSH reasoningEffort：off / high / max 等；
    /// 请求时透传 reasoning_effort 参数；空 = 不发送）
    #[serde(default)]
    pub default_reasoning_effort: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        ProviderConfig {
            id: String::new(),
            name: String::new(),
            provider_type: ProviderType::OpenAI,
            base_url: String::new(),
            api_key: String::new(),
            organization: None,
            azure_api_version: None,
            default_model: String::new(),
            models: Vec::new(),
            enabled: true,
            input_price_per_1m: 0.0,
            output_price_per_1m: 0.0,
            monthly_token_limit: None,
            monthly_cost_limit: None,
            extra_headers: std::collections::HashMap::new(),
            created_at: String::new(),
            updated_at: String::new(),
            model_meta: std::collections::HashMap::new(),
            default_reasoning_effort: None,
        }
    }
}

/// 单个模型的元数据（类型 + 标签），用于切换模型时展示其能力
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ModelMeta {
    /// 模型类型（对话 / 生图 / 视频 / 语音 / 嵌入 / 重排序等），单选
    #[serde(default)]
    pub model_type: Option<String>,
    /// 模型能力标签（视觉 / MoE / 推理 / Tools / FIM / Math / Coder 等），可多选
    #[serde(default)]
    pub tags: Vec<String>,
    /// 推理等级选择（DSH reasoningEfforts 迁移）：off / high / max 等。
    /// 空= 未声明（不展示等级选择，请求不带 reasoning_effort）
    #[serde(default)]
    pub reasoning_efforts: Vec<String>,
    /// 上下文窗口（token；DSH contextWindow 迁移；用于上下文仪表容量显示）
    #[serde(default)]
    pub context_window: Option<u64>,

    // ─── 模态 (Modalities) ───
    /// 输入模态：支持的输入类型，可多选
    #[serde(default)]
    pub input_modalities: Vec<String>,
    /// 输出模态：支持的输出类型，可多选
    #[serde(default)]
    pub output_modalities: Vec<String>,

    // ─── 模型能力 (Capabilities) ───
    /// 深度思考：是否支持深度推理
    #[serde(default)]
    pub reasoning: bool,
    /// 工具调用：是否支持 function calling / tool use
    #[serde(default)]
    pub tool_use: bool,
    /// 流式输出：是否支持 streaming
    #[serde(default)]
    pub streaming: bool,
    /// 联网搜索：是否支持 web search
    #[serde(default)]
    pub web_search: bool,
    /// 结构化输出：是否支持 JSON mode / structured output
    #[serde(default)]
    pub structured_output: bool,
    /// Prompt 缓存：是否支持 prompt caching
    #[serde(default)]
    pub prompt_cache: bool,
    /// 多模态分析：是否支持图片/音视频理解（用于知识库文件上传多模态分析）
    #[serde(default)]
    pub multimodal: bool,

    // ─── 性能 (Performance) ───
    /// 最大输出长度（tokens）
    #[serde(default)]
    pub max_output_tokens: Option<u64>,
    /// 每分钟请求数限制 (RPM)
    #[serde(default)]
    pub requests_per_minute: Option<u64>,
    /// 每分钟 Token 数限制 (TPM)
    #[serde(default)]
    pub tokens_per_minute: Option<u64>,
}

/// 全局大模型配置：所有提供方列表 + 默认提供方
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct LlmConfig {
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    pub default_provider_id: Option<String>,
    /// 最后一次全局调用的 提供方 id（重启后自动恢复到该会话）
    #[serde(default)]
    pub last_chat_provider_id: Option<String>,
    /// 最后一次全局调用的 模型 id
    #[serde(default)]
    pub last_chat_model: Option<String>,
    /// 最后一次文本嵌入调用的 提供方 id（与聊天记忆分离，避免互相覆盖）
    #[serde(default)]
    pub last_embedding_provider_id: Option<String>,
    /// 最后一次文本嵌入调用的 模型 id
    #[serde(default)]
    pub last_embedding_model: Option<String>,
}

/// 多模态消息内容片段
/// - text：纯文本片段
/// - image_url：图片（url 为 https 链接或 data URL，含 base64）
/// - file：已上传的文件（name/mime 为元信息，data 为不含 `data:` 前缀的 base64 原文）
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContentPart {
    /// 片段类型："text" | "image_url" | "file"
    #[serde(rename = "type")]
    pub part_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<ImageUrl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    /// 持久化到本地的文件路径（用于上传附件在聊天记录中恢复）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
}

/// image_url 类型片段的 url 包装
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ImageUrl {
    pub url: String,
}

/// 工具调用（OpenAI tool_calls 风格，供代理循环使用）
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ToolCall {
    /// 调用 id（工具结果回传时用于关联）
    pub id: String,
    /// 固定 "function"
    #[serde(rename = "type", default = "default_tool_call_type")]
    pub call_type: String,
    pub function: ToolCallFunction,
}

fn default_tool_call_type() -> String {
    "function".to_string()
}

/// 工具调用的函数名与参数（arguments 为 JSON 字符串）
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

/// 单条对话消息（兼容旧的历史纯文本，新增多模态 parts）
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatMessage {
    pub role: String,
    /// 纯文本内容（无多模态时直接填充；有 parts 时通常为空字符串）
    pub content: String,
    /// 多模态内容片段；为空时仅使用 content
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<Vec<ContentPart>>,
}

/// 图像生成请求（兼容 OpenAI /images/generations）
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ImageGenRequest {
    /// 指定提供方 id；为空时使用全局默认提供方
    #[serde(default)]
    pub provider_id: Option<String>,
    /// 指定图像模型；为空时使用提供方默认模型
    #[serde(default)]
    pub model: Option<String>,
    /// 提示词
    pub prompt: String,
    /// 生成数量，默认 1
    #[serde(default)]
    pub n: Option<u32>,
    /// 尺寸，如 "1024x1024"
    #[serde(default)]
    pub size: Option<String>,
}

/// 图像生成返回
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ImageGenResult {
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
    /// 生成结果（data URL 或 https URL 列表）
    pub urls: Vec<String>,
}

/// 视频生成返回
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VideoGenResult {
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
    /// 生成结果（https URL 列表）
    pub urls: Vec<String>,
}

/// 视频生成请求（兼容 OpenAI /videos/generations 风格）
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VideoGenRequest {
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    pub prompt: String,
    #[serde(default)]
    pub n: Option<u32>,
}

/// 语音合成（文本转语音，TTS）请求（兼容 OpenAI /audio/speech）
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SpeechRequest {
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    /// 待合成文本
    pub input: String,
    /// 音色（如 alloy / echo / nova ...），为空时使用提供方默认
    #[serde(default)]
    pub voice: Option<String>,
    /// 返回音频格式（mp3 / wav / opus / aac / flac），为空时默认为 mp3
    #[serde(default)]
    pub response_format: Option<String>,
    /// 语速倍率（0.5 ~ 2.0，默认 1.0）
    #[serde(default)]
    pub speed: Option<f64>,
}

/// 语音合成结果：音频以 base64 返回，前端直接构造 data URL 播放
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SpeechResult {
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
    /// 音频字节的 base64 编码（不含 `data:` 前缀）
    pub audio_data: String,
    /// 音频格式（mp3 / wav / ogg ...），用于构造 data URL 的 MIME
    pub format: String,
    /// 实际使用的音色
    pub voice: String,
}

/// 文本嵌入请求（兼容 OpenAI /embeddings）
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct EmbeddingRequest {
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    /// 单行文本；若包含换行，则按行拆分为多条输入
    pub input: String,
}

/// 文本嵌入结果
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct EmbeddingResult {
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
    /// 每条输入对应的向量（二维）
    pub embeddings: Vec<Vec<f64>>,
    /// 向量维度
    pub dimensions: usize,
    pub prompt_tokens: u64,
    pub total_tokens: u64,
}

/// 重排序单项结果
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct RerankItem {
    /// 在原始 documents 中的下标
    pub index: u32,
    /// 对应文档文本
    pub document: String,
    /// 相关性得分
    pub score: f64,
}

/// 重排序请求（兼容 Cohere /rerank 风格）
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct RerankRequest {
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    pub query: String,
    pub documents: Vec<String>,
    #[serde(default)]
    pub top_n: Option<u32>,
}

/// 重排序结果
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct RerankResult {
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
    pub results: Vec<RerankItem>,
}

/// 全局调用请求
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatRequest {
    /// 指定提供方 id；为空时使用全局默认提供方
    pub provider_id: Option<String>,
    /// 指定模型；为空时使用提供方默认模型
    pub model: Option<String>,
    /// 关联的 AI 角色 ID（来自 Agent 模块的「AI 角色定位」）。
    /// 后端会根据角色配置自动注入对应的系统提示词。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_id: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    /// top_p（AI 角色可覆盖）
    #[serde(default)]
    pub top_p: Option<f32>,
    /// 存在惩罚（AI 角色可覆盖）
    #[serde(default)]
    pub presence_penalty: Option<f32>,
    /// 频率惩罚（AI 角色可覆盖）
    #[serde(default)]
    pub frequency_penalty: Option<f32>,
}

/// 全局调用返回
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatResult {
    pub content: String,
    pub model: String,
    pub provider_id: String,
    pub provider_name: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    /// 本次调用估算成本（USD）
    pub cost: f64,
}

/// 连接测试结果
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TestResult {
    pub ok: bool,
    pub latency_ms: u128,
    pub model: Option<String>,
    pub error: Option<String>,
}

/// 单个提供方、单月的用量统计
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ProviderUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub cost: f64,
    pub call_count: u64,
}

/// 流量与成本统计：按 "YYYY-MM" 月份、提供方 id 两层聚合
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct LlmUsage {
    pub months: std::collections::HashMap<String, std::collections::HashMap<String, ProviderUsage>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_type_serde_roundtrip_and_as_str() {
        // lowercase rename 兼容：枚举 ↔ 字符串往返 + as_str 映射
        assert_eq!(ProviderType::OpenAI.as_str(), "openai");
        assert_eq!(ProviderType::Azure.as_str(), "azure");
        assert_eq!(ProviderType::Ollama.as_str(), "ollama");
        assert_eq!(ProviderType::Custom.as_str(), "custom");
        for t in [
            ProviderType::OpenAI,
            ProviderType::Azure,
            ProviderType::Ollama,
            ProviderType::Custom,
        ] {
            let json = serde_json::to_string(&t).unwrap();
            let back: ProviderType = serde_json::from_str(&json).unwrap();
            assert_eq!(t, back, "serde 往返应一致: {json}");
        }
        // 默认值 OpenAI
        assert_eq!(ProviderType::default(), ProviderType::OpenAI);
    }

    #[test]
    fn provider_config_default_and_model_meta_shape() {
        // 配置默认值：空提供方；ModelMeta 字段形状（B15）
        let p = ProviderConfig::default();
        assert!(p.id.is_empty());
        assert_eq!(p.provider_type, ProviderType::OpenAI);
        let meta = ModelMeta::default();
        assert!(meta.reasoning_efforts.is_empty());
        assert!(meta.context_window.is_none());
    }
}
