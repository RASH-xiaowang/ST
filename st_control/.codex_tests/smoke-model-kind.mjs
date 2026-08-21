// ============================================================
// 大模型能力分类 — 运行期冒烟测试
// 锁定 modelKind.ts 下沉后的可观测输出：
//   classifyModelType / modelSendLabel
// 运行：node st_control/.codex_tests/smoke-model-kind.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'llm', 'modelKind.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'model-kind.mjs');
writeFileSync(outFile, code);

const { classifyModelType, modelSendLabel } = await import(pathToFileURL(outFile).href);

// ── classifyModelType ──
assert.equal(classifyModelType('生图'), 'image');
assert.equal(classifyModelType('视频'), 'video');
assert.equal(classifyModelType('语音'), 'speech');
assert.equal(classifyModelType('嵌入'), 'embed');
assert.equal(classifyModelType('重排序'), 'rerank');
assert.equal(classifyModelType('对话'), 'chat');
assert.equal(classifyModelType(undefined), 'chat');
assert.equal(classifyModelType(null), 'chat');

// ── modelSendLabel ──
assert.equal(modelSendLabel('image'), '生成');
assert.equal(modelSendLabel('video'), '生成');
assert.equal(modelSendLabel('embed'), '生成');
assert.equal(modelSendLabel('speech'), '合成');
assert.equal(modelSendLabel('rerank'), '排序');
assert.equal(modelSendLabel('chat'), '发送');

console.log('smoke-model-kind: all assertions passed');
