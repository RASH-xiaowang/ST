// ============================================================
// 微信安全/时间展示纯函数 — 运行期冒烟测试
// 锁定 security 下沉后的可观测输出：
//   令牌长度与 hex 格式 / 最后活跃日期格式化
// 运行：node st_control/.codex_tests/smoke-wechat-security.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'wechat', 'utils', 'security.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'wechat-security.mjs');
writeFileSync(outFile, code);

// mock crypto.getRandomValues：确定性填充（Node 的 globalThis.crypto 只读，替换方法）
Object.defineProperty(globalThis.crypto, 'getRandomValues', {
  value: (buf) => {
    for (let i = 0; i < buf.length; i++) buf[i] = i % 256;
    return buf;
  },
  configurable: true,
});

const mod = await import(pathToFileURL(outFile).href);
const { generateApiToken, fmtLastActive } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// 令牌
const t1 = generateApiToken();
ok(t1.length === 64, '令牌 64 字符');
ok(/^[0-9a-f]{64}$/.test(t1), '令牌为小写 hex');
const expected = Array.from({ length: 32 }, (_, i) => (i % 256).toString(16).padStart(2, '0')).join('');
ok(t1 === expected, '确定性 mock 下输出与预期一致');
// 实际随机性由 crypto.getRandomValues 保证；这里验证长度/格式即可

// 日期
ok(fmtLastActive(0) === '未知', '0 时间戳 → 未知');
ok(fmtLastActive(NaN) === '未知', 'NaN → 未知');
ok(fmtLastActive(1786622400) === '2026-08-13', '时间戳 → YYYY-MM-DD');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
