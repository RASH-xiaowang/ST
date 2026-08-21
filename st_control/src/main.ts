import App from './App.svelte';
import { mount } from 'svelte';
import './app.css';

const app = mount(App, { target: document.getElementById('app')! });

// 首帧挂载完成后淡出启动页：纯 HTML/CSS 启动页在 JS 加载前即显示，
// 此处确保界面已渲染至少再停留一小段时间，避免一闪而过或白屏。
requestAnimationFrame(() => {
  const splash = document.getElementById('app-splash');
  if (!splash) return;
  // performance.now() 从页面加载起算，启动页已显示这段时间，因此只补足最短展示时长
  const wait = Math.max(0, 600 - performance.now());
  window.setTimeout(() => {
    splash.classList.add('splash-hidden');
    window.setTimeout(() => splash.remove(), 450);
  }, wait);
});

export default app;
