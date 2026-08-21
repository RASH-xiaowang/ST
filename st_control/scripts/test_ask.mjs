import { chromium } from 'playwright-core';
const browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
const page = browser.contexts()[0].pages()[0];
const out = await page.evaluate(async () => {
  const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);
  const res = {};
  for (const kw of ['转账', '红包', '项目', '你好']) {
    try {
      const r = await invoke('search_wechat_messages', { query: kw, limit: 5 });
      const hits = r?.hits?.length ?? r?.items?.length ?? 0;
      res[`search:${kw}`] = { hits, total: r?.total ?? 0, indexed: r?.indexed, sample: (r?.hits?.[0] ?? r?.items?.[0] ?? null) };
    } catch (e) {
      res[`search:${kw}`] = { err: String(e).slice(0, 200) };
    }
  }
  for (const q of ['我最近的转账记录', '我们聊过什么项目', '谁给我发过红包']) {
    try {
      const r = await invoke('ask_wechat', { question: q, limit: 10 });
      res[`ask:${q}`] = {
        citations: (r?.citations ?? []).length,
        answer: (r?.answer ?? '').slice(0, 120),
        error: r?.error ?? null,
        plan: r?.plan,
        first: r?.citations?.[0] ?? null,
      };
    } catch (e) {
      res[`ask:${q}`] = { err: String(e).slice(0, 300) };
    }
  }
  return res;
});
console.log(JSON.stringify(out, null, 1));
await browser.close();
