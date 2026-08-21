// ============================================================
// 微信模块格式化/图标纯函数冒烟测试
// 锁定 wechat/utils/format.ts 的可观测输出（含刻意保留的
// 历史语义：formatDividerTime 无“今天/昨天”前缀、avatarLetter
// 前导空格行为、favFileSize 与 fmtFileSize 的 GB 差异）。
// 运行：node st_control/.codex_tests/smoke-format-utils.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'wechat', 'utils', 'format.ts'), 'utf8');
// format.ts 现经 `export { errText } from '../../format'` 引用共享 lib/format.ts，
// 需打包（bundle）才能解析该依赖，产物保持自包含。
const build = await esbuild.build({
  stdin: {
    contents: src,
    resolveDir: path.join(root, 'src', 'lib', 'wechat', 'utils'),
    loader: 'ts',
    sourcefile: 'format.ts',
  },
  bundle: true,
  write: false,
  format: 'esm',
  platform: 'node',
  logLevel: 'silent',
});
const code = build.outputFiles[0].text;
const outFile = path.join(outDir, 'format.mjs');
writeFileSync(outFile, code);

const {
  errText, formatDividerTime, avatarLetter, colorFromName, fmtDur,
  iconSvg, ICON_PATHS, fileIcon, favIcon, payStateClass,
  transferStatusLabel, redPacketLabel, chatlogPreview,
  fmtFileSize, favFileSize, cellText, cellTextSmart,
} = await import(pathToFileURL(outFile).href);

// ── errText ──
assert.equal(errText(new Error('boom')), 'boom');
assert.equal(errText('boom'), 'boom');
assert.equal(errText({ message: 'msg' }), 'msg');
assert.equal(errText(null), '');
assert.equal(errText(undefined), '');
assert.equal(errText(42), '42');

// ── formatDividerTime（保持原语义：今天只显示 HH:mm，无“昨天”分支） ──
assert.equal(formatDividerTime(undefined), '');
assert.equal(formatDividerTime('not-a-date'), 'not-a-date');
assert.match(formatDividerTime(new Date()), /^\d{2}:\d{2}$/);
const yesterday = new Date();
yesterday.setDate(yesterday.getDate() - 1);
assert.match(formatDividerTime(yesterday), /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/);
assert.ok(!formatDividerTime(yesterday).includes('昨天'));

// ── avatarLetter（保留前导空格原行为） ──
assert.equal(avatarLetter('张三'), '张');
assert.equal(avatarLetter('alice'), 'A');
assert.equal(avatarLetter(' alice'), ' ');
assert.equal(avatarLetter(''), '?');

// ── colorFromName（确定性 + 属于固定色板） ──
const palette = ['#f44336','#e91e63','#9c27b0','#673ab7','#3f51b5','#2196f3','#009688','#4caf50','#ff9800','#795548','#607d8b','#ff5722'];
assert.ok(palette.includes(colorFromName('张三')));
assert.equal(colorFromName('张三'), colorFromName('张三'));

// ── fmtDur ──
assert.equal(fmtDur(65), '1:05');
assert.equal(fmtDur(0), '0:00');
assert.equal(fmtDur(-5), '0:00');
assert.equal(fmtDur(3599), '59:59');

// ── 图标 ──
assert.ok(iconSvg(ICON_PATHS.file, 24).includes('width="24"'));
assert.ok(fileIcon('PNG').includes('svg'));
assert.equal(fileIcon('exe'), fileIcon('apk'));
assert.ok(favIcon('图片').includes('svg'));
assert.ok(favIcon('未知类型').includes('svg'));

// ── 转账/红包状态 ──
assert.equal(payStateClass('已退还'), 'wc-pay-returned');
assert.equal(payStateClass('对方已收款'), 'wc-pay-received');
assert.equal(payStateClass(''), '');
assert.equal(transferStatusLabel(true, '3'), '已被接收');
assert.equal(transferStatusLabel(false, '3'), '已收款');
assert.equal(transferStatusLabel(true, '4'), '已退还');
assert.equal(transferStatusLabel(true, '7'), '待领取');
assert.equal(redPacketLabel('4'), '已退还');
assert.equal(redPacketLabel('8'), '已领取');

// ── chatlogPreview ──
assert.deepEqual(
  chatlogPreview([{ name: 'A', text: 'hi there' }, { text: '  hello   world  ' }]),
  ['A: hi there', 'hello world'],
);
assert.deepEqual(chatlogPreview(null), []);
assert.deepEqual(chatlogPreview([{ name: '', text: 'x' }]), ['x']);

// ── 文件大小（刻意保留 fmtFileSize / favFileSize 的 GB 差异） ──
assert.equal(fmtFileSize(0), '');
assert.equal(fmtFileSize(500), '500 B');
assert.equal(fmtFileSize(2048), '2.0 KB');
assert.equal(fmtFileSize(5 * 1048576), '5.0 MB');
assert.equal(fmtFileSize(2 * 1073741824), '2.00 GB');
assert.equal(favFileSize(500), '500 B');
assert.equal(favFileSize(2048), '2.0 KB');
assert.equal(favFileSize(5 * 1048576), '5.0 MB');
assert.equal(favFileSize(2 * 1073741824), '2048.0 MB');

// ── cellText / cellTextSmart ──
assert.equal(cellText(null), '');
assert.equal(cellText(undefined), '');
assert.equal(cellText(5), '5');
assert.equal(cellText({ a: 1 }), '{"a":1}');
const epoch = 1700000000;
const d = new Date(epoch * 1000);
const p = (n) => String(n).padStart(2, '0');
const expectTs = `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
assert.equal(cellTextSmart(epoch, 'create_time'), expectTs);
assert.equal(cellTextSmart(epoch, 'user_name'), String(epoch));
assert.equal(cellTextSmart('plain', 'time'), 'plain');

console.log('smoke-format-utils: all assertions passed');
