// ============================================================
// 微信展示格式化纯函数 — 运行期冒烟测试
// 锁定 display 下沉后的可观测输出：
//   相对时间 / 榜单排名 / 数量缩写
// 运行：node st_control/.codex_tests/smoke-wechat-display.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'wechat', 'utils', 'display.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'wechat-display.mjs');
writeFileSync(outFile, code);

// 固定时钟：2026-08-13 12:00:00 UTC
const FIXED_NOW = 1784001600000;
const origNow = Date.now;
Date.now = () => FIXED_NOW;

try {
  const mod = await import(pathToFileURL(outFile).href);
  const { relTime, rankOf, fmtCount } = mod;

  let passed = 0;
  const ok = (cond, msg) => {
    assert.ok(cond, msg);
    passed++;
    console.log('✓', msg);
  };

  const nowSec = FIXED_NOW / 1000;
  ok(relTime(undefined) === '—', '无时间戳 → —');
  ok(relTime(0) === '—', '0 时间戳 → —');
  ok(relTime(nowSec - 30) === '刚刚', '30 秒前 → 刚刚');
  ok(relTime(nowSec - 600) === '10 分钟前', '10 分钟前');
  ok(relTime(nowSec - 7200) === '2 小时前', '2 小时前');
  ok(relTime(nowSec - 100000) === '昨天', '超过 24h → 昨天');
  ok(relTime(nowSec - 300000) === '3 天前', '3 天前');
  // 超 7 天 → 日期（期望值按固定时钟动态计算，避免依赖运行时区）
  const targetDt = new Date((nowSec - 20000000) * 1000);
  const mm = String(targetDt.getMonth() + 1).padStart(2, '0');
  const dd = String(targetDt.getDate()).padStart(2, '0');
  ok(relTime(nowSec - 20000000) === `${targetDt.getFullYear()}-${mm}-${dd}`, '超 7 天 → 日期');

  // 榜单排名
  const list = [{ id: 'a', v: 10 }, { id: 'b', v: 30 }, { id: 'c', v: 20 }];
  ok(rankOf(list, 'b', (n) => n.v) === 1, '最高值排名 1');
  ok(rankOf(list, 'c', (n) => n.v) === 2, '次高值排名 2');
  ok(rankOf(list, 'a', (n) => n.v) === 3, '最低值排名 3');
  ok(rankOf(list, 'zzz', (n) => n.v) === 0, '不在榜返回 0');
  ok(rankOf([], 'a', (n) => n.v) === 0, '空列表返回 0');

  // 数量缩写
  ok(fmtCount(999) === '999', '千以下原样');
  ok(fmtCount(1500) === '1.5k', '千 → k');
  ok(fmtCount(12345) === '1.2w', '万 → w');
  ok(fmtCount(0) === '0', '零 → 0');

  console.log(`\n全部通过：${passed} 项断言`);
} finally {
  Date.now = origNow;
  rmSync(outDir, { recursive: true, force: true });
}
