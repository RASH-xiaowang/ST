// ============================================================
// 通用异步工具 — 运行期冒烟测试
// 锁定 delay 下沉后的行为：按毫秒延时后 resolve
// 运行：node st_control/.codex_tests/smoke-async-utils.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'async.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'async-utils.mjs');
writeFileSync(outFile, code);

const mod = await import(pathToFileURL(outFile).href);
const { delay } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

const t0 = Date.now();
await delay(30);
const elapsed = Date.now() - t0;
ok(elapsed >= 25, 'delay(30) 至少等待约 30ms');
ok(await delay(0).then(() => 'ok') === 'ok', 'delay(0) 立即 resolve');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
