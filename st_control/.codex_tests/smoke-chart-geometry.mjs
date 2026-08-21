// ============================================================
// 图表几何纯函数 — 运行期冒烟测试
// 锁定 chartGeometry 下沉后的可观测输出：
//   调色板循环 / 极坐标 / 扇形路径 / 饼图切片角度
// 运行：node st_control/.codex_tests/smoke-chart-geometry.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'components', 'chartGeometry.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'chart-geometry.mjs');
writeFileSync(outFile, code);

const mod = await import(pathToFileURL(outFile).href);
const { chartColor, PALETTE, polar, arcPath, pieSliceAngles } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// 调色板
ok(chartColor(0) === PALETTE[0] && chartColor(10) === PALETTE[0], '取色循环');
ok(chartColor(1) === PALETTE[1], '索引取色');

// 极坐标：角度 0 = 正上方
const top = polar(100, 100, 50, 0);
ok(Math.abs(top.x - 100) < 1e-9 && Math.abs(top.y - 50) < 1e-9, '0° = 正上方');
const right = polar(100, 100, 50, 90);
ok(Math.abs(right.x - 150) < 1e-9 && Math.abs(right.y - 100) < 1e-9, '90° = 正右方');

// 扇形路径
const p = arcPath(100, 100, 80, 0, 90);
ok(p.startsWith('M 100 100 L '), '路径以圆心起笔');
ok(p.includes('A 80 80 0 0 0'), '小弧（≤180°）large-arc=0');
const big = arcPath(100, 100, 80, 0, 200);
ok(big.includes('A 80 80 0 1 0'), '大弧（>180°）large-arc=1');

// 饼图切片角度（自 ChartView pieSlices 下沉）
const slices = pieSliceAngles([{ value: 10 }, { value: 30 }, { value: 60 }], (i) => `c${i}`);
ok(JSON.stringify(slices) === JSON.stringify([
  { value: 10, start: 0, end: 36, color: 'c0' },
  { value: 30, start: 36, end: 144, color: 'c1' },
  { value: 60, start: 144, end: 360, color: 'c2' },
]), '累积角度（10/30/60 → 36°/108°/216°）');
ok(JSON.stringify(pieSliceAngles([{ value: 0 }], () => 'c')) ===
  JSON.stringify([{ value: 0, start: 0, end: 0, color: 'c' }]), '全零 total 兜底 1 不除零');
ok(pieSliceAngles([], () => 'c').length === 0, '空列表返回空');
ok(pieSliceAngles([{ value: 50 }, { value: 50 }], (i) => `c${i}`)[1].start === 180, '半圆边界');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
