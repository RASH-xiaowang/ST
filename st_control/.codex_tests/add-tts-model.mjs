// 向硅基流动添加 CosyVoice2 语音模型并标记为「语音」类型
const CDP_BASE = 'http://127.0.0.1:9222';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function findTarget() {
  for (let i = 0; i < 20; i++) {
    try {
      const res = await fetch(`${CDP_BASE}/json/list`);
      const list = await res.json();
      const t = list.find((x) => x.type === 'page' && x.url.includes('localhost:1420'));
      if (t) return t;
    } catch {}
    await sleep(500);
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
    if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails));
    return r.result.value;
  }
}
const target = await findTarget();
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
const cdp = new Cdp(ws);
const out = await cdp.eval(`(async () => {
  const inv = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);
  // 找到含 TeleASR 模型的提供方
  const cfg = await inv('get_llm_config');
  const p = (cfg.providers ?? []).find((x) => (x.models ?? []).some((m) => m.includes('TeleAI') || m.includes('SpeechASR')));
  if (!p) return JSON.stringify({ error: 'provider not found', got: (cfg.providers ?? []).map((x) => x.models) });
  const pid = p.id;
  let r1, r2;
  try {
    r1 = await inv('add_llm_model', { id: pid, model: 'FunAudioLLM/CosyVoice2-0.5B' });
  } catch (e) { r1 = { error: String(e) }; }
  try {
    r2 = await inv('set_llm_model_meta', { id: pid, model: 'FunAudioLLM/CosyVoice2-0.5B', modelType: '语音', tags: ['TTS', 'CosyVoice2'] });
  } catch (e) { r2 = { error: String(e) }; }
  return JSON.stringify({
    provider: p.name,
    added: (r1?.models ?? r1?.error ?? '?'),
    meta: (r2?.model_meta?.['FunAudioLLM/CosyVoice2-0.5B'] ?? r2?.error ?? '?'),
  });
})()`);
console.log('RESULT=' + out);
ws.close();
process.exit(0);
