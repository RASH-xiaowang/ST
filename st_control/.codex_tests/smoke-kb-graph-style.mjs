// ============================================================
// Wiki 图谱着色/分类纯函数 — 运行期冒烟测试
// 锁定 graphStyle 下沉后的可观测输出：
//   节点类型归类 / 颜色组优先 / 状态着色 / 连线颜色 / slug
// 运行：node st_control/.codex_tests/smoke-kb-graph-style.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'kb', 'graphStyle.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'graph-style.mjs');
writeFileSync(outFile, code);

const mod = await import(pathToFileURL(outFile).href);
const { edgeColor, colorSlug, nodeMatches, nodeTypeName, nodeColor, NODE_TYPE_COLORS } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

const nd = (dirName, title = 'x', docTitle = null) => ({ id: 1, pageId: 1, title, docId: null, docTitle, dirName, inDegree: 0, outDegree: 0, status: 'created' });

// 节点类型归类
ok(nodeTypeName(nd('实体')) === '实体', '目录「实体」归类为实体');
ok(nodeTypeName(nd('人物')) === '实体', '目录「人物」归类为实体');
ok(nodeTypeName(nd('概念')) === '概念', '目录「概念」归类为概念');
ok(nodeTypeName(nd(null)) === '页面', '无目录 → 页面');
ok(nodeTypeName(nd('自定义')) === '自定义', '未知目录 → 原目录名');

// 颜色组优先于类型/状态
ok(nodeColor('created', nd(null, '财务报告'), [{ query: '财务', color: '#ff0000' }]) === '#ff0000', '颜色组命中优先');
ok(nodeColor('created', nd(null, '其他'), [{ query: '财务', color: '#ff0000' }]) === NODE_TYPE_COLORS['页面'], '颜色组未命中走类型颜色');
ok(nodeColor('created', nd('概念')) === NODE_TYPE_COLORS['概念'], '类型颜色生效');
ok(nodeColor('draft', nd('自定义')) === '#f6bd16', 'draft 状态黄色（无类型颜色时）');
ok(nodeColor('missing', nd('自定义')) === '#8d99ae', 'missing 状态灰色（无类型颜色时）');

// 节点匹配（标题/文档标题）
ok(nodeMatches(nd(null, 'Meeting', 'Notes 2026'), 'meeting'), '标题匹配大小写不敏感');
ok(nodeMatches(nd(null, 'Meeting', 'Notes'), 'notes'), '文档标题匹配');
ok(!nodeMatches(nd(null, 'Meeting'), 'zzz'), '不匹配返回 false');

// 连线颜色与 slug
ok(edgeColor('related') === '#5b8ff9', '连线类型颜色');
ok(edgeColor('unknown') === '#7d8899', '未知连线类型回退灰');
ok(colorSlug('#5B8FF9') === '5b8ff9', '颜色 slug 小写去 #');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
