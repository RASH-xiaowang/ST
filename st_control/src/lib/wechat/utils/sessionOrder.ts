/* ============================================================
 * 微信数据管理模块 — 会话排序/实时重排纯函数
 * 自 WeChatPanel.svelte 下沉：置顶优先 + sort_ts 降序比较器，
 * 以及实时消息到达时的有序插入（命中头部原地替换 / 二分插入 / 追加）。
 * ============================================================ */
import type { WeChatSession } from '../types';

/** 会话顺序比较：置顶优先，其余按 sort_ts 降序（与后端 get_session_list 一致） */
export function sessionBefore(a: WeChatSession, b: WeChatSession): boolean {
  const pa = !!a.pinned;
  const pb = !!b.pinned;
  if (pa !== pb) return pa;
  return (a.sort_ts ?? 0) > (b.sort_ts ?? 0);
}

/**
 * 实时更新会话列表：命中头部原地替换（保持引用顺序），命中其他位置
 * 删除后按序二分插入（O(log n)），未命中追加到末尾（与后端已排序列表
 * 的增量语义一致）。返回新数组，不修改入参。
 */
export function upsertSessionOrdered(
  list: WeChatSession[],
  username: string,
  updated: WeChatSession,
): WeChatSession[] {
  const idx = list.findIndex((s) => s.username === username);
  if (idx === 0) {
    const next = list.slice();
    next[0] = updated;
    return next;
  }
  if (idx > 0) {
    const arr = list.slice();
    arr.splice(idx, 1);
    let lo = 0;
    let hi = arr.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (sessionBefore(arr[mid], updated)) {
        lo = mid + 1;
      } else {
        hi = mid;
      }
    }
    arr.splice(lo, 0, updated);
    return arr;
  }
  return [...list, updated];
}
