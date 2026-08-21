/* ============================================================
 * 大模型对话 — AI 角色提示词组装纯函数
 * 统一 AiRolesPanel / GlobalChatTab 的重复实现（语义等价）。
 * ============================================================ */
import type { AiRole } from './types';

/** 按角色组装系统提示词：基础提示 → 行为约束 → 背景知识 → 回复语言 */
export function composeSystemPrompt(role: AiRole): string {
  const sections: string[] = [];
  const prompt = (role.system_prompt || '').trim();
  if (prompt) sections.push(prompt);
  const c = (role.behavior_constraints || []).map((s) => s.trim()).filter(Boolean);
  if (c.length) sections.push('【行为约束】\n' + c.map((x) => `- ${x}`).join('\n'));
  const k = (role.knowledge_context || '').trim();
  if (k) sections.push('【背景知识】\n' + k);
  const lang = (role.response_language || '').trim();
  if (lang && lang !== '跟随用户') sections.push(`【回复语言】请使用 ${lang} 回复。`);
  return sections.join('\n\n');
}

/** 规范化：Option<String> 的 null 转为空串，避免输入框显示 "null"（深拷贝，不改原对象） */
export function normalizeRole(r: AiRole): AiRole {
  const c = JSON.parse(JSON.stringify(r)) as AiRole;
  c.preferred_provider_name = c.preferred_provider_name ?? '';
  c.preferred_model = c.preferred_model ?? '';
  c.behavior_constraints = c.behavior_constraints ?? [];
  c.capabilities = c.capabilities ?? [];
  return c;
}

/** 新建空角色（表单默认值） */
export function createEmptyRole(): AiRole {
  return {
    id: '', name: '', emoji: '🤖', description: '',
    enabled: true, system_prompt: '',
    preferred_provider_name: '', preferred_model: '',
    temperature: 0.7, max_tokens: 2048, top_p: 1,
    presence_penalty: 0, frequency_penalty: 0,
    behavior_constraints: [], capabilities: [],
    response_language: '跟随用户', knowledge_context: '',
    created_at: '', updated_at: '',
  };
}
