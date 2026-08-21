// ============================================================
// 系统指标 SVG 路径 — 运行期冒烟测试
// 锁定 system/chartPaths 下沉后的可观测输出：
//   折线归一化 / 面积闭合 / 雷达多边形与轴线
// 运行：node st_control/.codex_tests/smoke-chart-paths.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'system', 'chartPaths.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'chart-paths.mjs');
writeFileSync(outFile, code);

const mod = await import(pathToFileURL(outFile).href);
const { buildLine, buildArea, buildRadar } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// 折线
ok(buildLine([], 100, 40) === '', '空数组返回空串');
const line = buildLine([0, 100], 100, 40, 2);
ok(line.startsWith('M') && line.includes('L'), '折线 M 起笔 + L 连接');
ok(line.includes('2.0') && line.includes('98.0'), 'pad 边界（x 从 2 起）');

// 面积
const area = buildArea([0, 100], 100, 40, 2);
ok(area.endsWith('L 2.0 38 Z'), '面积闭合到底部（baseY=38 无小数）');
ok(buildArea([], 100, 40) === '', '空数组面积空串');

// 雷达
const radar = buildRadar([1, 0.5, 0], 70, 70, 52);
ok(radar.poly.endsWith(' Z'), '雷达多边形闭合');
ok(radar.poly.startsWith('M'), '雷达 M 起笔');
ok(radar.axes.includes('dvr-radar-axis') && radar.axes.split('line').length === 4, '3 轴 3 条轴线');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
