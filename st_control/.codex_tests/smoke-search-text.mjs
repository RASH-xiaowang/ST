// ============================================================
// 全局搜索文本处理 — 运行期冒烟测试
// 锁定 searchText / apiUrl 下沉后的可观测输出：
//   高亮包 mark / 摘要截取 / API 调试地址拼接
// 运行：node st_control/.codex_tests/smoke-search-text.mjs
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

async function compileTs(rel, out) {
  const src = readFileSync(path.join(root, 'src', 'lib', rel), 'utf8');
  const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
  writeFileSync(path.join(outDir, out), code);
}

await compileTs('search/searchText.ts', 'search-text.mjs');
await compileTs('components/apiUrl.ts', 'api-url.mjs');
const mod1 = await import(pathToFileURL(path.join(outDir, 'search-text.mjs')).href);
const mod2 = await import(pathToFileURL(path.join(outDir, 'api-url.mjs')).href);
const { highlight, excerpt } = mod1;
const { apiDebugUrl } = mod2;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// 高亮
ok(highlight('hello world', 'world') === 'hello <mark>world</mark>', '命中词包 mark');
ok(highlight('HELLO there', 'hello') === '<mark>HELLO</mark> there', '大小写不敏感且保留原文');
ok(highlight('a.b(c)', 'a.b(c)') === '<mark>a.b(c)</mark>', '特殊正则字符转义');
ok(highlight('text', '') === 'text' && highlight('', 'kw') === '', '空输入原样返回');
ok(highlight('no match here', 'zzz') === 'no match here', '无命中原样返回');

// 摘要
ok(excerpt('', 'kw') === '', '空文本返回空');
ok(excerpt('short text', 'x') === 'short text', '未命中且短文本原样');
ok(excerpt('a'.repeat(200), 'z') === 'a'.repeat(140) + '…', '未命中超长截断');
const hit = excerpt('prefix keyword suffix', 'keyword');
ok(hit.includes('keyword'), '命中词保留在摘要中');
ok(hit === 'prefix keyword suffix', 'max 足够大时整段保留');
ok(excerpt('x'.repeat(50) + ' keyword ' + 'y'.repeat(50), 'keyword', 30).startsWith('…'), '短 max 命中靠后时前置省略号');
ok(excerpt('keyword suffix', 'keyword').startsWith('keyword'), '命中靠前无前置省略号');

// API 调试地址
ok(apiDebugUrl('/api/v1/ping', 5032, 'tok') === 'http://127.0.0.1:5032/api/v1/ping?access_token=tok', '无查询串用 ?');
ok(apiDebugUrl('/api/v1/x?foo=1', 8080, 'a b') === 'http://127.0.0.1:8080/api/v1/x?foo=1&access_token=a%20b', '有查询串用 & 且 token 编码');
ok(apiDebugUrl('/api/v1/ping', 5032, null) === 'http://127.0.0.1:5032/api/v1/ping', '无 token 不附加参数');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
