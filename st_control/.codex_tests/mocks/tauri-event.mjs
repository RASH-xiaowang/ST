// 模拟 @tauri-apps/api/event：记录注册的监听器，并支持测试代码主动触发事件
export const __handlers = new Map();

export async function listen(event, handler) {
  __handlers.set(event, handler);
  return () => {
    __handlers.delete(event);
  };
}

export async function __fire(event, payload) {
  const h = __handlers.get(event);
  if (h) {
    await h({ event, payload });
  }
}
