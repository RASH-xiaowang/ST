// 语音/普通聊天统一输入行验证
const CDP_BASE = 'http://127.0.0.1:9222';
import fs from 'node:fs';
import path from 'node:path';
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
let failures = 0;
const check = (ok, msg) => {
  console.log((ok ? 'PASS: ' : 'FAIL: ') + msg);
  if (!ok) failures++;
};

// 授权麦克风
await cdp.send('Browser.grantPermissions', { origin: 'http://localhost:1420', permissions: ['audioCapture'] });

// 1) 切换到对话模型
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

// 2) 普通态检查：输入行 = 附件 + 麦克风 + 输入框 + 发送（无语音条）
const normal = await cdp.eval(`JSON.stringify({
  micBtn: !!document.querySelector('.llm-mic-btn'),
  voiceLine: !!document.querySelector('.llm-voice-line'),
  icoCount: document.querySelectorAll('.llm-input-row .llm-ico-btn').length,
})`);
console.log('NORMAL=' + normal);
const nrm = JSON.parse(normal);
check(nrm.micBtn && !nrm.voiceLine, `普通聊天：输入行含麦克风按钮、无语音条`);
check(nrm.icoCount === 2, `输入行图标按钮 = 2（附件+麦克风）`);

// 3) 点麦克风开启语音模式
await cdp.eval(`(() => { const b = document.querySelector('.llm-mic-btn'); if (b) b.click(); return 'true'; })()`);
await sleep(2200);
const voice = await cdp.eval(`JSON.stringify({
  voiceLine: !!document.querySelector('.llm-voice-line'),
  chips: [...document.querySelectorAll('.llm-voice-chip')].map((e) => e.textContent.trim()),
  status: document.querySelector('.llm-voice-status')?.textContent?.trim() ?? '',
  micOn: document.querySelector('.llm-mic-btn')?.classList.contains('voice-on') ?? false,
})`);
console.log('VOICE=' + voice);
const v = JSON.parse(voice);
check(v.voiceLine, '开启语音后出现细状态行');
check(v.chips.includes('语音回复') && v.chips.includes('连续对话'), `紧凑开关 chips: ${v.chips.join('|')}`);
check(v.micOn, '输入行麦克风高亮（voice-on）');

// 4) 打开语音设置浮层
await cdp.eval(`(() => { const b = document.querySelector('.llm-voice-gear'); if (b) b.click(); return 'true'; })()`);
await sleep(500);
const pop = await cdp.eval(`JSON.stringify({
  hasPop: !!document.querySelector('.llm-voice-pop'),
  rows: [...document.querySelectorAll('.llm-voice-pop-row .llm-voice-pop-label')].map((e) => e.textContent.trim()),
  meta: document.querySelector('.llm-voice-pop-meta')?.textContent?.trim() ?? '',
})`);
console.log('POP=' + pop);
const p = JSON.parse(pop);
check(p.hasPop, '语音设置浮层出现');
check(p.rows.includes('音色') && p.rows.includes('语速'), `浮层含音色/语速（${p.rows.join('/')}）`);
check(p.meta.includes('转写'), `浮层引擎信息: ${p.meta}`);

// 5) 截图
const shot = await cdp.send('Page.captureScreenshot', { format: 'png', fromSurface: true });
const out = path.resolve('E:/ST/st_control/data/ui-audit/llm-voice-unified.png');
fs.writeFileSync(out, Buffer.from(shot.data, 'base64'));
console.log('SAVED=' + out);

// 6) 退出语音模式 → 回到普通态
await cdp.eval(`(() => { const b = document.querySelector('.llm-voice-exit'); if (b) b.click(); return 'true'; })()`);
await sleep(800);
const back = await cdp.eval(`(() => document.querySelector('.llm-voice-line') ? 'false' : 'true')()`);
check(back === 'true', '退出后回到普通聊天界面（无残留语音条）');

console.log(failures === 0 ? 'ALL_PASS' : `FAILURES=${failures}`);
ws.close();
process.exit(failures === 0 ? 0 : 1);
