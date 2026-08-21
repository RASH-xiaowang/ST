// 复现「进入群聊后最新消息未显示」：进入群聊后测量消息容器滚动状态
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
(async () => {
  const list = await (await fetch('http://127.0.0.1:9222/json/list')).json();
  const t = list.find((x) => x.type === 'page' && x.url.includes('localhost:1420'));
  if (!t) { console.log('no page'); process.exit(1); }
  const ws = new WebSocket(t.webSocketDebuggerUrl);
  await new Promise((r) => ws.onopen = r);
  let id = 0; const pending = new Map();
  ws.onmessage = (ev) => { const m = JSON.parse(ev.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m.result); pending.delete(m.id); } };
  const send = (method, params) => new Promise((res) => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method, params })); });
  const ev = async (expr) => (await send('Runtime.evaluate', { expression: expr, awaitPromise: true, returnByValue: true })).result?.value;

  await ev(`(() => { const el = [...document.querySelectorAll('.nav-item')].find(e => (e.getAttribute('title')||'').includes('微信数据')); if (el) el.click(); return !!el; })()`);
  await sleep(2500);
  await ev(`(() => { const el = [...document.querySelectorAll('button.wc-ihb')].find(e => (e.textContent||'').includes('聊天')); if (el) el.click(); return !!el; })()`);
  await sleep(2500);
  // 先切到另一个会话，再进入目标群聊（模拟「从别的会话进入群聊」）
  await ev(`(() => { const els = [...document.querySelectorAll('button.wc-chat-item')]; const other = els.find(e => !(e.textContent||'').includes('沃融')); if (other) other.click(); return !!other; })()`);
  await sleep(2500);
  await ev(`(() => { const el = [...document.querySelectorAll('button.wc-chat-item')].find(e => (e.textContent||'').includes('沃融')); if (el) el.click(); return !!el; })()`);
  console.log('clicked group, waiting for messages…');
  await sleep(6000);

  const state = await ev(`(() => {
    const el = document.querySelector('.wc-msgs');
    if (!el) return { err: 'no .wc-msgs' };
    const rows = [...el.querySelectorAll('[data-idx]')];
    const idxs = rows.map(r => Number(r.dataset.idx));
    const last = rows[rows.length - 1];
    const lastBottom = last ? last.getBoundingClientRect().bottom : null;
    const rect = el.getBoundingClientRect();
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    return {
      scrollTop: el.scrollTop, scrollHeight: el.scrollHeight, clientHeight: el.clientHeight,
      gapToBottom: atBottom,
      renderedCount: rows.length, minIdx: idxs.length ? Math.min(...idxs) : null, maxIdx: idxs.length ? Math.max(...idxs) : null,
      lastMsgBottomInView: lastBottom != null ? (lastBottom <= rect.bottom + 1) : null,
      lastMsgBottomVsContainer: lastBottom != null ? Math.round(lastBottom - rect.bottom) : null,
    };
  })()`);
  console.log('state:', JSON.stringify(state, null, 2));

  // 若未贴底，等待图片加载后再看一次（吸底守护是否生效）
  await sleep(6000);
  const state2 = await ev(`(() => {
    const el = document.querySelector('.wc-msgs');
    if (!el) return { err: 'no .wc-msgs' };
    const rows = [...el.querySelectorAll('[data-idx]')];
    const last = rows[rows.length - 1];
    const lastBottom = last ? last.getBoundingClientRect().bottom : null;
    const rect = el.getBoundingClientRect();
    return {
      scrollTop: el.scrollTop, scrollHeight: el.scrollHeight, clientHeight: el.clientHeight,
      gapToBottom: el.scrollHeight - el.scrollTop - el.clientHeight,
      renderedCount: rows.length,
      lastMsgBottomInView: lastBottom != null ? (lastBottom <= rect.bottom + 1) : null,
    };
  })()`);
  console.log('state2 (after 6s):', JSON.stringify(state2, null, 2));
  ws.close();
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
