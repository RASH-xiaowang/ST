/* ============================================================
 * 消息通道 — 发送步骤状态机（纯函数）
 * 自 BotPanel.svelte 下沉：步骤 UI 状态推导，可独立单测。
 * ============================================================ */

export type TraceMode = 'idle' | 'text' | 'media';
export type SendStage = 'idle' | 'preparing' | 'uploading' | 'sending' | 'done' | 'error';
export type StepState = 'pending' | 'active' | 'done' | 'error';
export type StepKey = 'prep' | 'upload' | 'send';

/**
 * 推导发送步骤 UI 状态：
 * - 非 media 模式仅展示「送达」步骤（done/pending）
 * - media 模式按 prep → upload → send 顺序推进；错误时定位失败步骤
 */
export function stepState(
  key: StepKey,
  traceMode: TraceMode,
  sendStage: SendStage,
  sendError: string,
): StepState {
  if (traceMode !== 'media') {
    return key === 'send' ? (sendStage === 'done' ? 'done' : 'pending') : 'pending';
  }
  if (sendStage === 'idle' || sendStage === 'preparing') {
    if (key === 'prep') return sendStage === 'preparing' ? 'active' : 'pending';
    return 'pending';
  }
  if (sendStage === 'error') {
    const failed = sendError.includes('读取文件')
      ? 'prep'
      : sendError.includes('获取上传地址') || sendError.includes('CDN 上传') || sendError.includes('上传')
        ? 'upload'
        : 'send';
    const order: StepKey[] = ['prep', 'upload', 'send'];
    const failIdx = order.indexOf(failed);
    const idx = order.indexOf(key);
    if (idx === failIdx) return 'error';
    return idx < failIdx ? 'done' : 'pending';
  }
  const order: StepKey[] = ['prep', 'upload', 'send'];
  const idx = order.indexOf(key);
  // done 时定位到末步骤使全部显示完成（原实现遗漏此分支，upload/send 会误显 pending）
  const activeKey: StepKey =
    sendStage === 'done'
      ? 'send'
      : sendStage === 'uploading'
        ? 'upload'
        : sendStage === 'sending'
          ? 'send'
          : 'prep';
  const activeIdx = order.indexOf(activeKey);
  if (idx < activeIdx) return 'done';
  if (idx === activeIdx) return sendStage === 'done' ? 'done' : 'active';
  return 'pending';
}
