// ============================================================
// 微信记录展示纯函数 — 运行期冒烟测试
// 锁定 wechat/utils/records 下沉后的可观测输出：
//   类型图标 / 转账/红包/直播状态映射 / 用户名截断
// 运行：node st_control/.codex_tests/smoke-wechat-records.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'wechat', 'utils', 'records.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'wechat-records.mjs');
writeFileSync(outFile, code);

const mod = await import(pathToFileURL(outFile).href);
const { kindIcon, transferSubType, hbStatus, liveStatus, shortUser, KIND_PATHS } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// 图标
ok(kindIcon('rewind').startsWith('<svg') && kindIcon('rewind').includes(KIND_PATHS.rewind), '已知类型图标');
ok(kindIcon('unknown').includes(KIND_PATHS.app), '未知类型回退 app 图标');
ok(kindIcon('film', 24).includes('width="24" height="24"'), '自定义尺寸');

// 转账子类型
ok(transferSubType('3') === '转账' && transferSubType(2) === '群收款', '转账子类型映射');
ok(transferSubType(99) === '类型 99', '未知类型回退');

// 红包/直播状态
ok(hbStatus('1') === '正常' && hbStatus(3) === '已领完', '红包状态映射');
ok(hbStatus(9) === '状态 9', '未知红包状态回退');
ok(liveStatus('1') === '直播中' && liveStatus(3) === '预告', '直播状态映射');
ok(liveStatus(9) === '状态 9', '未知直播状态回退');

// 用户名截断
ok(shortUser(null) === '—' && shortUser('') === '—', '空用户名 → —');
ok(shortUser('abc') === 'abc', '短用户名原样');
ok(shortUser('x'.repeat(40)).length === 31, '超长截断（30 + …）');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
