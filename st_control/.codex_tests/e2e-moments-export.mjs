// ============================================================
// E2E：朋友圈导出修复验证
//   1. 后端 export_moments 三格式（json/csv/txt）落盘校验
//   2. author_username 过滤：只导出当前联系人
//   3. UI：格式选择器 + 过滤态导出按钮 title
// 运行：node st_control/.codex_tests/e2e-moments-export.mjs
// ============================================================

import fs from 'node:fs';
import path from 'node:path';

const CDP_BASE = 'http://127.0.0.1:9222';
const OUT_DIR = 'E:/ST/st_control/data/ui-audit';

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

const target = await findTarget();
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
const cdp = new Cdp(ws);

fs.mkdirSync(OUT_DIR, { recursive: true });
let failures = 0;
const check = (ok, msg) => {
  console.log((ok ? 'PASS: ' : 'FAIL: ') + msg);
  if (!ok) failures++;
};

// ── 0) 取一个真实作者（总览 Top 15 第一位）──
const authorRaw = await cdp.eval(`(async () => {
  try {
    const d = await window.__TAURI_INTERNALS__.invoke('get_wechat_data_overview');
    const a = (d.moments_authors ?? [])[0];
    return JSON.stringify({ username: a?.username ?? '', name: a?.name ?? '' });
  } catch (e) { return 'ERROR: ' + String(e); }
})()`);
const author = JSON.parse(authorRaw);
check(!!author.username, `拿到测试作者: ${author.name} (${author.username})`);

// ── 1) JSON + 作者过滤：只导出当前联系人 ──
const jsonPath = path.join(OUT_DIR, 'moments_author_test.json');
const jsonRes = await cdp.eval(`(async () => {
  try {
    const r = await window.__TAURI_INTERNALS__.invoke('export_moments', { format: 'json', authorUsername: ${JSON.stringify(author.username)}, path: ${JSON.stringify(jsonPath.replaceAll('\\', '/'))} });
    return JSON.stringify(r);
  } catch (e) { return 'ERROR: ' + String(e); }
})()`);
const jr = JSON.parse(jsonRes);
check(typeof jr.count === 'number' && jr.count > 0, `JSON 作者导出 count=${jr.count}`);
const jsonData = JSON.parse(fs.readFileSync(jsonPath, 'utf8'));
check(Array.isArray(jsonData) && jsonData.length === jr.count, `JSON 文件条目数 = ${jsonData.length} 与返回一致`);
const jsonOthers = jsonData.filter((m) => m.username !== author.username);
check(jsonOthers.length === 0, `JSON 全部 ${jsonData.length} 条均属于「${author.name}」`);

// ── 2) CSV 全部导出：BOM + 表头 ──
const csvPath = path.join(OUT_DIR, 'moments_all_test.csv');
const csvRes = await cdp.eval(`(async () => {
  try {
    const r = await window.__TAURI_INTERNALS__.invoke('export_moments', { format: 'csv', authorUsername: null, path: ${JSON.stringify(csvPath.replaceAll('\\', '/'))} });
    return JSON.stringify(r);
  } catch (e) { return 'ERROR: ' + String(e); }
})()`);
const cr = JSON.parse(csvRes);
const csvBuf = fs.readFileSync(csvPath);
check(csvBuf[0] === 0xef && csvBuf[1] === 0xbb && csvBuf[2] === 0xbf, 'CSV 带 BOM（Excel 兼容）');
const csvText = fs.readFileSync(csvPath, 'utf8').replace(/^\uFEFF/, '');
const csvLines = csvText.split('\n');
check(csvLines[0] === '动态ID,作者,用户名,时间,正文,位置,链接标题,媒体', 'CSV 表头正确');
check(csvLines.length - 1 === cr.count, `CSV 行数(${csvLines.length - 1}) = count(${cr.count})`);
check(cr.count > jr.count, `CSV 全量 count(${cr.count}) > 作者过滤 count(${jr.count})`);

// ── 3) TXT + 作者过滤 ──
const txtPath = path.join(OUT_DIR, 'moments_author_test.txt');
const txtRes = await cdp.eval(`(async () => {
  try {
    const r = await window.__TAURI_INTERNALS__.invoke('export_moments', { format: 'txt', authorUsername: ${JSON.stringify(author.username)}, path: ${JSON.stringify(txtPath.replaceAll('\\', '/'))} });
    return JSON.stringify(r);
  } catch (e) { return 'ERROR: ' + String(e); }
})()`);
const tr = JSON.parse(txtRes);
const txt = fs.readFileSync(txtPath, 'utf8');
check(txt.startsWith('微信朋友圈导出'), 'TXT 开头标题正确');
const txtBlocks = txt.split('【').length - 1;
check(txtBlocks === tr.count, `TXT 条目数(${txtBlocks}) = count(${tr.count})`);
check(txt.includes(author.name), `TXT 包含作者名「${author.name}」`);

// ── 4) UI：格式选择器 + 过滤态导出按钮 title ──
await cdp.eval(`(() => {
  const b = [...document.querySelectorAll('button')].find((el) => el.textContent.trim() === '朋友圈');
  if (b) b.click();
})()`);
const ui1 = await cdp.waitFor(`(() => {
  const sel = document.querySelector('.wc-moments-fmt');
  return sel ? JSON.stringify([...sel.options].map((o) => o.value)) : 'false';
})()`, 20000);
check(ui1 === JSON.stringify(['csv', 'json', 'txt']), `格式选择器选项 = ${ui1}`);
// 进入某位作者过滤态（点击洞察 Top 作者第一个）
await cdp.waitFor(`(() => {
  const b = document.querySelector('.wc-mi-author');
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`, 20000);
const ui2 = await cdp.waitFor(`(() => {
  const badge = document.querySelector('.wc-moments-filtered');
  return badge ? badge.textContent.trim() : 'false';
})()`, 15000);
check(!!ui2 && ui2.includes('正在看'), `过滤态徽标: ${ui2}`);
const btnTitle = await cdp.eval(`(() => {
  const btns = [...document.querySelectorAll('.wc-moments-actions button')];
  const b = btns.find((el) => el.textContent.includes('导出'));
  return b ? b.title : '';
})()`);
check(btnTitle.includes(ui2.replace('正在看「', '').replace('」', '')), `导出按钮 title 含当前作者: ${btnTitle.slice(0, 60)}…`);
const btnTitleFmt = await cdp.eval(`(() => {
  const btns = [...document.querySelectorAll('.wc-moments-actions button')];
  const b = btns.find((el) => el.textContent.includes('导出'));
  return b ? b.title : '';
})()`);
check(btnTitleFmt.includes('JSON') === false && btnTitleFmt.includes('CSV'), `默认格式 CSV 反映在 title: ${btnTitleFmt.slice(0, 60)}…`);

console.log(failures === 0 ? 'ALL_PASS' : `FAILURES=${failures}`);
ws.close();
process.exit(failures === 0 ? 0 : 1);
