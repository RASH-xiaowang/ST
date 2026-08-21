import App from './App.svelte';
import { mount } from 'svelte';
import { getCurrentWindow } from '@tauri-apps/api/window';

const app = mount(App, {
  target: document.getElementById('app')!,
});

// ============================================================
// 严格锁定窗口：禁止全屏模式，禁止拖拽改变窗口尺寸
// ============================================================
const appWindow = getCurrentWindow();

async function lockWindow() {
  try {
    // 禁止调整窗口大小（覆盖拖拽边缘/角落 + 系统菜单操作）
    await appWindow.setResizable(false);
    // 禁止全屏模式
    await appWindow.setFullscreen(false);
  } catch (err) {
    console.warn('窗口锁定失败（可能在非 Tauri 环境运行）:', err);
  }
}

lockWindow();

// 定期强制执行全屏锁定（防止通过 devtools / 快捷键绕过）
setInterval(async () => {
  try {
    const isFullscreen = await appWindow.isFullscreen();
    if (isFullscreen) {
      await appWindow.setFullscreen(false);
    }
  } catch (_) {}
}, 1000);

// 阻止所有可能触发全屏的快捷键
document.addEventListener('keydown', (e: KeyboardEvent) => {
  if (
    e.key === 'F11' ||
    e.code === 'F11' ||
    (e.key === 'Enter' && e.altKey)      // Alt+Enter（WebView2 全屏）
  ) {
    e.preventDefault();
  }
});

// 阻止 CSS 层面的缩放（例如 Ctrl+滚轮、Ctrl+0/+/−等）
document.addEventListener('wheel', (e: WheelEvent) => {
  if (e.ctrlKey || e.metaKey) {
    e.preventDefault();
  }
}, { passive: false });

document.addEventListener('keydown', (e: KeyboardEvent) => {
  if ((e.ctrlKey || e.metaKey) && (e.key === '=' || e.key === '-' || e.key === '0')) {
    e.preventDefault();
  }
});

// 阻止 Fullscreen API 调用（如 element.requestFullscreen()）
document.addEventListener('fullscreenchange', (e) => {
  if (document.fullscreenElement) {
    document.exitFullscreen();
  }
}, true);
document.addEventListener('webkitfullscreenchange', (e) => {
  if ((document as any).webkitFullscreenElement) {
    (document as any).webkitExitFullscreen();
  }
}, true);

export default app;
