// 社交关系图谱（WeQ 迁移）— 运行期验证
const list = await (await fetch('http://127.0.0.1:9222/json/list')).json();
const target = list.find((x) => x.type === 'page');
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
let id = 0;
const pending = new Map();
const exceptions = [];
ws.onmessage = (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m.result); pending.delete(m.id); }
  else if (m.method === 'Runtime.exceptionThrown') {
    exceptions.push(m.params.exceptionDetails?.exception?.description || m.params.exceptionDetails?.text || 'unknown');
  }
};
const send = (method, params = {}) =>
  new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const ev = async (expression) => {
  const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails));
  return r.result.value;
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
await send('Runtime.enable');
let passed = 0;
const ok = (cond, msg) => {
  if (!cond) throw new Error('断言失败：' + msg);
  passed++;
  console.log('✓', msg);
};

// 强制全量刷新，确保加载最新代码
await ev(`location.reload()`);
for (let i = 0; i < 25; i++) {
  const hasNav = await ev(`!!document.querySelector('.nav-item')`);
  if (hasNav) break;
  await ev(`location.reload()`);
  await sleep(1500);
}

// 后端数据：新字段（group_codes / is_friend / shared_count）
const data = await ev(`window.__TAURI_INTERNALS__.invoke('get_relationship_graph', { limit: 1000 })`);
const persons = (data?.nodes ?? []).filter((n) => n.kind === 'contact' || n.kind === 'official');
const groups = (data?.nodes ?? []).filter((n) => n.kind === 'group');
ok(Array.isArray(data?.nodes) && data.nodes.length > 0, '图谱数据已生成（节点 ' + data.nodes.length + '）');
ok(
  persons.some((p) => Array.isArray(p.group_codes) && p.group_codes.length > 0),
  '联系人节点带共同群 group_codes（群友圈子数据源就绪）',
);
ok(persons.every((p) => typeof p.is_friend === 'boolean'), '联系人节点带 is_friend 好友标记');
ok(groups.every((g) => typeof g.shared_count === 'number'), '群节点带 shared_count 命中数');
ok(Array.isArray(data?.self_accounts?.wxids) && data.self_accounts.wxids.includes(data.self), '返回 self_accounts（本机账号清单）');
ok(
  /^wxid_[a-z0-9]+$/.test(String(data?.self ?? '')),
  'self 已剥离实例后缀（' + String(data?.self ?? '') + '）',
);
const selfSet = new Set(data?.self_accounts?.wxids ?? []);
ok(
  persons.every((p) => !selfSet.has(p.id)),
  '联系人节点已排除本机其他微信账号（避免出现第二个“我”）',
);

// 前端：进入 微信数据 → 关系图谱，Canvas 渲染
await ev(`document.querySelector('.nav-item[title="微信数据"]').click()`);
// 等待微信面板就绪（bootstrap 通过、图谱入口出现）后点击
let tabClicked = false;
for (let i = 0; i < 40 && !tabClicked; i++) {
  tabClicked = await ev(`(() => {
    if (!document.querySelector('.wc-root')) return false;
    const btns = [...document.querySelectorAll('button')];
    const b = btns.find((x) => (x.textContent || '').includes('关系图谱'));
    if (!b) return false;
    b.click();
    return true;
  })()`);
  if (!tabClicked) await sleep(1500);
}
ok(tabClicked, '已进入「关系图谱」页签');
let canvasFound = false;
for (let i = 0; i < 60; i++) {
  canvasFound = await ev(`!!document.querySelector('.gx-canvas-wrap')`);
  if (canvasFound) break;
  await sleep(1500);
}
ok(canvasFound, '关系图谱 Canvas 已渲染');

const ui = await ev(`(() => {
  const root = [...document.querySelectorAll('.panel')].find((el) => !el.classList.contains('panel-hidden'));
  if (!root) return null;
  return {
    hasSeg: !!root.querySelector('.rg-seg'),
    hasControls: !!root.querySelector('.rg-controls'),
    hasChips: !!root.querySelector('.rg-chips'),
    hasCanvas: !!root.querySelector('.gx-canvas-wrap'),
  };
})()`);
ok(ui?.hasSeg && ui?.hasControls && ui?.hasChips && ui?.hasCanvas, '图谱界面：模式切换/控制面板/统计/画布齐全');

// —— 交互回归（WeQ 迁移的两种模式与控制面板） ——
const snap = () => ev(`(() => {
  const btns = [...document.querySelectorAll('.rg-seg button')];
  const stats = document.querySelector('.rg-chips') ? (document.querySelector('.rg-chips').textContent || '').replace(/\\s+/g, ' ').trim() : '';
  const toggles = [...document.querySelectorAll('.rg-toggle')].map((x) => ({
    t: (x.textContent || '').replace(/\\s+/g, ' ').trim(),
    aria: x.getAttribute('aria-checked'),
  }));
  const sliders = [...document.querySelectorAll('.rg-controls input[type="range"]')].map((x) => x.value);
  const filterBtn = document.querySelector('.rg-filter-btn')?.textContent?.replace(/\\s+/g, ' ').trim() || '';
  return { active: (btns.find((b) => b.className.includes('on'))?.textContent || '').trim(), stats, toggles, sliders, filterBtn };
})()`);

let s = await snap();
ok(s.active.includes('群友圈子'), '默认处于「群友圈子」模式');
ok(s.toggles.some((t) => t.t.includes('消息量决定大小')), '群友圈子模式：消息量开关展示');
ok(
  s.toggles.find((t) => t.t.includes('消息量决定亲疏'))?.aria === 'true',
  '「消息量决定亲疏」默认启用',
);

await ev(`(() => { [...document.querySelectorAll('.rg-seg button')].find((x) => (x.textContent || '').includes('群聊网络')).click(); return true; })()`);
await sleep(1200);
s = await snap();
ok(s.active.includes('群聊网络'), '可切换到「群聊网络」模式');
ok(s.toggles.some((t) => t.t.includes('命中数决定大小')) && s.toggles.some((t) => t.t.includes('命中数决定亲疏')), '群聊网络模式：命中数开关展示');
ok(
  s.toggles.find((t) => t.t.includes('命中数决定大小'))?.aria === 'true' &&
    s.toggles.find((t) => t.t.includes('命中数决定亲疏'))?.aria === 'true',
  '群聊网络「命中数决定大小/亲疏」默认启用',
);
ok(s.stats.includes('个圈子'), '群聊网络统计更新（' + s.stats + '）');

const sizeAria = () => ev(`(() => [...document.querySelectorAll('.rg-toggle')].find((x) => x.textContent.includes('命中数决定大小')).getAttribute('aria-checked'))()`);
const aria1 = await sizeAria();
await ev(`(() => { [...document.querySelectorAll('.rg-toggle')].find((x) => x.textContent.includes('命中数决定大小')).click(); return true; })()`);
await sleep(600);
const aria2 = await sizeAria();
ok(aria1 !== aria2, '「命中数决定大小」开关可切换（' + aria1 + ' -> ' + aria2 + '）');
await ev(`(() => { [...document.querySelectorAll('.rg-toggle')].find((x) => x.textContent.includes('命中数决定大小')).click(); return true; })()`);
await sleep(500);

const oldThreshold = await ev(`document.querySelectorAll('.rg-controls input[type="range"]')[1]?.value`);
await ev(`(() => {
  const r = document.querySelectorAll('.rg-controls input[type="range"]')[1];
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(r, '3');
  r.dispatchEvent(new Event('input', { bubbles: true }));
  r.dispatchEvent(new Event('change', { bubbles: true }));
  return true;
})()`);
await sleep(1000);
ok((await ev(`document.querySelectorAll('.rg-controls input[type="range"]')[1]?.value`)) === '3', '连线阈值滑块可调（' + oldThreshold + ' -> 3）');

await ev(`document.querySelector('.rg-filter-btn').click()`);
await sleep(400);
ok(await ev(`!!document.querySelector('.rg-picker')`), '群过滤弹窗可打开');
await ev(`(() => { [...document.querySelectorAll('.rg-picker-modes button')].find((x) => x.textContent.trim() === '白名单').click(); return true; })()`);
await sleep(400);
ok((await snap()).filterBtn.startsWith('白名单'), '群过滤可切换为白名单');
ok((await ev(`document.querySelectorAll('.rg-picker-list input[type="checkbox"]').length`)) > 0, '白名单列表包含可选群聊');
await ev(`(() => { document.querySelector('.rg-picker-list input[type="checkbox"]').click(); return true; })()`);
await sleep(400);
ok((await snap()).filterBtn.includes('1'), '勾选后白名单计数更新');
await ev(`document.querySelector('.rg-picker-close').click()`);
await sleep(300);
await ev(`document.querySelector('.rg-filter-btn').click()`);
await sleep(300);
await ev(`(() => { [...document.querySelectorAll('.rg-picker-modes button')].find((x) => x.textContent.trim() === '全部').click(); return true; })()`);
await ev(`document.querySelector('.rg-picker-close').click()`);
await sleep(400);

const refreshCycle = await ev(`(async () => {
  const btn = document.querySelector('.rg-refresh');
  const phases = [];
  const mo = new MutationObserver(() => {
    const t = btn.textContent.trim();
    if (!phases.length || phases[phases.length - 1] !== t) phases.push(t);
  });
  mo.observe(btn, { attributes: true, childList: true, subtree: true, characterData: true });
  btn.click();
  await new Promise((r) => setTimeout(r, 8000));
  mo.disconnect();
  return phases;
})()`);
ok(Array.isArray(refreshCycle) && refreshCycle.includes('刷新中…') && refreshCycle[refreshCycle.length - 1] === '刷新', '刷新按钮进入「刷新中…」并恢复');

const hit = await ev(`(async () => {
  const wrap = document.querySelector('.gx-canvas-wrap');
  const tip = document.querySelector('.gx-tooltip');
  if (!wrap || !tip) return null;
  const rect = wrap.getBoundingClientRect();
  for (let y = 15; y < rect.height - 15; y += 24) {
    for (let x = 15; x < rect.width - 15; x += 24) {
      wrap.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, clientX: rect.left + x, clientY: rect.top + y }));
      await new Promise((r) => setTimeout(r, 0));
      if (tip.style.display !== 'none' && tip.textContent.trim()) {
        return { x: Math.round(rect.left + x), y: Math.round(rect.top + y), label: tip.textContent.replace(/\\s+/g, ' ').trim() };
      }
    }
  }
  return null;
})()`);
ok(!!hit, 'Canvas 悬停可识别节点（' + (hit?.label || '未命中') + '）');
await send('Input.dispatchMouseEvent', { type: 'mousePressed', x: hit.x, y: hit.y, button: 'left', clickCount: 1 });
await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: hit.x, y: hit.y, button: 'left', clickCount: 1 });
await sleep(600);
ok(await ev(`!!document.querySelector('.rg-detail')`), '点选节点显示详情卡');
ok(await ev(`!!document.querySelector('.rg-open')`), '详情卡含「打开聊天」入口');
await ev(`document.querySelector('.rg-detail-close').click()`);
await sleep(300);
ok(!(await ev(`!!document.querySelector('.rg-detail')`)), '详情卡可关闭');

ok(exceptions.length === 0, '交互全程无未捕获异常');

console.log(`\n社交图谱迁移验证通过：${passed} 项`);
ws.close();
process.exit(0);
