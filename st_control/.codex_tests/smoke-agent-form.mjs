// ============================================================
// 智能体表单工厂 / 数值解析 — 运行期冒烟测试
// 锁定 agentForm / numOrNull 下沉后的可观测输出：
//   空白表单默认值 / AgentItem 映射 / 数值容错
// 运行：node st_control/.codex_tests/smoke-agent-form.mjs
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

async function compile(rel, out) {
  const src = readFileSync(path.join(root, 'src', 'lib', rel), 'utf8');
  const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
  writeFileSync(path.join(outDir, out), code);
}
await compile('agents/agentForm.ts', 'agent-form.mjs');
await compile('llm/numOrNull.ts', 'num-or-null.mjs');

const m1 = await import(pathToFileURL(path.join(outDir, 'agent-form.mjs')).href);
const m2 = await import(pathToFileURL(path.join(outDir, 'num-or-null.mjs')).href);
const { createBlankAgentForm, agentToForm } = m1;
const { numOrNull } = m2;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// 空白表单
const blank = createBlankAgentForm();
ok(blank.name === '' && blank.kbId === null, '空白表单默认空值');
ok(blank.temperature === 0.7 && blank.maxTokens === 2048 && blank.topP === 1, '空白表单模型参数');

// AgentItem → 表单
const form = agentToForm({ name: 'n', description: 'd', roleId: 'r', providerId: 'p', model: 'm', kbId: 3, temperature: 0.5, maxTokens: 1000, topP: 0.9 });
ok(form.name === 'n' && form.kbId === 3 && form.temperature === 0.5, 'AgentItem 字段映射');

// 数值解析
ok(numOrNull('42') === 42, '整数解析');
ok(numOrNull(' 3.5 ') === 3.5, '浮点 + 空格');
ok(numOrNull('') === null && numOrNull('   ') === null, '空串 → null');
ok(numOrNull('abc') === null && numOrNull('NaN') === null, '非法 → null');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
