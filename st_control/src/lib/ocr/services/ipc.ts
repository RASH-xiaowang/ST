// 图文识别（OCR）— Tauri IPC 封装层
// 组件层统一通过本模块调用后端，避免直接 invoke。
import { invoke } from '@tauri-apps/api/core';
import type { OcrConfig, OcrResource, OcrStats } from '../types';

export const ocrApi = {
  getConfig: () => invoke<OcrConfig>('ocr_get_config'),
  getStats: () => invoke<OcrStats>('ocr_get_stats'),
  listResources: (params: {
    page: number;
    pageSize: number;
    status?: string | null;
    category?: string | null;
    keyword?: string | null;
  }) => invoke<{ total: number; items: OcrResource[] }>('ocr_list_resources', params),
  getResource: (id: number) => invoke<OcrResource>('ocr_get_resource', { id }),
  simulateTest: (testIndex: number) => invoke<number>('ocr_simulate_test', { index: testIndex }),
  deleteResource: (id: number) => invoke<void>('ocr_delete_resource', { id }),
  retryResource: (id: number) => invoke<void>('ocr_retry_resource', { id }),
  setConfig: (config: OcrConfig) => invoke<void>('ocr_set_config', { config }),
  ingestLocalFiles: (paths: string[]) => invoke<number>('ocr_ingest_local_files', { paths }),
  updateResourceFields: (id: number, fields: string) =>
    invoke<void>('ocr_update_resource_fields', { id, fields }),
  exportCsv: () => invoke<{ path: string; filename: string; count: number }>('ocr_export_csv'),
};
