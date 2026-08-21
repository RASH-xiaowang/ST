// ============================================================
// 消息通道发送步骤状态机 + 文件元信息 — 运行期冒烟测试
// 锁定 bot/steps.ts / bot/fileMeta.ts 下沉后的可观测输出：
//   非 media 简化 / 推进顺序 / 错误定位 / 文件类型分类
// 运行：node st_control/.codex_tests/smoke-bot-steps.mjs
// ============================================================
import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import esbuild from 'esbuild';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(here, '..');
const outDir = path.join(here, 'out');
mkdirSync(outDir, { recursive: true });

const src = readFileSync(path.join(root, 'src', 'lib', 'bot', 'steps.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'bot-steps.mjs');
writeFileSync(outFile, code);

const { stepState } = await import(pathToFileURL(outFile).href);
const S = (k, m, st, e = '') => stepState(k, m, st, e);

// ── fileMetaOf（bot/fileMeta.ts，自 BotPanel fileMeta 下沉） ──
const fmSrc = readFileSync(path.join(root, 'src', 'lib', 'bot', 'fileMeta.ts'), 'utf8');
const fmOut = path.join(outDir, 'bot-file-meta.mjs');
writeFileSync(fmOut, (await esbuild.transform(fmSrc, { loader: 'ts', format: 'esm' })).code);
const { fileMetaOf } = await import(pathToFileURL(fmOut).href);
assert.deepEqual(fileMetaOf('C:/dir/photo.PNG'), { name: 'photo.PNG', kind: 'image' }, '图片（大小写不敏感后缀）');
assert.deepEqual(fileMetaOf('/data/video.mp4'), { name: 'video.mp4', kind: 'video' }, '视频');
assert.deepEqual(fileMetaOf('D:\\dir\\voice.silk'), { name: 'voice.silk', kind: 'audio' }, '音频（反斜杠路径）');
assert.deepEqual(fileMetaOf('doc.pdf'), { name: 'doc.pdf', kind: 'file' }, '未知类型回退 file');
assert.deepEqual(fileMetaOf('a.heif'), { name: 'a.heif', kind: 'image' }, 'HEIF 图片');
assert.deepEqual(fileMetaOf('noext'), { name: 'noext', kind: 'file' }, '无扩展名回退 file');
assert.deepEqual(fileMetaOf('/'), { name: '', kind: 'file' }, '纯分隔符路径容错');

// 非 media 模式：仅 send 步骤按 done 显示
assert.equal(S('send', 'text', 'done'), 'done', 'text 模式 done');
assert.equal(S('send', 'text', 'idle'), 'pending', 'text 模式未完成');
assert.equal(S('prep', 'text', 'done'), 'pending', 'text 模式 prep 恒 pending');

// media + idle / preparing
assert.equal(S('prep', 'media', 'idle'), 'pending', 'idle prep pending');
assert.equal(S('upload', 'media', 'idle'), 'pending', 'idle upload pending');
assert.equal(S('prep', 'media', 'preparing'), 'active', 'preparing prep active');

// media + error：读取文件失败定位 prep
assert.equal(S('prep', 'media', 'error', '读取文件失败'), 'error', '读取文件错误 prep');
assert.equal(S('upload', 'media', 'error', '读取文件失败'), 'pending', '读取文件错误 upload pending');
assert.equal(S('send', 'media', 'error', '读取文件失败'), 'pending', '读取文件错误 send pending');

// media + error：上传失败定位 upload
assert.equal(S('prep', 'media', 'error', 'CDN 上传失败'), 'done', '上传错误 prep done');
assert.equal(S('upload', 'media', 'error', 'CDN 上传失败'), 'error', '上传错误 upload');
assert.equal(S('send', 'media', 'error', 'CDN 上传失败'), 'pending', '上传错误 send pending');

// media + error：其他失败定位 send
assert.equal(S('prep', 'media', 'error', '发送超时'), 'done', '发送错误 prep done');
assert.equal(S('upload', 'media', 'error', '发送超时'), 'done', '发送错误 upload done');
assert.equal(S('send', 'media', 'error', '发送超时'), 'error', '发送错误 send');

// media + uploading / sending / done
assert.equal(S('prep', 'media', 'uploading'), 'done', 'uploading prep done');
assert.equal(S('upload', 'media', 'uploading'), 'active', 'uploading upload active');
assert.equal(S('send', 'media', 'uploading'), 'pending', 'uploading send pending');
assert.equal(S('send', 'media', 'sending'), 'active', 'sending send active');
assert.equal(S('prep', 'media', 'done'), 'done', 'done prep');
assert.equal(S('upload', 'media', 'done'), 'done', 'done upload');
assert.equal(S('send', 'media', 'done'), 'done', 'done send');

console.log('smoke-bot-steps: all assertions passed');
