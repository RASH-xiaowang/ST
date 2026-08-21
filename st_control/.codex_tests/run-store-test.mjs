// ============================================================
// 大模型配置实时同步 — 前端 store 链路运行期验证
// 验证：后端广播 llm-config-changed → store 静默刷新 →
//       共享配置更新 → 订阅者（知识库/智能体等）被通知
// 运行：node st_control/.codex_tests/run-store-test.mjs
// ============================================================
import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import esbuild from 'esbuild';
import { compileModule } from 'svelte/compiler';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(here, '..');
const outDir = path.join(here, 'out');
mkdirSync(outDir, { recursive: true });

// 1) 编译 store.svelte.ts（esbuild 先去 TS 类型，Svelte compileModule 处理 runes）
const storeSrc = readFileSync(path.join(root, 'src', 'lib', 'llm', 'store.svelte.ts'), 'utf8');
const stripped = await esbuild.transform(storeSrc, { loader: 'ts' });
const compiled = compileModule(stripped.code, {
  filename: 'store.svelte.ts',
  generate: 'client',
}).js.code;
writeFileSync(path.join(outDir, 'store.mjs'), compiled);

// 2) 入口：导出 store + 测试钩子
const mockEvent = path.join(here, 'mocks', 'tauri-event.mjs');
const mockIpcSrc = readFileSync(path.join(here, 'mocks', 'llm-ipc.mjs'), 'utf8');
// 编译产物里是相对导入 './services/ipc'（esbuild 不允许 './' 前缀的 alias），
// 因此把 IPC mock 写到编译产物同目录的 services/ipc.js，保证与入口共用同一实例。
const ipcOutDir = path.join(outDir, 'services');
mkdirSync(ipcOutDir, { recursive: true });
writeFileSync(path.join(ipcOutDir, 'ipc.js'), mockIpcSrc);
writeFileSync(
  path.join(outDir, 'entry.mjs'),
  [
    `export * from './store.mjs';`,
    `export { __fire, __handlers } from 'mock:event';`,
    `export { __setConfig } from './services/ipc.js';`,
  ].join('\n'),
);

await esbuild.build({
  entryPoints: [path.join(outDir, 'entry.mjs')],
  bundle: true,
  platform: 'node',
  format: 'esm',
  outfile: path.join(outDir, 'bundle.mjs'),
  alias: {
    'mock:event': mockEvent,
    '@tauri-apps/api/event': mockEvent,
  },
  plugins: [
    {
      name: 'resolve-shared-format',
      setup(b) {
        // store.mjs 编译产物中的 `../format` 相对 outDir 无法解析，
        // 显式指向共享 src/lib/format.ts。
        b.onResolve({ filter: /^\.\.\/format$/ }, () => ({
          path: path.join(root, 'src', 'lib', 'format.ts'),
        }));
      },
    },
  ],
  logLevel: 'silent',
});

// 3) 运行断言
const mod = await import(pathToFileURL(path.join(outDir, 'bundle.mjs')).href);
const {
  llmStore,
  refreshLlmConfig,
  onLlmConfigChanged,
  startLlmSync,
  LLM_CONFIG_CHANGED_EVENT,
  __fire,
  __handlers,
  __setConfig,
} = mod;

let passed = 0;
const ok = (cond, msg) => {
  assert.ok(cond, msg);
  passed++;
  console.log('✓', msg);
};

// 初始状态：未加载完成
ok(llmStore.loading === true, '初始 loading = true（首次加载中）');
ok(llmStore.config.providers.length === 0, '初始配置为空');

// 注册监听 + 订阅者
startLlmSync();
ok(
  __handlers.has(LLM_CONFIG_CHANGED_EVENT),
  `startLlmSync 已注册 ${LLM_CONFIG_CHANGED_EVENT} 监听`,
);
let notifyCount = 0;
const unsub = onLlmConfigChanged(() => {
  notifyCount++;
});

// 手动刷新（等价于各界面挂载时首次拉取）
await refreshLlmConfig();
ok(llmStore.config.providers.length === 1, '刷新后拿到 1 个提供方');
ok(llmStore.config.providers[0].models.includes('m1'), '刷新后包含 m1 模型');
ok(llmStore.revision === 1, '首次刷新 revision = 1');
ok(notifyCount === 1, '刷新后订阅者被通知 1 次');
ok(llmStore.loading === false, '刷新完成后 loading = false');

// 关键场景：后端广播「大模型管理添加了模型 m2」
// 注意：llmStore.config 是 Svelte proxy，用展开构造普通对象
const nextCfg = {
  ...llmStore.config,
  providers: llmStore.config.providers.map((p) => ({ ...p, models: [...p.models, 'm2'] })),
};
__setConfig(nextCfg);
await __fire(LLM_CONFIG_CHANGED_EVENT, { changed_at: '2026-08-09T00:00:00Z' });
await new Promise((r) => setTimeout(r, 20)); // 等待 store 内异步刷新完成

ok(
  llmStore.config.providers[0].models.includes('m2'),
  '收到广播后共享配置实时包含新添加的 m2 模型',
);
ok(llmStore.revision === 2, '收到广播后 revision 自增到 2');
ok(notifyCount === 2, '收到广播后订阅者（知识库/智能体等）被再次通知');
ok(llmStore.loading === false, '后台事件刷新为静默刷新，不闪 loading');

// 取消订阅后不再通知
unsub();
await __fire(LLM_CONFIG_CHANGED_EVENT, { changed_at: '2026-08-09T00:00:01Z' });
await new Promise((r) => setTimeout(r, 20));
ok(notifyCount === 2, '取消订阅后不再收到通知');

console.log(`\n全部通过：${passed} 项断言`);
rmSync(outDir, { recursive: true, force: true });
