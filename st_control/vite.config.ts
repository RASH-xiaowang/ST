import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";
import path from "path";

const host = process.env.TAURI_DEV_HOST;

// 桌面内部工具：过滤 a11y 建议与未用 CSS 选择器噪音，保留其余警告（响应式、废弃用法等）
function filterNoiseWarnings(warning: any, handler: any) {
  if (
    warning?.code?.startsWith("a11y_") ||
    warning?.code === "css_unused_selector"
  ) {
    return;
  }
  handler?.(warning);
}

export default defineConfig(async () => ({
  plugins: [
    svelte({
      onwarn: filterNoiseWarnings,
    }),
    tailwindcss(),
  ],
  clearScreen: false,
  resolve: {
    alias: {
      '@wechat': path.resolve(__dirname, 'src/lib/wechat'),
      '$lib': path.resolve(__dirname, 'src/lib'),
      'src': path.resolve(__dirname, 'src'),
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 排除运行时输出目录：data/ 是应用统一数据目录（J-15），微信监控
      // 高频写入 db-wal/-shm 等文件，若不排除，每次写入都触发 Vite 热更新
      // 事件，海量事件会把 dev server 打挂（表现为页面卡在启动界面）。
      // 同时排除测试产物与文档目录，减少无谓的 page reload。
      ignored: [
        "**/src-tauri/**",
        "**/data/**",
        "**/.codex_tests/**",
        "**/docs/**",
        "/*.txt",
        "/*.tmp",
        "/*.log",
        "/*.bak",
        "/*.bak2",
        "/*.png",
      ],
    },
  },
}));
