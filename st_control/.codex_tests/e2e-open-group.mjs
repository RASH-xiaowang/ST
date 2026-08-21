// 打开指定群聊，读取消息区显示状态
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const TARGET = process.argv[2] || '22104597050@chatroom';
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
  const found = await ev(`(() => {
    const target = ${JSON.stringify(TARGET)};
    const el = [...document.querySelectorAll('button.wc-chat-item')].find(e => (e.textContent||'').includes(target));
    if (!el) return null;
    el.click();
    return el.textContent.trim().slice(0, 80);
  })()`);
  console.log('clicked:', found);
  if (!found) { console.log('chat not found in list'); ws.close(); process.exit(1); }
  await sleep(5000);
  const st = await ev(`(() => {
    const msgs = document.querySelector('.wc-msgs');
    const inner = msgs?.querySelector('.wc-msgs-inner');
    const empty = msgs?.querySelector('.wc-empty');
    const err = msgs?.querySelector('.wc-error-hint');
    return {
      hasMsgs: !!msgs,
      msgRows: inner ? inner.querySelectorAll('[data-idx]').length : 0,
      emptyText: empty ? empty.textContent.trim() : null,
      errText: err ? err.textContent.trim().slice(0, 120) : null,
    };
  })()`);
  console.log('state:', JSON.stringify(st));
  ws.close();
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
