// 验证「问我的微信」深度优化：实时进度、统计表、内联引用、复制/重问按钮
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
(async () => {
  const list = await (await fetch('http://127.0.0.1:9222/json/list')).json();
  const t = list.find((x) => x.type === 'page' && x.url.includes('localhost:1420'));
  if (!t) { console.log('no page'); process.exit(1); }
  const ws = new WebSocket(t.webSocketDebuggerUrl);
  await new Promise((r) => ws.onopen = r);
  let id = 0; const pending = new Map();
  const errs = [];
  ws.onmessage = (ev) => {
    const m = JSON.parse(ev.data);
    if (m.id && pending.has(m.id)) { pending.get(m.id)(m.result); pending.delete(m.id); return; }
    if (m.method === 'Runtime.exceptionThrown') {
      const d = m.params.exceptionDetails;
      errs.push(((d.exception && (d.exception.description || d.exception.value)) || d.text || '').slice(0, 120));
    }
  };
  const send = (method, params) => new Promise((res) => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method, params })); });
  const ev = async (expr) => (await send('Runtime.evaluate', { expression: expr, awaitPromise: true, returnByValue: true })).result?.value;

  await send('Runtime.enable');
  await ev(`(() => { const el = [...document.querySelectorAll('.nav-item')].find(e => (e.getAttribute('title')||'').includes('微信数据')); if (el) el.click(); return !!el; })()`);
  await sleep(2500);
  await ev(`(() => { const el = [...document.querySelectorAll('button.wc-ihb')].find(e => (e.textContent||'').includes('AI 问答')); if (el) el.click(); return !!el; })()`);
  await sleep(2500);

  // 提问：统计类（走聚合 + 无索引兜底）
  const typed = await ev(`(() => {
    const input = document.querySelector('.wc-ask-input-row input');
    if (!input) return 'no-input';
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
    setter.call(input, '我上个月和谁聊得最多？');
    input.dispatchEvent(new Event('input', { bubbles: true }));
    return 'typed';
  })()`);
  console.log(typed);
  await sleep(300);
  await ev(`(() => { const b = [...document.querySelectorAll('.wc-ask-input-row button')].pop(); if (b) b.click(); return !!b; })()`);
  console.log('asked, watching live progress…');
  // 观察进行中的实时步骤（最多 20 秒）
  let sawLive = false;
  for (let i = 0; i < 20; i++) {
    await sleep(1500);
    const live = await ev(`(() => {
      const el = document.querySelector('.wc-ask-steps-live');
      return el ? el.textContent.trim().slice(0, 120) : null;
    })()`);
    const done = await ev(`(() => !!document.querySelector('.wc-ask-a'))`);
    if (live) { sawLive = true; console.log('  live:', live); }
    if (done) break;
  }
  console.log('saw live progress:', sawLive);

  // 等待完成（最多 90s）
  let ans = null;
  for (let i = 0; i < 60; i++) {
    await sleep(1500);
    ans = await ev(`(() => {
      const item = document.querySelector('.wc-ask-item');
      const a = item?.querySelector('.wc-ask-a');
      if (!a) return null;
      return {
        answer: a.textContent.trim().slice(0, 120),
        stats: [...(item?.querySelectorAll('.wc-ask-stat') ?? [])].map(s => s.textContent.trim().slice(0, 80)),
        cites: item?.querySelectorAll('.wc-ask-cite').length ?? 0,
        inlineCites: a.querySelectorAll('.wc-ask-inline-cite').length,
        meta: item?.querySelector('.wc-ask-q-meta')?.textContent?.trim() ?? '',
        steps: [...(item?.querySelectorAll('.wc-ask-step') ?? [])].map(s => s.textContent.trim().slice(0, 60)),
        actions: [...(item?.querySelectorAll('.wc-ask-mini-btn') ?? [])].map(b => b.textContent.trim()),
      };
    })()`);
    if (ans) break;
  }
  console.log('answer state:', JSON.stringify(ans, null, 2));
  console.log('page exceptions:', errs.length);
  ws.close();
  const ok = ans && ans.answer && (ans.stats.length > 0 || ans.cites > 0 || ans.inlineCites > 0);
  console.log(ok ? 'VERIFY PASS' : 'VERIFY FAIL');
  process.exit(ok ? 0 : 1);
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
