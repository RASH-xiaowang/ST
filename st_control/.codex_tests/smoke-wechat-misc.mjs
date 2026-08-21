// ============================================================
// 微信杂项纯函数 — 运行期冒烟测试
// 锁定 misc 下沉后的可观测输出：
//   文件类型分类 / 小程序 URL 解码 / 缺失图占比 / 客服会话识别
// 运行：node st_control/.codex_tests/smoke-wechat-misc.mjs
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

const src = readFileSync(path.join(root, 'src', 'lib', 'wechat', 'utils', 'misc.ts'), 'utf8');
const { code } = await esbuild.transform(src, { loader: 'ts', format: 'esm' });
const outFile = path.join(outDir, 'wechat-misc.mjs');
writeFileSync(outFile, code);

const mod = await import(pathToFileURL(outFile).href);
const { extTone, miniAppPageUrl, checkupPct, checkupRatePct, countMissingChats, isKefuSession, isMiniAppKefuSession } = mod;

// 共享 utils 的 safeParseInt（WeChatConfig 收敛来源）
const utilsSrc = readFileSync(path.join(root, 'src', 'lib', 'wechat', 'utils', 'index.ts'), 'utf8');
const { code: utilsCode } = await esbuild.transform(utilsSrc, { loader: 'ts', format: 'esm' });
writeFileSync(path.join(outDir, 'wechat-utils.mjs'), utilsCode);
const utilsMod = await import(pathToFileURL(path.join(outDir, 'wechat-utils.mjs')).href);
const { safeParseInt } = utilsMod;

// backdrop.ts 的 gargantuaFrameUrl（GargantuaBackdrop 下沉）
const backdropSrc = readFileSync(path.join(root, 'src', 'lib', 'wechat', 'utils', 'backdrop.ts'), 'utf8');
const backdropOut = path.join(outDir, 'wechat-backdrop.mjs');
writeFileSync(backdropOut, (await esbuild.transform(backdropSrc, { loader: 'ts', format: 'esm' })).code);
const { gargantuaFrameUrl } = await import(pathToFileURL(backdropOut).href);

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// 文件类型分类（大小写不敏感）
ok(extTone('pdf') === 'doc', 'pdf → doc');
ok(extTone('XLSX') === 'sheet', 'XLSX（大写）→ sheet');
ok(extTone('mp4') === 'video', 'mp4 → video');
ok(extTone('png') === 'image', 'png → image');
ok(extTone('exe') === 'app', 'exe → app');
ok(extTone('') === 'file', '空扩展名 → file');
ok(extTone('xyz') === 'file', '未知扩展名 → file');

// 小程序 URL 解码
ok(miniAppPageUrl({ pagepath: 'pages/x?url=https%3A%2F%2Fdocs.qq.com%2Fdoc' }) === 'https://docs.qq.com/doc', '解码 url 参数');
ok(miniAppPageUrl({ pagepath: 'pages/x?url=javascript%3Aalert(1)' }) === '', '非 http 链接拒绝');
ok(miniAppPageUrl({ pagepath: 'no-url-here' }) === '', '无 url 参数返回空');
ok(miniAppPageUrl(null) === '', 'null 返回空');

// 缺失图占比
ok(checkupPct(50, 200) === '25.0', '占比计算 1 位小数');
ok(checkupPct(10, 0) === '0.0', 'total=0 返回 0.0');
ok(checkupRatePct(3, 10) === '30.0', '会话占比计算');
ok(checkupRatePct(0, 0) === '0.0', '会话 total=0 返回 0.0');
ok(countMissingChats([{ missing: 2 }, { missing: 0 }, {}]) === 1, '缺失会话计数');
ok(countMissingChats([]) === 0, '空列表 0 个缺失会话');

// 客服会话识别
ok(isKefuSession('wxid@kefu.openim'), 'kefu.openim 识别');
ok(isKefuSession('BRANDSESSIONHOLDER'), '品牌会话（大小写不敏感）');
ok(!isKefuSession('normaluser'), '普通会话不是客服');
ok(isMiniAppKefuSession('foo@openim'), '小程序客服 @openim 识别');
ok(isMiniAppKefuSession('xopencustomerservicemsg'), 'opencustomerservicemsg 识别');
ok(!isMiniAppKefuSession('foo@kefu.openim'), '客服会话不是小程序客服');

// safeParseInt（WeChatConfig 收敛到共享实现）
ok(safeParseInt('42', 0) === 42, '字符串解析');
ok(safeParseInt(3.7, 0) === 3.7, '数字原样（不截断）');
ok(safeParseInt('', 5) === 5 && safeParseInt('abc', 5) === 5, '空/非法 → fallback');
ok(safeParseInt('999', 0, 0, 100) === 100, '上限钳制');

// gargantuaFrameUrl（GargantuaBackdrop 下沉）
ok(gargantuaFrameUrl({}) === '/gargantua/index.html?bg=1&q=standard', '默认参数');
const full = gargantuaFrameUrl({ steps: 64, cam: 'poster', motion: false, bright: 1.5, star: 2, sky: 0.1 });
ok(full.includes('steps=64') && full.includes('cam=poster') && full.includes('nocine=1')
  && full.includes('bright=1.5') && full.includes('star=2') && full.includes('sky=0.1'), '全参数透传');
ok(!gargantuaFrameUrl({ steps: 0 }).includes('steps='), 'steps=0 truthy 判断不设置');
ok(!gargantuaFrameUrl({ bright: null }).includes('bright='), 'bright=null 不设置');
ok(!gargantuaFrameUrl({ motion: true }).includes('nocine'), 'motion=true 无 nocine');

rmSync(outDir, { recursive: true, force: true });
console.log(`\n全部通过：${passed} 项断言`);