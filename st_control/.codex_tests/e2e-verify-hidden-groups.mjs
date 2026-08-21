// 验证隐藏群聊出现在列表并可打开看到消息
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
  await sleep(4000);
  // 统计列表：总数、隐藏徽标数
  const stats = await ev(`(() => {
    const items = [...document.querySelectorAll('button.wc-chat-item')];
    const badges = [...document.querySelectorAll('.wc-chat-hidden-badge')];
    return { total: items.length, hiddenBadges: badges.length };
  })()`);
  console.log('chat list:', JSON.stringify(stats));

  // 打开 45968630945@chatroom（3398 条消息的隐藏群）——通过搜索或直接找文本
  const opened = await ev(`(() => {
    const el = [...document.querySelectorAll('button.wc-chat-item')].find(e => (e.textContent||'').includes('45968630945'));
    if (!el) return null;
    el.click();
    return el.textContent.trim().slice(0, 60);
  })()`);
  console.log('opened:', opened);
  if (!opened) { ws.close(); process.exit(1); }
  await sleep(6000);
  const msgs = await ev(`(() => {
    const inner = document.querySelector('.wc-msgs-inner');
    const rows = inner ? [...inner.querySelectorAll('[data-idx]')] : [];
    const el = document.querySelector('.wc-msgs');
    return {
      rows: rows.length,
      firstText: rows[0] ? (rows[0].textContent || '').trim().slice(0, 50) : null,
      lastText: rows[rows.length-1] ? (rows[rows.length-1].textContent || '').trim().slice(0, 50) : null,
      gapToBottom: el ? el.scrollHeight - el.scrollTop - el.clientHeight : null,
      empty: !!document.querySelector('.wc-msgs .wc-empty'),
    };
  })()`);
  console.log('messages:', JSON.stringify(msgs));
  ws.close();
  const ok = msgs.rows > 0 && !msgs.empty;
  console.log(ok ? 'VERIFY PASS' : 'VERIFY FAIL');
  process.exit(ok ? 0 : 1);
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
