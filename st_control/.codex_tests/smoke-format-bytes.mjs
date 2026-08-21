// ============================================================
// 通用字节格式化 — 运行期冒烟测试
// 锁定 formatBytes 统一后各组件原语义：
//   dbUtils/DataDashboard（默认 1 位小数）/
//   KbDashboard（GB 2 位小数）/ KbDocs（null 占位、无 GB 分支）
// 运行：node st_control/.codex_tests/smoke-format-bytes.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'format.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'format-bytes.mjs');
writeFileSync(outFile, code);

const mod = await import(pathToFileURL(outFile).href);
const { formatBytes, formatTs, formatIsoTime, formatDate, formatDateOnly } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// 默认语义（dbUtils / DataDashboard 一致）
ok(formatBytes(0) === '0 B', '默认：0 → 0 B');
ok(formatBytes(512) === '512 B', '默认：B 无小数');
ok(formatBytes(1536) === '1.5 KB', '默认：KB 1 位小数');
ok(formatBytes(1048576) === '1.0 MB', '默认：MB 1 位小数');
ok(formatBytes(1610612736) === '1.5 GB', '默认：GB 1 位小数');
ok(formatBytes(1099511627776) === '1.0 TB', '默认：TB 1 位小数');

// KbDashboard：GB 及以上 2 位小数
ok(formatBytes(1610612736, { gbPrecision: 2 }) === '1.50 GB', 'KbDashboard：GB 2 位小数');
ok(formatBytes(1099511627776, { gbPrecision: 2 }) === '1024.00 GB', 'KbDashboard：TB 按 GB 2 位小数（无 TB 分支）');

// KbDocs：null 占位 '-'，无独立 GB 分支（GB 值显示为 MB）
ok(formatBytes(null, { nullPlaceholder: '-', units: ['B', 'KB', 'MB'] }) === '-', 'KbDocs：null → -');
ok(formatBytes(undefined, { nullPlaceholder: '-', units: ['B', 'KB', 'MB'] }) === '-', 'KbDocs：undefined → -');
ok(formatBytes(1610612736, { nullPlaceholder: '-', units: ['B', 'KB', 'MB'] }) === '1536.0 MB', 'KbDocs：1.5GB 按原语义显示为 MB');

// DataDashboard：含 PB 单位
ok(formatBytes(1125899906842624, { units: ['B', 'KB', 'MB', 'GB', 'TB', 'PB'] }) === '1.0 PB', 'DataDashboard：PB 单位');

// 时间格式化：KbDocs 风格（带年份）
const D = new Date(2026, 7, 13, 20, 15); // 2026-08-13 20:15（本地时区）
ok(formatDate(D, { showYear: true }) === '2026-08-13 20:15', 'KbDocs 风格：YYYY-MM-DD HH:mm');
ok(formatIsoTime('2026-08-13 20:15', { showYear: true }) === '2026-08-13 20:15', 'KbDocs 风格：ISO 空格分隔解析');

// KbChat 风格（无年份）
ok(formatDate(D, { showYear: false }) === '08-13 20:15', 'KbChat 风格：MM-DD HH:mm');
ok(formatIsoTime('2026-08-13 20:15', { showYear: false }) === '08-13 20:15', 'KbChat 风格：无年份');

// AutomationPanel 风格（zh-CN locale）
ok(formatIsoTime('2026-08-13 20:15', { showYear: false, useLocale: true }).length > 0, 'AutomationPanel 风格：locale 输出非空');
ok(formatIsoTime('invalid-date', { showYear: true }) === 'invalid-date', '非法 ISO 返回原文');
ok(formatIsoTime('') === '', '空 ISO 返回空串');

// 时间戳自适应（秒/毫秒/微秒）
ok(formatTs(1786622400, { showYear: true }) === '2026-08-13 20:00', '秒级时间戳（UTC+8 本地）');
ok(formatTs(0) === '', '0 时间戳返回空');
ok(formatTs(1786622400000, { showYear: false }) === '08-13 20:00', '毫秒级时间戳');
ok(formatTs(1786622400000000, { showYear: false }) === '08-13 20:00', '微秒级时间戳');

// formatDateOnly（KbDashboard 日期格式）
ok(formatDateOnly('2026-08-13 20:15') === '2026-08-13', '仅日期（空格分隔）');
ok(formatDateOnly('invalid') === 'invalid', '非法日期返回原文');
ok(formatDateOnly('') === '', '空串返回空');

// formatDate dateOnly（GraphView 时间轴刻度）
ok(formatDate(D, { dateOnly: true }) === '2026-08-13', 'dateOnly：YYYY-MM-DD');
ok(formatDate(D, { dateOnly: true, showYear: false }) === '08-13', 'dateOnly：MM-DD');
ok(formatTs(1786622400, { dateOnly: true }) === '2026-08-13', 'dateOnly 秒级时间戳');
ok(formatTs(0, { dateOnly: true, invalidPlaceholder: '' }) === '', 'dateOnly 0 → 占位');

// BackupManager 收敛（fmtSize=gbPrecision 2；fmtDate=formatTs）
ok(formatBytes(1610612736, { gbPrecision: 2 }) === '1.50 GB', 'BackupManager fmtSize 收敛（GB 2 位小数）');
ok(formatTs(1786622400, { showYear: true }) === '2026-08-13 20:00', 'BackupManager fmtDate 收敛（秒级时间戳）');
ok(formatTs(0, { showYear: true }) === '', 'BackupManager fmtDate 空时间戳');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
