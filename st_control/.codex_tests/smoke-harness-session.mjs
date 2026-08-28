// ============================================================
// Harness 会话类型契约与投影逻辑烟测
// 校验：
// 1. 前端 types.ts 与后端 session.rs 结构对齐（字段名、枚举判别）
// 2. WorkflowRun stage 展示应为 1-based（与 trajectory 一致）
// 3. FeedbackRecord 字段完整性（comment/message_seq）
// 4. DisplayMessage 三态投影（user/assistant/meta）
// 运行：node st_control/.codex_tests/smoke-harness-session.mjs
// ============================================================

import { readFileSync } from 'node:fs';
import { join } from 'node:path';

const ROOT = new URL('..', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1');
let failures = 0;
function assert(cond, msg) {
  if (!cond) { console.error(`✗ ${msg}`); failures++; }
  else { console.log(`✓ ${msg}`); }
}

// ─── 1. 前端 types.ts 结构校验 ───
const typesSrc = readFileSync(join(ROOT, 'src', 'lib', 'harness', 'types.ts'), 'utf8');

// HarnessDisplayMessage 必须有 role: "user" | "assistant" | "meta"
assert(typesSrc.includes('role: "user" | "assistant" | "meta"'), 'DisplayMessage role 三态');
// DisplayMessage 含 seq 字段（interface，所有 role 共享）
assert(typesSrc.match(/seq:\s*number/s), '消息含 seq 字段');
// meta 需要 kind 和 workflow 字段
assert(typesSrc.match(/kind\??\s*:\s*string/s), 'meta 消息含 kind');
assert(typesSrc.match(/workflow\??\s*:\s*\{[^}]*stage:\s*number[^}]*total:\s*number/s), 'meta workflow 含 stage/total');

// FeedbackRecord 完整性
assert(typesSrc.match(/comment:\s*string;\s*\n\s*\/\*\*/), 'FeedbackRecord.comment 与 message_seq 分行（JSDoc 正确）');
assert(typesSrc.includes('message_seq?: number | null'), 'FeedbackRecord 含 message_seq');

// HarnessStreamEvent 必须含 done 事件（含 seq/model/tokens/cost）
// HarnessStreamEvent 是 type union，done 在其中
const streamSection = typesSrc.slice(typesSrc.indexOf('HarnessStreamEvent'));
assert(streamSection.match(/type:\s*"done"[^}]*seq:\s*number/s), 'done 事件含 seq');
assert(streamSection.match(/type:\s*"done"[^}]*model:\s*string/s), 'done 事件含 model');
assert(streamSection.match(/type:\s*"done"[^}]*cost:\s*number/s), 'done 事件含 cost');

// 2. 后端 session.rs WorkflowRun stage 展示校验 ───
const sessionSrc = readFileSync(join(ROOT, 'src-tauri', 'src', 'harness', 'session.rs'), 'utf8');

// derive_display_messages 中 WorkflowRun 的 stage 应为 +1 展示（1-based）
// Rust 源码中 format! 参数含 stage + 1
const stagePlusOneMatches = [...sessionSrc.matchAll(/stage\s*\+\s*1/g)];
assert(stagePlusOneMatches.length >= 2, 'WorkflowRun stage 1-based（display_messages 和 trajectory 均使用 stage + 1）');

// ─── 3. HarnessEvent 枚举判别（session.rs）与前端类型对齐 ───
const eventVariants = [
  'UserMessage', 'AssistantChunk', 'AssistantMessage',
  'AssistantToolCalls', 'ToolResult', 'TodoUpdate',
  'PlanEnter', 'PlanExit', 'GoalSet', 'GoalUpdate',
  'WorkflowRun', 'AttachmentAdded', 'Compaction',
  'SessionForked', 'SessionTitle', 'RoleSet',
  'SessionCleared', 'ContextInjected', 'SkillInjected',
];
for (const v of eventVariants) {
  assert(sessionSrc.includes(`${v} {`) || sessionSrc.includes(`${v},`), `HarnessEvent 含 ${v} 变体`);
}

// ─── 4. 前端 HarnessEvent（旧日志）type 判别含关键事件 ───
const harnessEventTypes = [
  'user_message', 'assistant_chunk', 'assistant_message',
  'assistant_tool_calls', 'tool_result', 'session_title',
  'session_forked', 'session_cleared', 'role_set',
];
for (const t of harnessEventTypes) {
  assert(typesSrc.includes(`"${t}"`), `前端 HarnessEvent 含 ${t} 判别`);
}

// ─── 5. IPC 参数键名一致性抽样（harness_* 命令）───
const ipcSrc = readFileSync(join(ROOT, 'src', 'lib', 'harness', 'services', 'ipc.ts'), 'utf8');
// 检查关键 IPC 函数存在
const ipcFunctions = [
  'listSessions', 'createSession', 'displayMessages',
  'trajectory', 'turnFiles', 'contextMeter',
  'chatStream', 'cancelTurn', 'goalAction',
  'usageSummary', 'sessionState', 'forkSession',
  'exportSession', 'clearSession',
];
for (const fn of ipcFunctions) {
  assert(ipcSrc.includes(`${fn}:`), `ipc.ts 含 ${fn} 函数`);
}

// ─── 结果 ───
if (failures > 0) {
  console.error(`\n${failures} 项检查未通过`);
  process.exit(1);
}
console.log('\n✓ Harness 会话类型契约与投影逻辑全部通过');
process.exit(0);
