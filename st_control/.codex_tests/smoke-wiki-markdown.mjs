// ============================================================
// 知识库 Wiki Markdown 渲染 — 运行期冒烟测试
// 锁定 kb/markdown.ts 下沉后的可观测输出：
//   标题 / 列表 / 代码块 / 引用 / 链接 / 图片 / Wiki 链接 / 行内样式
// 运行：node st_control/.codex_tests/smoke-wiki-markdown.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'kb', 'markdown.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'wiki-markdown.mjs');
writeFileSync(outFile, code);

const { renderMd } = await import(pathToFileURL(outFile).href);

// 标题层级
assert.equal(renderMd('# 一级'), '<h1 class="wiki-md-h">一级</h1>', '一级标题');
assert.equal(renderMd('### 三级'), '<h3 class="wiki-md-h">三级</h3>', '三级标题');
assert.equal(renderMd('---'), '<hr class="wiki-md-hr">', '分隔线');

// 列表
assert.equal(renderMd('- a\n- b'), '<ul><li>a</li><li>b</li></ul>', '无序列表');
assert.equal(renderMd('1. a\n2. b'), '<ol><li>a</li><li>b</li></ol>', '有序列表');

// 代码块（内容转义）
assert.equal(
  renderMd('```\nconst x = "<b>";\n```'),
  '<pre class="wiki-md-code"><code>const x = &quot;&lt;b&gt;&quot;;</code></pre>',
  '代码块转义',
);

// 引用 / 段落
assert.equal(renderMd('> 引用'), '<blockquote class="wiki-md-quote"><p>引用</p></blockquote>', '引用');
assert.equal(renderMd('普通段落'), '<p>普通段落</p>', '段落');

// 行内：代码 / 链接 / 图片 / 粗体 / 斜体
assert.equal(renderMd('`code`'), '<p><code>code</code></p>', '行内代码');
assert.equal(
  renderMd('[链接](https://example.com)'),
  '<p><a class="wiki-md-a" href="https://example.com" target="_blank" rel="noreferrer">链接</a></p>',
  '链接',
);
assert.equal(
  renderMd('![图](img.png)'),
  '<p><img class="wiki-md-img" src="img.png" alt="图"></p>',
  '图片',
);
assert.equal(renderMd('**粗**'), '<p><b>粗</b></p>', '粗体');
assert.equal(renderMd('*斜*'), '<p><i>斜</i></p>', '斜体');

// Wiki 链接
assert.equal(
  renderMd('[[页面]]'),
  '<p><button type="button" class="wiki-md-wl" data-wiki-page="页面">页面</button></p>',
  'Wiki 链接',
);
assert.equal(
  renderMd('[[目标|别名]]'),
  '<p><button type="button" class="wiki-md-wl" data-wiki-page="目标">别名</button></p>',
  'Wiki 链接别名',
);

// XSS：原始文本转义 + Wiki 链接 key 属性注入防护
assert.equal(renderMd('<script>alert(1)</script>'), '<p>&lt;script&gt;alert(1)&lt;/script&gt;</p>', '原始文本转义');
assert.equal(
  renderMd('[[a" onmouseover="x]]'),
  '<p><button type="button" class="wiki-md-wl" data-wiki-page="a&quot; onmouseover=&quot;x">a&quot; onmouseover=&quot;x</button></p>',
  'Wiki key 属性注入防护',
);

// 空输入
assert.equal(renderMd(''), '', '空输入');
assert.equal(renderMd(undefined), '', 'undefined 输入');

console.log('smoke-wiki-markdown: all assertions passed');
