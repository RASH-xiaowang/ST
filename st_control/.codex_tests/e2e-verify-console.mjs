// 收集页面 console 错误/异常，验证三类问题已修复
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
(async () => {
  const list = await (await fetch('http://127.0.0.1:9222/json/list')).json();
  const t = list.find((x) => x.type === 'page' && x.url.includes('localhost:1420'));
  if (!t) { console.log('no page'); process.exit(1); }
  const ws = new WebSocket(t.webSocketDebuggerUrl);
  await new Promise((r) => ws.onopen = r);
  let id = 0; const pending = new Map();
  const events = [];
  ws.onmessage = (ev) => {
    const m = JSON.parse(ev.data);
    if (m.id && pending.has(m.id)) { pending.get(m.id)(m.result); pending.delete(m.id); return; }
    if (m.method === 'Runtime.consoleAPICalled' && ['error', 'warning'].includes(m.params.type)) {
      const text = (m.params.args || []).map((a) => a.value ?? a.description ?? '').join(' ');
      events.push('[console.' + m.params.type + '] ' + text.slice(0, 160));
    }
    if (m.method === 'Runtime.exceptionThrown') {
      const d = m.params.exceptionDetails;
      events.push('[exception] ' + ((d.exception && (d.exception.description || d.exception.value)) || d.text || '').slice(0, 160));
    }
  };
  const send = (method, params) => new Promise((res) => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method, params })); });
  await send('Runtime.enable');
  await send('Page.enable');
  await send('Page.reload', { ignoreCache: true });
  console.log('reloaded, collecting 45s…');
  await sleep(45000);
  const scan = events.filter((e) => e.includes('扫描目录失败'));
  const ack = events.filter((e) => e.includes('ACK 失败') || e.includes('missing required key'));
  const gl = events.filter((e) => e.includes('GL_INVALID'));
  console.log('scan dir errors:', scan.length);
  console.log('ack errors:', ack.length, ack.slice(0, 2));
  console.log('webgl errors:', gl.length, gl.slice(0, 2));
  console.log('total console errors collected:', events.length);
  events.slice(0, 8).forEach((e) => console.log('  sample:', e));
  ws.close();
  const ok = scan.length === 0 && ack.length === 0 && gl.length === 0;
  console.log(ok ? 'VERIFY PASS' : 'VERIFY FAIL');
  process.exit(ok ? 0 : 1);
})().catch((e) => { console.error('ERR', e.message); process.exit(1); });
