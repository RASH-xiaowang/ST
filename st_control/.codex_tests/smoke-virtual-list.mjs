// ============================================================
// 消息虚拟滚动纯计算 — 运行期冒烟测试
// 锁定 virtualList 下沉后的可观测输出：
//   消息高度估算 / 前缀和 / 二分定位 / 可见条数 / 可视窗口 / 裁剪 / 分隔条
// 运行：node st_control/.codex_tests/smoke-virtual-list.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'wechat', 'utils', 'virtualList.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'virtual-list.mjs');
writeFileSync(outFile, code);

const mod = await import(pathToFileURL(outFile).href);
const { estimateMsgHeight, computePrefixSums, upperBoundPrefix, estimateVisibleCount, computeVisRange, trimMessageWindow, shouldShowDivider, MSG_MIN_H } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// 消息高度估算
ok(estimateMsgHeight({ is_notice: true }) === 50, '通知消息高 50');
ok(estimateMsgHeight({ type: 3 }) === 254, '图片消息高 240+14');
ok(estimateMsgHeight({ type: 1, text: 'x'.repeat(44) }) >= MSG_MIN_H, '长文本高度不低于最小值');
ok(estimateMsgHeight({ type: 1, text: 'x'.repeat(44), sender_name: '张三' }) > estimateMsgHeight({ type: 1, text: 'x'.repeat(44) }), '群聊发送者名增加高度');
ok(estimateMsgHeight({ rich: { type: 'file' } }) === 98, '富媒体文件 84+14');
ok(estimateMsgHeight({ rich: { type: 'emoji' } }) === 134, '富媒体表情 120+14');
ok(estimateMsgHeight({ rich: { type: 'link', thumb: 'x', url: 'https://mp.weixin.qq.com/a', articles: [] } }) === 238, '公众号链接带封面 224+14');

// 前缀和
const { prefix, total } = computePrefixSums([10, 20, 30]);
ok(prefix[0] === 0 && prefix[1] === 10 && prefix[2] === 30, '前缀和 p[i]=前 i 条总高');
ok(total === 60, '总高为末元素之和');
ok(computePrefixSums([]).total === 0, '空数组总高 0');

// 二分定位
ok(upperBoundPrefix([0, 10, 30, 60], 5) === 1, '5px 落在第 1 条');
ok(upperBoundPrefix([0, 10, 30, 60], 10) === 2, '10px 落在第 2 条（> 语义）');
ok(upperBoundPrefix([0, 10, 30, 60], 999) === 4, '超出总高返回末尾');

// 可见条数
ok(estimateVisibleCount(0, 0, 600) === 8, '空列表至少 8 条');
ok(estimateVisibleCount(100, 10000, 600) === 8, '平均高低于 24 时至少 8 条');
ok(estimateVisibleCount(10, 6000, 600) === 8, '平均高 600 时 1 条但下限 8');
ok(estimateVisibleCount(10, 1200, 600) === 8, '平均高 120 时视口推算 5 条但下限 8');
ok(estimateVisibleCount(10, 480, 600) === 13, '平均高 48 时推算 12.5 → 13 条');

// 可视窗口（computeVisRange）：与原 visRange 派生逻辑等价
ok(JSON.stringify(computeVisRange(0, 0, 600, [], 0, false)) === '{"start":0,"end":0,"topPad":0,"bottomPad":0}', '空列表零窗口');
const heights = Array(20).fill(100);
const { prefix: pf } = computePrefixSums(heights); // 0,100,...,1900, total=2000
// 非贴底：scrollTop=500 → idx=6，vis=8（下限），buffer=1 时窗口 [5,15)
const r1 = computeVisRange(20, 2000, 600, pf, 500, false, 1);
ok(r1.start === 5 && r1.end === 15, `非贴底窗口按滚动定位（${r1.start}..${r1.end}）`);
ok(r1.topPad === pf[5] && r1.topPad === 500, 'topPad 为起始前缀和');
ok(r1.bottomPad === 2000 - (pf[15] ?? 2000) && r1.bottomPad === 500, 'bottomPad 为剩余估算高度');
// 贴底：窗口覆盖到最后一条
const r2 = computeVisRange(20, 2000, 600, pf, 0, true, 1);
ok(r2.end === 20 && r2.bottomPad === 0, '贴底模式窗口覆盖末尾且无底部占位');
ok(r2.start === Math.max(0, 20 - 8 - 2) && r2.start === 10, '贴底 start = 末尾 - 可见数 - 2×buffer');
ok(r2.topPad === pf[10], '贴底 topPad 为窗口起始前缀和');

// 裁剪（trimMessageWindow）
const msgs5 = [{ local_id: 1 }, { local_id: 2 }, { local_id: 3 }, { local_id: 4 }, { local_id: 5 }];
const t1 = trimMessageWindow(msgs5, [10, 20, 30, 40, 50], 3);
ok(t1.messages.length === 3 && t1.messages[0].local_id === 3, '裁剪保留尾部 maxKeep 条');
ok(t1.estH.join(',') === '30,40,50', '裁剪后的估算高度同步');
ok(t1.removedH === 30, '移除高度为前 keep 条之和（10+20）');
const t2 = trimMessageWindow(msgs5, [10, 20, 30, 40, 50], 10);
ok(t2.messages === msgs5 && t2.removedH === 0, '未超上限返回原引用且不移除');

// 分隔条（shouldShowDivider）
ok(shouldShowDivider({ ts: 1000 }, { ts: 1301 }) === true, '间隔 301ms 需分隔');
ok(shouldShowDivider({ ts: 1000 }, { ts: 1300 }) === false, '间隔恰 300ms 不分隔');
ok(shouldShowDivider(undefined, { ts: 1 }) === true, '无前一条需分隔（首条）');
ok(shouldShowDivider({ ts: 1 }, undefined) === false, '当前为空不分隔');
ok(shouldShowDivider({ ts: null }, { ts: null }) === false, '时间戳缺失按 0 处理');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
