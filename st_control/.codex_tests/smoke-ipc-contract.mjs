// ============================================================
// IPC 参数键名契约审计（前端 invoke 实参 ↔ Rust 命令参数）
// 规则：Tauri 2 自动把前端 camelCase 参数转为 Rust snake_case 参数名；
//       State/Arc/Window/AppHandle/Manager 由 Tauri 注入，不比对；
//       Channel 是真实参数（on_chunk ← onChunk），需比对。
// 任一不一致即失败（防止 ack/resync 类键名漂移回归）。
// 运行：node st_control/.codex_tests/smoke-ipc-contract.mjs
// ============================================================

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';

const ROOT = new URL('..', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1');

// Tauri 2 默认 argument_case=Camel：Rust 参数名经 heck ToLowerCamelCase 转成前端键。
// heck 语义：所有下划线/非字母数字都是词边界，前导/尾随边界丢弃，首词小写、其余词首字母大写。
// 例：user_id→userId、_user_id→userId、top_k→topK、vector_scan_cap→vectorScanCap。
const rustToCamel = (s) =>
  s
    .split(/[^A-Za-z0-9]+/)
    .filter(Boolean)
    .flatMap((w) => w.split(/(?<=[a-z0-9])(?=[A-Z])/))
    .map((w, i) => (i === 0 ? w.toLowerCase() : w[0].toUpperCase() + w.slice(1).toLowerCase()))
    .join('');

function walk(dir, exts) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    const p = join(dir, entry);
    const st = statSync(p);
    if (st.isDirectory()) out.push(...walk(p, exts));
    else if (exts.some((e) => p.endsWith(e))) out.push(p);
  }
  return out;
}

function rustCommands() {
  const cmds = new Map();
  for (const f of walk(join(ROOT, 'src-tauri', 'src'), ['.rs'])) {
    const src = readFileSync(f, 'utf8');
    // 匹配 #[tauri::command(...)]? 之后的 pub fn / pub async fn
    const re = /#\[tauri::command(?:\([^)]*\))?\]\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)\s*\(([^)]*)\)/g;
    let m;
    while ((m = re.exec(src))) {
      const [, name, paramsRaw] = m;
      // 顶层逗号切分（跳过 <> 与嵌套括号）
      const parts = [];
      let depth = 0, cur = '';
      for (const ch of paramsRaw) {
        if (ch === '<' || ch === '(' || ch === '[') depth++;
        else if (ch === '>' || ch === ')' || ch === ']') depth--;
        if (ch === ',' && depth === 0) { parts.push(cur); cur = ''; continue; }
        cur += ch;
      }
      if (cur.trim()) parts.push(cur);
      const args = [];
      for (const p of parts) {
        const pm = p.match(/^\s*([a-z_][a-z0-9_]*)\s*:\s*(?!:)(.+?)\s*$/);
        if (!pm) continue;
        const [, pname, ptype] = pm;
        // Tauri 注入参数：State / Arc / Window / AppHandle / Manager
        if (/State<|Arc<|\bWindow\b|AppHandle|\bManager\b/.test(ptype)) continue;
        args.push(pname);
      }
      cmds.set(name, args);
    }
  }
  return cmds;
}

function frontCalls() {
  const calls = [];
  for (const f of walk(join(ROOT, 'src'), ['.ts', '.svelte'])) {
    const src = readFileSync(f, 'utf8');
    const re = /\binvoke(?:<[^>]*>)?\(\s*'([^']+)'/g;
    let m;
    while ((m = re.exec(src))) {
      const cmd = m[1];
      // 命令串之后：若紧跟 ',' + '{'，做括号配对提取顶层键
      let i = m.index + m[0].length;
      while (i < src.length && /\s/.test(src[i])) i++;
      if (src[i] !== ',') continue; // 无实参对象（变量/无参调用），跳过
      i++;
      while (i < src.length && /\s/.test(src[i])) i++;
      if (src[i] !== '{') continue; // 实参是变量（Record 透传），无法静态比对，跳过
      let depth = 0, j = i;
      const keys = [];
      let curKey = '';
      let inValue = false; // 深度 1 下是否处于值位置（冒号后）
      for (; j < src.length; j++) {
        const ch = src[j];
        if (ch === '{') {
          depth++;
          if (depth === 2) { curKey = ''; inValue = false; }
          continue;
        }
        if (ch === '}') {
          depth--;
          if (depth === 0) { if (curKey) keys.push(curKey); break; }
          continue;
        }
        if (depth === 1) {
          if (ch === ':' || ch === ',') {
            if (curKey) { keys.push(curKey); curKey = ''; }
            inValue = ch === ':';
          } else if (/[A-Za-z_$]/.test(ch) || (/\d/.test(ch) && curKey)) {
            if (!inValue) curKey += ch;
          } else if (/\s/.test(ch)) {
            /* 允许键与冒号间空白 */
          } else if (!inValue) {
            curKey = '';
          }
        }
      }
      const line = src.slice(0, m.index).split('\n').length;
      calls.push({ file: relative(ROOT, f).replace(/\\/g, '/'), line, cmd, keys });
    }
  }
  return calls;
}

const cmds = rustCommands();
const calls = frontCalls();
const problems = [];
let audited = 0;

for (const { file, line, cmd, keys } of calls) {
  const rargs = cmds.get(cmd);
  if (!rargs) continue; // 后端未注册的命令不在本审计范围
  if (rargs.length === 0) continue;
  const got = keys.slice().sort();
  const want = rargs.map(rustToCamel).sort();
  audited++;
  if (JSON.stringify(got) !== JSON.stringify(want)) {
    problems.push({ file, line, cmd, want, got });
  }
}

console.log(`IPC 契约审计：Rust 命令 ${cmds.size} 个，前端 invoke 调用 ${calls.length} 处，参数可比对 ${audited} 处`);
if (problems.length) {
  console.error(`发现 ${problems.length} 处参数键名不一致：`);
  for (const p of problems) {
    console.error(`- ${p.file}:${p.line} ${p.cmd}\n    rust=${JSON.stringify(p.want)}\n    front=${JSON.stringify(p.got)}`);
  }
  process.exit(1);
}
console.log('✓ 全部一致（含 Tauri snake_case→camelCase 自动转换）');
process.exit(0);
