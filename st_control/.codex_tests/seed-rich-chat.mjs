// 植入一段富文本对话（表格/引用/代码/长文），用于 UI 变形检查
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
}
const target = await findTarget();
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
const cdp = new Cdp(ws);
const out = await cdp.eval(`(async () => {
  const cfg = await window.__TAURI_INTERNALS__.invoke('get_llm_config');
  const p = (cfg.providers ?? []).find((x) => (x.models ?? []).includes('deepseek-v4-flash'));
  const rich = ${JSON.stringify(`Markdown 常用语法如下：

| 语法 | 作用 | 示例 |
| --- | --- | --- |
| **加粗** | 强调重点 | \`**文本**\` |
| *斜体* | 次要强调 | \`*文本*\` |
| 标题 | 分层结构 | \`# 一级标题\` |
| 列表 | 分点罗列 | \`- 项目\` |
| 代码块 | 代码展示 | \`\\\`\\\`\\\`js ... \\\`\\\`\\\`\` |
| 引用 | 引述内容 | \`> 引用文字\` |
| 链接 | 跳转地址 | \`[文字](网址)\` |
| 分割线 | 内容分隔 | \`---\` |

> 引用块示例：Markdown 是一种轻量级标记语言，它允许人们使用易读易写的纯文本格式编写文档，然后转换成有效的 HTML 文档。

代码示例（JavaScript 快速排序）：

\`\`\`js
function quickSort(arr) {
  if (arr.length <= 1) return arr;
  const pivot = arr[0];
  const left = arr.slice(1).filter((x) => x < pivot);
  const right = arr.slice(1).filter((x) => x >= pivot);
  return [...quickSort(left), pivot, ...quickSort(right)];
}
\`\`\`

要点总结：
1. 表格适合结构化对比
2. 引用适合标注来源
3. 代码块适合展示程序
4. 分割线适合分隔章节

这是一段很长的说明文字，用来验证长文本在气泡内的换行与排版表现：Markdown 的设计哲学是「易读易写」，源文件本身就应该像纯文本一样可以直接阅读，因此没有复杂的排版命令。它由 John Gruber 在 2004 年创建，如今已成为 GitHub、Stack Overflow 等平台的事实标准，广泛应用于技术文档、README 文件、博客写作与即时通讯工具的内容排版中。`)}
  const msgs = [
    { role: 'user', content: '帮我介绍一下 Markdown 的常用语法，用表格总结，再给一段代码示例和引用说明。' },
    { role: 'assistant', content: rich },
    { role: 'user', content: '再补充一下分割线和嵌套列表的用法。' },
    { role: 'assistant', content: '分割线用三个及以上短横线或星号表示：\\n\\n---\\n\\n嵌套列表在父项下缩进两个空格：\\n\\n- 父项一\\n  - 子项 A\\n  - 子项 B\\n- 父项二\\n\\n> 嵌套引用也可以使用多个 > 符号实现。' },
    { role: 'user', content: '好的谢谢，最后用一句话总结。' },
    { role: 'assistant', content: '总结：Markdown 用最少量的符号实现了结构化排版，核心就是「易读的纯文本」本身。' },
  ];
  await window.__TAURI_INTERNALS__.invoke('clear_llm_chat_history', { providerId: p.id, model: 'deepseek-v4-flash' });
  await window.__TAURI_INTERNALS__.invoke('append_llm_chat_messages', { providerId: p.id, model: 'deepseek-v4-flash', messages: msgs });
  return JSON.stringify({ ok: true, provider: p.name });
})()`);
console.log('SEED=' + out);
ws.close();
process.exit(0);
