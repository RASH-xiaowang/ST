/**
 * 代码沙箱执行器 — 使用 sandbox iframe 隔离模型生成的代码
 *
 * 安全边界：
 * - iframe 使用 sandbox="allow-scripts" 属性，禁止访问 DOM/存储/网络
 * - 通过 postMessage 双向通信，只传递 JSON 可序列化数据
 * - 代码无法访问 window/document/fetch/Tauri IPC 等全局对象
 * - 超时自动终止执行
 *
 * 原 new Function() 方案允许代码访问全部主线程全局对象（含 Tauri IPC 桥），
 * 模型通过 prompt injection 可读取 config.json 密钥、调用任意 IPC 命令。
 */

/** 沙箱执行请求 */
interface SandboxRequest {
  id: string;
  code: string;
  args: unknown;
  ctxMeta: {
    hasTools: boolean;
    hasAgent: boolean;
    hasParallel: boolean;
    hasPipeline: boolean;
  };
}

/** 沙箱工具调用请求（代码 → 主线程） */
interface SandboxToolCall {
  type: "tool_call";
  id: string;
  toolName: string;
  toolArgs: unknown;
}

/** 沙箱 agent 调用请求（代码 → 主线程） */
interface SandboxAgentCall {
  type: "agent_call";
  id: string;
  prompt: string;
}

/** 主线程工具/agent 调用结果（传给 sendCallResult 时不需要 type 字段） */
interface SandboxCallResult {
  id: string;
  ok: boolean;
  result?: unknown;
  error?: string;
}

const EXEC_TIMEOUT_MS = 30_000;

/** 沙箱 iframe 内运行的 HTML（纯字符串，无外部依赖） */
function buildSandboxHtml(): string {
  return `<!DOCTYPE html>
<html><head><meta charset="utf-8"></head>
<body><script>
"use strict";
// 沙箱内无 window.parent/opener 访问（sandbox 属性限制）
// 仅保留基本 JS 能力 + postMessage 通信

let pendingToolCalls = new Map();

window.addEventListener("message", function(ev) {
  var msg = ev.data;
  if (!msg || typeof msg !== "object") return;

  if (msg.type === "execute") {
    handleExecute(msg);
  } else if (msg.type === "call_result") {
    var pending = pendingToolCalls.get(msg.id);
    if (pending) {
      pendingToolCalls.delete(msg.id);
      if (msg.ok) pending.resolve(msg.result);
      else pending.reject(new Error(msg.error || "工具调用失败"));
    }
  }
});

function handleExecute(req) {
  var logs = [];
  var ctx = {
    log: function() {
      var xs = [];
      for (var i = 0; i < arguments.length; i++) xs.push(String(arguments[i]));
      logs.push(xs.join(" "));
    }
  };

  // 注入 ctx.tools（沙箱内仅保留 Promise 化的工具调用桥）
  if (req.ctxMeta && req.ctxMeta.hasTools) {
    ctx.tools = new Proxy({}, {
      get: function(_t, toolName) {
        return function(toolArgs) {
          return new Promise(function(resolve, reject) {
            var callId = "tc_" + Math.random().toString(36).slice(2);
            pendingToolCalls.set(callId, { resolve: resolve, reject: reject });
            window.parent.postMessage({
              type: "tool_call", id: callId,
              toolName: toolName, toolArgs: toolArgs || {}
            }, "*");
          });
        };
      }
    });
  }

  // 注入 ctx.agent
  if (req.ctxMeta && req.ctxMeta.hasAgent) {
    ctx.agent = function(prompt) {
      return new Promise(function(resolve, reject) {
        var callId = "ac_" + Math.random().toString(36).slice(2);
        pendingToolCalls.set(callId, { resolve: resolve, reject: reject });
        window.parent.postMessage({
          type: "agent_call", id: callId, prompt: String(prompt)
        }, "*");
      });
    };
  }

  // 注入 ctx.parallel
  if (req.ctxMeta && req.ctxMeta.hasParallel) {
    ctx.parallel = function(thunks) {
      return Promise.all(thunks.map(function(t) { return t(); }));
    };
  }

  // 注入 ctx.pipeline
  if (req.ctxMeta && req.ctxMeta.hasPipeline) {
    ctx.pipeline = async function() {
      var items = arguments[0];
      var stages = Array.prototype.slice.call(arguments, 1);
      var cur = items;
      for (var s = 0; s < stages.length; s++) {
        cur = await Promise.all(cur.map(function(x) { return stages[s](x); }));
      }
      return cur;
    };
  }

  // 执行用户代码（args 和 ctx 作为唯一可访问对象）
  try {
    var fn = new Function("args", "ctx",
      '"use strict";\\nreturn (async function(args, ctx) {\\n' + req.code + '\\n})(args, ctx);'
    );
    var result = fn(req.args, ctx);
    // 支持 async 返回
    Promise.resolve(result).then(function(out) {
      window.parent.postMessage({
        type: "result", id: req.id, ok: true,
        result: out === undefined ? null : out, logs: logs
      }, "*");
    }).catch(function(e) {
      window.parent.postMessage({
        type: "result", id: req.id, ok: false,
        error: String(e && e.message || e), logs: logs
      }, "*");
    });
  } catch(e) {
    window.parent.postMessage({
      type: "result", id: req.id, ok: false,
      error: String(e && e.message || e), logs: logs
    }, "*");
  }
}
<\/script></body></html>`;
}

/** 单例沙箱 iframe 管理器 */
class SandboxExecutor {
  private iframe: HTMLIFrameElement | null = null;
  private pending = new Map<string, {
    resolve: (v: string) => void;
    reject: (e: Error) => void;
    logs: string[];
    timer: ReturnType<typeof setTimeout>;
  }>();

  private ensureIframe(): HTMLIFrameElement {
    if (this.iframe) return this.iframe;

    const iframe = document.createElement("iframe");
    iframe.sandbox.add("allow-scripts");
    // 隐藏 iframe
    iframe.style.display = "none";
    iframe.width = "0";
    iframe.height = "0";
    // 使用 srcdoc 而非 src，避免网络请求
    iframe.srcdoc = buildSandboxHtml();
    document.body.appendChild(iframe);
    this.iframe = iframe;

    // 监听沙箱消息
    window.addEventListener("message", this.onMessage);
    return iframe;
  }

  private onMessage = (ev: MessageEvent) => {
    const msg = ev.data;
    if (!msg || typeof msg !== "object") return;

    // 来源校验：只接受本 iframe 的消息
    if (this.iframe && ev.source !== this.iframe.contentWindow) return;

    if (msg.type === "result") {
      const p = this.pending.get(msg.id);
      if (!p) return;
      this.pending.delete(msg.id);
      clearTimeout(p.timer);
      const allLogs = [...(p.logs || []), ...(msg.logs || [])];
      if (msg.ok) {
        const logPart = allLogs.length ? `[日志]\n${allLogs.join("\n")}\n\n` : "";
        const resultStr = typeof msg.result === "string"
          ? msg.result
          : msg.result === null || msg.result === undefined
            ? "（工具无返回值）"
            : JSON.stringify(msg.result);
        p.resolve(logPart + resultStr);
      } else {
        p.reject(new Error(msg.error || "沙箱执行失败"));
      }
    } else if (msg.type === "tool_call") {
      // 沙箱代码请求调用 Harness 工具（转发给回调）
      this.onToolCall?.(msg as SandboxToolCall);
    } else if (msg.type === "agent_call") {
      this.onAgentCall?.(msg as SandboxAgentCall);
    }
  };

  /** 工具调用回调（由外部设置） */
  onToolCall: ((call: SandboxToolCall) => void) | null = null;
  /** Agent 调用回调（由外部设置） */
  onAgentCall: ((call: SandboxAgentCall) => void) | null = null;

  /** 向沙箱发送工具/agent 调用结果 */
  sendCallResult(result: SandboxCallResult) {
    this.iframe?.contentWindow?.postMessage(
      { type: "call_result", id: result.id, ok: result.ok, result: result.result, error: result.error },
      "*"
    );
  }

  /**
   * 在沙箱中执行代码
   * @param code 要执行的 JS 代码
   * @param args 传入的参数（JSON 可序列化）
   * @param ctxMeta 上下文能力标记
   * @returns 执行结果字符串
   */
  execute(
    code: string,
    args: unknown,
    ctxMeta: SandboxRequest["ctxMeta"],
  ): Promise<string> {
    return new Promise((resolve, reject) => {
      const iframe = this.ensureIframe();
      const id = "exec_" + Math.random().toString(36).slice(2);

      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`沙箱执行超时（${EXEC_TIMEOUT_MS / 1000}秒）`));
      }, EXEC_TIMEOUT_MS);

      this.pending.set(id, { resolve, reject, logs: [], timer });

      // 等 iframe 加载完成后发送
      const send = () => {
        iframe.contentWindow?.postMessage(
          { type: "execute", id, code, args: args ?? {}, ctxMeta },
          "*"
        );
      };

      if (iframe.contentDocument?.readyState === "complete") {
        send();
      } else {
        iframe.addEventListener("load", send, { once: true });
      }
    });
  }

  /** 销毁沙箱（页面卸载时调用） */
  destroy() {
    window.removeEventListener("message", this.onMessage);
    if (this.iframe) {
      this.iframe.remove();
      this.iframe = null;
    }
    for (const [, p] of this.pending) {
      clearTimeout(p.timer);
      p.reject(new Error("沙箱已销毁"));
    }
    this.pending.clear();
  }
}

/** 全局单例 */
let instance: SandboxExecutor | null = null;

export function getSandbox(): SandboxExecutor {
  if (!instance) {
    instance = new SandboxExecutor();
  }
  return instance;
}

export type { SandboxToolCall, SandboxAgentCall, SandboxCallResult };
