// ============================================================
// 汇总展示纯函数 — 运行期冒烟测试
// 锁定 wechat/utils/summary 下沉后的可观测输出：
//   日期时间 / 时长 / 数量格式化边界
// 运行：node st_control/.codex_tests/smoke-daily-summary.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'wechat', 'utils', 'summary.ts'), 'utf8');
// summary.ts 依赖 ../../format（fmtTime 收敛到 formatTs），需 bundle 解析，产物自包含。
const build = await esbuild.build({
  stdin: {
    contents: src,
    resolveDir: path.join(root, 'src', 'lib', 'wechat', 'utils'),
    loader: 'ts',
    sourcefile: 'summary.ts',
  },
  bundle: true,
  write: false,
  format: 'esm',
  platform: 'node',
  logLevel: 'silent',
});
const code = build.outputFiles[0].text;
const outFile = path.join(outDir, 'daily-summary.mjs');
writeFileSync(outFile, code);

const mod = await import(pathToFileURL(outFile).href);
const { summarizeRecords } = mod;
const { fmtTime, fmtDate, fmtDuration, fmtTokens } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// 时间（本地时区）
const D = new Date(2026, 7, 13, 20, 15);
ok(fmtTime(D.getTime()) === '2026-08-13 20:15', '毫秒时间戳 → 完整日期时间');
ok(fmtTime(0) === '—' && fmtTime(undefined) === '—', '空时间戳 → —');
ok(fmtTime(NaN) === '—', 'NaN → —');

// 日期
ok(fmtDate(D) === '2026-08-13', 'Date → YYYY-MM-DD');

// 时长
ok(fmtDuration(0) === '' && fmtDuration(-5) === '', '≤0 → 空');
ok(fmtDuration(500) === '500 ms', '毫秒级');
ok(fmtDuration(1500) === '1.5 s', '秒级 1 位小数');

// 数量
ok(fmtTokens(0) === '' && fmtTokens(undefined) === '', '≤0 → 空');
ok(fmtTokens(999) === '999', '万以下原样');
ok(fmtTokens(12345) === '1.2万', '万缩写');
ok(fmtTokens(20000) === '2万', '整万去尾 0');

// 记录统计
const rs = summarizeRecords([
  { status: 'done', char_count: 100 },
  { status: 'done', char_count: 200 },
  { status: 'error' },
]);
ok(rs.total === 3 && rs.ok === 2 && rs.fail === 1, '总数/成功/失败');
ok(rs.avgChars === 150, '成功平均字符数');
ok(summarizeRecords([]).avgChars === 0, '空记录平均 0');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
