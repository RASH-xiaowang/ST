// ============================================================
// 系统指标展示格式化 — 运行期冒烟测试
// 锁定 system/format 下沉后的可观测输出：
//   历史窗口 / 速率 / 带宽 / 在线时长 / 颜色 / 百分比
// 运行：node st_control/.codex_tests/smoke-system-format.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'system', 'format.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'system-format.mjs');
writeFileSync(outFile, code);

const mod = await import(pathToFileURL(outFile).href);
const { pushHist, fmtRate, fmtLink, fmtUptime, colorFor, fmtPct, HIST } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// 历史窗口
ok(pushHist([1, 2], 3).join(',') === '1,2,3', '追加新值');
const full = Array.from({ length: HIST }, (_, i) => i);
const rolled = pushHist(full, 999);
ok(rolled.length === HIST && rolled[rolled.length - 1] === 999 && rolled[0] === 1, '超限移除最旧');

// 速率
ok(fmtRate(0) === '0 B/s' && fmtRate(NaN) === '0 B/s', '0/NaN → 0 B/s');
ok(fmtRate(512) === '512 B/s', 'B/s 无小数');
ok(fmtRate(1536) === '1.5 KB/s', 'KB/s 1 位小数');
ok(fmtRate(1048576) === '1.0 MB/s', 'MB/s');

// 带宽
ok(fmtLink(0) === '--' && fmtLink(NaN) === '--', '0/NaN → --');
ok(fmtLink(100e6) === '100 Mbps', 'Mbps 取整');
ok(fmtLink(1.5e9) === '1.5 Gbps', 'Gbps 1 位小数');

// 在线时长
ok(fmtUptime(45) === '0分 45秒', '秒级');
ok(fmtUptime(3725) === '1时 2分 5秒', '时级');
ok(fmtUptime(90061) === '1天 1时 1分', '天级');

// 颜色阈值
ok(colorFor(49) === '#22d3ee' && colorFor(50) === '#fbbf24', '50 边界黄色');
ok(colorFor(74) === '#fbbf24' && colorFor(75) === '#fb923c', '75 边界橙色');
ok(colorFor(90) === '#f87171', '90+ 红色');

// 百分比
ok(fmtPct(null) === 'N/A' && fmtPct(undefined) === 'N/A', 'null/undefined → N/A');
ok(fmtPct(12.345) === '12.3%', '1 位小数 + %');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
