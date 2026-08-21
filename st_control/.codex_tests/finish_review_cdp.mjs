// Finish review: bounded live re-measure of the current build via CDP.
// Reports only REAL clip defects (non-scrollable, non-ellipsis overflow),
// theme tokens, sidebar geometry, small targets, and console errors.
import { writeFile } from "node:fs/promises";

const CDP_HTTP = "http://127.0.0.1:9222/json";
const OUT = "E:/ST/st_control/.impeccable/finish-review-live.json";

async function getTarget() {
  const res = await fetch(CDP_HTTP);
  const list = await res.json();
  const page = list.find((t) => t.type === "page" && t.url.includes("1420"));
  if (!page) throw new Error("no page target");
  return page;
}

class CDP {
  constructor(url) {
    this.ws = new WebSocket(url);
    this.id = 0;
    this.pending = new Map();
    this.console = [];
    this.ws.onmessage = (ev) => {
      const msg = JSON.parse(ev.data);
      if (msg.id && this.pending.has(msg.id)) {
        const { resolve, reject } = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        if (msg.error) reject(new Error(JSON.stringify(msg.error)));
        else resolve(msg.result);
      } else if (msg.method === "Runtime.consoleAPICalled" && ["error", "warning"].includes(msg.params.type)) {
        this.console.push({
          type: msg.params.type,
          text: msg.params.args.map((a) => a.value ?? a.description ?? "").join(" ").slice(0, 200),
        });
      }
    };
  }
  async open() {
    if (this.ws.readyState === WebSocket.OPEN) return;
    await new Promise((res, rej) => {
      this.ws.onopen = res;
      this.ws.onerror = rej;
    });
  }
  send(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = ++this.id;
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }
  close() { try { this.ws.close(); } catch {} }
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const PANELS = [
  ["monitor", "首页"],
  ["ai_copy", "AI 文案"],
  ["ai_roles", "AI 角色"],
  ["llm", "大模型"],
  ["automation", "自动化"],
  ["wechat", "微信数据"],
  ["kb", "知识库"],
  ["data_dashboard", "数据看板"],
  ["db_manager", "数据库"],
];

const QA_JS = `(() => {
  const visible = (el) => {
    const r = el.getBoundingClientRect();
    const cs = getComputedStyle(el);
    return cs.display !== "none" && cs.visibility !== "hidden" && r.width > 0 && r.height > 0;
  };
  const issues = [];
  for (const el of document.querySelectorAll("body *")) {
    if (!visible(el)) continue;
    const cs = getComputedStyle(el);
    const dx = el.scrollWidth - el.clientWidth;
    const dy = el.scrollHeight - el.clientHeight;
    const scrollableX = cs.overflowX === "auto" || cs.overflowX === "scroll";
    const scrollableY = cs.overflowY === "auto" || cs.overflowY === "scroll";
    const isEllipsis = cs.textOverflow === "ellipsis" && cs.whiteSpace === "nowrap";
    const lineClamped = cs.display === "-webkit-box" && cs.webkitLineClamp !== "none";
    if (dx > 4 && !scrollableX && !isEllipsis) {
      issues.push({ tag: el.tagName, cls: (typeof el.className === "string" ? el.className : "").slice(0, 72), ax: "x", px: Math.round(dx), text: (el.textContent || "").trim().replace(/\\s+/g, " ").slice(0, 48) });
    }
    if (dy > 4 && !scrollableY && !lineClamped) {
      issues.push({ tag: el.tagName, cls: (typeof el.className === "string" ? el.className : "").slice(0, 72), ax: "y", px: Math.round(dy), text: (el.textContent || "").trim().replace(/\\s+/g, " ").slice(0, 48) });
    }
  }
  issues.sort((a, b) => b.px - a.px);
  const nav = document.querySelector(".nav-list");
  const dot = document.querySelector(".footer-status-dot");
  const primBtn = document.querySelector("button[class*=h-9]");
  const btnGeo = primBtn ? (() => {
    const r = primBtn.getBoundingClientRect();
    const cs = getComputedStyle(primBtn);
    return { h: Math.round(r.height), sh: primBtn.scrollHeight, ch: primBtn.clientHeight, lh: getComputedStyle(primBtn.querySelector(".relative.z-10, div") || primBtn).lineHeight };
  })() : null;
  const dotGeo = dot ? (() => {
    const r = dot.getBoundingClientRect();
    return { w: Math.round(r.width), h: Math.round(r.height), sh: dot.scrollHeight, ch: dot.clientHeight };
  })() : null;
  const main = document.querySelector("main.content") || document.body;
  return {
    tab: (document.querySelector(".nav-item.active") || {}).title || null,
    bodyBg: getComputedStyle(document.body).backgroundColor,
    sidebarBg: document.querySelector("aside.sidebar") ? getComputedStyle(document.querySelector("aside.sidebar")).backgroundColor : null,
    sampleCardBg: document.querySelector(".monitor-stats .card, .card, [class*=card]") ? getComputedStyle(document.querySelector(".monitor-stats .card, .card, [class*=card]")).backgroundColor : null,
    tokenBg: getComputedStyle(document.documentElement).getPropertyValue("--app-bg-color").trim(),
    tokenFg: getComputedStyle(document.documentElement).getPropertyValue("--app-font-color").trim(),
    textHead: (main.innerText || "").replace(/\\n+/g, " / ").slice(0, 160),
    issues: issues.slice(0, 12),
    sidebar: nav ? { scrollH: nav.scrollHeight, clientH: nav.clientHeight } : null,
    dotGeo,
    btnGeo,
  };
})()`;

async function main() {
  const target = await getTarget();
  const cdp = new CDP(target.webSocketDebuggerUrl);
  await cdp.open();
  await cdp.send("Runtime.enable");
  await cdp.send("Log.enable");
  const results = [];
  for (const [key, title] of PANELS) {
    await cdp.send("Runtime.evaluate", {
      expression: `document.querySelector('.nav-item[title=${JSON.stringify(title)}]')?.click()`,
    });
    await sleep(2000);
    const r = await cdp.send("Runtime.evaluate", { expression: QA_JS, returnByValue: true });
    results.push({ key, ...r.result.value });
    console.log(`[${key}] issues=${r.result.value.issues.length}`);
  }
  await writeFile(OUT, JSON.stringify({ panels: results, console: cdp.console.slice(0, 20) }, null, 2));
  console.log("console:", JSON.stringify(cdp.console.slice(0, 20)));
  cdp.close();
}

main().catch((e) => { console.error("FATAL:", e); process.exit(1); });
