// ============================================================
// Hook 状态展示纯函数 — 运行期冒烟测试
// 锁定 wechat/utils/hook 下沉后的可观测输出：
//   hookStatusLabel / hookStatusCls
// 运行：node st_control/.codex_tests/smoke-hook.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'wechat', 'utils', 'hook.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'hook.mjs');
writeFileSync(outFile, code);

const { hookStatusLabel, hookStatusCls } = await import(pathToFileURL(outFile).href);

const mk = (p) => ({
  supported: true,
  enabled: true,
  hooked: false,
  pid: null,
  whitelist: [],
  error: '',
  dll_ok: true,
  ...p,
});

assert.equal(hookStatusLabel(null), '检测中…');
assert.equal(hookStatusLabel(mk({ supported: false })), '不支持');
assert.equal(hookStatusLabel(mk({ enabled: false })), '未启用');
assert.equal(hookStatusLabel(mk({ dll_ok: false })), 'DLL 缺失');
assert.equal(hookStatusLabel(mk({ hooked: true })), '正在监控');
assert.equal(hookStatusLabel(mk({})), '等待连接');

assert.equal(hookStatusCls(null), 'hm-status-pending');
assert.equal(hookStatusCls(mk({ enabled: false })), 'hm-status-off');
assert.equal(hookStatusCls(mk({ dll_ok: false })), 'hm-status-err');
assert.equal(hookStatusCls(mk({ hooked: true })), 'hm-status-on');
assert.equal(hookStatusCls(mk({})), 'hm-status-pending');

rmSync(outDir, { recursive: true, force: true });
console.log('smoke-hook: all assertions passed');
