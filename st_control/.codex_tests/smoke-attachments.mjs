// ============================================================
// 大模型对话附件转换 — 运行期冒烟测试
// 锁定 attachments 下沉后的可观测输出：
//   图片 data URL / 过大标记 / 文本内联 / 普通文件 / ID 序列
// 运行：node st_control/.codex_tests/smoke-attachments.mjs
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

// mock FileReader（Node 无 DOM FileReader）
class MockFileReader {
  onload = null;
  onerror = null;
  result = null;
  error = null;
  readAsDataURL(file) {
    this.result = 'data:image/png;base64,AAA';
    queueMicrotask(() => this.onload?.());
  }
  readAsText(file) {
    this.result = 'hello 附件';
    queueMicrotask(() => this.onload?.());
  }
}
globalThis.FileReader = MockFileReader;

// 编译 attachments.ts；其相对导入 '../services/ipc' 用 mock 提供 saveUploadedFile
const src = readFileSync(path.join(root, 'src', 'lib', 'llm', 'attachments.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
// 编译产物保留源文件的 './services/ipc'（无扩展名）：补 .mjs 支持 esbuild 解析
const patched = code.replace('"./services/ipc"', '"./services/ipc.mjs"');
writeFileSync(path.join(outDir, 'attachments.mjs'), patched);

const svcDir = path.join(outDir, 'services');
mkdirSync(svcDir, { recursive: true });
writeFileSync(
  path.join(svcDir, 'ipc.mjs'),
  `export const llmApi = {
  saveUploadedFile: async (name, data) => 'C:/saved/' + name,
};
`,
);
writeFileSync(path.join(outDir, 'entry.mjs'), "export * from './attachments.mjs';");

await esbuild.build({
  entryPoints: [path.join(outDir, 'entry.mjs')],
  bundle: true,
  platform: 'node',
  format: 'esm',
  outfile: path.join(outDir, 'bundle-attachments.mjs'),
  logLevel: 'silent',
});

const mod = await import(pathToFileURL(path.join(outDir, 'bundle-attachments.mjs')).href);
const { fileToAttachment, attachmentsToParts, MAX_IMAGE_BYTES } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

let seq = 0;
const nextId = () => `att-${++seq}`;

// 图片：data URL + 持久化
const img = new File(['img'], 'a.png', { type: 'image/png' });
Object.defineProperty(img, 'size', { value: 100 });
const att1 = await fileToAttachment(img, nextId);
ok(att1.kind === 'image', '图片附件 kind=image');
ok(att1.url === 'data:image/png;base64,AAA', '图片附件带 data URL');
ok(att1.savedPath === 'C:/saved/a.png', '图片附件持久化路径');
ok(att1.id === 'att-1', 'ID 序列由调用方生成');

// 超大图片：tooBig 标记
const big = new File(['big'], 'big.png', { type: 'image/png' });
Object.defineProperty(big, 'size', { value: MAX_IMAGE_BYTES + 1 });
const att2 = await fileToAttachment(big, nextId);
ok(att2.kind === 'image' && att2.tooBig === true, '超大图片标记 tooBig');
ok(att2.url === undefined, '超大图片无 data URL');

// 文本文件：内联内容 + 截断
const txt = new File(['hello 附件'], 'note.md', { type: 'text/markdown' });
const att3 = await fileToAttachment(txt, nextId);
ok(att3.kind === 'text' && att3.text === 'hello 附件', '文本附件内联内容');
ok(att3.savedPath === 'C:/saved/note.md', '文本附件持久化路径');

// 普通文件：仅保存
const bin = new File([new Uint8Array([1, 2, 3])], 'data.bin', { type: 'application/octet-stream' });
const att4 = await fileToAttachment(bin, nextId);
ok(att4.kind === 'file' && att4.savedPath === 'C:/saved/data.bin', '普通文件仅保存');
ok(att4.id === 'att-4', 'ID 持续递增');

// attachmentsToParts：三种附件 → ContentPart
const parts = attachmentsToParts([
  { id: '1', name: 'a.png', mime: 'image/png', kind: 'image', url: 'data:image/png;base64,x', savedPath: 'p1' },
  { id: '2', name: 'note.md', mime: 'text/markdown', kind: 'text', text: '内容', savedPath: 'p2' },
  { id: '3', name: 'data.bin', mime: 'application/octet-stream', kind: 'file', savedPath: 'p3' },
]);
ok(parts[0].type === 'image_url' && parts[0].image_url?.url === 'data:image/png;base64,x' && parts[0].file_path === 'p1', '图片转 image_url');
ok(parts[1].type === 'text' && parts[1].text === '内容' && parts[1].name === 'note.md', '文本转 text part');
ok(parts[2].type === 'file' && parts[2].mime === 'application/octet-stream' && parts[2].file_path === 'p3', '文件转 file part');
ok(attachmentsToParts([]).length === 0, '空附件返回空');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
