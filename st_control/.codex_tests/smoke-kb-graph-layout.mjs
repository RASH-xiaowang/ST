// ============================================================
// Wiki 图谱纯函数 — 运行期冒烟测试
// 锁定 graphLayout 下沉后的可观测输出：
//   radialTreeLayout 坐标 / 根节点选择 / 孤岛挂载 / glob 匹配
// 运行：node st_control/.codex_tests/smoke-kb-graph-layout.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'kb', 'graphLayout.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'graph-layout.mjs');
writeFileSync(outFile, code);

const mod = await import(pathToFileURL(outFile).href);
const { radialTreeLayout, matchGlob } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

const params = { forceRepulsion: 2600, forceAttraction: 0.04, forceEdgeLength: 1, forceCentripetal: 0.02, nodeScale: 1 };

// 单节点：居中
const single = radialTreeLayout({ nodes: [{ id: 1, pageId: 1, title: 'a', docId: null, docTitle: null, dirName: null, inDegree: 0, outDegree: 0, status: 'created' }], edges: [] }, 800, 600, params);
ok(single[1].x === 400 && single[1].y === 300, '单节点布局居中');

// 多节点：所有节点都有坐标；连接度最高的节点为根（id=2）
const g = {
  nodes: [
    { id: 1, pageId: 1, title: 'a', docId: null, docTitle: null, dirName: null, inDegree: 1, outDegree: 0, status: 'created' },
    { id: 2, pageId: 2, title: 'b', docId: null, docTitle: null, dirName: null, inDegree: 2, outDegree: 1, status: 'created' },
    { id: 3, pageId: 3, title: 'c', docId: null, docTitle: null, dirName: null, inDegree: 0, outDegree: 1, status: 'created' },
    { id: 4, pageId: 4, title: 'd', docId: null, docTitle: null, dirName: null, inDegree: 0, outDegree: 0, status: 'created' },
  ],
  edges: [
    { from: 1, to: 2, linkType: 'child_of', weight: 1 },
    { from: 3, to: 2, linkType: 'child_of', weight: 1 },
  ],
};
const pos = radialTreeLayout(g, 800, 600, params);
ok(Object.keys(pos).length === 4, '全部节点都有坐标');
ok(pos[2].x === 400 && pos[2].y === 300, '根节点（最高连接度）居中');
ok(pos[4].x !== undefined && pos[4].y !== undefined, '孤岛节点也获得坐标（挂到根下）');

// glob 匹配：* 通配、大小写不敏感
ok(matchGlob('Meeting Notes', 'Meeting*'), 'glob 前缀匹配');
ok(matchGlob('daily/2026/report', '*/report'), 'glob 跨层级匹配');
ok(matchGlob('Report', 'report'), 'glob 大小写不敏感');
ok(!matchGlob('notes', 'report*'), 'glob 不匹配返回 false');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
