// ============================================================
// 端到端验证：统一目录方案后，微信设置页展示后端解析路径
//（不再硬编码 AppData），数据库面板应用库列表来自统一 data 目录
// 运行：node st_control/.codex_tests/e2e-verify-paths.mjs
// ============================================================
const CDP_BASE = 'http://127.0.0.1:9222';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function findTarget() {
  for (let i = 0; i < 60; i++) {
    try {
      const res = await fetch(`${CDP_BASE}/json/list`);
      const list = await res.json();
      const t = list.find((x) => x.type === 'page');
      if (t) return t;
    } catch {}
    await sleep(1000);
  }
  throw new Error('60s 内未发现 CDP 页面');
}

class Cdp {
  constructor(ws) {
    this.ws = ws;
    this.id = 0;
    this.pending = new Map();
    ws.onmessage = (ev) => {
      const m = JSON.parse(ev.data);
      if (m.id && this.pending.has(m.id)) {
        const { resolve, reject } = this.pending.get(m.id);
        this.pending.delete(m.id);
        if (m.error) reject(new Error(JSON.stringify(m.error)));
        else resolve(m.result);
      }
    };
  }
  send(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = ++this.id;
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }
  async eval(expression) {
    const r = await this.send('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails));
    return r.result.value;
  }
}

async function main() {
  const target = await findTarget();
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
  const cdp = new Cdp(ws);

  // 1) 进入微信数据 → 设置 tab，读固定路径显示
  await cdp.eval(`(() => {
    const el = [...document.querySelectorAll('.nav-item')].find(e => (e.getAttribute('title')||'').includes('微信数据'));
    if (el) el.click(); return !!el;
  })()`);
  await sleep(2500);
  await cdp.eval(`(() => {
    const el = [...document.querySelectorAll('button.wc-ihb')].find(e => (e.textContent||'').includes('设置'));
    if (el) el.click(); return !!el;
  })()`);
  await sleep(3000);
  const paths = await cdp.eval(`[...document.querySelectorAll('.wc-fixed-path')].map(e => e.textContent.trim())`);
  console.log('settings fixed paths:');
  paths.forEach((p) => console.log('  ', p));
  const hardcoded = paths.some((p) => p.includes('Administrator') || p.includes('AppData'));
  console.log(hardcoded ? 'FAIL: 仍含硬编码路径' : 'OK: 无硬编码路径');
  const expected = paths.filter((p) => p.includes('data') && p.includes('wechat'));
  console.log(expected.length >= 2 ? 'OK: 路径已指向统一 data/wechat' : 'FAIL: 未指向 data/wechat');

  // 2) 数据库面板：应用库快捷入口
  await cdp.eval(`(() => {
    const el = [...document.querySelectorAll('.nav-item')].find(e => (e.getAttribute('title')||'').includes('数据库'));
    if (el) el.click(); return !!el;
  })()`);
  await sleep(2500);
  const dbNames = await cdp.eval(`[...document.querySelectorAll('.db-app-entry, .db-app-item, [class*="db-app"]')].map(e => e.textContent.trim().slice(0, 40))`);
  console.log('db panel entries:', JSON.stringify(dbNames.slice(0, 12)));

  ws.close();
  const ok = !hardcoded && expected.length >= 2;
  console.log(ok ? 'VERIFY PASS' : 'VERIFY FAIL');
  process.exit(ok ? 0 : 1);
}

main().catch((e) => { console.error('ERR', e.message); process.exit(1); });
