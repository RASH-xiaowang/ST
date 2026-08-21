// 已停用（2026-08）：原「AI 聊天」板块已并入 Harness 会话，本脚本断言的独立面板 UI 不再存在；
// 功能验证改由 e2e-harness-phase* 与 verify-harness-chat-* 承担。
// AI 聊天重设计 — 渲染冒烟验证
const list = await (await fetch('http://127.0.0.1:9222/json/list')).json();
const target = list.find((x) => x.type === 'page');
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
let id = 0;
const pending = new Map();
ws.onmessage = (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m.result); pending.delete(m.id); }
};
const send = (method, params = {}) =>
  new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const ev = async (expression) => {
  const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails));
  return r.result.value;
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let passed = 0;
const ok = (cond, msg) => {
  if (!cond) throw new Error('断言失败：' + msg);
  passed++;
  console.log('✓', msg);
};

// 确保页面已加载（dev 页面可能停在 chrome-error，需要 reload 后轮询等待）
for (let i = 0; i < 25; i++) {
  const hasNav = await ev(`!!document.querySelector('.nav-item')`);
  if (hasNav) break;
  await ev(`location.reload()`);
  await sleep(1500);
}
// AI 聊天已整合进 Harness：先进入 Harness 面板，再点「AI 聊天」子视图
await ev(`document.querySelector('.nav-item[title="Harness"]').click()`);
await sleep(800);
await ev(`(() => { const b = [...document.querySelectorAll('.hns-viewbar button')].find((x) => (x.textContent || '').trim() === 'AI 聊天'); if (b) b.click(); return !!b; })()`);
await sleep(1200);

const state = await ev(`(() => {
  const root = [...document.querySelectorAll('.panel')].find((el) => !el.classList.contains('panel-hidden'));
  if (!root) return null;
  const text = root.innerText;
  return {
    hasToolbarLeft: !!root.querySelector('.llm-toolbar-left'),
    hasToolbarRight: !!root.querySelector('.llm-toolbar-right'),
    hasToolbarSep: !!root.querySelector('.llm-toolbar-sep'),
    hasModelSelects: root.querySelectorAll('.llm-select').length >= 2,
    hasChatWindow: !!root.querySelector('.llm-chat-window'),
    hasComposer: !!root.querySelector('.llm-chat-input textarea'),
    hasSendBtn: !!root.querySelector('.llm-btn-primary'),
    hasPaperclip: root.querySelector('.llm-ico-btn svg') !== null,
    hasEraser: root.querySelector('.llm-toolbar-right .llm-btn svg') !== null,
    textSnippet: text.slice(0, 120),
  };
})()`);
ok(!!state, 'AI 聊天面板已渲染');
ok(state.hasToolbarLeft && state.hasToolbarRight && state.hasToolbarSep, '工具栏已分为左右两组');
ok(state.hasModelSelects, '提供方/模型联动下拉存在');
ok(state.hasChatWindow && state.hasComposer && state.hasSendBtn, '消息区与输入区完整');
ok(state.hasPaperclip && state.hasEraser, '附件与清空按钮使用 SVG 图标');

console.log(`\nAI 聊天重设计冒烟通过：${passed} 项`);
ws.close();
process.exit(0);
