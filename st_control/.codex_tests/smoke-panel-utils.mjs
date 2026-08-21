// ============================================================
// WeChatPanel 纯函数 — 运行期冒烟测试
// 锁定 panel.ts 下沉后的可观测输出：
//   trimRecord / calHeat / cmpTid / editKey / sessionMatchesKeyword / filterByAnyKeyword
//   / groupMembersByRoom（群成员按所在群分组）
// 运行：node st_control/.codex_tests/smoke-panel-utils.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'wechat', 'utils', 'panel.ts'), 'utf8');
// panel.ts 依赖 ./misc（isKefuSession 等），需打包（bundle）解析，产物自包含。
const build = await esbuild.build({
  stdin: {
    contents: src,
    resolveDir: path.join(root, 'src', 'lib', 'wechat', 'utils'),
    loader: 'ts',
    sourcefile: 'panel.ts',
  },
  bundle: true,
  write: false,
  format: 'esm',
  platform: 'node',
  logLevel: 'silent',
});
const code = build.outputFiles[0].text;
const outFile = path.join(outDir, 'panel-utils.mjs');
writeFileSync(outFile, code);

const {
  trimRecord,
  calHeat,
  cmpTid,
  editKey,
  sessionMatchesKeyword,
  mergeMoments,
  groupContactsByInitial,
  sessionKeywordMatch,
  filterMainSessions,
  filterSortCheckupChats,
  filterFavoriteItems,
  selectedIdsFromRecord,
  filterByKeyword,
  filterByAnyKeyword,
  matchMonitors,
  collectSessionImages,
  zoomStepIndex,
  VIEWER_ZOOM_STEPS,
  buildStaticEmoticonMap,
  filterStaticEmoticons,
  filterSortResourceFiles,
  filterSettingsCats,
  groupMembersByRoom,
  groupMomentsByDate,
} = await import(pathToFileURL(outFile).href);

// ── trimRecord ──
const rec = { a: 1, b: 2, c: 3, d: 4 };
trimRecord(rec, 2);
assert.deepEqual(rec, { c: 3, d: 4 }, '超过 max 时删除最先插入的键');
const small = { a: 1, b: 2 };
trimRecord(small, 5);
assert.deepEqual(small, { a: 1, b: 2 }, '未超 max 时保持不变');

// ── calHeat ──
assert.equal(calHeat(0), 'var(--wc-bg2)');
assert.equal(calHeat(3), 'color-mix(in srgb, var(--wc-theme) 25%, transparent)');
assert.equal(calHeat(100), 'color-mix(in srgb, var(--wc-theme) 90%, transparent)');

// ── cmpTid（负数 tid：更负 = 更旧 = 排后） ──
assert.ok(cmpTid('-3463300', '-1000') > 0, '更负的 tid（更旧）排在后面');
assert.ok(cmpTid('-1000', '-3463300') < 0, '更新的 tid 排在前面');
assert.equal(cmpTid('5', '5'), 0);
// 非数值兜底：长度优先，其次字典序降序
assert.ok(cmpTid('abc', 'a') < 0);
assert.ok(cmpTid('b', 'a') < 0);

// ── editKey ──
assert.equal(editKey('wxid_x', 42), 'wxid_x:42');
assert.equal(editKey(null, 7), ':7');

// ── sessionMatchesKeyword ──
const s = { username: 'wxid_zhang', name: '张三' };
assert.equal(sessionMatchesKeyword(s, ''), true, '空关键词匹配全部');
assert.equal(sessionMatchesKeyword(s, '张'), true, '名称子串命中');
assert.equal(sessionMatchesKeyword(s, 'WXID'), true, 'username 大小写不敏感');
assert.equal(sessionMatchesKeyword(s, '李'), false, '无关关键词不命中');

// ── mergeMoments ──
const mk = (tid, text) => ({ tid, username: 'u', author: 'a', text, ts: 0, time: '', media_count: 0, media_desc: '', images: [], videos: [] });
// 无新增：原样返回
const noFresh = mergeMoments([mk('2', 'old')], [mk('2', 'updated')]);
assert.equal(noFresh.fresh.length, 0, '已存在 tid 不算新增');
assert.equal(noFresh.items.length, 1);
assert.equal(noFresh.items[0].text, 'updated', '已存在条目按最新数据更新');
// 新 tid 置顶 + tid 降序（负数 tid：更负更旧排后）
const withFresh = mergeMoments([mk('-100', 'existing')], [mk('-50', 'newer'), mk('-200', 'older')]);
assert.equal(withFresh.fresh.length, 2);
assert.deepEqual(withFresh.items.map((m) => m.text), ['newer', 'existing', 'older']);
// 空列表 + 新数据
const emptyStart = mergeMoments([], [mk('9', 'a'), mk('1', 'b')]);
assert.equal(emptyStart.fresh.length, 2);
assert.deepEqual(emptyStart.items.map((m) => m.tid), ['9', '1']);

// ── groupContactsByInitial ──
const contact = (u, initial) => ({ username: u, initial });
const groups = groupContactsByInitial([
  contact('c1', 'B'),
  contact('c2', undefined),
  contact('c3', 'A'),
  contact('c4', 'B'),
]);
assert.deepEqual(
  groups.map(([k]) => k),
  ['A', 'B', '#'],
  '按首字母排序，# 置底',
);
assert.equal(groups[1][1].length, 2, '同组联系人聚合');
assert.deepEqual(groupContactsByInitial([]), [], '空列表返回空');

// ── groupMembersByRoom（群成员按所在群聊分组） ──
const member = (u, g) => ({ username: u, group_name: g });
const roomGroups = groupMembersByRoom([
  member('m1', '项目群'),
  member('m2', undefined),
  member('m3', '家人群'),
  member('m4', '项目群'),
  member('m5', '  '),
]);
assert.deepEqual(
  roomGroups.map(([k]) => k),
  ['家人群', '项目群', '未归属群聊'],
  '按群名中文排序，无归属置底',
);
assert.deepEqual(roomGroups[1][1].map((c) => c.username), ['m1', 'm4'], '同群成员聚合');
assert.deepEqual(roomGroups[2][1].map((c) => c.username), ['m2', 'm5'], '空/空白群名归入未归属群聊');
assert.deepEqual(groupMembersByRoom([]), [], '空列表返回空');

// ── groupMomentsByDate（朋友圈日期分组）──
const mkMoment = (ts, tid) => ({ tid, ts, time: '', username: 'u', author: 'a', text: '', media_count: 0, media_desc: '', images: [], videos: [], location: '', link_title: '', is_self: false, likes: [], comments: [] });
const now = Date.now();
const dayMs = 86400_000;
const today = mkMoment(Math.floor(now / 1000), 't1');
const yest = mkMoment(Math.floor((now - dayMs) / 1000), 't2');
const older = mkMoment(Math.floor((now - 3 * dayMs) / 1000), 't3');
const unknown = mkMoment(0, 't4');
const dayGroups = groupMomentsByDate([today, yest, older, unknown]);
assert.equal(dayGroups.length, 4, '四天各一组');
assert.equal(dayGroups[0].label, '今天', '首条为今天');
assert.equal(dayGroups[1].label, '昨天', '次日为昨天');
assert.ok(/^\d{4}-\d{2}-\d{2}$/.test(dayGroups[2].label), '更早按日期显示');
assert.equal(dayGroups[3].label, '未知时间', '无时间置底');
// 同一天合并
const sameDay = groupMomentsByDate([today, mkMoment(Math.floor(now / 1000), 't5')]);
assert.equal(sameDay.length, 1, '同一天合并为一组');
assert.equal(sameDay[0].items.length, 2, '同组聚合两条');
assert.deepEqual(groupMomentsByDate([]), [], '空列表返回空');

// ── sessionKeywordMatch / filterMainSessions ──
const s2 = { username: 'wxid_zhang', name: '张三', summary: '上周聊过项目' };
assert.equal(sessionKeywordMatch(s2, '项目'), true, '摘要命中');
assert.equal(sessionKeywordMatch(s2, '张'), true, '名称命中');
assert.equal(sessionKeywordMatch(s2, ''), true, '空关键词命中全部');
const list = filterMainSessions(
  [
    { username: 'u1', name: '张三' },
    { username: 'gh_x', name: '公众号', is_official: true },
    { username: 'u2@kefu.openim', name: '客服' },
  ],
  '张',
);
assert.deepEqual(list.map((x) => x.username), ['u1'], '排除公众号/客服后按关键词过滤');
assert.equal(filterMainSessions(list, '').length, 1, '空关键词返回过滤后全量');

// ── filterSortCheckupChats ──
const ck = [
  { username: 'a', name: '张三', missing: 3, total_images: 10 },
  { username: 'b', name: '李四', missing: 0, total_images: 5 },
  { username: 'c', name: '王五', missing: 1, total_images: 20 },
];
assert.deepEqual(
  filterSortCheckupChats(ck, { q: '', onlyMissing: false, sort: 'missing' }).map((x) => x.username),
  ['a', 'c', 'b'],
  '缺失数降序（并列按总量降序）',
);
assert.deepEqual(
  filterSortCheckupChats(ck, { q: '', onlyMissing: true, sort: 'total' }).map((x) => x.username),
  ['c', 'a'],
  '仅缺失 + 总量降序',
);
assert.deepEqual(
  filterSortCheckupChats(ck, { q: '张', onlyMissing: false, sort: 'name' }).map((x) => x.username),
  ['a'],
  '关键词过滤 + 名称排序',
);
assert.deepEqual(filterSortCheckupChats([], { q: '', onlyMissing: false, sort: 'missing' }), [], '空列表');

// ── filterFavoriteItems ──
const favs = [
  { local_id: 1, type_label: 'link', title: '项目文档', source: '微信' },
  { local_id: 2, type_label: 'text', title: '会议纪要', desc: '本周计划' },
  { local_id: 3, type_label: 'link', title: '博客', source: '网页' },
];
assert.deepEqual(
  filterFavoriteItems(favs, { type: 'link', q: '' }).map((x) => x.local_id),
  [1, 3],
  '按类型过滤',
);
assert.deepEqual(
  filterFavoriteItems(favs, { type: 'all', q: '会议' }).map((x) => x.local_id),
  [2],
  '按标题关键词过滤',
);
assert.deepEqual(
  filterFavoriteItems(favs, { type: 'all', q: '网页' }).map((x) => x.local_id),
  [3],
  '按来源关键词过滤',
);
assert.deepEqual(
  filterFavoriteItems(favs, { type: 'all', q: '' }).length,
  3,
  '空过滤返回全量',
);

// ── selectedIdsFromRecord ──
assert.deepEqual(
  selectedIdsFromRecord({ '1': true, '2': false, '3': true, 'abc': true, '-5': true }),
  [1, 3],
  '仅保留选中且为正数的 id',
);
assert.deepEqual(selectedIdsFromRecord({}), [], '空记录返回空');

// ── filterByKeyword ──
const kwItems = [{ name: '张三' }, { name: '李四' }, { name: '' }];
assert.deepEqual(
  filterByKeyword(kwItems, '张', (i) => i.name),
  [{ name: '张三' }],
  '关键词子串过滤（大小写不敏感）',
);
assert.equal(filterByKeyword(kwItems, '', (i) => i.name).length, 3, '空关键词返回全量');
assert.deepEqual(
  filterByKeyword([{ md5: 'ABC123' }], 'abc', (i) => i.md5),
  [{ md5: 'ABC123' }],
  'md5 大小写不敏感匹配',
);

// ── filterByAnyKeyword ──
const anyItems = [
  { name: 'Alpha 科技', username: 'gh_a1' },
  { name: 'Beta 服务号', username: 'gh_b2' },
  { name: '', username: 'gh_alpha3' },
];
assert.deepEqual(
  filterByAnyKeyword(anyItems, 'ALPHA', (i) => i.name, (i) => i.username),
  [anyItems[0], anyItems[2]],
  '任一字段命中即保留（大小写不敏感）',
);
assert.deepEqual(
  filterByAnyKeyword(anyItems, '服务号', (i) => i.name, (i) => i.username),
  [anyItems[1]],
  '第二字段命中',
);
assert.equal(filterByAnyKeyword(anyItems, '', (i) => i.name, (i) => i.username) === anyItems, true, '空关键词返回原数组引用');
assert.equal(filterByAnyKeyword(anyItems, '   ', (i) => i.name, (i) => i.username) === anyItems, true, '纯空白关键词返回原数组引用');
assert.deepEqual(
  filterByAnyKeyword(anyItems, '不存在', (i) => i.name, (i) => i.username),
  [],
  '未命中返回空',
);
assert.deepEqual(
  filterByAnyKeyword(anyItems, 'gh_', (i) => i.name, (i) => i.username),
  anyItems,
  '多字段前缀匹配全量',
);

// ── matchMonitors ──
const rules = [
  { id: 1, kind: 'keyword', value: '你好', enabled: true },
  { id: 2, kind: 'regex', value: '\\d+条', enabled: true },
  { id: 3, kind: 'sender', value: 'wxid_a', enabled: true },
  { id: 4, kind: 'media', value: 'image', enabled: true },
  { id: 5, kind: 'keyword', value: 'disabled', enabled: false },
];
assert.deepEqual(matchMonitors({ content: '你好，共 3条消息', sender_username: 'wxid_a', media_type: 'text' }, rules), [1, 2, 3], '多规则命中');
assert.deepEqual(matchMonitors({ content: '图', media_type: 'image' }, rules), [4], '媒体规则精确匹配');
assert.deepEqual(matchMonitors({ content: 'disabled 规则', media_type: 'image' }, rules), [4], '未启用规则不参与');
assert.deepEqual(matchMonitors({ content: 'ok' }, rules), [], '无命中返回空');
assert.deepEqual(matchMonitors({ content: 'x' }, [{ id: 9, kind: 'regex', value: '[', enabled: true }]), [], '非法正则容错');
assert.deepEqual(matchMonitors({}, []), [], '空规则返回空');

// ── collectSessionImages（图片查看器数据源，自 WeChatPanel 下沉） ──
const msgs = [
  { type: 3, local_id: 1, image_url: 'data:img1', time: '10:00', sender_username: 'a', is_group: false },
  { type: 3, local_id: 2, image_url: '', time: '10:01', sender_username: 'b' },
  { type: 1, local_id: 3, text: '文本', time: '10:02' },
  { type: 3, local_id: 4, time: '10:03', sender_username: 'c', is_group: true },
];
const cache = { 'wx_s:2': 'data:img2', 'wx_s:4': 'data:img4' };
assert.deepEqual(
  collectSessionImages(msgs, cache, 'wx_s').map((i) => i.local_id),
  [1, 2, 4],
  '图片消息收集（直链 + 缓存回退，文本消息排除）',
);
assert.deepEqual(
  collectSessionImages(msgs, cache, 'wx_s')[0],
  { src: 'data:img1', time: '10:00', local_id: 1, sender_username: 'a', is_group: false },
  '条目结构（直链优先）',
);
assert.deepEqual(
  collectSessionImages(msgs, cache, 'wx_s')[2],
  { src: 'data:img4', time: '10:03', local_id: 4, sender_username: 'c', is_group: true },
  '缓存回退 + 群聊标记',
);
assert.deepEqual(collectSessionImages(msgs, cache, null), [], '无会话返回空');
assert.deepEqual(collectSessionImages([], cache, 'wx_s'), [], '空消息返回空');
assert.deepEqual(
  collectSessionImages([{ type: 3, local_id: 9, time: '' }], {}, 'wx_s'),
  [],
  '无 src（直链/缓存均无）不收集',
);

// ── zoomStepIndex / VIEWER_ZOOM_STEPS（图片查看器缩放步进，自 WeChatPanel 下沉） ──
const STEPS = [1, 1.5, 2, 3, 4];
assert.deepEqual([...VIEWER_ZOOM_STEPS], STEPS, '缩放档位常量');
assert.equal(zoomStepIndex(STEPS, 1, 1, 'cycle'), 1, 'cycle 推进');
assert.equal(zoomStepIndex(STEPS, 4, 1, 'cycle'), 0, 'cycle 末位回绕');
assert.equal(zoomStepIndex(STEPS, 2.5, 1, 'cycle'), 1, '未命中值从 0 起算');
assert.equal(zoomStepIndex(STEPS, 1, 1, 'clamp'), 1, 'clamp 推进');
assert.equal(zoomStepIndex(STEPS, 4, 1, 'clamp'), 4, 'clamp 上限封顶');
assert.equal(zoomStepIndex(STEPS, 1, -1, 'clamp'), 0, 'clamp 下限封顶');
assert.equal(zoomStepIndex(STEPS, 2, -1, 'clamp'), 1, 'clamp 后退（2 → 1.5 档）');
assert.equal(zoomStepIndex(STEPS, 0, -1, 'clamp'), 0, '未命中值后退从 0 起算');

// ── 静态表情（buildStaticEmoticonMap / filterStaticEmoticons，自 WeChatPanel 下沉） ──
const emoCats = [
  { category: 'smile', label: '微笑', files: [{ name: 'a.png', path: '/emo/a.png' }, { name: 'b.PNG', path: '/emo/b.PNG' }] },
  { category: 'angry', label: '愤怒', files: [{ name: 'a.png', path: '/emo2/a.png' }, { name: 'c.gif', path: '/emo2/c.gif' }] },
  { category: 'empty', label: '空', files: [] },
];
const emoMap = buildStaticEmoticonMap(emoCats);
assert.equal(emoMap.get('a'), '/emo/a.png', '去 .png 后缀映射（大小写不敏感后缀）');
assert.equal(emoMap.get('b'), '/emo/b.PNG', 'PNG 大写后缀');
assert.equal(emoMap.get('c.gif'), '/emo2/c.gif', '非 png 以原名映射');
assert.equal(emoMap.size, 3, '同名不同分类首现优先');
assert.deepEqual(filterStaticEmoticons(emoCats, 'all', '').map((x) => x.file.name), ['a.png', 'b.PNG', 'a.png', 'c.gif'], '全量（含空分类跳过）');
assert.deepEqual(filterStaticEmoticons(emoCats, 'angry', '').map((x) => x.file.name), ['a.png', 'c.gif'], '分类过滤');
assert.deepEqual(filterStaticEmoticons(emoCats, 'all', 'A').map((x) => x.file.name), ['a.png', 'a.png'], '关键词匹配文件名（大小写不敏感）');
assert.deepEqual(filterStaticEmoticons(emoCats, 'all', '愤怒').map((x) => x.file.name), ['a.png', 'c.gif'], '关键词匹配分类标签');
assert.deepEqual(filterStaticEmoticons(emoCats, 'all', 'zzz'), [], '无命中返回空');

// ── filterSortResourceFiles（资源文件过滤排序，自 WeChatPanel shownFiles 下沉） ──
const fileData = {
  images: [
    { file_name: 'a.png', md5: 'md5a', category: 'image', modify_time: 100 },
    { file_name: 'b.png', md5: 'md5b', category: 'image', modify_time: 300 },
  ],
  videos: [{ file_name: 'v.mp4', md5: 'md5v', category: 'video', modify_time: 200 }],
  files: [{ file_name: 'doc.pdf', md5: 'md5d', category: 'file', modify_time: 150 }],
};
assert.deepEqual(
  filterSortResourceFiles(fileData, 'all', '').map((f) => f.file_name),
  ['b.png', 'v.mp4', 'doc.pdf', 'a.png'],
  '三列表合并 + modify_time 降序',
);
assert.deepEqual(
  filterSortResourceFiles(fileData, 'video', '').map((f) => f.file_name),
  ['v.mp4'],
  '分类过滤',
);
assert.deepEqual(
  filterSortResourceFiles(fileData, 'all', 'MD5V').map((f) => f.file_name),
  ['v.mp4'],
  'md5 关键词（大小写不敏感）',
);
assert.deepEqual(
  filterSortResourceFiles(fileData, 'all', 'png').map((f) => f.file_name),
  ['b.png', 'a.png'],
  'file_name 关键词 + 时间排序',
);

// ── filterSettingsCats（设置分类行内搜索，自 WeChatPanel settingsFilteredCats 下沉） ──
const cats = [
  { key: 'c1', label: '会话', table: 'session', columns: ['a'], column_labels: ['A'], rows: [['hello'], ['world']], count: 2, total: 2 },
  { key: 'c2', label: '文件', table: 'file', columns: ['b'], column_labels: ['B'], rows: [['foo']], count: 1, total: 1 },
];
const catsRef = filterSettingsCats(cats, '');
assert.equal(catsRef, cats, '空关键词返回原数组引用');
assert.deepEqual(
  filterSettingsCats(cats, 'HELLO').map((c) => ({ key: c.key, count: c.count, rows: c.rows })),
  [{ key: 'c1', count: 1, rows: [['hello']] }],
  '行内命中（大小写不敏感）+ count 更新',
);
assert.deepEqual(
  filterSettingsCats(cats, '文件').map((c) => c.key),
  ['c2'],
  'label 命中',
);
assert.deepEqual(
  filterSettingsCats(cats, 'session').map((c) => c.key),
  ['c1'],
  'table 命中',
);
assert.deepEqual(filterSettingsCats(cats, 'zzz'), [], '无命中返回空');

console.log('smoke-panel-utils: all assertions passed');
