/**
 * AI 角色定义（前端类型）。
 * 字段与 st_agent 后端 role_store.rs 中的 AiRole 结构体一一对应，
 * 外部调用接口（大模型管理「全局调用」）也基于同一份共享 JSON。
 */
export interface AiRole {
  /** 角色唯一 ID（为空时由后端自动生成） */
  id: string;
  /** 角色名称 */
  name: string;
  /** 头像（emoji 或短文本） */
  emoji: string;
  /** 角色简介 */
  description: string;
  /** 是否启用（禁用后全局调用不可检索到） */
  enabled: boolean;
  /** 系统提示词（核心，对标 system prompt） */
  system_prompt: string;
  /** 偏好提供方名称（可选） */
  preferred_provider_name?: string | null;
  /** 偏好模型（可选） */
  preferred_model?: string | null;
  /** 温度 */
  temperature: number;
  /** 单次最大生成 token */
  max_tokens: number;
  /** top_p */
  top_p: number;
  /** 存在惩罚 */
  presence_penalty: number;
  /** 频率惩罚 */
  frequency_penalty: number;
  /** 行为约束 */
  behavior_constraints: string[];
  /** 能力标签 */
  capabilities: string[];
  /** 回复语言约束（如：中文 / English / 跟随用户） */
  response_language: string;
  /** 背景知识 / 上下文注入 */
  knowledge_context: string;
  /** 创建时间（RFC3339） */
  created_at: string;
  /** 更新时间（RFC3339） */
  updated_at: string;
}
