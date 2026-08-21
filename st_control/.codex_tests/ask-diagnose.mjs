// ============================================================
// 诊断「问我的微信」：用真实问题调用 ask_wechat，落盘完整结果
// 输出 → data/ui-audit/ask-diagnose.json（避免放 .codex_tests 触发 HMR）
// 运行：node st_control/.codex_tests/ask-diagnose.mjs
// ============================================================

import fs from 'node:fs';
import path from 'node:path';

const CDP_BASE = 'http://127.0.0.1:9222';
const OUT = 'E:/ST/st_control/data/ui-audit/ask-diagnose.json';

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
  throw new Error('no CDP target');
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
}

const target = await findTarget();
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
const cdp = new Cdp(ws);

const QUESTIONS = [
  '我上个月和谁聊得最多？',
  '我和东兰民中1410王勤最近聊了什么？',
  '最近7天大家在群里聊了什么？',
  '我去年收了多少个红包？',
  '王勤最近发了什么朋友圈？',
];

const results = {};
for (const q of QUESTIONS) {
  console.log(`\n=== ASK: ${q} ===`);
  const t0 = Date.now();
  const raw = await cdp.eval(`(async () => {
    try {
      const r = await window.__TAURI_INTERNALS__.invoke('ask_wechat', { question: ${JSON.stringify(q)}, limit: 24, history: null });
      return JSON.stringify(r);
    } catch (e) { return JSON.stringify({ ERROR: String(e) }); }
  })()`);
  const r = JSON.parse(raw);
  const secs = ((Date.now() - t0) / 1000).toFixed(1);
  results[q] = r;
  console.log(`[${secs}s] llm_used=${r.llm_used} rounds=${r.rounds} citations=${(r.citations ?? []).length} stats=${(r.stats ?? []).length}`);
  console.log('plan:', JSON.stringify(r.plan ?? {}));
  console.log('answer:', (r.answer ?? '(无)').slice(0, 300));
  if (r.error) console.log('error:', r.error);
  console.log('steps:', JSON.stringify(r.steps ?? []).slice(0, 400));
  console.log('citations[0..2]:', JSON.stringify((r.citations ?? []).slice(0, 3).map((c) => ({ kind: c.kind, name: c.name, time: c.time, snippet: c.snippet.slice(0, 60) }))));
}

fs.writeFileSync(OUT, JSON.stringify(results, null, 2));
console.log('\nSAVED=' + OUT);
ws.close();
process.exit(0);
