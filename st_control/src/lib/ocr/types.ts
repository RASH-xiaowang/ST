// 图文识别（OCR）— 前端类型定义（与后端 ocr 模块字段保持一致）

export interface EndpointRule {
  enabled: boolean;
  endpoint: string;
}

export interface OcrConfig {
  appId: string;
  secretCode: string;
  enabled: boolean;
  bindHost: string;
  port: number;
  token: string;
  /** 是否先使用开源 OCR（RapidOCR）预检，识别出有效文本才调用证件分类 */
  precheckEnabled: boolean;
  /** 预检文本最小字符数（低于该值视为无有效文本，跳过证件分类） */
  precheckMinChars: number;
  /** RapidOCR 模型缓存目录（空 = 默认应用数据目录） */
  precheckModelDir: string;
  endpointMap: Record<string, EndpointRule>;
}

export interface OcrResource {
  id: number;
  senderUsername: string;
  sessionType: string;
  timestamp: string;
  username: string;
  mediaUrl: string;
  mediaPath: string;
  category: string;
  categoryDesc: string;
  status: string;
  error: string;
  /** 开源 OCR 预检识别出的文本（未识别到时为空） */
  precheckText: string;
  classifyRaw: string;
  ocrRaw: string;
  ocrFields: string;
  createdAt: string;
  updatedAt: string;
}

export interface OcrStats {
  total: number;
  byStatus: Record<string, number>;
  byCategory: Record<string, number>;
}
