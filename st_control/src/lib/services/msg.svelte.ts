/* ============================================================
 * 通用轻量操作反馈消息（toast/banner）
 * 自 wechat/services/msg.svelte.ts 上移（跨 feature 共享）：
 * 统一 text/ok 状态 + 自动清空（clearTimeout 消除竞态）。
 * ============================================================ */

/** 操作反馈消息状态工厂（每组件一个实例，durationMs 控制自动清空延迟） */
export function createMsg(durationMs = 3500) {
  const state = $state({ text: '', ok: true });
  let timer: ReturnType<typeof setTimeout> | undefined;

  function show(text: string, ok = true) {
    state.text = text;
    state.ok = ok;
    // 先清旧定时器：连续消息时旧 timer 不会提前清空新消息
    clearTimeout(timer);
    timer = setTimeout(() => {
      state.text = '';
    }, durationMs);
  }

  return { state, show };
}
