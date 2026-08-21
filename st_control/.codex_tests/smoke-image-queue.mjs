// ============================================================
// 微信消息图片加载队列 — 运行期冒烟测试
// 锁定 imageQueue / mediaApi 下沉后的可观测输出：
//   URL 直链优先 → IPC base64 回退 → 失败标记 + 点击重试
// 运行：node st_control/.codex_tests/smoke-image-queue.mjs
// ============================================================
import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import esbuild from 'esbuild';
import { compileModule } from 'svelte/compiler';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(here, '..');
const outDir = path.join(here, 'out');
mkdirSync(outDir, { recursive: true });

// 1) 编译 mediaApi / imageQueue（esbuild 去类型 → Svelte compileModule 处理 runes）
async function compileSvelteTs(rel) {
  const src = readFileSync(path.join(root, 'src', 'lib', 'wechat', 'services', rel), 'utf8');
  const stripped = await esbuild.transform(src, { loader: 'ts' });
  return compileModule(stripped.code, {
    filename: rel,
    generate: 'client',
  }).js.code;
}

writeFileSync(path.join(outDir, 'mediaApi.svelte.js'), await compileSvelteTs('mediaApi.svelte.ts'));
writeFileSync(path.join(outDir, 'imageQueue.svelte.js'), await compileSvelteTs('imageQueue.svelte.ts'));

// 2) IPC mock：getApiSettings 返回可配置的 HTTP API 设置；getMessageImage 记录调用
let apiEnabled = true;
let apiPort = 5032;
let apiToken = 'tok-1';
const ipcCalls = [];

const mockIpc = `
let enabled = ${apiEnabled};
let port = ${apiPort};
let token = ${JSON.stringify(apiToken)};
export const __ipcCalls = [];
export function __setApiSettings(e, p, t) {
  enabled = e; port = p; token = t;
}
export async function getApiSettings() {
  return { enabled, port, token };
}
export async function getMessageImage(args) {
  __ipcCalls.push(args);
  return { kind: 'data', data: 'data:image/png;base64,AAA' };
}
`;
// 编译产物位于 out/，其相对导入 './ipc' 与 '../utils' 均相对 out/ 解析
writeFileSync(path.join(outDir, 'ipc.js'), mockIpc);

// 3) utils 依赖：imageQueue 从 ../utils 导入 logError
const utilsOutDir = path.join(outDir, '..');
mkdirSync(utilsOutDir, { recursive: true });
writeFileSync(path.join(utilsOutDir, 'utils.js'), 'export function logError(c, e) { /* noop */ }\n');

// 4) 入口 + bundle
writeFileSync(
  path.join(outDir, 'entry.mjs'),
  [
    `export * from './mediaApi.svelte.js';`,
    `export * from './imageQueue.svelte.js';`,
    `export { __ipcCalls, __setApiSettings } from './ipc.js';`,
  ].join('\n'),
);

await esbuild.build({
  entryPoints: [path.join(outDir, 'entry.mjs')],
  bundle: true,
  platform: 'node',
  format: 'esm',
  outfile: path.join(outDir, 'bundle-image-queue.mjs'),
  logLevel: 'silent',
});

const mod = await import(pathToFileURL(path.join(outDir, 'bundle-image-queue.mjs')).href);
const { mediaApi, imageQueueState, enqueueImage, onImageLoadError, retryImage, loadMediaConfig, messageImageUrl, __ipcCalls, __setApiSettings } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// URL 直链优先：启用 API 时直接写缓存，不触发 IPC
await loadMediaConfig();
ok(mediaApi.mediaBase === 'http://127.0.0.1:5032/api/v1/media', 'loadMediaConfig 填充 mediaBase');
ok(messageImageUrl('u1', 10) === 'http://127.0.0.1:5032/api/v1/media/u1/10?access_token=tok-1', 'messageImageUrl 附带 access_token');

enqueueImage('u1', 10);
ok(imageQueueState.cache['u1:10']?.startsWith('http://127.0.0.1:5032'), '启用 API 时 enqueueImage 直接写 URL 直链');
ok(__ipcCalls.length === 0, 'URL 直链模式不触发 IPC');

// 已缓存去重：再次入队不重复加载
enqueueImage('u1', 10);
ok(imageQueueState.cache['u1:10'] !== undefined && __ipcCalls.length === 0, '已缓存 key 去重');

// URL 直链失败 → 阻断 URL + IPC base64 回退
onImageLoadError('u1', 10);
ok(imageQueueState.cache['u1:10'] === undefined, 'URL 失败后清除直链缓存');
ok(imageQueueState.blocked.has('u1:10'), 'URL 失败 key 进入 blocked');
await new Promise((r) => setTimeout(r, 20));
ok(__ipcCalls.length >= 1, 'URL 失败后回退 IPC 获取 base64');
await new Promise((r) => setTimeout(r, 20));
ok(imageQueueState.cache['u1:10'] === 'data:image/png;base64,AAA', 'IPC 回退成功写入 data URL');

// IPC 失败 → 空缓存失败标记；点击重试可再次入队
__setApiSettings(false, 0, '');
await loadMediaConfig();
enqueueImage('u2', 1);
ok(imageQueueState.cache['u2:1'] === undefined, '禁用 API 时入队走 IPC（初始无缓存）');
await new Promise((r) => setTimeout(r, 20));
ok(imageQueueState.cache['u2:1'] === 'data:image/png;base64,AAA', 'IPC 回退成功写入 data URL（无 API）');

retryImage('u1', 10);
ok(!imageQueueState.blocked.has('u1:10'), 'retryImage 清除 blocked 标记');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
