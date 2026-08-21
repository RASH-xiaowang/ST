// ============================================================
// 用量/成本展示格式化 — 运行期冒烟测试
// 锁定 costFormat 下沉后的可观测输出：
//   不限额度 / 千分位 / 使用率
// 运行：node st_control/.codex_tests/smoke-cost-format.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'llm', 'costFormat.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'cost-format.mjs');
writeFileSync(outFile, code);

const mod = await import(pathToFileURL(outFile).href);
const { fmtLimit, fmtRatio } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

ok(fmtLimit(null) === '不限' && fmtLimit(undefined) === '不限', 'null/undefined → 不限');
ok(fmtLimit(1234567) === '1,234,567', '千分位');
ok(fmtLimit(0) === '0', '零原样');
ok(fmtRatio(12.345) === '12.3%', '使用率 1 位小数');
ok(fmtRatio(100) === '100.0%', '整数补 1 位小数');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
