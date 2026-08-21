// ============================================================
// 知识库对话展示纯函数 — 运行期冒烟测试
// 锁定 chatUtils 下沉后的可观测输出：
//   命中高亮分段（含多词/大小写/相邻命中）/ 引用解析
// 运行：node st_control/.codex_tests/smoke-kb-chat-utils.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'kb', 'chatUtils.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'kb-chat-utils.mjs');
writeFileSync(outFile, code);

const mod = await import(pathToFileURL(outFile).href);
const { highlightSegments, parseCitations } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// 高亮分段
const segs = highlightSegments('abc hello def', 'hello');
ok(segs.length === 3, '命中词被切为 3 段');
ok(segs[1].text === 'hello' && segs[1].hit === true, '命中段标记 hit');
ok(segs[0].hit === false && segs[2].hit === false, '非命中段标记未命中');

ok(highlightSegments('hello world', 'hello world').filter((s) => s.hit).length === 2, '多词查询各自命中');
ok(highlightSegments('HELLO there', 'hello').some((s) => s.hit && s.text === 'HELLO'), '大小写不敏感命中');
ok(highlightSegments('abc', '').length === 1 && highlightSegments('abc', '')[0].hit === false, '空查询整体一段');
ok(highlightSegments('abc', 'a').length === 1, '单字符词被忽略（≥2 字符）');

// 引用解析
ok(parseCitations(null).length === 0, 'null → 空');
ok(parseCitations('not-json').length === 0, '非法 JSON → 空');
ok(parseCitations('{"a":1}').length === 0, '非数组 JSON → 空');
const cites = parseCitations('[{"doc_id":1,"chunk_id":2},{"doc_id":3}]');
ok(cites.length === 2 && cites[0].doc_id === 1 && cites[0].chunk_id === 2, '合法数组解析');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
