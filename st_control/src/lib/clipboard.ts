/* ============================================================
 * 通用剪贴板工具（跨 feature 共享）
 * 收敛 WeChatPanel / DbManager 等处的 try/catch 复制重复；
 * 浏览器不支持或权限拒绝时返回 false，由调用方决定反馈。
 * ============================================================ */

/** 复制文本到剪贴板；成功返回 true，失败（不支持/权限拒绝）返回 false */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}
