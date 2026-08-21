// ============================================================
// 年度总结展示纯函数 — 运行期冒烟测试
// 锁定 wechat/utils/annual 下沉后的可观测输出：
//   热力色 / 数量缩写 / 千分位 / 占比
// 运行：node st_control/.codex_tests/smoke-annual-summary.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'wechat', 'utils', 'annual.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'annual-summary.mjs');
writeFileSync(outFile, code);

const mod = await import(pathToFileURL(outFile).href);
const { heatPeak, hourShare, bestIndex, calmIndex, weekendShareOf, buildPersonaTags, peakInfoOf } = mod;
const { heatBg, fmtNum, fmtInt, pct } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// 热力色
ok(heatBg(0, 100) === 'rgba(7,193,96,0.05)' && heatBg(5, 0) === 'rgba(7,193,96,0.05)', '零值/零分母 → 最浅色');
ok(heatBg(100, 100) === 'rgba(7,193,96,0.930)', '满值 → 最深色（0.08+0.85）');
ok(heatBg(50, 100) === 'rgba(7,193,96,0.505)', '半值中间透明度');

// 数量缩写
ok(fmtNum(9999) === '9999', '万以下原样');
ok(fmtNum(12345) === '1.2万', '万缩写 1 位小数');
ok(fmtNum(20000) === '2万', '整万去尾 0');
ok(fmtNum(NaN) === '0', 'NaN → 0');

// 千分位
ok(fmtInt(1234567) === '1,234,567', '千分位');
ok(fmtInt(NaN) === '0', 'NaN → 0');

// 占比
ok(pct(25, 200) === 12.5, '占比 0.1% 精度');
ok(pct(10, 0) === 0, '零分母 → 0');
ok(pct(1, 3) === 33.3, '四舍五入');

// 热力峰值
const hm = [
  [0, 5, 3],
  [2, 9, 1],
];
ok(JSON.stringify(heatPeak(hm)) === JSON.stringify({ w: 1, h: 1, value: 9 }), '峰值坐标与值');
ok(heatPeak([]) === null, '空矩阵 → null');
ok(heatPeak([[]]).value === 0, '全 0 → 首个坐标 0');

// ── peakInfoOf（峰值展示，自 AnnualSummary peakInfo 下沉） ──
ok(JSON.stringify(peakInfoOf({ weekdayLabels: ['周一', '周二'] }, hm)) ===
  JSON.stringify({ weekday: '周二', hour: '01', value: 9 }), '星期标签 + 小时补零');
ok(JSON.stringify(peakInfoOf({ weekdayLabels: undefined }, hm)) ===
  JSON.stringify({ weekday: '', hour: '01', value: 9 }), '标签缺失回退空串');
ok(JSON.stringify(peakInfoOf(null, [])) ===
  JSON.stringify({ weekday: '', hour: '', value: 0 }), '空矩阵默认值');
ok(peakInfoOf(undefined, hm).weekday === '', 'heatmap 缺省容错');

// 小时占比
ok(hourShare([[25, 50], [25, 0]], [1]) === 50, '指定小时占总热力 50%');
ok(hourShare([], [0]) === 0, '空矩阵 → 0');
ok(hourShare([[5]], [2]) === 0, '未命中小时 → 0');

// 最佳/最静月份
ok(bestIndex([0, 5, 10, 3]) === 2, '最大正值索引');
ok(bestIndex([0, 0, 0]) === -1, '全 0 → -1');
ok(bestIndex([]) === -1, '空 → -1');
ok(calmIndex([0, 5, 2, 8]) === 2, '最小正值索引');
ok(calmIndex([0, 0, 0]) === 0, '无正值取首个非正值');
ok(calmIndex([]) === -1, '空 → -1');

// 周末占比
ok(weekendShareOf(Array.from({ length: 7 }, () => [10])) === 28.6, '周末两行占总热力 28.6%');
ok(weekendShareOf([]) === 0, '空矩阵 → 0');
ok(weekendShareOf(Array.from({ length: 6 }, () => [1])) === 0, '行数不足 7 → 0');

// 人物标签
ok(buildPersonaTags({ nightShare: 40, morningShare: 0, weekendShare: 50, groupShare: 60, dayAvg: 80 }).includes('夜猫子'), '夜猫子');
ok(buildPersonaTags({ nightShare: 10, morningShare: 30, weekendShare: 10, groupShare: 10, dayAvg: 5 }).includes('早起党'), '早起党');
ok(buildPersonaTags({ nightShare: 0, morningShare: 0, weekendShare: 0, groupShare: 0, dayAvg: 0 }).includes('作息均衡'), '默认均衡');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
