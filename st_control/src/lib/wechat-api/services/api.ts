/**
 * 微信 API 核心服务层
 * 从 viewapi/core.js 迁入，适配 TypeScript
 */
import type { ApiResponse, ApiLogEntry } from '../types';

const BASE_URL = 'http://api.wechatapi.net/finder/v2/api';

// ---------- 运行日志 ----------
const API_LOG_MAX = 2000;
let _apiRuntimeLog: ApiLogEntry[] = [];
let _logListeners: Array<() => void> = [];

export function pushApiRuntimeLog(entry: ApiLogEntry) {
  _apiRuntimeLog.push(entry);
  if (_apiRuntimeLog.length > API_LOG_MAX) {
    _apiRuntimeLog.splice(0, _apiRuntimeLog.length - API_LOG_MAX);
  }
  _logListeners.forEach((fn) => { try { fn(); } catch {} });
}

export function getApiRuntimeLog(): ApiLogEntry[] {
  return _apiRuntimeLog.map((e) => JSON.parse(JSON.stringify(e)));
}

export function clearApiRuntimeLog() {
  _apiRuntimeLog.length = 0;
  _logListeners.forEach((fn) => { try { fn(); } catch {} });
}

export function subscribeApiLog(fn: () => void) {
  if (typeof fn === 'function') _logListeners.push(fn);
  return () => { _logListeners = _logListeners.filter((f) => f !== fn); };
}

// ---------- Token 校验 ----------
const TOKEN_INVALID_PATTERNS = ['不可用', '已过期'];

function isTokenInvalidMessage(msg: string): boolean {
  const text = String(msg || '').trim();
  if (!text) return false;
  return TOKEN_INVALID_PATTERNS.some((part) => text.includes(part));
}

function isTokenInvalidPayload(payload: unknown): boolean {
  if (!payload || typeof payload !== 'object') return false;
  return isTokenInvalidMessage((payload as Record<string, unknown>).msg as string);
}

export async function probeTokenByCheckOnline(tokenValue: string): Promise<{
  ok: boolean;
  invalid: boolean;
  message: string;
  payload: unknown;
}> {
  const token = String(tokenValue || '').trim();
  if (!token) {
    return { ok: false, invalid: false, message: '请先去API管理后台申请TOKEN', payload: null };
  }
  const res = await fetch(BASE_URL + '/login/checkOnline', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', 'VideosApi-token': token },
    body: JSON.stringify({ appId: '' }),
  });
  const text = await res.text();
  let payload: unknown = {};
  try { payload = text ? JSON.parse(text) : {}; } catch { payload = { raw: text }; }
  if (isTokenInvalidPayload(payload)) {
    return { ok: false, invalid: true, message: 'TOKEN不可用或已过期，请先去API管理后台重新申请', payload };
  }
  return { ok: true, invalid: false, message: '', payload };
}

// ---------- 统一 POST 请求 ----------
export async function apiPost<T = unknown>(
  path: string,
  body: Record<string, unknown> = {},
  state: { token: string; appId: string },
): Promise<ApiResponse<T>> {
  const payload = { ...body };
  if (!Object.prototype.hasOwnProperty.call(payload, 'appId')) {
    payload.appId = state.appId ?? '';
  }
  const t0 = Date.now();
  const clonedRawBody = JSON.parse(JSON.stringify(body));
  const clonedMergedBody = JSON.parse(JSON.stringify(payload));
  const logEntry: ApiLogEntry = {
    at: new Date().toISOString(),
    path,
    method: 'POST',
    fullUrl: BASE_URL + path,
    requestHeaders: {
      'Content-Type': 'application/json',
      'VideosApi-token': state.token ? '***已配置***' : '',
    },
    requestBodyRaw: clonedRawBody,
    requestBody: clonedMergedBody,
    responseHttpStatus: null,
    responseBody: null,
    error: null,
    durationMs: null,
  };

  try {
    const res = await fetch(BASE_URL + path, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'VideosApi-token': state.token || '',
      },
      body: JSON.stringify(payload),
    });
    const text = await res.text();
    logEntry.responseHttpStatus = res.status;
    let json: ApiResponse<T>;
    try {
      json = (text ? JSON.parse(text) : {}) as ApiResponse<T>;
    } catch (pe) {
      logEntry.responseBody = { _nonJsonBody: true, rawPreview: text.slice(0, 8000) };
      logEntry.error = '响应 JSON 解析失败: ' + ((pe as Error)?.message || String(pe));
      logEntry.durationMs = Date.now() - t0;
      pushApiRuntimeLog(logEntry);
      throw new Error('响应不是合法 JSON');
    }
    logEntry.responseBody = json;
    logEntry.durationMs = Date.now() - t0;
    pushApiRuntimeLog(logEntry);
    return { ok: res.ok, status: res.status, data: json } as unknown as ApiResponse<T>;
  } catch (e) {
    if (logEntry.durationMs == null) logEntry.durationMs = Date.now() - t0;
    if (logEntry.responseHttpStatus == null && !logEntry.error) {
      logEntry.error = (e as Error)?.message || String(e);
      pushApiRuntimeLog(logEntry);
    }
    throw e;
  }
}

/** ret !== 200 时抛错，成功则返回 data */
export function assertApiOk<T = unknown>(apiResult: ApiResponse<T>): T {
  const body = apiResult?.data as unknown as { ret: number; msg: string; data: T };
  const { ret, msg, data } = body ?? {};
  if (ret !== 200) {
    const err = new Error(msg || `接口异常 ret=${ret}`);
    (err as Error & { payload: unknown }).payload = body;
    throw err;
  }
  return data;
}

export { BASE_URL, isTokenInvalidPayload };
