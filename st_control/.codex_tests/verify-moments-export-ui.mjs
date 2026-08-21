// ============================================================
// 朋友圈导出 UI 最终验证（单脚本原子执行）：
//   1. 可见性判定（offsetParent）导航到微信数据 → 朋友圈
//   2. 校验工具栏：格式选择器 csv/json/txt + 导出按钮 title
//   3. 点击洞察作者进入过滤态 → 徽标 + 导出按钮 title 含作者
//   4. fromSurface 截图 → data/ui-audit/moments-export-final.png
// 运行：node st_control/.codex_tests/verify-moments-export-ui.mjs
// ============================================================

import fs from 'node:fs';
import path from 'node:path';

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

// 仅匹配「可见」按钮（offsetParent 非 null 且文本完全相等）
const VISIBLE_BTN = (label) => `(() => {
  const b = [...document.querySelectorAll('button')].find((el) => el.offsetParent !== null && el.textContent.trim() === ${JSON.stringify(label)});
  if (b) { b.click(); return true; }
  return false;
})()`;

const target = await findTarget();
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
const cdp = new Cdp(ws);

let failures = 0;
const check = (ok, msg) => {
  console.log((ok ? 'PASS: ' : 'FAIL: ') + msg);
  if (!ok) failures++;
};

// 1) 进入微信数据面板（全局导航）
const enteredWc = await cdp.waitFor(VISIBLE_BTN('微信数据'), 15000);
check(enteredWc === true, '点击全局导航「微信数据」');
await sleep(1200);
// 2) 进入朋友圈页签（面板内导航）
const enteredMoments = await cdp.waitFor(VISIBLE_BTN('朋友圈'), 15000);
check(enteredMoments === true, '点击面板内「朋友圈」页签');
await sleep(1500);

// 3) 工具栏校验（可见性 + 内容）
const toolbar = await cdp.waitFor(`(() => {
  const sel = document.querySelector('.wc-moments-fmt');
  if (!sel || sel.offsetParent === null) return 'false';
  const opts = [...sel.options].map((o) => o.value);
  const btns = [...document.querySelectorAll('.wc-moments-actions button')];
  const exp = btns.find((el) => el.offsetParent !== null && el.textContent.includes('导出'));
  return JSON.stringify({ opts, exportTitle: exp ? exp.title : '', exportText: exp ? exp.textContent.trim() : '' });
})()`, 20000);
const tb = JSON.parse(toolbar ?? '{}');
check(tb.opts?.join(',') === 'csv,json,txt,html', `格式选择器选项 = [${tb.opts?.join(',')}]`);
check(!!tb.exportText && tb.exportText.includes('导出'), `导出按钮存在（文本="${tb.exportText}"）`);
check(tb.exportTitle?.includes('导出全部朋友圈') && tb.exportTitle.includes('格式 CSV'), `全量态 title: ${tb.exportTitle}`);

// 4) 点击洞察 Top 作者进入过滤态
const clickedAuthor = await cdp.waitFor(`(() => {
  const b = document.querySelector('.wc-mi-author');
  if (!b || b.offsetParent === null) return 'false';
  b.click();
  return b.querySelector('.wc-mi-author-name')?.textContent?.trim() ?? 'true';
})()`, 20000);
check(typeof clickedAuthor === 'string' && clickedAuthor.length > 0, `点击作者: ${clickedAuthor}`);
await sleep(1500);

const filtered = await cdp.waitFor(`(() => {
  const badge = document.querySelector('.wc-moments-filtered');
  if (!badge || badge.offsetParent === null) return 'false';
  const exp = [...document.querySelectorAll('.wc-moments-actions button')].find((el) => el.offsetParent !== null && el.textContent.includes('导出'));
  return JSON.stringify({ badge: badge.textContent.trim(), title: exp ? exp.title : '' });
})()`, 20000);
const f = JSON.parse(filtered ?? '{}');
check(!!f.badge && f.badge.includes('正在看'), `过滤徽标: ${f.badge}`);
check(!!f.title && f.title.includes('当前筛选') && f.title.includes('CSV'), `过滤态导出 title: ${f.title}`);

// 5) fromSurface 截图
const shot = await cdp.send('Page.captureScreenshot', { format: 'png', fromSurface: true });
const out = path.resolve('E:/ST/st_control/data/ui-audit/moments-export-final.png');
fs.mkdirSync(path.dirname(out), { recursive: true });
fs.writeFileSync(out, Buffer.from(shot.data, 'base64'));
console.log('SAVED=' + out);

console.log(failures === 0 ? 'ALL_PASS' : `FAILURES=${failures}`);
ws.close();
process.exit(failures === 0 ? 0 : 1);
