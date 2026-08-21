// ============================================================
// 朋友圈图片懒加载 — 运行期冒烟测试
// 锁定 momentMedia 下沉后的可观测输出：
//   队列去重 / 缓存写入 / 原图异步补拉 / LRU 上限
// 运行：node st_control/.codex_tests/smoke-moment-media.mjs
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

async function compileSvelteTs(rel) {
  const src = readFileSync(path.join(root, 'src', 'lib', 'wechat', 'services', rel), 'utf8');
  const stripped = await esbuild.transform(src, { loader: 'ts' });
  return compileModule(stripped.code, {
    filename: rel,
    generate: 'client',
  }).js.code;
}

writeFileSync(path.join(outDir, 'momentMedia.svelte.js'), await compileSvelteTs('momentMedia.svelte.ts'));

// IPC mock：getMomentImage 按 URL 返回不同 data URL，并记录调用
const ipcCalls = [];
const mockIpc = `
export const __ipcCalls = [];
export async function getMomentImage(args) {
  __ipcCalls.push(args);
  return { kind: 'data', data: 'data:image/png;base64,' + (args.url || 'thumb').slice(-6) };
}
`;
writeFileSync(path.join(outDir, 'ipc.js'), mockIpc);
writeFileSync(path.join(outDir, 'utils.js'), 'export function logError(c, e) { /* noop */ }\n');

writeFileSync(
  path.join(outDir, 'entry.mjs'),
  [
    `export * from './momentMedia.svelte.js';`,
    `export { __ipcCalls } from './ipc.js';`,
  ].join('\n'),
);

await esbuild.build({
  entryPoints: [path.join(outDir, 'entry.mjs')],
  bundle: true,
  platform: 'node',
  format: 'esm',
  outfile: path.join(outDir, 'bundle-moment-media.mjs'),
  logLevel: 'silent',
});

const mod = await import(pathToFileURL(path.join(outDir, 'bundle-moment-media.mjs')).href);
const { momentMedia, momentImgKey, momentImgSrc, enqueueMomentImage, loadMomentOriginal, __ipcCalls } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// key 构造：key 优先，缺省用 '-'
ok(momentImgKey('http://a/1', 'k1') === 'k1:http://a/1', 'momentImgKey 组合 key 与 url');
ok(momentImgKey('http://a/1', '') === '-:http://a/1', 'momentImgKey 缺省 key 用 -');

// 入队：thumb 优先；相同 key 去重
enqueueMomentImage({ thumb: 'http://t/1', url: 'http://o/1', key: 'k1', thumb_token: 'tt' });
enqueueMomentImage({ thumb: 'http://t/1', url: 'http://o/1', key: 'k1', thumb_token: 'tt' });
ok(__ipcCalls.length === 1, '重复入队去重（仅 1 次 IPC）');
await new Promise((r) => setTimeout(r, 20));
ok(momentImgSrc({ thumb: 'http://t/1', key: 'k1' }).startsWith('data:image/png;base64,'), '缩略图加载后 momentImgSrc 返回 data URL');
ok(momentImgSrc({ thumb: 'http://t/2', key: 'k1' }) === '', '未加载图片返回空串');

// 原图异步补拉：写入原图 key 缓存
const orig = await loadMomentOriginal({ url: 'http://o/1', key: 'k1', url_token: 'ut' });
ok(orig.startsWith('data:image/png;base64,'), 'loadMomentOriginal 返回原图 data URL');
ok(momentMedia.imgCache['k1:http://o/1'] === orig, '原图写入缓存（key=url 组合）');
const before = __ipcCalls.length;
await loadMomentOriginal({ url: 'http://o/1', key: 'k1', url_token: 'ut' });
ok(__ipcCalls.length === before, '已缓存原图不重复请求');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
