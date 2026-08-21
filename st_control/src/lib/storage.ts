/* ============================================================
 * 通用安全 localStorage 工具（跨 feature 共享）
 * 收敛 GlobalChatTab / WikiPanel / WeChatPanel 等处的
 * try/catch 读写重复实现；隐私模式或存储不可用时安全降级。
 * ============================================================ */

/** 安全读取 localStorage（存储不可用时返回 null） */
export function lsGet(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

/** 安全写入 localStorage（存储不可用时忽略，仅本次生效） */
export function lsSet(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* 存储不可用时忽略 */
  }
}

/** 安全删除 localStorage 键（存储不可用时忽略） */
export function lsRemove(key: string): void {
  try {
    localStorage.removeItem(key);
  } catch {
    /* 存储不可用时忽略 */
  }
}
