// 从 shadcn-svelte 官方 registry 拉取组件（非交互式）
// 用法: node scripts/fetch-shadcn.mjs button card ...
import { writeFileSync, mkdirSync } from "node:fs";
import path from "node:path";

const REGISTRY = "https://shadcn-svelte.com/registry";
const UI_DIR = "src/lib/components/ui";

// registry 文件里的占位符 → 项目别名（保留组件源码中的 .js 后缀约定）
const ALIASES = {
  "$UTILS$": "src/lib/utils",
  "$LIB$": "src/lib",
  "$COMPONENTS$": "src/lib/components",
  "$HOOKS$": "src/lib/hooks",
  "$UI$": "src/lib/components/ui",
};

const wanted = process.argv.slice(2);
if (!wanted.length) {
  console.error("用法: node scripts/fetch-shadcn.mjs <component...>");
  process.exit(1);
}

const index = await (await fetch(`${REGISTRY}/index.json`)).json();
const byName = new Map(index.map((i) => [i.name, i]));

const queue = [...wanted];
const done = new Set();
const npmDeps = new Set();
const files = [];

while (queue.length) {
  const name = queue.shift();
  if (done.has(name)) continue;
  const meta = byName.get(name);
  if (!meta) {
    console.warn(`[skip] 未知组件: ${name}`);
    continue;
  }
  done.add(name);
  const item = await (await fetch(`${REGISTRY}/${meta.relativeUrl}`)).json();
  for (const dep of item.registryDependencies ?? []) queue.push(dep);
  for (const dep of item.dependencies ?? []) npmDeps.add(dep);
  for (const dep of item.devDependencies ?? []) npmDeps.add(dep);
  for (const f of item.files ?? []) {
    let content = f.content;
    for (const [k, v] of Object.entries(ALIASES)) {
      content = content.split(k).join(v);
    }
    files.push({ name, target: f.target, content });
  }
  console.log(`[ok] ${name}`);
}

for (const f of files) {
  const p = path.join(UI_DIR, f.target);
  mkdirSync(path.dirname(p), { recursive: true });
  writeFileSync(p, f.content, "utf8");
  console.log(`   → ${p}`);
}

if (npmDeps.size) {
  console.log("\n需要安装的依赖:\nnpm i " + [...npmDeps].join(" "));
}
