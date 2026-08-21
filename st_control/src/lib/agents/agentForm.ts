/* ============================================================
 * 智能体 — 表单数据工厂
 * 自 AgentPanel.svelte 下沉：空白表单与 AgentItem → 表单映射。
 * ============================================================ */

/** 智能体表单数据（创建/编辑共用） */
export interface AgentInput {
  name: string;
  description?: string | null;
  roleId?: string | null;
  providerId?: string | null;
  model?: string | null;
  kbId?: number | null;
  temperature?: number;
  maxTokens?: number;
  topP?: number;
}

/** 新建智能体的空白表单 */
export function createBlankAgentForm(): AgentInput {
  return { name: '', description: '', roleId: '', providerId: '', model: '', kbId: null, temperature: 0.7, maxTokens: 2048, topP: 1 };
}

/** AgentItem → 编辑表单 */
export function agentToForm(a: {
  name: string; description: string; roleId: string; providerId: string;
  model: string; kbId: number | null; temperature: number; maxTokens: number; topP: number;
}): AgentInput {
  return {
    name: a.name, description: a.description, roleId: a.roleId, providerId: a.providerId,
    model: a.model, kbId: a.kbId, temperature: a.temperature, maxTokens: a.maxTokens, topP: a.topP,
  };
}
