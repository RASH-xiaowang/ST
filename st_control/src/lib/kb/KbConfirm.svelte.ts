// 知识库全局确认弹窗状态（Svelte 5 rune 模块）
// 替代原生 confirm()，提供统一的自定义确认弹窗。

interface ConfirmOptions {
  title?: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
  danger?: boolean;
}

let _resolve: ((v: boolean) => void) | null = null;

export const confirmState = $state<{
  open: boolean;
  title: string;
  message: string;
  confirmText: string;
  cancelText: string;
  danger: boolean;
}>({
  open: false,
  title: '确认操作',
  message: '',
  confirmText: '确认',
  cancelText: '取消',
  danger: false,
});

/** 替代原生 confirm()，返回 Promise<boolean> */
export function kbConfirm(opts: ConfirmOptions): Promise<boolean> {
  // 若前一个确认弹窗尚未关闭，先以 false 结束它（避免 promise 泄漏和 UI 卡死）
  if (_resolve) {
    _resolve(false);
    _resolve = null;
  }
  confirmState.open = true;
  confirmState.title = opts.title ?? '确认操作';
  confirmState.message = opts.message;
  confirmState.confirmText = opts.confirmText ?? '确认';
  confirmState.cancelText = opts.cancelText ?? '取消';
  confirmState.danger = opts.danger ?? false;
  return new Promise<boolean>((resolve) => {
    _resolve = resolve;
  });
}

export function confirmOk() {
  confirmState.open = false;
  _resolve?.(true);
  _resolve = null;
}

export function confirmCancel() {
  confirmState.open = false;
  _resolve?.(false);
  _resolve = null;
}
