// ============================================================
// OCR 展示纯函数 — 运行期冒烟测试
// 锁定 ocr/display 下沉后的可观测输出：
//   JSON 美化 / 状态标签与徽章 / 分类标签 / 常量完整性
// 运行：node st_control/.codex_tests/smoke-ocr-display.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'ocr', 'display.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'ocr-display.mjs');
writeFileSync(outFile, code);

const mod = await import(pathToFileURL(outFile).href);
const { prettyJson, statusLabel, statusCls, catLabel, STATUS_META, CATEGORY_ORDER, COMMON_ENDPOINTS } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// JSON 美化
ok(prettyJson('') === '（空）' && prettyJson('{}') === '（空）', '空/{} → 占位');
ok(prettyJson('{"a":1}') === '{\n  "a": 1\n}', '合法 JSON 缩进美化');
ok(prettyJson('not-json') === 'not-json', '非法 JSON 原样返回');

// 状态标签与徽章
ok(statusLabel('success') === '识别成功', '已知状态中文标签');
ok(statusLabel('unknown') === 'unknown', '未知状态原样');
ok(statusCls('failed') === 'destructive', '失败徽章类');
ok(statusCls('pending') === 'outline', '待处理徽章类');
ok(statusCls('unknown') === 'outline', '未知状态回退 outline');
ok(Object.keys(STATUS_META).length === 7, '状态元数据完整（7 项）');

// 分类标签
ok(catLabel('id_card') === '身份证', '分类中文标签');
ok(catLabel('') === '未分类', '空分类 → 未分类');
ok(catLabel('new_cat') === 'new_cat', '未知分类原样');

// 常量完整性
ok(CATEGORY_ORDER.length === 24 && CATEGORY_ORDER[0] === 'id_card', '分类顺序完整');
ok(COMMON_ENDPOINTS.length === 12, '常用端点完整');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
