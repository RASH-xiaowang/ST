// E2E：Harness 阶段 7+8（skill / feedback / session-query / storage KV / spill / CLI / 示例种子）
// 前置：app 运行中（CDP 9222）+ Vite 1420。
const CDP_BASE = 'http://127.0.0.1:9222';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
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
  async waitFor(expression, timeoutMs = 120000, stepMs = 250) {
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
const invoke = (cmd, args = {}) =>
  cdp.eval(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`)
    .catch((e) => ({ __err: String(e) }));

// ─── 1) 示例预设种子（DSH examples 迁移） ───
const presets = await invoke('list_harness_presets');
check((presets ?? []).some((p) => p.id === 'preset-example-readonly'), '示例预设「示例-只读办公」已种子');

// ─── 2) 技能（skill） ───
const skillId = `e2e-skill-${Date.now().toString(36)}`;
const savedSkill = await invoke('save_harness_skill', {
  skill: {
    id: skillId, name: 'E2E 技能', description: '测试技能',
    content: '# E2E 技能\n\n测试技能内容：请回复 SKILL_OK。',
  },
});
check(!!savedSkill.id && savedSkill.id === skillId, `技能已保存（${skillId}）`);
const toolsNow = await invoke('get_harness_tools');
check((toolsNow ?? []).some((t) => t.name === 'skill_list') && (toolsNow ?? []).some((t) => t.name === 'skill_load'), '技能工具注册进目录（skill_list / skill_load）');

// ─── 3) 会话 + 反馈 + 查询 ───
const cliCreate = await invoke('harness_cli', { input: 'session create' });
const newSid = String(cliCreate).match(/h-[a-f0-9]+/)?.[0];
check(!!newSid, `CLI 创建会话（${newSid}）`);
await invoke('harness_submit_feedback', { sessionId: newSid, rating: 'good', comment: 'E2E 反馈' });
const feedbacks = await invoke('harness_list_feedback');
check((feedbacks ?? []).some((f) => f.session_id === newSid && f.rating === 'good'), '反馈已落库');
// CLI 对话（含唯一关键词供查询）
await invoke('harness_cli', { input: `session chat ${newSid} 请只回复：PHASE78_KEYWORD_OK` });
const hits = await invoke('harness_search_sessions', { query: 'PHASE78_KEYWORD_OK' });
check(Array.isArray(hits) && hits.length >= 1, `会话查询命中（${hits?.length ?? 0} 条）`);

// ─── 4) KV 存储（storage） ───
await invoke('harness_kv_put', { key: 'e2e-kv', value: 'kv-value-ok' });
const kvGet = await invoke('harness_kv_get', { key: 'e2e-kv' });
check(kvGet === 'kv-value-ok', `KV 存储读取（${kvGet}）`);
await invoke('harness_kv_delete', { key: 'e2e-kv' });
const kvGone = await invoke('harness_kv_get', { key: 'e2e-kv' });
check(kvGone === null || kvGone === undefined, 'KV 删除生效');

// ─── 5) spill（压缩溢写） ───
await invoke('save_harness_settings', {
  settings: {
    last_provider_id: '', last_model: '', tool_timeout_secs: null, max_agent_rounds: null,
    preset_id: null, allow_workspace_escape: false,
    context_budget_tokens: 4000, enable_compaction: true,
  },
});
const longText = '溢写测试填充文本。'.repeat(600); // ~4200 字 ≈ 2100 token，两轮超预算
await invoke('harness_cli', { input: `session chat ${newSid} ${longText}` });
await invoke('harness_cli', { input: `session chat ${newSid} ${longText} 第二段` });
const spills = await invoke('harness_list_spills', { sessionId: newSid });
check(Array.isArray(spills) && spills.length >= 1, `spill 溢写文件已生成（${spills?.length ?? 0} 个）`);
await invoke('save_harness_settings', {
  settings: {
    last_provider_id: '', last_model: '', tool_timeout_secs: null, max_agent_rounds: null,
    preset_id: null, allow_workspace_escape: false,
    context_budget_tokens: null, enable_compaction: true,
  },
});

// ─── 6) CLI 面板 ───
const cliTools = await invoke('harness_cli', { input: 'tools list' });
check(typeof cliTools === 'string' && cliTools.includes('web_search'), `CLI tools list（${String(cliTools).slice(0, 40)}…）`);
const cliUsage = await invoke('harness_cli', { input: `usage ${newSid}` });
check(typeof cliUsage === 'string' && cliUsage.includes('轮'), `CLI usage（${cliUsage}）`);

// ─── 7) UI：技能/CLI 标签与反馈按钮 ───
await cdp.waitFor(`(() => {
  const b = [...document.querySelectorAll('button.nav-item')].find((el) => el.offsetParent !== null && el.title === 'Harness');
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`, 20000);
await sleep(1200);
// 先关闭可能遗留的抽屉，再点击治理打开（触发 openDrawer 全量刷新技能列表）
await cdp.eval(`(() => { const c = document.querySelector('.hns-drawer-close'); if (c) { c.click(); return 'true'; } return 'false'; })()`);
await sleep(400);
await cdp.eval(`(() => {
  const b = [...document.querySelectorAll('.hns-bar-icon')].find((x) => (x.title || '').includes('设置 / 钩子'));
  if (b) { b.click(); return 'true'; }
  return 'false';
})()`);
await sleep(800);
const tabs = await cdp.eval(`(() => JSON.stringify([...document.querySelectorAll('.hns-drawer-tabs button')].map((x) => x.textContent.trim())))()`);
const tabList = JSON.parse(tabs);
check(tabList.includes('技能') && tabList.includes('CLI'), `治理抽屉含技能/CLI 标签（${tabs}）`);
await cdp.eval(`(() => {
  const tabs = [...document.querySelectorAll('.hns-drawer-tabs button')];
  const t = tabs.find((x) => x.textContent.trim() === '技能');
  if (t) { t.click(); return 'true'; }
  return 'false';
})()`);
await sleep(500);
const skillUi = await cdp.waitFor(`(() => {
  const items = [...document.querySelectorAll('.hns-preset-item')];
  return items.some((x) => x.textContent.includes(${JSON.stringify(skillId)})) ? 'true' : 'false';
})()`, 15000);
check(skillUi === 'true', '技能 UI 显示已保存技能');
const fbBtn = await cdp.eval(`(() => document.querySelector('.hns-feedback') ? 'true' : 'false')()`);
check(fbBtn === 'true', '助手回复反馈按钮（👍/👎）显示');
// 关闭抽屉，避免影响后续探针的开关语义
await cdp.eval(`(() => { const c = document.querySelector('.hns-drawer-close'); if (c) { c.click(); return 'true'; } return 'false'; })()`);
await sleep(300);

// ─── 8) 清理 + 截图 ───
await invoke('delete_harness_skill', { id: skillId });
await invoke('harness_delete_session', { id: newSid });
const shot = await cdp.send('Page.captureScreenshot', { format: 'png', fromSurface: true });
const out = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../data/ui-audit/llm-harness-phase78.png');
fs.writeFileSync(out, Buffer.from(shot.data, 'base64'));
console.log('SAVED=' + out);

console.log(failures === 0 ? 'ALL_PASS' : `FAILURES=${failures}`);
ws.close();
process.exit(failures === 0 ? 0 : 1);
