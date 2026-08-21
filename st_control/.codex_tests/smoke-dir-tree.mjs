// ============================================================
// 知识库 Wiki 目录树纯算法 — 运行期冒烟测试
// 锁定 dirTreeUtils.ts 下沉后的可观测输出：
//   buildDirSubtree / buildDirTree / filterPagesByDir
// 运行：node st_control/.codex_tests/smoke-dir-tree.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'kb', 'dirTreeUtils.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'kb-dir-tree-utils.mjs');
writeFileSync(outFile, code);

const { buildDirSubtree, buildDirTree, filterPagesByDir } = await import(
  pathToFileURL(outFile).href
);

const dirs = [
  { id: 1, parentId: null, name: '根', count: 3 },
  { id: 2, parentId: 1, name: '子A', count: 2 },
  { id: 3, parentId: 1, name: '子B', count: 1 },
  { id: 4, parentId: 2, name: '孙', count: 1 },
];

// ── buildDirSubtree ──
const sub = buildDirSubtree(dirs);
assert.deepEqual([...sub.get(1)].sort(), [1, 2, 3, 4], '根含全部子孙');
assert.deepEqual([...sub.get(2)].sort(), [2, 4], '中间节点含自身与子孙');
assert.deepEqual([...sub.get(3)], [3], '叶子仅自身');
assert.deepEqual([...sub.get(4)], [4]);
assert.deepEqual(buildDirSubtree([]), new Map(), '空目录返回空 Map');

// ── buildDirTree ──
assert.deepEqual(
  buildDirTree(dirs),
  [
    { id: 1, name: '根', count: 3, depth: 0 },
    { id: 2, name: '子A', count: 2, depth: 1 },
    { id: 4, name: '孙', count: 1, depth: 2 },
    { id: 3, name: '子B', count: 1, depth: 1 },
  ],
  '前序展开，同级保持输入顺序',
);
assert.deepEqual(buildDirTree([]), [], '空目录返回空列表');

// ── filterPagesByDir ──
const pages = [
  { id: 1, dirId: 1, title: 'p1' },
  { id: 2, dirId: 2, title: 'p2' },
  { id: 3, dirId: 4, title: 'p3' },
  { id: 4, dirId: null, title: 'p4' },
];
assert.deepEqual(
  filterPagesByDir(pages, null, sub).map((p) => p.id),
  [1, 2, 3, 4],
  'null 不过滤',
);
assert.deepEqual(filterPagesByDir(pages, 1, sub).map((p) => p.id), [1, 2, 3], '根目录含子树页面');
assert.deepEqual(filterPagesByDir(pages, 2, sub).map((p) => p.id), [2, 3], '子目录含孙目录页面');
assert.deepEqual(filterPagesByDir(pages, 4, sub).map((p) => p.id), [3], '叶子仅自身页面');
assert.deepEqual(filterPagesByDir(pages, 999, sub), [], '未知目录返回空');
assert.deepEqual(filterPagesByDir([], null, new Map()), [], '空页面返回空');

console.log('smoke-dir-tree: all assertions passed');
