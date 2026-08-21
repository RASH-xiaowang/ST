// 验证：AI 聊天已并入 Harness 会话（无独立板块；角色注入迁移进会话）
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const list = await (await fetch('http://127.0.0.1:9222/json/list')).json();
const t = list.find((x) => x.type === 'page' && x.url.includes('localhost:1420'));
if (!t) { console.log('NO PAGE'); process.exit(1); }
const ws = new WebSocket(t.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
let id = 0; const pend = new Map();
ws.onmessage = (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id && pend.has(m.id)) { const { resolve, reject } = pend.get(m.id); pend.delete(m.id); m.error ? reject(new Error(JSON.stringify(m.error))) : resolve(m.result); }
};
const send = (method, params = {}) => new Promise((resolve, reject) => {
  pend.set(++id, { resolve, reject });
  ws.send(JSON.stringify({ id, method, params }));
});
const ev = (expression) => send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })
  .then((r) => { if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails)); return r.result?.value; });
const invoke = (cmd, args = {}) => ev(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`);
let fails = 0;
const check = (ok, msg) => { console.log((ok ? 'PASS: ' : 'FAIL: ') + msg); if (!ok) fails++; };

// 1. 导航栏：无独立「AI 聊天」入口；无视图切换条（单一聊天界面）
await send('Page.reload', { ignoreCache: true });
await sleep(5000);
const navTitles = await ev(`JSON.stringify([...document.querySelectorAll('.nav-item')].map((x) => x.title))`);
const navList = JSON.parse(navTitles);
check(!navList.includes('AI 聊天'), `导航栏无独立「AI 聊天」入口（${navList.filter((x) => x.includes('对话') || x === 'Harness')}）`);

// 2. 进入 Harness：直接是会话聊天界面（无 viewbar）
await ev(`(() => { const b = [...document.querySelectorAll('button.nav-item')].find((x) => x.title === 'Harness'); if (b) b.click(); return 'true'; })()`);
await sleep(2500);
const viewbar = await ev(`document.querySelector('.hns-viewbar') ? 'true' : 'false'`);
check(viewbar === 'false', '已移除视图切换条（单一聊天界面）');
const sideShown = await ev(`(() => { const el = document.querySelector('.hns-side'); return el && el.offsetParent !== null ? 'true' : 'false'; })()`);
check(sideShown === 'true', 'Harness 会话侧栏直接可见');

// 3. AI 角色注入迁移：头部角色选择器 + IPC 持久化（日志投影）
const roles = await invoke('get_ai_roles');
const anyRole = (roles ?? []).find((r) => r.enabled);
check(Array.isArray(roles), '角色列表 IPC 可用');
if (anyRole) {
  // 直接经 IPC 应用到当前会话
  const sessions = await invoke('harness_list_sessions');
  const sid = sessions?.[0]?.id;
  const roleView = await invoke('harness_get_session_role', { id: sid });
  check(typeof roleView?.name === 'string' && typeof roleView?.prompt === 'string', `读取会话角色（name=${roleView?.name}）`);
  const selCount = await ev(`(() => {
    const sels = [...document.querySelectorAll('.hns-bar-right select')];
    const r = sels.find((x) => x.title && x.title.includes('AI 角色'));
    return r ? [...r.options].map((o) => o.textContent.trim()).length : 0;
  })()`);
  check(selCount >= 1, `头部角色选择器存在（${selCount} 个选项，含「无角色」）`);
}

// 4. 概览页卡片「AI 对话」→ Harness
await ev(`(() => { const b = [...document.querySelectorAll('button.nav-item')].find((x) => x.title === '首页'); if (b) b.click(); return 'true'; })()`);
await sleep(1500);
const cardOk = await ev(`(() => {
  const cards = [...document.querySelectorAll('button')];
  const c = cards.find((x) => (x.textContent ?? '').includes('AI 对话') && (x.textContent ?? '').includes('智能体 + 角色对话'));
  if (!c) return 'no-card';
  c.click();
  return 'clicked';
})()`);
check(cardOk === 'clicked', `概览卡「AI 对话」存在（${cardOk}）`);
await sleep(2000);
const afterCard = await ev(`(() => { const el = document.querySelector('.hns-side'); return el && el.offsetParent !== null ? 'true' : 'false'; })()`);
check(afterCard === 'true', '概览卡跳转后进入 Harness 会话界面');

console.log(fails === 0 ? 'ALL_PASS' : `FAILURES=${fails}`);
process.exit(fails === 0 ? 0 : 1);
