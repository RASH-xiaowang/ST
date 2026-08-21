// ============================================================
// 端到端验证：图片唯一性修复后，群聊「黑龙江沃融-燎引擎」的
// 图片是否全部正常渲染（无「解密失败」占位）
// 运行：node st_control/.codex_tests/e2e-verify-images.mjs
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

  // 1) 左导航进入微信数据面板
  const navTitles = await cdp.eval(`[...document.querySelectorAll('.nav-item')].map(e => e.getAttribute('title'))`);
  console.log('nav items:', JSON.stringify(navTitles));
  const navClicked = await cdp.eval(`(() => {
    const el = [...document.querySelectorAll('.nav-item')].find(e => (e.getAttribute('title')||'').includes('微信数据'));
    if (!el) return false; el.click(); return true;
  })()`);
  console.log('click 微信数据 nav:', navClicked);
  await sleep(2500);

  // 2) 查看 wc-ihb 标签页
  const tabs = await cdp.eval(`[...document.querySelectorAll('button.wc-ihb')].map(e => e.textContent.trim())`);
  console.log('tabs:', JSON.stringify(tabs));

  // 3) 激活聊天 tab
  const tabClicked = await cdp.eval(`(() => {
    const el = [...document.querySelectorAll('button.wc-ihb')].find(e => (e.textContent||'').includes('聊天') || (e.textContent||'').includes('会话'));
    if (!el) return 'none';
    el.click(); return el.textContent.trim();
  })()`);
  console.log('click tab:', tabClicked);
  await sleep(2500);

  // 4) 找目标群聊
  const chatFound = await cdp.eval(`(() => {
    const el = [...document.querySelectorAll('button.wc-chat-item')].find(e => (e.textContent||'').includes('45862433809') || (e.textContent||'').includes('沃融'));
    if (!el) return null;
    el.click();
    return el.textContent.trim().slice(0, 60);
  })()`);
  console.log('click chat:', chatFound);
  if (!chatFound) {
    console.log('FAIL: chat not found');
    ws.close();
    process.exit(1);
  }
  await sleep(4000);

  // 5) 统计图片状态
  const stats = await cdp.eval(`(() => {
    const imgs = [...document.querySelectorAll('img.wc-msg-noise-img')];
    const fails = [...document.querySelectorAll('.wc-msg-image-fail')];
    const okImgs = imgs.filter(i => i.complete && i.naturalWidth > 0);
    const failTitles = fails.map(f => (f.getAttribute('title')||'').slice(0, 80));
    return { total: imgs.length, loaded: okImgs.length, loading: imgs.length - okImgs.length, fails: fails.length, failTitles: [...new Set(failTitles)].slice(0, 10) };
  })()`);
  console.log('image stats:', JSON.stringify(stats, null, 2));

  // 6) 滚动加载更多消息后再统计一次
  await cdp.eval(`(() => { const sc = document.querySelector('.wc-msgs, .wc-msg-list'); if (sc) sc.scrollTop = 0; const lists = [...document.querySelectorAll('div')].filter(d => d.scrollHeight > d.clientHeight + 400); lists[0] && (lists[0].scrollTop = 0); return true; })()`);
  await sleep(6000);
  const stats2 = await cdp.eval(`(() => {
    const imgs = [...document.querySelectorAll('img.wc-msg-noise-img')];
    const fails = [...document.querySelectorAll('.wc-msg-image-fail')];
    const okImgs = imgs.filter(i => i.complete && i.naturalWidth > 0);
    const failTitles = fails.map(f => (f.getAttribute('title')||'').slice(0, 80));
    return { total: imgs.length, loaded: okImgs.length, loading: imgs.length - okImgs.length, fails: fails.length, failTitles: [...new Set(failTitles)].slice(0, 12) };
  })()`);
  console.log('after scroll stats:', JSON.stringify(stats2, null, 2));

  ws.close();
  const ok = stats2.fails === 0 && stats2.loaded > 0;
  console.log(ok ? 'VERIFY PASS' : 'VERIFY FAIL');
  process.exit(ok ? 0 : 1);
}

main().catch((e) => { console.error('ERR', e.message); process.exit(1); });
