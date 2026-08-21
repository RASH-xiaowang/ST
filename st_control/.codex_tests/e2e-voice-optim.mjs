// 已停用（2026-08）：原「AI 聊天」板块已并入 Harness 会话，本脚本断言的独立面板 UI 不再存在；
// 功能验证改由 e2e-harness-phase* 与 verify-harness-chat-* 承担。
// ============================================================
// E2E：语音对话优化验证
//   1. 提供方 TTS（CosyVoice2）：不同音色/语速合成成功且音频不同
//   2. 系统 SAPI 兜底：rate 参数生效
//   3. UI：TTS 输入行出现 音色/格式/语速 选择
//   4. 后台启动本地 Whisper Base 模型下载（语音输入离线可用）
// 运行：node st_control/.codex_tests/e2e-voice-optim.mjs
// ============================================================

import fs from 'node:fs';

const CDP_BASE = 'http://127.0.0.1:9222';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function findTarget() {
  for (let i = 0; i < 40; i++) {
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

// ── 1) 提供方 TTS：找到 CosyVoice2 所属提供方 ──
const pidRaw = await cdp.eval(`(async () => {
  const cfg = await window.__TAURI_INTERNALS__.invoke('get_llm_config');
  const p = (cfg.providers ?? []).find((x) => (x.models ?? []).includes('FunAudioLLM/CosyVoice2-0.5B'));
  return p ? p.id : '';
})()`);
check(!!pidRaw, `CosyVoice2 提供方已配置 (${pidRaw})`);
const pid = pidRaw;

// ── 2) 语速/音色合成对比 ──
const tts1 = await cdp.eval(`(async () => {
  try {
    const r = await window.__TAURI_INTERNALS__.invoke('create_speech', { request: {
      provider_id: ${JSON.stringify(pid)},
      model: 'FunAudioLLM/CosyVoice2-0.5B',
      input: '你好，我是你的智能助手，很高兴为你服务。',
      voice: 'FunAudioLLM/CosyVoice2-0.5B:anna',
      response_format: 'mp3',
      speed: 1.0,
    }});
    return JSON.stringify({ ok: true, len: (r.audio_data?.length ?? 0), fmt: r.format, voice: r.voice });
  } catch (e) { return JSON.stringify({ ok: false, err: String(e) }); }
})()`);
const t1 = JSON.parse(tts1);
check(t1.ok && t1.len > 1000, `anna 1.0x 合成成功 (${Math.round(t1.len * 3 / 4)}B mp3)`);

const tts2 = await cdp.eval(`(async () => {
  try {
    const r = await window.__TAURI_INTERNALS__.invoke('create_speech', { request: {
      provider_id: ${JSON.stringify(pid)},
      model: 'FunAudioLLM/CosyVoice2-0.5B',
      input: '你好，我是你的智能助手，很高兴为你服务。',
      voice: 'FunAudioLLM/CosyVoice2-0.5B:alex',
      response_format: 'mp3',
      speed: 1.3,
    }});
    return JSON.stringify({ ok: true, len: (r.audio_data?.length ?? 0) });
  } catch (e) { return JSON.stringify({ ok: false, err: String(e) }); }
})()`);
const t2 = JSON.parse(tts2);
check(t2.ok && t2.len > 1000, `alex 1.3x 合成成功 (${Math.round(t2.len * 3 / 4)}B mp3)`);
check(t1.ok && t2.ok && t1.len !== t2.len, `不同音色/语速音频长度不同 (${t1.len} vs ${t2.len})`);

// ── 3) 系统 SAPI 兜底：语速 rate 生效 ──
const nat = await cdp.eval(`(async () => {
  try {
    const a = await window.__TAURI_INTERNALS__.invoke('synthesize_native_speech', { text: '这是一段用于测试语速的语音合成文本。', rate: -4 });
    const b = await window.__TAURI_INTERNALS__.invoke('synthesize_native_speech', { text: '这是一段用于测试语速的语音合成文本。', rate: 4 });
    return JSON.stringify({ ok: true, slow: a.audio_data?.length ?? 0, fast: b.audio_data?.length ?? 0 });
  } catch (e) { return JSON.stringify({ ok: false, err: String(e) }); }
})()`);
const n = JSON.parse(nat);
check(n.ok, `系统语音合成成功 (rate 生效)`);
check(n.ok && n.slow > 1000 && n.slow > n.fast, `语速越慢音频越长 (slow=${n.slow} > fast=${n.fast})`);

// ── 4) UI：切到 CosyVoice2 模型 → 输入行出现 音色/格式/语速 ──
await cdp.eval(`(() => {
  const b = [...document.querySelectorAll('.hns-viewbar button')].find((el) => (el.textContent || '').trim() === 'AI 聊天' && el.offsetParent !== null) ?? (() => { const n = [...document.querySelectorAll('button.nav-item')].find((el) => el.title === 'Harness'); if (n) n.click(); return null; })();
  if (b) b.click();
  return 'true';
})()`);
await sleep(1500);
const uiSw = await cdp.eval(`(async () => {
  const sel = (list, val) => {
    const el = list.find((s) => [...s.options].some((o) => o.value === val));
    if (!el) return false;
    el.value = val;
    el.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  };
  const selects = [...document.querySelectorAll('.llm-chat-toolbar select')];
  const okP = sel(selects, ${JSON.stringify(pid)});
  await new Promise((r) => setTimeout(r, 600));
  const selects2 = [...document.querySelectorAll('.llm-chat-toolbar select')];
  const okM = sel(selects2, 'FunAudioLLM/CosyVoice2-0.5B');
  await new Promise((r) => setTimeout(r, 800));
  const hint = [...document.querySelectorAll('.llm-img-gen-hint')].map((e) => e.textContent.trim()).join('|');
  const sizeSels = [...document.querySelectorAll('.llm-size-sel select')].length;
  const labels = [...document.querySelectorAll('.llm-size-sel')].map((e) => e.getAttribute('title'));
  return JSON.stringify({ okP, okM, hint, sizeSels, labels });
})()`);
console.log('UI=' + uiSw);
const ui = JSON.parse(uiSw);
check(ui.hint.includes('语音'), `已切换到语音模型（${ui.hint.slice(0, 30)}…）`);
check(ui.sizeSels >= 3, `输入行参数选择器数量=${ui.sizeSels}（音色/格式/语速）`);
check(ui.labels.includes('语速（倍率，1.0 为正常）'), `含语速选择器`);

// ── 5) 后台启动本地 Whisper Base 下载（语音输入离线可用）──
const dl = await cdp.eval(`(async () => {
  const st = await window.__TAURI_INTERNALS__.invoke('get_local_stt_status');
  if (st.model_exists) return JSON.stringify({ started: false, reason: 'already exists' });
  window.__TAURI_INTERNALS__.invoke('download_local_stt_model', { size: 'base' })
    .then((r) => console.log('[stt-dl]', r))
    .catch((e) => console.error('[stt-dl]', e));
  return JSON.stringify({ started: true });
})()`);
console.log('STT_DL=' + dl);

console.log(failures === 0 ? 'ALL_PASS' : `FAILURES=${failures}`);
ws.close();
process.exit(failures === 0 ? 0 : 1);
