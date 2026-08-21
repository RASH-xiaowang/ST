// ============================================================
// 朋友圈视频播放器 — 运行期冒烟测试
// 锁定 momentVideo 下沉后的可观测输出：
//   打开/关闭、解密成功→HTTP 播放、失败→错误态
// 运行：node st_control/.codex_tests/smoke-moment-video.mjs
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

writeFileSync(path.join(outDir, 'momentVideo.svelte.js'), await compileSvelteTs('momentVideo.svelte.ts'));
writeFileSync(path.join(outDir, 'mediaApi.svelte.js'), await compileSvelteTs('mediaApi.svelte.ts'));

// IPC mock：getMomentVideo 结果可控
let videoResult = 'ok';
const mockIpc = `
let result = ${JSON.stringify(videoResult)};
export function __setVideoResult(r) { result = r; }
export async function getApiSettings() {
  return { enabled: true, port: 5032, token: 'tok-1' };
}
export async function getMomentVideo(args) {
  if (result === 'ok') return { kind: 'data', file_key: 'fk-1' };
  if (result === 'empty') return { kind: 'error', error: '视频解密失败' };
  throw new Error('boom');
}
`;
writeFileSync(path.join(outDir, 'ipc.js'), mockIpc);
writeFileSync(path.join(outDir, 'utils.js'), 'export function logError(c, e) { /* noop */ }\n');

writeFileSync(
  path.join(outDir, 'entry.mjs'),
  [
    `export * from './momentVideo.svelte.js';`,
    `export * from './mediaApi.svelte.js';`,
    `export { __setVideoResult } from './ipc.js';`,
  ].join('\n'),
);

await esbuild.build({
  entryPoints: [path.join(outDir, 'entry.mjs')],
  bundle: true,
  platform: 'node',
  format: 'esm',
  outfile: path.join(outDir, 'bundle-moment-video.mjs'),
  logLevel: 'silent',
});

const mod = await import(pathToFileURL(path.join(outDir, 'bundle-moment-video.mjs')).href);
const { momentVideo, playMomentVideo, closeMomentVideo, handleVideoError, mediaApi, loadMediaConfig, __setVideoResult } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

await loadMediaConfig();
ok(mediaApi.videoBase === 'http://127.0.0.1:5032/api/v1/sns/video', 'loadMediaConfig 填充 videoBase');

// 成功路径：解密成功 → HTTP 播放（附带 token）
await playMomentVideo({ text: '朋友圈视频', videos: [{ url: 'http://v/1', key: 'vk' }] }, 0);
ok(momentVideo.open === true, '播放：open = true');
ok(momentVideo.title === '朋友圈视频', '播放：title 来自消息文本');
ok(momentVideo.src === 'http://127.0.0.1:5032/api/v1/sns/video/fk-1?access_token=tok-1', '播放：解密成功拼接 HTTP 播放地址（含 token）');
ok(momentVideo.error === '', '播放：无错误');

// 关闭：清空源与错误
closeMomentVideo();
ok(momentVideo.open === false && momentVideo.src === '' && momentVideo.error === '', '关闭：open/src/error 全部复位');

// 失败路径：后端返回错误
__setVideoResult('empty');
await playMomentVideo({ text: 't', videos: [{ url: 'http://v/1', key: 'vk' }] }, 0);
ok(momentVideo.error === '视频解密失败', '解密失败：显示后端错误');
ok(momentVideo.src === '', '解密失败：无播放源');

// 异常路径：IPC 抛错
__setVideoResult('throw');
await playMomentVideo({ text: 't', videos: [{ url: 'http://v/1', key: 'vk' }] }, 0);
ok(momentVideo.error === '视频加载失败', 'IPC 异常：显示加载失败');

// 视频元素加载失败（onerror 下沉逻辑）
handleVideoError();
ok(momentVideo.error === '视频播放失败（文件可能已失效）' && momentVideo.src === '', '播放失败：清空源并显示错误');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
