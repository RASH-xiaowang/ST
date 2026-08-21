// 插件抽屉 UI 验证
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

// 打开插件抽屉
await cdp.eval(`(() => {
  const b = [...document.querySelectorAll('.llm-chat-toolbar button')].find((el) => el.textContent.includes('插件'));
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`);
const drawer = await cdp.waitFor(`(() => {
  const d = document.querySelector('.llm-plugin-drawer');
  return d ? JSON.stringify({
    title: d.querySelector('.llm-role-title')?.textContent?.trim() ?? '',
    hasNew: [...d.querySelectorAll('button')].some((b) => b.textContent.includes('新建插件')),
    hasEmpty: !!d.querySelector('.llm-role-empty'),
  }) : 'false';
})()`, 10000);
console.log('DRAWER=' + drawer);
const d = JSON.parse(drawer);
check(d.title === '动态插件', `抽屉标题（${d.title}）`);
check(d.hasNew, '含「新建插件」按钮');

// 打开新建表单
await cdp.eval(`(() => {
  const b = [...document.querySelectorAll('.llm-plugin-drawer button')].find((el) => el.textContent.includes('新建插件'));
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`);
const form = await cdp.waitFor(`(() => {
  const f = document.querySelector('.llm-plugin-form');
  return f ? JSON.stringify({
    fields: f.querySelectorAll('input, textarea').length,
    hasCode: !!f.querySelector('.llm-plugin-code textarea'),
    hasApproval: !!f.querySelector('.llm-plugin-check input[type=checkbox]'),
  }) : 'false';
})()`, 10000);
console.log('FORM=' + form);
const f = JSON.parse(form);
check(f.fields >= 5, `新建表单字段数=${f.fields}`);
check(f.hasCode && f.hasApproval, '含代码编辑器与审批开关');

const shot = await cdp.send('Page.captureScreenshot', { format: 'png', fromSurface: true });
const out = path.resolve('E:/ST/st_control/data/ui-audit/llm-plugin-drawer.png');
fs.writeFileSync(out, Buffer.from(shot.data, 'base64'));
console.log('SAVED=' + out);
console.log(failures === 0 ? 'ALL_PASS' : `FAILURES=${failures}`);
ws.close();
process.exit(failures === 0 ? 0 : 1);
