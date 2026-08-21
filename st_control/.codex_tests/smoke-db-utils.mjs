// ============================================================
// 数据库管理工具 — 运行期冒烟测试
// 锁定 dbUtils 下沉后的可观测输出：
//   CSV 转义 / base64 / BLOB 识别与扩展名 / 字节格式化 /
//   fmtTsValue 时间戳单元格格式化
// 运行：node st_control/.codex_tests/smoke-db-utils.mjs
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

async function compileTs(rel) {
  const src = readFileSync(path.join(root, 'src', 'lib', rel), 'utf8');
  const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
  return code;
}

// dbUtils 依赖 ../format：按源目录结构落位（out/lib/db/ + out/lib/format.mjs），
// 保持相对导入可解析
const libOut = path.join(outDir, 'lib');
mkdirSync(libOut, { recursive: true });
mkdirSync(path.join(libOut, 'db'), { recursive: true });
// 编译产物保留源文件的 `../format`（无扩展名）：补 .mjs 以支持 esbuild 解析
const dbUtilsOut = (await compileTs('db/dbUtils.ts')).replace('from "../format"', 'from "../format.mjs"');
writeFileSync(path.join(libOut, 'db', 'dbUtils.mjs'), dbUtilsOut);
writeFileSync(path.join(libOut, 'format.mjs'), await compileTs('format.ts'));
writeFileSync(path.join(outDir, 'entry.mjs'), "export * from './lib/db/dbUtils.mjs';");

await esbuild.build({
  entryPoints: [path.join(outDir, 'entry.mjs')],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  outfile: path.join(outDir, 'bundle-db-utils.cjs'),
  logLevel: 'silent',
});

const { createRequire } = await import('node:module');
const require = createRequire(import.meta.url);
const mod = require(path.join(outDir, 'bundle-db-utils.cjs'));
const { csvEscape, utf8ToBase64, isBlobPreview, blobDataUrl, blobExt, fmtBytes, fmtTsValue, TS_COLS, groupDbTables, groupDbFilesByRoot } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// CSV 转义
ok(csvEscape('plain') === 'plain', 'CSV 普通值不转义');
ok(csvEscape('a,b') === '"a,b"', 'CSV 含逗号加引号');
ok(csvEscape('say "hi"') === '"say ""hi"""', 'CSV 引号双写');
ok(csvEscape(null) === '', 'CSV null → 空串');
ok(csvEscape(undefined) === '', 'CSV undefined → 空串');

// UTF-8 → base64（中文安全）
ok(utf8ToBase64('中文') === '5Lit5paH', 'UTF-8 base64 中文编码正确');
ok(utf8ToBase64('a') === 'YQ==', 'UTF-8 base64 ASCII 编码正确');

// BLOB 识别 / data URL / 扩展名
ok(isBlobPreview('hex 1A2B…[64 bytes]'), 'BLOB 预览文本识别');
ok(!isBlobPreview('plain text'), '普通文本不误判为 BLOB');
ok(blobDataUrl({ mime: 'image/png', base64: 'AAA' }) === 'data:image/png;base64,AAA', 'BLOB data URL 拼接');
ok(blobDataUrl({ base64: 'AAA' }) === 'data:application/octet-stream;base64,AAA', 'BLOB 缺省 MIME');
ok(blobExt('image/png') === 'png', 'MIME → 扩展名');
ok(blobExt('application/pdf') === 'pdf', 'PDF 扩展名');
ok(blobExt('unknown/type') === 'bin', '未知 MIME → bin');

// 字节格式化
ok(fmtBytes(0) === '0 B', '字节格式化 0');
ok(fmtBytes(512) === '512 B', '字节格式化 B');
ok(fmtBytes(1536) === '1.5 KB', '字节格式化 KB（1 位小数）');
ok(fmtBytes(1048576) === '1.0 MB', '字节格式化 MB');

// fmtTsValue 时间戳单元格格式化（自 DbManager 下沉，行为逐字保持）
ok(TS_COLS.length === 7, '时间戳列白名单 7 项');
ok(TS_COLS.includes('ts') && TS_COLS.includes('create_time'), '白名单含 ts/create_time');
// 1786622400 = 2026-08-13 20:00:00（本地时区 UTC+8）
ok(fmtTsValue(1786622400, 'ts') === '2026-08-13 20:00:00', '秒级时间戳（含秒）');
ok(fmtTsValue('1786622400', 'ts') === '2026-08-13 20:00:00', '字符串数字时间戳');
ok(fmtTsValue(1786622400000, 'create_time') === '2026-08-13 20:00:00', '毫秒级时间戳换算');
ok(fmtTsValue(1786622400, 'other_col') === null, '非时间列返回 null');
ok(fmtTsValue(1786622400, 'TS') === null, '列名大小写敏感');
ok(fmtTsValue(null, 'ts') === null, 'null 返回 null');
ok(fmtTsValue(undefined, 'ts') === null, 'undefined 返回 null');
ok(fmtTsValue('', 'ts') === null, '空串返回 null');
ok(fmtTsValue('abc', 'ts') === null, '非数字返回 null');
ok(fmtTsValue(0, 'ts') === null, '0 返回 null');
ok(fmtTsValue(-5, 'ts') === null, '负数返回 null');
ok(fmtTsValue(123, 'ts') === null, '超出有效窗口（过小）返回 null');
ok(fmtTsValue(1e15, 'ts') === null, '超出有效窗口（过大）返回 null');
ok(fmtTsValue('  1786622400  ', 'ts') === '2026-08-13 20:00:00', '空白环绕值仍解析');

// ── groupDbTables（表列表分组，自 DbManager dbTableSections 下沉） ──
const tables = ['a', 'b', 'c', 'doc'];
ok(JSON.stringify(groupDbTables(tables, ['c'], '')) ===
  JSON.stringify([{ label: '★ 收藏', tables: ['c'] }, { label: '全部表', tables: ['a', 'b', 'doc'] }]),
  '收藏优先分组');
ok(JSON.stringify(groupDbTables(tables, [], '')) ===
  JSON.stringify([{ label: '全部表', tables: ['a', 'b', 'c', 'doc'] }]),
  '无收藏仅全部');
ok(JSON.stringify(groupDbTables(tables, ['c'], 'DO')) ===
  JSON.stringify([{ label: '匹配「do」', tables: ['doc'] }]),
  '搜索过滤（大小写不敏感）+ 无收藏命中');
ok(JSON.stringify(groupDbTables(['a'], ['x'], '')) ===
  JSON.stringify([{ label: '全部表', tables: ['a'] }]),
  '收藏不在列表中被排除');

// ── groupDbFilesByRoot（外部库按扫描根分组，自 DbManager 下沉） ──
const files = [
  { path: 'D:/data/a/x.db', name: 'x.db' },
  { path: 'D:/data/b/y.db', name: 'y.db' },
  { path: 'D:/other/z.db', name: 'z.db' },
];
const roots = ['D:/data/a', 'D:/data'];
const grouped = groupDbFilesByRoot(files, roots);
ok(grouped.length === 3, '分组数');
ok(grouped.find((g) => g.dir === 'D:/data/a').files.length === 1, '最长前缀命中（data/a 优先于 data）');
ok(grouped.find((g) => g.dir === 'D:/data').files.length === 1, '前缀命中 data 根');
ok(grouped.find((g) => g.dir === 'D:/other').files.length === 1, '未命中根按所在目录');
ok(grouped.find((g) => g.dir === 'D:/other').dirName === 'other', '目录名取末段');
ok(groupDbFilesByRoot([], roots).length === 0, '空列表');
ok(groupDbFilesByRoot(files, []).every((g) => !g.dir.startsWith('D:/data') || g.dir.includes('data')), '无根时按目录分组');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
