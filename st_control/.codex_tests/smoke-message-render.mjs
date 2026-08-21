// ============================================================
// 助手消息渲染 — 运行期冒烟测试
// 锁定 messageRender 下沉后的可观测输出：
//   行内样式 / 媒体识别 / 块解析 / 图表块
// 运行：node st_control/.codex_tests/smoke-message-render.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'llm', 'messageRender.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'message-render.mjs');
writeFileSync(outFile, code);

const mod = await import(pathToFileURL(outFile).href);
const { inlineMd, miniMarkdown, parseBlocks, safeJson, isAudioUrl } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// 行内 markdown
ok(inlineMd('**bold** and *it* and `code`') === '<b>bold</b> and <i>it</i> and <code>code</code>', '粗体/斜体/行内代码');
ok(inlineMd('[link](https://x.com)') === '<a class="llm-ext-link" href="https://x.com" target="_blank" rel="noreferrer">link</a>', '链接');
ok(inlineMd('![alt](https://x.com/a.png)') === '<img class="llm-md-img" src="https://x.com/a.png" alt="alt">', '图片');
ok(inlineMd('![🎬](https://x.com/v.mp4)').includes('<video'), '视频标记渲染 video');
ok(inlineMd('![🎙](https://x.com/a)').includes('<audio'), '音频标记渲染 audio');

// 媒体 URL 识别
ok(isAudioUrl('https://x.com/a.mp3') === true, 'mp3 识别');
ok(isAudioUrl('data:audio/wav;base64,AAA') === true, 'data:audio 识别');
ok(isAudioUrl('https://x.com/a.txt') === false, '非音频不识别');

// mini markdown
ok(miniMarkdown('# Title') === '<h1>Title</h1>', '标题');
ok(miniMarkdown('- a\n- b') === '<ul><li>a</li><li>b</li></ul>', '无序列表');
ok(miniMarkdown('1. a\n2. b') === '<ol><li>a</li><li>b</li></ol>', '有序列表');
ok(miniMarkdown('plain') === '<p>plain</p>', '段落');
ok(miniMarkdown('https://x.com/a.png') === '<img class="llm-md-img" src="https://x.com/a.png" alt="">', '裸图片 URL 行');

// parseBlocks
const blocks = parseBlocks('hello\n\n```js\nconst a = 1;\n```\n\nworld');
ok(blocks.length === 3, 'prose/code/prose 三块');
ok(blocks[1].type === 'code' && blocks[1].lang === 'js' && blocks[1].code === 'const a = 1;', '代码块');
ok(blocks[0].type === 'prose' && blocks[0].html.includes('hello'), 'prose 块');

// 图表块
const chart = parseBlocks('```chart\n{"type":"line"}\n```');
ok(chart.length === 1 && chart[0].type === 'chart' && chart[0].spec.type === 'line', 'chart 块解析 JSON');

// 新增：引用块 / 分割线 / 表格
ok(miniMarkdown('> 引用文字').includes('<blockquote>引用文字</blockquote>'), '引用块');
ok(miniMarkdown('---') === '<hr>', '分割线');
const table = miniMarkdown('| A | B |\n| --- | --- |\n| 1 | 2 |');
ok(table.includes('llm-md-table') && table.includes('<table>') && table.includes('<th>A</th>') && table.includes('<td>1</td>'), '表格');
const tableMd = miniMarkdown('| 语法 | 示例 |\n| --- | --- |\n| **加粗** | `**x**` |');
ok(tableMd.includes('<b>加粗</b>') && tableMd.includes('<code>**x**</code>'), '表格单元格行内样式');
ok(safeJson('{"a":1}').a === 1 && safeJson('bad') === null, 'safeJson 容错');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
