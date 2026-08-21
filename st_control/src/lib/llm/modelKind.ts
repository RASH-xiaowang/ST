/* ============================================================
 * 大模型 — 模型能力分类纯函数
 * 自 GlobalChatTab.svelte 下沉：model_type 文本 → 能力类别/发送文案。
 * ============================================================ */

/** LLM 模型能力类别 */
export type ModelKind = 'chat' | 'image' | 'video' | 'speech' | 'embed' | 'rerank';

/** 按后端 model_type 文本分类（未知/缺失视为对话模型） */
export function classifyModelType(
  modelType: string | null | undefined,
): ModelKind {
  switch (modelType) {
    case '生图':
      return 'image';
    case '视频':
      return 'video';
    case '语音':
      return 'speech';
    case '嵌入':
      return 'embed';
    case '重排序':
      return 'rerank';
    default:
      return 'chat';
  }
}

/** 发送按钮文案（按能力类别） */
export function modelSendLabel(kind: ModelKind): string {
  switch (kind) {
    case 'image':
    case 'video':
    case 'embed':
      return '生成';
    case 'speech':
      return '合成';
    case 'rerank':
      return '排序';
    case 'chat':
      return '发送';
  }
}
