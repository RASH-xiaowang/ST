// ============================================================
// 共享关键词过滤纯函数 — 运行期冒烟测试
// 锁定 src/lib/utils/filter.ts 的可观测输出：
//   filterByKeyword / filterByAnyKeyword（单/多字段、数组分段、trim 语义）
// 运行：node st_control/.codex_tests/smoke-filter-utils.mjs
// ============================================================
import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import esbuild from 'esbuild';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(here, '..');
const outDir = path.join(here, 'out');
mkdirSync(outDir, { recursive: true });

const src = readFileSync(path.join(root, 'src', 'lib', 'utils', 'filter.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'filter-utils.mjs');
writeFileSync(outFile, code);

const { filterByKeyword, filterByAnyKeyword } = await import(pathToFileURL(outFile).href);

// ── filterByKeyword（单字段）──
const items = [{ name: '张三' }, { name: '李四' }, { name: '' }];
assert.deepEqual(filterByKeyword(items, '张', (i) => i.name), [{ name: '张三' }], '子串过滤');
assert.equal(filterByKeyword(items, '', (i) => i.name) === items, true, '空关键词返回原引用');
assert.equal(filterByKeyword(items, '   ', (i) => i.name) === items, true, '纯空白返回原引用（trim 语义）');
assert.deepEqual(filterByKeyword([{ md5: 'ABC123' }], 'abc', (i) => i.md5), [{ md5: 'ABC123' }], '大小写不敏感');
assert.deepEqual(
  filterByKeyword([{ tags: ['AI', 'Agent'] }], 'agent', (i) => i.tags),
  [{ tags: ['AI', 'Agent'] }],
  '数组字段任一分段命中',
);
assert.deepEqual(filterByKeyword(items, '不存在', (i) => i.name), [], '未命中返回空');

// ── filterByAnyKeyword（多字段 + 数组）──
const roles = [
  { name: '客服', description: '处理售前问题', capabilities: ['接单', '答疑'] },
  { name: '运营', description: null, capabilities: ['数据分析'] },
];
assert.deepEqual(
  filterByAnyKeyword(roles, '答疑', (r) => r.name || '', (r) => r.description || '', (r) => r.capabilities || []),
  [roles[0]],
  '数组字段命中',
);
assert.deepEqual(
  filterByAnyKeyword(roles, '运营', (r) => r.name || '', (r) => r.description || '', (r) => r.capabilities || []),
  [roles[1]],
  'name 字段命中且 description null 安全',
);
assert.equal(
  filterByAnyKeyword(roles, ' ', (r) => r.name || '', (r) => r.capabilities || []) === roles,
  true,
  '空白返回原引用',
);
assert.deepEqual(filterByAnyKeyword(roles, 'xx', (r) => r.name || ''), [], '未命中返回空');

console.log('smoke-filter-utils: all assertions passed');
