/* ============================================================
 * 微信数据管理模块 — 统一导出入口（barrel）
 *
 * 外部模块（如 App.svelte）应只通过本文件引入微信功能：
 *   import { WeChatPanel, wechatIpc, onWechatMessage } from '$lib/wechat'
 * ============================================================ */

// ─── 组件 ───
export { default as WeChatPanel } from './components/WeChatPanel.svelte';
export { default as WeChatConfig } from './components/WeChatConfig.svelte';
export { default as WeChatBootstrap } from './components/WeChatBootstrap.svelte';

// ─── 类型 ───
export * from './types';

// ─── 常量 ───
export * from './constants';

// ─── 服务层（IPC 调用） ───
export * as wechatIpc from './services/ipc';

// ─── 事件监听 ───
export * from './events';

// ─── 工具函数 ───
export * from './utils';
