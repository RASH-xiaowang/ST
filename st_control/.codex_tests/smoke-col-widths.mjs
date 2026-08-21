// ============================================================
// 数据库列宽持久化格式 — 运行期冒烟测试
// 锁定 colWidths 下沉后的可观测输出：
//   数据源 key 派生 / 键拼接 / 配置解析（含非法项过滤）
// 运行：node st_control/.codex_tests/smoke-col-widths.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'db', 'colWidths.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'col-widths.mjs');
writeFileSync(outFile, code);

const mod = await import(pathToFileURL(outFile).href);
const { dbWidthKeyFromPath, colWidthKey, parseColWidths } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// 数据源 key 派生
ok(dbWidthKeyFromPath(null) === 'internal', '无外部路径 → internal');
ok(dbWidthKeyFromPath('C:/data/ext.db') === 'ext.db', '外部库取文件名');
ok(dbWidthKeyFromPath('E:\\dir\\wx.db') === 'wx.db', '反斜杠路径取文件名');
ok(dbWidthKeyFromPath('/') === 'ext', '空文件名回退 ext');

// 键拼接
ok(colWidthKey('internal', 'messages', 'content') === 'col_width:internal:messages:content', '配置键拼接格式');

// 配置解析
const items = [
  { key: 'col_width:internal:messages:content', value: '180' },
  { key: 'col_width:internal:messages:time', value: '90' },
  { key: 'col_width:ext.db:messages:id', value: '60' },
  { key: 'unrelated:key', value: '1' },          // 非列宽项忽略
  { key: 'col_width:broken', value: '50' },       // 无分隔符忽略
  { key: 'col_width:internal:messages:bad', value: '0' }, // 非正数忽略
];
const w = parseColWidths(items);
ok(w['internal:messages:content'] === 180, '解析有效列宽（数字）');
ok(w['internal:messages:time'] === 90, '解析多列');
ok(w['ext.db:messages:id'] === 60, '解析外部库条目');
ok(w['unrelated:key'] === undefined, '非列宽项过滤');
ok(w['col_width:broken'] === undefined, '格式非法项过滤');
ok(w['internal:messages:bad'] === undefined, '非正数值过滤');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
