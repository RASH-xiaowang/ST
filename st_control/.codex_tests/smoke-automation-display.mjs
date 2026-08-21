// ============================================================
// 自动化展示纯函数 — 运行期冒烟测试
// 锁定 automation/display 下沉后的可观测输出：
//   消息类型分类 / 徽章配色 / 标签 / 状态徽章 HTML / 媒体标签
// 运行：node st_control/.codex_tests/smoke-automation-display.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'automation', 'display.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'automation-display.mjs');
writeFileSync(outFile, code);

const mod = await import(pathToFileURL(outFile).href);
const { classifyMessageType, kindColor, kindLabel, statusBadge, mediaLabel, STATUS_META } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// 消息类型分类：media_type 优先
ok(classifyMessageType({ media_type: 'image' }) === 'image', 'media_type=image → image');
ok(classifyMessageType({ media_type: 'video' }) === 'video', 'media_type=video → video');
ok(classifyMessageType({ media_type: 'voice' }) === 'file', '其他 media_type → file');
// msg_type 数字分支
ok(classifyMessageType({ msg_type: 3 }) === 'image', 'msg_type=3 → image');
ok(classifyMessageType({ msg_type: 43 }) === 'video', 'msg_type=43 → video');
ok(classifyMessageType({ msg_type: 49 }) === 'file', 'msg_type=49 → file');
ok(classifyMessageType({ msg_type: 1 }) === 'text', 'msg_type=1 → text');
ok(classifyMessageType(null) === 'text', 'null → text');

// 徽章配色/标签
ok(kindColor('image') === 'bg-violet-500/15 text-violet-400', 'image 配色');
ok(kindColor('video') === 'bg-rose-500/15 text-rose-400', 'video 配色');
ok(kindColor('text') === 'bg-cyan-500/15 text-cyan-400', 'text 配色');
ok(kindLabel('image') === '图片' && kindLabel('file') === '文件', '类型标签');

// 状态徽章
ok(statusBadge('pending').includes('待处理') && statusBadge('pending').includes('bg-amber-500/15'), '已知状态徽章');
ok(statusBadge('unknown').includes('unknown') && statusBadge('unknown').includes('bg-muted'), '未知状态回退原文+灰底');
ok(Object.keys(STATUS_META).includes('replied') && STATUS_META.replied.label === '已回复', '状态元数据完整');

// 媒体标签
ok(mediaLabel(null) === '文本' && mediaLabel('') === '文本', '空媒体 → 文本');
ok(mediaLabel('video') === 'video', '非空媒体原样返回');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
