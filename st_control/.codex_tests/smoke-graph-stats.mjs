// ============================================================
// 微信关系图谱统计派生 — 运行期冒烟测试
// 锁定 graphStats.ts / graphModel.toGraphData /
// graphPoster.makePosterInput 的可观测输出：
//   topByField / groupCommunities / connectedEdgesOf /
//   sharedGroupNames / toGraphData / makePosterInput
// 运行：node st_control/.codex_tests/smoke-graph-stats.mjs
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

// graphStats 仅含类型导入（esbuild transform 剥离），直接转换
const src = readFileSync(path.join(root, 'src', 'lib', 'wechat', 'graph', 'graphStats.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'graph-stats.mjs');
writeFileSync(outFile, code);
const { topByField, groupCommunities, connectedEdgesOf, sharedGroupNames } = await import(pathToFileURL(outFile).href);

// graphModel 有运行时依赖（../utils clamp）：esbuild bundle 自包含产物
const graphModelSrc = readFileSync(path.join(root, 'src', 'lib', 'wechat', 'graph', 'graphModel.ts'), 'utf8');
const graphModelOut = path.join(outDir, 'graph-model.cjs');
await esbuild.build({
  stdin: {
    contents: graphModelSrc,
    resolveDir: path.join(root, 'src', 'lib', 'wechat', 'graph'),
    loader: 'ts',
    sourcefile: 'graphModel.ts',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  outfile: graphModelOut,
  logLevel: 'silent',
});
const { createRequire } = await import('node:module');
const require = createRequire(import.meta.url);
const modelMod = require(graphModelOut);
const { toGraphData } = modelMod;

// graphPoster 的 makePosterInput 为纯函数（不触碰 buildPoster 的 DOM 路径）：
// bundle 后仅调用 makePosterInput/fmtCount
const posterSrc = readFileSync(path.join(root, 'src', 'lib', 'wechat', 'graph', 'graphPoster.ts'), 'utf8');
const posterOut = path.join(outDir, 'graph-poster.cjs');
await esbuild.build({
  stdin: {
    contents: posterSrc,
    resolveDir: path.join(root, 'src', 'lib', 'wechat', 'graph'),
    loader: 'ts',
    sourcefile: 'graphPoster.ts',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  outfile: posterOut,
  logLevel: 'silent',
});
const posterMod = require(posterOut);
const { makePosterInput } = posterMod;

// ── topByField ──
const nodes = [
  { id: 'a', v: 3 },
  { id: 'b', v: 10 },
  { id: 'c', v: 1 },
  { id: 'd' },
];
assert.deepEqual(
  topByField(nodes, (n) => n.v ?? 0, 2).map((n) => n.id),
  ['b', 'a'],
  '按字段降序取前 N，缺失按 0',
);
assert.equal(topByField(nodes, (n) => n.v ?? 0, 10).length, 4, 'N 超列表长度返回全量');
assert.deepEqual(
  nodes.map((n) => n.id),
  ['a', 'b', 'c', 'd'],
  '不修改原数组',
);

// ── groupCommunities ──
const gnode = (id, kind, community) => ({ id, kind, community });
const groups = groupCommunities([
  gnode('s', 'self', 0),
  gnode('a1', 'person', 1),
  gnode('a2', 'person', 1),
  gnode('b1', 'group', 2),
  gnode('neg', 'person', -1),
]);
assert.deepEqual(
  groups.map((g) => g.id),
  [1, 2],
  '排除 self 与负 community，按成员数降序',
);
assert.equal(groups[0].members.length, 2, '同圈子成员聚合');
assert.deepEqual(groupCommunities([]), [], '空图返回空');

// ── connectedEdgesOf ──
const graph = {
  nodes: [
    { id: 'a' }, { id: 'b' }, { id: 'c' },
  ],
  edges: [
    { source: 'a', target: 'b', weight: 5 },
    { source: 'c', target: 'a', weight: 9 },
    { source: 'b', target: 'c', weight: 1 },
    { source: 'x', target: 'y', weight: 99 },
  ],
};
const ces = connectedEdgesOf(graph, 'a');
assert.equal(ces.length, 2, '只取相连边');
assert.deepEqual(ces.map((ce) => ce.edge.weight), [9, 5], '按权重降序');
assert.equal(ces[0].other.id, 'c', '对端节点解析');
assert.equal(ces[1].other.id, 'b', '对端节点解析（另一端）');
assert.equal(connectedEdgesOf(graph, 'a', 1).length, 1, 'limit 生效');
assert.deepEqual(connectedEdgesOf(graph, 'nope'), [], '无相连边返回空');

// ── sharedGroupNames ──
const gnode2 = { id: 'g1', groupCodes: ['c1', 'c2', 'c3'] };
assert.deepEqual(
  sharedGroupNames(gnode2, { c1: '群一' }, 6),
  ['群一', 'c2', 'c3'],
  '群名映射 + 缺失回退 code',
);
assert.deepEqual(sharedGroupNames(gnode2, undefined, 6), ['c1', 'c2', 'c3'], '无映射全回退 code');
assert.deepEqual(sharedGroupNames(gnode2, { c1: '群一' }, 1), ['群一'], 'limit 生效');
assert.deepEqual(sharedGroupNames({ id: 'g2' }, { c1: '群一' }, 6), [], '无 groupCodes 返回空');

// ── toGraphData ──
const raw = {
  self: 'wx_self',
  self_avatar: 'data:image/png;base64,AAA',
  group_names: { g1: '群一' },
  summary: { total_contacts: 5, selected_groups: 2 },
  nodes: [
    { id: 'wx_self', kind: 'contact' },
    { id: 'c1', kind: 'contact' },
    { id: 'o1', kind: 'official' },
    { id: 'g1', kind: 'group' },
    { id: 'x1', kind: 'unknown' },
  ],
};
const td = toGraphData(raw);
assert.deepEqual(td.persons.map((n) => n.id), ['c1', 'o1'], '排除 self，保留联系人/公众号');
assert.deepEqual(td.groups.map((n) => n.id), ['g1'], '群节点筛选');
assert.equal(td.selfUin, 'wx_self', 'selfUin');
assert.equal(td.selfAvatar, 'data:image/png;base64,AAA', 'selfAvatar');
assert.deepEqual(td.groupNames, { g1: '群一' }, 'groupNames');
assert.equal(td.scannedGroups, 2, 'scannedGroups 取 summary');
assert.equal(td.builtAt, 0, 'builtAt 恒 0');
assert.deepEqual(toGraphData({}).persons, [], '空数据容错');
assert.deepEqual(toGraphData({ nodes: null }).groups, [], 'nodes 缺失容错');

// ── makePosterInput（海报文案配置，自 RelationshipGraph.doExport 下沉） ──
const base = {
  ratio: '1:1',
  isPeople: true,
  dateStr: '2026-08-14',
  timeStr: '2026-08-14 20:00',
  contactBookFriends: 1234,
  personCount: 50,
  groupCount: 20,
  edgesCount: 321,
  communityCount: 8,
  totalGroups: 0,
};
const pi = makePosterInput(base);
assert.equal(pi.title, '我的微信社交图谱', '海报标题');
assert.equal(pi.tag, 'WECHAT SOCIAL GRAPH', '海报标签');
assert.equal(pi.ratio, '1:1', '比例透传');
assert.equal(pi.subtitle, '群友圈子 · 数据来自本地微信记录 · 2026-08-14', '人脉模式副标题');
assert.deepEqual(pi.stats, [
  { label: '好友', value: '1234' },
  { label: '展示节点', value: '50' },
  { label: '连线', value: '321' },
  { label: '圈子', value: '8' },
], '人脉模式统计');
assert.equal(pi.legend, '● 颜色 = 圈子　— 连线 = 共同群数　◍ 大小 = 消息量', '人脉模式图例');
assert.equal(pi.footer, '由 ST 控制台生成 · 2026-08-14 20:00', '页脚时间戳');
const pg = makePosterInput({ ...base, isPeople: false, totalGroups: 42 });
assert.equal(pg.subtitle.startsWith('群聊网络'), true, '群聊模式副标题');
assert.deepEqual(pg.stats[0], { label: '群聊', value: '42' }, '群聊模式优先 totalGroups');
assert.deepEqual(makePosterInput({ ...base, isPeople: false }).stats[0], { label: '群聊', value: '20' }, 'totalGroups 缺省回退 groupCount');
assert.equal(pg.legend, '● 颜色 = 圈子　— 连线 = 共同成员数　◍ 大小 = 消息量', '群聊模式图例');

console.log('smoke-graph-stats: all assertions passed');
