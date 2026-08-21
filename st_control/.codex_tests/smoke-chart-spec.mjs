// ============================================================
// LLM 图表规范归一化 — 运行期冒烟测试
// 锁定 llm/chartSpec.ts 下沉后的可观测输出：
//   类型判定 / 饼图映射 / 退化单系列 / 数据转换
// 运行：node st_control/.codex_tests/smoke-chart-spec.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'llm', 'chartSpec.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'chart-spec.mjs');
writeFileSync(outFile, code);

const { normalizeChart } = await import(pathToFileURL(outFile).href);

// 空/非法输入 → 默认饼图空结构
assert.deepEqual(normalizeChart(null), { kind: 'pie', labels: [], series: [], pie: [] }, 'null 默认');
assert.deepEqual(normalizeChart(undefined), { kind: 'pie', labels: [], series: [], pie: [] }, 'undefined 默认');
assert.deepEqual(normalizeChart('x'), { kind: 'pie', labels: [], series: [], pie: [] }, '非对象默认');

// 类型判定
assert.equal(normalizeChart({ type: 'line' }).kind, 'line', 'line 类型');
assert.equal(normalizeChart({ type: 'bar' }).kind, 'bar', 'bar 类型');
assert.equal(normalizeChart({ type: 'pie' }).kind, 'pie', 'pie 类型');
assert.equal(normalizeChart({}).kind, 'pie', '缺省类型按饼图');

// 常规轴类
const line = normalizeChart({
  type: 'line',
  title: '趋势',
  labels: ['a', 'b'],
  series: [{ name: 's1', data: [1, 2] }],
});
assert.equal(line.title, '趋势', '标题保留');
assert.deepEqual(line.labels, ['a', 'b'], '标签转字符串');
assert.deepEqual(line.series[0].data, [1, 2], '系列数据保留');

// 饼图数据（多字段兼容 + 过滤非数值）
const pie = normalizeChart({
  type: 'pie',
  data: [
    { label: 'A', value: 10 },
    { name: 'B', y: '20' },
    { x: 'C', value: NaN },
  ],
});
assert.deepEqual(pie.pie, [{ label: 'A', value: 10 }, { label: 'B', value: 20 }], '字段兼容并过滤 NaN');

// 退化：非 pie 无 series 时饼图数据当单系列柱状
const degraded = normalizeChart({ type: 'bar', data: [{ label: 'x', value: 5 }] });
assert.equal(degraded.kind, 'bar', '退化保留类型');
assert.deepEqual(degraded.labels, ['x'], '退化标签取饼图 label');
assert.deepEqual(degraded.series[0].data, [5], '退化单系列');

// 数据转换：非数值 → 0
const numConv = normalizeChart({ type: 'bar', labels: ['a'], series: [{ data: ['x', 3] }] });
assert.deepEqual(numConv.series[0].data, [0, 3], '非数值转 0');

console.log('smoke-chart-spec: all assertions passed');
