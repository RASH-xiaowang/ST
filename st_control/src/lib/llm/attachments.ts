/* ============================================================
 * 大模型对话 — 多模态附件转换/持久化
 * 自 GlobalChatTab.svelte 下沉：File → Attachment（图片/文本/文件），
 * 行为保持原样（图片大小上限、文本截断、磁盘持久化）。
 * ============================================================ */
import { llmApi } from './services/ipc';
import type { ContentPart } from './types';

/** 多模态附件（图片 data URL / 文本内容 / 普通文件） */
export type Attachment = {
  id: string;
  name: string;
  mime: string;
  kind: "image" | "file" | "text";
  url?: string; // 图片 data URL
  text?: string; // 文本文件内容
  tooBig?: boolean;
  savedPath?: string; // 持久化到 st_result 的文件路径
};

/** Attachment[] → ContentPart[]（多模态消息载荷；自 GlobalChatTab 下沉） */
export function attachmentsToParts(attachments: Attachment[]): ContentPart[] {
  const parts: ContentPart[] = [];
  for (const a of attachments) {
    if (a.kind === "image" && a.url) {
      parts.push({ type: "image_url", image_url: { url: a.url }, file_path: a.savedPath });
    } else if (a.kind === "text" && a.text != null) {
      parts.push({ type: "text", text: a.text, name: a.name, file_path: a.savedPath });
    } else {
      parts.push({ type: "file", name: a.name, mime: a.mime, file_path: a.savedPath });
    }
  }
  return parts;
}

/** 文本类文件扩展名（代码/文档直接内联供模型读取） */
export const TEXT_FILE_EXT_RE =
  /\.(txt|md|json|csv|log|xml|html|js|ts|py|rs|go|java|c|cpp|h|sh|yml|yaml|toml|srt|vtt)$/i;

/** 图片大小上限（默认 8MB，与原组件常量一致） */
export const MAX_IMAGE_BYTES = 8 * 1024 * 1024;

function readAsDataURL(file: File): Promise<string> {
  return new Promise((res, rej) => {
    const r = new FileReader();
    r.onload = () => res(r.result as string);
    r.onerror = () => rej(r.error);
    r.readAsDataURL(file);
  });
}

function readAsText(file: File): Promise<string> {
  return new Promise((res, rej) => {
    const r = new FileReader();
    r.onload = () => res(r.result as string);
    r.onerror = () => rej(r.error);
    r.readAsText(file);
  });
}

/**
 * File → Attachment（图片走 data URL + 持久化；文本内联；其他仅保存磁盘）。
 * @param nextId 附件 ID 生成器（原组件 attSeq 语义，由调用方持有保证单调递增）
 */
export async function fileToAttachment(
  file: File,
  nextId: () => string,
  maxImageBytes = MAX_IMAGE_BYTES,
): Promise<Attachment> {
  const base: Attachment = {
    id: nextId(),
    name: file.name,
    mime: file.type || "application/octet-stream",
    kind: "file",
  };
  if (file.type.startsWith("image/")) {
    if (file.size > maxImageBytes) {
      return { ...base, kind: "image", tooBig: true };
    }
    try {
      base.url = await readAsDataURL(file);
      base.kind = "image";
      // 保存文件到持久目录（用于聊天记录恢复）
      const buf = await file.arrayBuffer();
      base.savedPath = await llmApi.saveUploadedFile(file.name, new Uint8Array(buf));
    } catch {
      /* 保留为 file */
    }
    return base;
  }
  // 文本类（代码/文档）：直接内联文本供模型读取
  if (file.type.startsWith("text/") || TEXT_FILE_EXT_RE.test(file.name)) {
    try {
      base.text = (await readAsText(file)).slice(0, 60000);
      base.kind = "text";
      // 文本文件也同样持久化
      const buf = await file.arrayBuffer();
      base.savedPath = await llmApi.saveUploadedFile(file.name, new Uint8Array(buf));
    } catch {
      /* 保留为 file */
    }
    return base;
  }
  // 其他文件：仅保存到磁盘，附带元信息
  try {
    const buf = await file.arrayBuffer();
    base.savedPath = await llmApi.saveUploadedFile(file.name, new Uint8Array(buf));
  } catch {
    /* 保存失败仍允许发送（仅传元数据） */
  }
  return base;
}
