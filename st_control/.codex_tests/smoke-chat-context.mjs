// ============================================================
// 大模型对话上下文裁剪 — 运行期冒烟测试
// 锁定 trimContext 下沉后的可观测输出：
//   条数上限 / 字符上限滑动裁剪 / 最小保留 / 主题锚点 / 整轮裁剪
// 运行：node st_control/.codex_tests/smoke-chat-context.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'llm', 'chatContext.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'chat-context.mjs');
writeFileSync(outFile, code);

const mod = await import(pathToFileURL(outFile).href);
const { trimContext, TRIMMED_CONTEXT_NOTE } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

const mk = (role, content) => ({ role, content });
// 交替构造一轮轮对话：q0, a0, q1, a1, ...
const turns = (n) => {
  const out = [];
  for (let i = 0; i < n; i++) {
    out.push(mk('user', `q-${i}`));
    out.push(mk('assistant', `a-${i}`));
  }
  return out;
};

// 条数上限：30 轮（60 条）→ 整轮裁剪后不超过 40 条
const many = turns(30);
const trimmed = trimContext(many);
ok(trimmed.messages.length <= 40, `超过条数上限时裁剪到 ≤40 条（实际 ${trimmed.messages.length}）`);
ok(trimmed.messages[0].content === 'q-0', '第一条用户消息作为主题锚点始终保留');
ok(trimmed.messages[trimmed.messages.length - 1].content === 'a-29', '最新消息始终保留');
ok(trimmed.trimmed === true, '发生裁剪时 trimmed=true');

// 整轮裁剪：保留的消息以完整轮次为边界（user 开头，assistant 结尾）
const anchorLast = trimmed.messages.slice(1);
const firstAfterAnchor = anchorLast[0];
ok(firstAfterAnchor.role === 'user', '锚点之后以完整轮次的 user 消息开头');

// 字符上限：长消息裁剪，且至少保留 6 条
const long = turns(8).map((m) => ({ ...m, content: m.content.repeat(40) }));
const chars = trimContext(long, 40, 200, 6);
ok(chars.messages.length >= 6, `字符超限时至少保留 6 条（实际 ${chars.messages.length}）`);
ok(chars.messages[chars.messages.length - 1].content === long[15].content, '最小保留时最新消息仍在');
ok(chars.trimmed === true, '字符裁剪也标记 trimmed');

// 未超限：原样返回且不标记裁剪
const few = [mk('user', 'x'), mk('assistant', 'y')];
const fewR = trimContext(few);
ok(fewR.messages.length === 2 && fewR.trimmed === false, '未超限时消息完整保留且不标记裁剪');

// 空数组
const none = trimContext([]);
ok(none.messages.length === 0 && none.trimmed === false, '空历史安全返回');

// 系统说明存在
ok(TRIMMED_CONTEXT_NOTE.includes('省略') && TRIMMED_CONTEXT_NOTE.includes('主题'), '裁剪说明文案存在');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
