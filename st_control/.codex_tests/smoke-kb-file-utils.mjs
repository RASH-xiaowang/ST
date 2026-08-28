// ============================================================
// 知识库文件展示/解析纯函数 — 运行期冒烟测试
// 锁定 fileUtils 下沉后的可观测输出：
//   文件图标 / 预览 MIME / 标签解析 / 目录展平 / 状态标签 / 模式标签 / 关键词过滤
// 运行：node st_control/.codex_tests/smoke-kb-file-utils.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'kb', 'fileUtils.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'kb-file-utils.mjs');
writeFileSync(outFile, code);

const mod = await import(pathToFileURL(outFile).href);
const { fileIco, previewMime, parseTags, flattenDirs, STATUS_LABEL, SOURCE_LABEL, MODE_LABEL, kbMonogram, trendArrow, trendClass, filterKbsByKeyword } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// 文件图标
ok(fileIco('pdf') === 'filePdf', 'pdf 图标');
ok(fileIco('xlsx') === 'fileXlsx', 'xlsx 图标');
ok(fileIco('ppt') === 'fileDoc', 'ppt 图标（文档类）');
ok(fileIco('md') === 'fileMd', 'md 图标');
ok(fileIco('csv') === 'fileCsv', 'csv 图标');
ok(fileIco(null) === 'file' && fileIco('zip') === 'file', '未知/空回退 file');

// 预览 MIME
ok(previewMime('pdf') === 'application/pdf', 'pdf MIME');
ok(previewMime('jpg') === 'image/jpeg', 'jpg MIME');
ok(previewMime('md') === 'text/markdown', 'md MIME');
ok(previewMime('docx') === 'application/octet-stream', '未知扩展名回退 octet-stream');

// 标签解析
ok(parseTags('a, b；c;d，a').join('|') === 'a|b|c|d', '中英文分隔符 + 去重');
ok(parseTags('  x  ').length === 1 && parseTags('  x  ')[0] === 'x', '标签去空格');
ok(parseTags('').length === 0, '空串无标签');
ok(parseTags('long'.repeat(10)).length === 0, '超 30 字符标签过滤');

// 目录展平
const tree = [
  { id: 1, name: 'root', children: [
    { id: 2, name: 'a', children: [] },
    { id: 3, name: 'b', children: [{ id: 4, name: 'c', children: [] }] },
  ] },
];
const flat = flattenDirs(tree);
ok(flat.length === 4, '目录全部展平');
ok(flat[0].depth === 0 && flat[1].depth === 1 && flat[3].depth === 2, '深度递增正确');
ok(flat[0].id === 1 && flat[3].id === 4, '顺序为深度优先');

// 标签映射
ok(STATUS_LABEL.ready === '解析完成' && SOURCE_LABEL.fetch === '网页抓取', '状态/来源标签');

// 检索模式标签
ok(MODE_LABEL.hybrid === '混合' && MODE_LABEL.vector === '向量' && MODE_LABEL.bm25 === '全文', '检索模式标签映射');
ok(MODE_LABEL.unknown === undefined, '未知模式不在映射内（调用方自行回退原文）');

// 知识库首字母
ok(kbMonogram('知识库') === '知', '中文首字保留');
ok(kbMonogram('📚 资料') === '📚'.charAt(0).toUpperCase(), 'emoji 开头返回首代理项（原实现行为，含历史怪癖）');
ok(kbMonogram('hello') === 'H', '英文首字大写');

// 趋势指示
ok(trendArrow('-12.5') === '▼ ' && trendArrow('3.2') === '▲ ', '趋势箭头');
ok(trendArrow('--') === '' && trendArrow('') === '', '无数据无指示');
ok(trendClass('-12.5') === 'kb-trend-down' && trendClass('3.2') === 'kb-trend-up', '趋势样式类');

// 知识库关键词过滤
const kbs = [
  { id: 1, name: 'Alpha KB', description: 'first base', docCount: 1, owner_id: null, pinned: false, isSystem: false, created_at: '' },
  { id: 2, name: 'Beta KB', description: null, docCount: 0, owner_id: null, pinned: false, isSystem: false, created_at: '' },
  { id: 3, name: 'alpha2', description: 'beta docs', docCount: 2, owner_id: null, pinned: false, isSystem: false, created_at: '' },
];
ok(filterKbsByKeyword(kbs, 'ALPHA').length === 2, '名称大小写不敏感匹配');
ok(filterKbsByKeyword(kbs, 'base').length === 1 && filterKbsByKeyword(kbs, 'base')[0].id === 1, '描述匹配');
ok(filterKbsByKeyword(kbs, '') === kbs, '空关键词返回原数组引用');
ok(filterKbsByKeyword(kbs, '  ').length === 3, '空白关键词不过滤');
ok(filterKbsByKeyword(kbs, 'none').length === 0, '未命中返回空');
ok(filterKbsByKeyword(kbs, 'beta').length === 2, '描述 null 安全且多命中');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
