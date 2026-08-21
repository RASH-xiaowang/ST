// ============================================================
// AI 角色提示词组装 — 运行期冒烟测试
// 锁定 roleUtils 统一后的可观测输出：
//   各节拼接顺序 / 空节过滤 / 语言跟随用户省略
// 运行：node st_control/.codex_tests/smoke-role-utils.mjs
// ============================================================
import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import esbuild from 'esbuild';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(here, '..');
const outDir = path.join(here, 'out');
mkdirSync(outDir, { recursive: true });

const src = readFileSync(path.join(root, 'src', 'lib', 'llm', 'roleUtils.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'role-utils.mjs');
writeFileSync(outFile, code);

const mod = await import(pathToFileURL(outFile).href);
const { composeSystemPrompt, normalizeRole, createEmptyRole } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

const role = {
  system_prompt: '  你是助手  ',
  behavior_constraints: [' 先说结论 ', ' 不要编造 '],
  knowledge_context: ' 背景A ',
  response_language: '中文',
};

const out = composeSystemPrompt(role);
ok(out.startsWith('你是助手'), '基础提示去首尾空格');
ok(out.includes('【行为约束】\n- 先说结论\n- 不要编造'), '约束为列表且按序');
ok(out.includes('【背景知识】\n背景A'), '背景知识节');
ok(out.includes('【回复语言】请使用 中文 回复。'), '语言节');
ok(out.split('\n\n').length === 4, '四节以空行分隔');

// 空节过滤 + 跟随用户省略
const minimal = composeSystemPrompt({ system_prompt: '', behavior_constraints: [], knowledge_context: '', response_language: '跟随用户' });
ok(minimal === '', '全空角色返回空串');
ok(!composeSystemPrompt({ ...role, response_language: '跟随用户' }).includes('【回复语言】'), '跟随用户省略语言节');

// 规范化：null → 空串/空数组，且不修改原对象
const withNulls = {
  ...role, id: 'r1', name: 'n', emoji: '🤖', description: '', enabled: true,
  preferred_provider_name: null, preferred_model: null,
  behavior_constraints: null, capabilities: null,
  temperature: 0.7, max_tokens: 2048, top_p: 1, presence_penalty: 0, frequency_penalty: 0,
  response_language: '跟随用户', knowledge_context: '', created_at: '', updated_at: '',
};
const normalized = normalizeRole(withNulls);
ok(normalized.preferred_provider_name === '' && normalized.preferred_model === '', 'null 提供方/模型 → 空串');
ok(Array.isArray(normalized.behavior_constraints) && normalized.behavior_constraints.length === 0, 'null 约束 → 空数组');
ok(Array.isArray(normalized.capabilities), 'null 能力 → 空数组');
ok(withNulls.preferred_provider_name === null, '原对象未被修改（深拷贝）');
ok(normalized !== withNulls, '规范化返回新对象');

// 空角色默认值
const empty = createEmptyRole();
ok(empty.name === '' && empty.emoji === '🤖' && empty.enabled === true, '空角色默认字段');
ok(empty.temperature === 0.7 && empty.max_tokens === 2048 && empty.response_language === '跟随用户', '空角色模型参数默认');
ok(Array.isArray(empty.behavior_constraints) && Array.isArray(empty.capabilities), '空角色数组字段');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
