// ============================================================
// 微信会话排序/实时重排纯函数冒烟测试
// 锁定 wechat/utils/sessionOrder.ts 的可观测输出：
// 置顶优先 + sort_ts 降序比较器，以及实时更新的有序插入语义。
// 运行：node st_control/.codex_tests/smoke-session-order.mjs
// ============================================================
import assert from 'node:assert/strict';
import { mkdirSync, rmSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import esbuild from 'esbuild';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(here, '..');
const outDir = path.join(here, 'out');
mkdirSync(outDir, { recursive: true });

const outFile = path.join(outDir, 'session-order.mjs');
await esbuild.build({
  entryPoints: [path.join(root, 'src', 'lib', 'wechat', 'utils', 'sessionOrder.ts')],
  bundle: true,
  format: 'esm',
  platform: 'neutral',
  outfile: outFile,
  logLevel: 'silent',
});

const { sessionBefore, upsertSessionOrdered } = await import(pathToFileURL(outFile).href);

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};
const sess = (username, sort_ts, pinned = false) => ({ username, sort_ts, pinned });
const names = (list) => list.map((s) => s.username);

// ─── 比较器：置顶优先，其余按 sort_ts 降序 ───
ok(sessionBefore(sess('a', 100, true), sess('b', 200)) === true, '置顶会话优先于普通会话');
ok(sessionBefore(sess('a', 100, true), sess('b', 50, true)) === true, '同为置顶：sort_ts 大者在前');
ok(sessionBefore(sess('a', 100), sess('b', 200)) === false, '普通会话：sort_ts 小者排后');
ok(sessionBefore(sess('a', 0), sess('b', 0)) === false, 'sort_ts 相等 → 不提前');

// ─── 实时重排 ───
const list = [sess('top', 300, true), sess('mid', 200), sess('low', 100)];

// 命中头部：原地替换，顺序不变
const head = upsertSessionOrdered(list, 'top', sess('top', 400, true));
ok(names(head).join(',') === 'top,mid,low', '命中头部 → 顺序不变');
ok(head[0].sort_ts === 400, '命中头部 → 内容更新');

// 命中中部：删除后按序二分插入（新 sort_ts 更大 → 前移）
const mid = upsertSessionOrdered(list, 'mid', sess('mid', 250));
ok(names(mid).join(',') === 'top,mid,low', '命中中部 → 仍在原区间');
const midUp = upsertSessionOrdered(list, 'mid', sess('mid', 350));
ok(names(midUp).join(',') === 'top,mid,low', 'sort_ts 增大仍排在置顶会话之后');
// 保持有序不变量：对任意相邻对，前面不应排在后面
const ordered = upsertSessionOrdered(list, 'mid', sess('mid', 350));
let sorted = true;
for (let i = 0; i < ordered.length - 1; i++) {
  if (sessionBefore(ordered[i + 1], ordered[i])) sorted = false;
}
ok(sorted, '重排后维持有序不变量');

// 未命中：追加到末尾（与后端已排序列表的增量语义一致）
const miss = upsertSessionOrdered(list, 'new', sess('new', 150));
ok(names(miss).join(',') === 'top,mid,low,new', '未命中 → 追加末尾');

// 不修改入参
const before = names(list);
upsertSessionOrdered(list, 'mid', sess('mid', 350));
ok(names(list).join(',') === before.join(','), '入参列表不被修改');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
