// ============================================================
// 微信会话类型识别 — 运行期冒烟测试
// 锁定 wechat/utils/session 下沉后的可观测输出：
//   群聊 / 公众号 / 单聊分类边界
// 运行：node st_control/.codex_tests/smoke-wechat-session.mjs
// ============================================================
import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import esbuild from 'esbuild';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(here, '..');
const outDir = path.join(here, 'out');
mkdirSync(outDir, { recursive: true });

const src = readFileSync(path.join(root, 'src', 'lib', 'wechat', 'utils', 'session.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'wechat-session.mjs');
writeFileSync(outFile, code);

const mod = await import(pathToFileURL(outFile).href);
const { isGroup, isOfficial, kindOf } = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// 群聊识别
ok(isGroup('wxid_abc@chatroom'), '@chatroom 后缀 → 群聊');
ok(isGroup('wxid@im.chatroom'), '@im.chatroom 包含 → 群聊');
ok(!isGroup('wxid_abc'), '普通用户非群聊');

// 公众号识别
ok(isOfficial('gh_abc123'), 'gh_ 前缀 → 公众号');
ok(isOfficial('gh_abc@gh'), '@gh 后缀 → 公众号');
ok(!isOfficial('wxid_abc'), '普通用户非公众号');

// 分类优先级
ok(kindOf('wxid@chatroom') === 'group', '群聊分类');
ok(kindOf('gh_abc') === 'official', '公众号分类');
ok(kindOf('wxid_abc') === 'private', '单聊分类');
ok(kindOf('gh_x@chatroom') === 'group', '群聊优先于公众号');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);
