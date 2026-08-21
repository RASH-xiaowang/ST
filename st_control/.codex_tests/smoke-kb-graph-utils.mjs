// ============================================================
// 知识库 Wiki 图纯算法 — 运行期冒烟测试
// 锁定 graphUtils.ts 下沉后的可观测输出：
//   graphNeighborSet / nodeDegreeMap / edgeLinkTypes / visibleNodeIds
// 运行：node st_control/.codex_tests/smoke-kb-graph-utils.mjs
// ============================================================
import assert from 'node:assert/strict';
import { mkdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import esbuild from 'esbuild';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(here, '..');
const outDir = path.join(here, 'out');
mkdirSync(outDir, { recursive: true });

// graphUtils 依赖 ./graphLayout（matchGlob 运行时依赖）：esbuild bundle
const src = readFileSync(path.join(root, 'src', 'lib', 'kb', 'graphUtils.ts'), 'utf8');
const outFile = path.join(outDir, 'kb-graph-utils.cjs');
await esbuild.build({
  stdin: {
    contents: src,
    resolveDir: path.join(root, 'src', 'lib', 'kb'),
    loader: 'ts',
    sourcefile: 'graphUtils.ts',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  outfile: outFile,
  logLevel: 'silent',
});
const { createRequire } = await import('node:module');
const require = createRequire(import.meta.url);
const { graphNeighborSet, nodeDegreeMap, edgeLinkTypes, visibleNodeIds } = require(outFile);

const edges = [
  { from: 1, to: 2, linkType: 'link', weight: 1 },
  { from: 2, to: 3, linkType: 'link', weight: 1 },
  { from: 1, to: 3, linkType: 'mention', weight: 1 },
];

// ── graphNeighborSet ──
assert.deepEqual([...graphNeighborSet(edges, 1)].sort(), [1, 2, 3], '邻居含自身');
assert.deepEqual([...graphNeighborSet(edges, 3)].sort(), [1, 2, 3]);
assert.deepEqual([...graphNeighborSet([], 5)], [5], '无边时仅自身');

// ── nodeDegreeMap ──
assert.deepEqual(nodeDegreeMap(edges), { 1: 2, 2: 2, 3: 2 }, '每条边两端各计一度');
assert.deepEqual(nodeDegreeMap([]), {}, '空图返回空');

// ── edgeLinkTypes ──
assert.deepEqual(edgeLinkTypes({ nodes: [], edges }), ['link', 'mention'], '去重且按首现序');
assert.deepEqual(edgeLinkTypes(null), [], 'null 图返回空');

// ── visibleNodeIds ──
const graph = {
  nodes: [
    { id: 1, title: 'Alpha', docTitle: 'doc-a', status: 'published', inDegree: 1, outDegree: 1 },
    { id: 2, title: 'Beta', docTitle: 'doc-b', status: 'missing', inDegree: 0, outDegree: 0 },
    { id: 3, title: 'Gamma', docTitle: null, status: 'published', inDegree: 1, outDegree: 1 },
    { id: 4, title: 'alpha-old', docTitle: 'doc-a', status: 'published', inDegree: 1, outDegree: 0 },
  ],
  edges: [
    { from: 1, to: 3, linkType: 'link', weight: 1 },
    { from: 1, to: 4, linkType: 'link', weight: 1 },
  ],
};
const baseOpts = {
  nodeDegree: { 1: 2, 2: 0, 3: 1, 4: 1 },
  ignorePatterns: [],
  createdOnly: false,
  showOrphans: true,
  query: '',
  localOnly: false,
  anchorId: null,
};
assert.deepEqual([...visibleNodeIds(graph, baseOpts)].sort(), [1, 2, 3, 4], '全量可见');
assert.deepEqual(
  [...visibleNodeIds(null, baseOpts)].length,
  0,
  'null 图返回空集',
);
assert.deepEqual(
  [...visibleNodeIds(graph, { ...baseOpts, createdOnly: true })].sort(),
  [1, 3, 4],
  'createdOnly 排除 missing',
);
assert.deepEqual(
  [...visibleNodeIds(graph, { ...baseOpts, showOrphans: false })].sort(),
  [1, 3, 4],
  'showOrphans=false 排除零连接度',
);
assert.deepEqual(
  [...visibleNodeIds(graph, { ...baseOpts, ignorePatterns: ['Beta*'] })].sort(),
  [1, 3, 4],
  '通配忽略模式排除命中（大小写不敏感）',
);
assert.deepEqual(
  [...visibleNodeIds(graph, { ...baseOpts, ignorePatterns: ['gamma'] })].sort(),
  [1, 2, 4],
  '忽略模式大小写不敏感命中',
);
assert.deepEqual(
  [...visibleNodeIds(graph, { ...baseOpts, query: 'alpha' })].sort(),
  [1, 4],
  '关键词大小写不敏感匹配标题',
);
assert.deepEqual(
  [...visibleNodeIds(graph, { ...baseOpts, localOnly: true, anchorId: 1 })].sort(),
  [1, 3, 4],
  'localOnly 保留锚点及其邻居',
);
assert.deepEqual(
  [...visibleNodeIds(graph, { ...baseOpts, localOnly: true, anchorId: 3 })].sort(),
  [1, 3],
  'localOnly 邻居过滤',
);

console.log('smoke-kb-graph-utils: all assertions passed');
