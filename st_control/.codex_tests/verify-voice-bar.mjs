// 语音条 UI 验证：授权麦克风 → 打开语音对话 → 检查音色/语速控件
const CDP_BASE = 'http://127.0.0.1:9222';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function findTarget() {
  for (let i = 0; i < 30; i++) {
    try {
      const res = await fetch(`${CDP_BASE}/json/list`);
      const list = await res.json();
      const t = list.find((x) => x.type === 'page' && x.url.includes('localhost:1420'));
      if (t) return t;
    } catch {}
    await sleep(1000);
  }
  throw new Error('no target');
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
        m.error ? reject(new Error(JSON.stringify(m.error))) : resolve(m.result);
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
    const r = await this.send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
    if (r.exceptionDetails) throw new Error('evaluate 异常: ' + JSON.stringify(r.exceptionDetails));
    return r.result.value;
  }
  async waitFor(expression, timeoutMs = 20000, stepMs = 500) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      try {
        const v = await this.eval(expression);
        if (v && v !== 'false' && v !== 'null' && v !== 'undefined') return v;
      } catch {}
      await sleep(stepMs);
    }
    return null;
  }
}
const target = await findTarget();
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
const cdp = new Cdp(ws);

// 授权麦克风
await cdp.send('Browser.grantPermissions', {
  origin: 'http://localhost:1420',
  permissions: ['audioCapture'],
});
console.log('MIC_GRANTED');
await sleep(500);

// 先切回对话模型（DeepSeek）
await cdp.eval(`(async () => {
  const cfg = await window.__TAURI_INTERNALS__.invoke('get_llm_config');
  const chatP = (cfg.providers ?? []).find((x) => (x.models ?? []).includes('deepseek-v4-flash'));
  const setSelect = (el, val) => {
    if (!el || ![...el.options].some((o) => o.value === val)) return false;
    el.value = val;
    el.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  };
  setSelect([...document.querySelectorAll('.llm-chat-toolbar select')][0], chatP.id);
  await new Promise((r) => setTimeout(r, 600));
  setSelect([...document.querySelectorAll('.llm-chat-toolbar select')][0], chatP.id);
  await new Promise((r) => setTimeout(r, 600));
  return 'true';
})()`);
await sleep(1200);

// 点击「语音对话」按钮
const clicked = await cdp.eval(`(() => {
  const b = [...document.querySelectorAll('button')].find((el) => el.offsetParent !== null && el.textContent.trim() === '语音对话');
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`);
console.log('CLICKED=' + clicked);
await sleep(2500);

const bar = await cdp.eval(`JSON.stringify({
  hasBar: !!document.querySelector('.llm-voice-bar'),
  voiceOptions: [...(document.querySelector('.llm-voice-bar select')?.options ?? [])].map((o) => o.textContent.trim()),
  speedOptions: [...(document.querySelectorAll('.llm-voice-bar select')?.[1]?.options ?? [])].map((o) => o.textContent.trim()),
  meta: document.querySelector('.llm-voice-meta')?.textContent?.trim() ?? '',
  status: document.querySelector('.llm-voice-status')?.textContent?.trim() ?? '',
})`);
console.log('BAR=' + bar);
const b = JSON.parse(bar);
console.log(b.hasBar && b.voiceOptions.length === 6 ? 'PASS: 音色 6 个 CosyVoice2' : 'FAIL: 音色列表=' + JSON.stringify(b.voiceOptions));
console.log(b.speedOptions.some((s) => s.includes('x')) ? 'PASS: 语速选项=' + JSON.stringify(b.speedOptions) : 'FAIL: 语速=' + JSON.stringify(b.speedOptions));
console.log(b.meta.includes('语速') ? 'PASS: 元信息含语速' : 'FAIL: meta=' + b.meta);
const ok = b.hasBar && b.voiceOptions.length === 6 && b.speedOptions.some((s) => s.includes('x')) && b.meta.includes('语速');
ws.close();
process.exit(ok ? 0 : 1);
