// ============================================================
// 微信实时消息映射纯函数冒烟测试
// 锁定 wechat/utils/realtimeMsg.ts 的 toRealtimeMsg 可观测输出：
// 时间戳换算、群聊发送者、is_self 判定、通知类型、转账状态文案。
// 运行：node st_control/.codex_tests/smoke-realtime-msg.mjs
// ============================================================
import assert from 'node:assert/strict';
import { mkdirSync, rmSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import esbuild from 'esbuild';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(here, '..');
const outDir = path.join(here, 'out');
mkdirSync(outDir, { recursive: true });

const outFile = path.join(outDir, 'realtime-msg.mjs');
await esbuild.build({
  entryPoints: [path.join(root, 'src', 'lib', 'wechat', 'utils', 'realtimeMsg.ts')],
  bundle: true,
  format: 'esm',
  platform: 'neutral',
  outfile: outFile,
  logLevel: 'silent',
});

const { toRealtimeMsg } = await import(pathToFileURL(outFile).href);

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// 基础映射：时间戳微秒→秒、文本/类型/图片回退
const base = toRealtimeMsg({
  username: 'wxid_a',
  local_id: 42,
  sort_seq: 7,
  timestamp: 1_700_000_000_000_000, // 微秒
  msg_type: 1,
  content: '你好',
  time: '10:30',
  sender_username: 'wxid_b',
  is_group: false,
});
ok(base.local_id === 42, 'local_id 透传');
ok(base.sort_seq === 7, 'sort_seq 透传');
ok(base.ts === 1_700_000_000, '时间戳微秒 → 秒');
ok(base.time === '10:30', 'm.time 优先于推导');
ok(base.type === 1, 'msg_type 透传');
ok(base.text === '你好', 'content → text');
ok(base.is_notice === false, '普通消息非通知');
ok(base.sender_name === '', '单聊不填 sender_name');
ok(base.image_url === null, 'image_url 缺失 → null');

// 通知类型
ok(toRealtimeMsg({ username: 'u', msg_type: 10000 }).is_notice === true, 'msg_type 10000 → 通知');
ok(toRealtimeMsg({ username: 'u', msg_type: 10002 }).is_notice === true, 'msg_type 10002 → 通知');

// 群聊发送者
const group = toRealtimeMsg({
  username: 'room@chatroom',
  is_group: true,
  sender: '张三',
  sender_username: 'wxid_c',
});
ok(group.sender_name === '张三', '群聊 sender → sender_name');

// is_self 判定：本机 wxid 优先，其次 is_send，缺失默认对方
ok(toRealtimeMsg({ username: 'u', sender_username: 'wxid_me' }, 'wxid_me').is_self === true, 'sender_username 命中 selfUsername → 自己');
ok(toRealtimeMsg({ username: 'u', sender_username: 'wxid_other' }, 'wxid_me').is_self === false, '他人 sender_username → 对方');
ok(toRealtimeMsg({ username: 'u', is_send: false }, 'wxid_me').is_self === false, 'is_send=false → 对方');
ok(toRealtimeMsg({ username: 'u' }).is_self === false, '缺失方向字段默认对方');

// 转账卡片：按方向重算状态文案，且 rich 为拷贝（不改原载荷）
const payload = {
  username: 'u',
  content: '转账',
  sender_username: 'wxid_me',
  rich: { type: 'transfer', paysubtype: '3', transfer_id: 't1', direction: '通用标签' },
};
const selfTx = toRealtimeMsg(payload, 'wxid_me');
ok(selfTx.rich?.direction === '已被接收', 'paysubtype 3 + 自己发出 → 已被接收');
ok(payload.rich.direction === '通用标签', '原载荷 rich 不被改写（浅拷贝）');
const otherTx = toRealtimeMsg({ ...payload, sender_username: 'wxid_other' }, 'wxid_me');
ok(otherTx.rich?.direction === '已收款', 'paysubtype 3 + 对方发出 → 已收款');
const refund = toRealtimeMsg({ ...payload, rich: { ...payload.rich, paysubtype: '4' } }, 'wxid_me');
ok(refund.rich?.direction === '已退还', 'paysubtype 4 → 已退还');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
