// ============================================================
// 验证数据总览「朋友圈活跃 Top 15」：IPC 返回 ≤15 作者 + UI 标签
// 运行：node st_control/.codex_tests/probe_top15.mjs
// ============================================================

const CDP_BASE = 'http://127.0.0.1:9222';

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function findTarget() {
  for (let i = 0; i < 30; i++) {
    try {
      const res = await fetch(`${CDP_BASE}/json/list`);
      const list = await res.json();
      const t = list.find((x) => x.type === 'page' && x.url.includes('localhost:1420'));
      if (t) return t;
    } catch {
      /* 应用尚未就绪 */
    }
    await sleep(1000);
  }
  throw new Error('30 秒内未发现 CDP 页面目标');
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
    if (r.exceptionDetails) {
      throw new Error('evaluate 异常: ' + JSON.stringify(r.exceptionDetails));
    }
    return r.result.value;
  }
}

const target = await findTarget();
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  ws.onopen = resolve;
  ws.onerror = reject;
});
const cdp = new Cdp(ws);

// 1) IPC 数据验证
const ipcResult = await cdp.eval(`(async () => {
  try {
    const d = await window.__TAURI_INTERNALS__.invoke('get_wechat_data_overview');
    return JSON.stringify({ count: (d.moments_authors ?? []).length, authors: (d.moments_authors ?? []).slice(0, 15).map((a) => a.name + ':' + a.posts) });
  } catch (e) {
    return 'ERROR: ' + String(e);
  }
})()`);
console.log('OVERVIEW=' + ipcResult);
const parsed = JSON.parse(ipcResult);
const count = parsed.count ?? 0;
const ipcOk = count > 0 && count <= 15;
console.log(ipcOk ? `PASS: IPC 返回 ${count} 个作者（≤15）` : `FAIL: IPC 作者数异常=${count}`);

// 2) UI 面板验证（导航到数据总览）
const uiResult = await cdp.eval(`(async () => {
  const b = [...document.querySelectorAll('.nav-item, [title]')].find((el) => (el.title || '').includes('数据总览'));
  if (b) b.click();
  await new Promise((r) => setTimeout(r, 1500));
  const texts = [...document.querySelectorAll('span')].map((el) => el.textContent.trim());
  const label = texts.find((t) => t.startsWith('朋友圈活跃 Top'));
  const chips = document.querySelectorAll('.ov-author').length;
  return JSON.stringify({ label: label ?? '', chips });
})()`);
console.log('UI=' + uiResult);
const ui = JSON.parse(uiResult);
const uiOk = ui.label === '朋友圈活跃 Top 15' && ui.chips > 0 && ui.chips <= 15;
console.log(uiOk ? `PASS: UI 标签="${ui.label}"，chips=${ui.chips}` : `FAIL: UI label=${ui.label}, chips=${ui.chips}`);

ws.close();
process.exit(ipcOk && uiOk ? 0 : 1);
