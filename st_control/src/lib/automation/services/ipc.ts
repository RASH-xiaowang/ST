// 自动化管理中心 — Tauri IPC 封装层
// 组件层统一通过本模块调用后端，避免直接 invoke。
import { invoke } from '@tauri-apps/api/core';

/** 规则条件（content/sender/session/media_type/is_send + op） */
export type RuleCondition = { field: string; op: string; value: string };
/** AI 提取字段定义 */
export type AnalyzeField = { name: string; desc: string };
/** 规则保存入参（与后端 RuleInput camelCase 对应；id 为 null 表示新建） */
export type AutomationRuleInput = {
  id: number | null;
  name: string;
  enabled: boolean;
  priority: number;
  conditions: RuleCondition[];
  analyzeFields: AnalyzeField[];
  promptOverride: string;
  providerId: string;
  model: string;
  dispatchMode: string;
  targetType: string;
  targetId: string;
  /** 绑定的 AI 角色 id（内置 Worker 执行时注入角色提示词） */
  roleId?: string;
};
/** 已保存规则（含统计字段） */
export type AutomationRule = AutomationRuleInput & {
  id: number;
  hitCount: number;
  createdAt: string;
  updatedAt: string;
};
/** 自动化任务行 */
export type AutomationTask = {
  id: number;
  ackId: string | null;
  content: string;
  senderUsername: string;
  sessionType: string | null;
  isGroup: boolean;
  isSend: boolean;
  mediaType: string | null;
  msgType: number | null;
  timestamp: number;
  username: string;
  ruleId: number | null;
  ruleName: string;
  aiExtract: unknown;
  fullJson: unknown;
  targetType: string;
  targetId: string;
  replyText: string;
  status: string;
  error: string;
  /** error 任务已被自动重试的次数（0 = 未重试过） */
  retryCount: number;
  createdAt: string;
  updatedAt: string;
};
/** 状态分布条目 */
export type AutomationStatusCount = { status: string; count: number };
/** 概览统计（与后端 AutomationStats camelCase 对应） */
export type AutomationStats = {
  todayPushed: number;
  totalTasks: number;
  pending: number;
  claimed: number;
  processing: number;
  toReply: number;
  replied: number;
  done: number;
  ignored: number;
  rulesEnabled: number;
  rulesTotal: number;
  statusDist: AutomationStatusCount[];
};

export const automationApi = {
  listRules: () => invoke<AutomationRule[]>('automation_list_rules'),
  saveRule: (input: AutomationRuleInput) => invoke<number>('automation_save_rule', { input }),
  deleteRule: (id: number) => invoke<void>('automation_delete_rule', { id }),
  toggleRule: (id: number, enabled: boolean) => invoke<void>('automation_toggle_rule', { id, enabled }),

  listTasks: (params: { status?: string | null; keyword?: string | null; page?: number; pageSize?: number }) =>
    invoke<{ total: number; items: AutomationTask[] }>('automation_list_tasks', params),
  setTaskStatus: (id: number, status: string) => invoke<void>('automation_set_task_status', { id, status }),
  setTaskTarget: (id: number, targetType: string, targetId: string) =>
    invoke<void>('automation_set_task_target', { id, targetType, targetId }),
  deleteTask: (id: number) => invoke<void>('automation_delete_task', { id }),
  editTaskReply: (id: number, replyText: string, status: string) =>
    invoke<void>('automation_edit_task_reply', { id, replyText, status }),
  editAiExtract: (id: number, aiExtract: string) => invoke<void>('automation_edit_ai_extract', { id, aiExtract }),

  stats: () => invoke<AutomationStats>('automation_stats'),
  connStatus: () =>
    invoke<{ connected: boolean; received: number; lastAt: string | null; url: string }>('automation_conn_status'),
  simulatePush: (args: { content?: string | null; senderUsername?: string | null; username?: string | null }) =>
    invoke<number>('automation_simulate_push', args),
  reconnect: () => invoke<void>('automation_reconnect'),
};
