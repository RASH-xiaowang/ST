// ============================================================
// 端到端验证：QQ 官方机器人 openid 自动收集面板
//   1. 进入「消息通道」→ 切到「QQ官方」平台 tab
//   2. 验证账号「QQ官方机器人」存在、发送台出现「openid 自动收集」
//   3. 调用 bot_list_qqbot_contacts 验证 IPC 连通（当前应为空列表）
//   4. 验证推送目标输入框与私聊/群聊切换存在
// 运行：node st_control/.codex_tests/e2e-qqbot-openid.mjs
// ============================================================

const CDP_BASE = 'http://127.0.0.1:9222';

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function findTarget() {
  for (let i = 0; i < 60; i++) {
    try {
      const res = await fetch(`${CDP_BASE}/json/list`);
      const list = await res.json();
      const t = list.find((x) => x.type === 'page');
      if (t) return t;
    } catch {
      /* 应用尚未就绪 */
    }
    await sleep(1000);
  }
  throw new Error('60 秒内未发现 CDP 页面目标');
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
    const r = await this.send('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (r.exceptionDetails) {
      throw new Error('evaluate 异常: ' + JSON.stringify(r.exceptionDetails));
    }
    return r.result.value;
  }
}

const target = await findTarget();
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((res, rej) => {
  ws.onopen = res;
  ws.onerror = rej;
});
const cdp = new Cdp(ws);
await cdp.send('Runtime.enable');

let passed = 0;
const ok = (cond, msg) => {
  if (!cond) throw new Error('断言失败：' + msg);
  passed++;
  console.log('✓', msg);
};

// 1. 进入消息通道
ok(
  await cdp.eval(`(() => { const b = document.querySelector('.nav-item[title="消息通道"]'); if (!b) return false; b.click(); return true; })()`),
  '点击导航「消息通道」',
);
await sleep(600);

// 2. 切到 QQ官方 平台 tab
const switched = await cdp.eval(`(() => {
  const btns = [...document.querySelectorAll('button')];
  const b = btns.find((x) => (x.textContent || '').includes('QQ官方'));
  if (!b) return false;
  b.click();
  return true;
})()`);
ok(switched, '点击平台 tab「QQ官方」');
await sleep(800);

// 3. 账号列表中有 QQ官方机器人
const accountShown = await cdp.eval(
  `[...document.querySelectorAll('*')].some((el) => (el.textContent || '').trim() === 'QQ官方机器人')`,
);
ok(accountShown, '账号「QQ官方机器人」出现在列表中');
await sleep(600);

// 4. 发送台出现 openid 自动收集面板与推送目标
const sendArea = await cdp.eval(`(() => {
  const hasCollect = [...document.querySelectorAll('*')].some((el) => (el.textContent || '').includes('openid 自动收集'));
  const hasTarget = [...document.querySelectorAll('*')].some((el) => (el.textContent || '').includes('推送目标'));
  const hasEmpty = [...document.querySelectorAll('*')].some((el) => (el.textContent || '').includes('还没有收集到 openid'));
  const listBtns = [...document.querySelectorAll('button')].filter((b) => {
    const t = b.textContent || '';
    return (t.includes('用户') || t.includes('群')) && /[A-F0-9]{16,}/.test(t);
  }).length;
  return { hasCollect, hasTarget, hasEmpty, listBtns };
})()`);
ok(sendArea.hasCollect, '发送台显示「openid 自动收集」面板');
ok(sendArea.hasTarget, '发送台显示「推送目标」行');
ok(
  sendArea.hasEmpty || sendArea.listBtns > 0,
  `openid 面板正常（空态 ${sendArea.hasEmpty ? '显示' : '不显示'}，已收集目标 ${sendArea.listBtns} 条）`,
);

// 5. IPC 连通：bot_list_qqbot_contacts 返回数组
const contacts = await cdp.eval(
  `window.__TAURI_INTERNALS__.invoke('bot_list_qqbot_contacts', { accountId: 5 })`,
);
ok(Array.isArray(contacts), 'IPC bot_list_qqbot_contacts 返回数组（当前 ' + contacts.length + ' 条）');
if (contacts.length > 0) {
  ok(
    contacts.every((c) => typeof c.openid === 'string' && c.openid.length > 8),
    '已收集条目含有效 openid',
  );
}

// 6. 私聊/群聊切换按钮存在
const toggles = await cdp.eval(`(() => {
  const btns = [...document.querySelectorAll('button')];
  return ['私聊', '群聊'].every((t) => btns.some((b) => (b.textContent || '').trim() === t));
})()`);
ok(toggles, '私聊 / 群聊目标切换按钮存在');

// 7. 切到「群聊」：无已收集群 openid 时应出现收集指引横幅
await cdp.eval(`(() => {
  const b = [...document.querySelectorAll('button')].find((x) => (x.textContent || '').trim() === '群聊');
  if (b) b.click();
  return true;
})()`);
await sleep(400);
const hasGroupOpenid = contacts.some((c) => c.kind === 'group');
const groupGuide = await cdp.eval(
  `[...document.querySelectorAll('*')].some((el) => (el.textContent || '').includes('还没有「群 openid」'))`,
);
if (hasGroupOpenid) {
  ok(!groupGuide, '已有群 openid，不显示空群指引横幅');
} else {
  ok(groupGuide, '无群 openid 时显示「群 openid 收集指引」横幅');
}

console.log(`\n全部通过（${passed} 项断言）`);
process.exit(0);
