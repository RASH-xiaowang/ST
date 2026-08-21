/* ============================================================
 * 微信数据管理模块 — 安全/时间展示纯函数
 * 自 WeChatConfig.svelte 下沉：API 令牌生成、最后活跃日期格式化。
 * 不依赖组件状态，可独立单测。
 * ============================================================ */

/** 生成 64 位随机十六进制令牌（32 字节 → 64 hex 字符） */
export function generateApiToken(): string {
  const buf = new Uint8Array(32);
  crypto.getRandomValues(buf);
  return Array.from(buf, (b) => b.toString(16).padStart(2, '0')).join('');
}

/** 最后活跃时间（Unix 秒）→ YYYY-MM-DD；非法/空返回"未知" */
export function fmtLastActive(ts: number): string {
  if (!ts) return '未知';
  const d = new Date(ts * 1000);
  if (Number.isNaN(d.getTime())) return '未知';
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}
