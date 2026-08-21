<script lang="ts">
  import { errText } from '../../format';
  import { onMount } from "svelte";
  import { llmApi } from "../services/ipc";
  import { classifyModelType, modelSendLabel } from "../modelKind";
  import { filterByAnyKeyword } from "../../utils/filter";
  import { lsGet, lsSet } from "../../storage";
  import type {
    LlmConfig,
    ChatMessage,
    ChatRequest,
    ChatResult,
    ProviderConfig,
    AiRole,
    SpeechResult,
    AgentStreamEvent,
    AgentToolInfo,
    AgentPlugin,
    AgentStepRecord,
  } from "../types";
  import MessageBody from "./MessageBody.svelte";
  import ModelSelect from "./ModelSelect.svelte";
  import BotIcon from "@lucide/svelte/icons/bot";
  import CheckIcon from "@lucide/svelte/icons/check";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import EraserIcon from "@lucide/svelte/icons/eraser";
  import FileTextIcon from "@lucide/svelte/icons/file-text";
  import MicIcon from "@lucide/svelte/icons/mic";
  import PaperclipIcon from "@lucide/svelte/icons/paperclip";
  import RefreshCwIcon from "@lucide/svelte/icons/refresh-cw";
  import SendIcon from "@lucide/svelte/icons/send";
  import SlidersHorizontalIcon from "@lucide/svelte/icons/sliders-horizontal";
  import SparklesIcon from "@lucide/svelte/icons/sparkles";
  import SquareIcon from "@lucide/svelte/icons/square";
  import Volume2Icon from "@lucide/svelte/icons/volume-2";
  import WrenchIcon from "@lucide/svelte/icons/wrench";
  import ShieldAlertIcon from "@lucide/svelte/icons/shield-alert";
  import PuzzleIcon from "@lucide/svelte/icons/puzzle";
  import XIcon from "@lucide/svelte/icons/x";
  import { listen } from "@tauri-apps/api/event";
  import { RippleButton } from "fancy-ui-svelte";
  import { NativeSelect, NativeSelectOption } from "../../components/ui/native-select";
  import {
    audioMime,
    blobToWav16kMono,
    buildSpeechAttempts,
    plainTextForSpeech,
    rmsLevel,
  } from "../services/voice";
  import {
    releaseVoiceRecorder,
    startVoiceRecorder,
    stopVoiceRecorder,
    voiceRecorder,
  } from "../services/voiceRecorder.svelte";
  import {
    playTtsAudio,
    setTtsPlayerHooks,
    stopTtsPlayer,
    ttsDataUrl,
    ttsPlayer,
  } from "../services/ttsPlayer.svelte";
  import {
    setSpeechSynthHooks,
    speechSynth,
    synthOneSpeech,
    type SpeechChunk,
  } from "../services/speechSynth.svelte";
  import {
    drainSpeechFlow,
    feedStreamSpeech,
    finishStreamSpeech,
    isCurrentSpeechSession,
    resetSpeechFlow,
    speechFlow,
    speechSessionId,
  } from "../services/speechFlow.svelte";
  import { getLocalSttStatus } from "../../wechat/services/ipc";
  import { trimContext, TRIMMED_CONTEXT_NOTE } from "../chatContext";
  import { attachmentsToParts, fileToAttachment, type Attachment } from "../attachments";
  import { composeSystemPrompt } from "../roleUtils";

  let { config }: { config: LlmConfig } = $props();

  let selectedId = $state("");
  let selected = $state<ProviderConfig | null>(null);
  let modelId = $state("");
  let messages = $state<ChatMessage[]>([]);
  let ctxTrimmed = $state(false);
  let input = $state("");
  let sending = $state(false);
  let lastResult = $state<ChatResult | null>(null);
  let error = $state("");
  let statusMsg = $state("");
  let loadingHistory = $state(false);
  let chatWindow = $state<HTMLDivElement | null>(null);

  // ─── 代理模式（工具调用，DeepSeek Harness 能力迁移） ───
  let agentMode = $state(lsGet("st.agentMode") === "1");
  let agentTools = $state<AgentToolInfo[]>([]);
  /** 当前回合的实时工具步骤（与持久化的 AgentStepRecord 同构） */
  let agentSteps = $state<AgentStepRecord[]>([]);
  let pendingApprovals = $state<
    { id: string; tool: string; description: string; arguments?: string }[]
  >([]);
  /** 当前展开详情面板的工具步骤 id（再点收起） */
  let expandedStep = $state<string | null>(null);
  let retryingStep = $state<string | null>(null);
  let copiedText = $state("");
  const agentToolsTitle = $derived(
    agentTools.length > 0
      ? "代理模式：模型可调用以下工具\n" +
          agentTools.map((t) => `· ${t.name}${t.requires_approval ? "（需审批）" : ""}`).join("\n")
      : "代理模式：模型可调用本地工具（联网搜索 / 知识库检索 / 文件读写 / 命令执行）",
  );
  $effect(() => {
    lsSet("st.agentMode", agentMode ? "1" : "0");
  });

  /** 格式化执行耗时（毫秒 → 123ms / 1.2s） */
  function fmtDuration(ms?: number): string {
    if (ms == null) return "";
    return ms >= 1000 ? `${(ms / 1000).toFixed(1)}s` : `${Math.round(ms)}ms`;
  }

  /** 参数/结果美化：JSON 可解析时缩进展示，否则原样返回 */
  function prettyText(s?: string): string {
    if (!s) return "";
    try {
      const v = JSON.parse(s);
      return JSON.stringify(v, null, 2);
    } catch {
      return s;
    }
  }

  /** 该工具名是否为插件工具（支持失败重试） */
  function isPluginTool(name: string): boolean {
    return agentPlugins.some((p) => p.tools.some((t) => t.name === name));
  }

  async function copyStepText(text: string) {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      /* 剪贴板不可用时静默忽略（WebView2 权限策略） */
    }
    copiedText = text.slice(0, 20);
    window.setTimeout(() => {
      if (copiedText === text.slice(0, 20)) copiedText = "";
    }, 1500);
  }

  /** 重试一个失败的插件工具（仅本地重跑查看结果，不回传模型） */
  async function retryStep(s: AgentStepRecord, msgIndex: number | null) {
    if (retryingStep) return;
    // 实时拉取插件列表，避免内存态滞后（插件可能在其他入口新建/更新）
    let plugins = agentPlugins;
    try {
      plugins = await llmApi.listAgentPlugins();
    } catch {
      /* 拉取失败时回退内存态 */
    }
    const tool = plugins.flatMap((p) => p.tools).find((t) => t.name === s.name);
    if (!tool) return;
    retryingStep = s.id;
    const t0 = performance.now();
    let ok = true;
    let result = "";
    const logs: string[] = [];
    try {
      const ctx = {
        fetch: (input: RequestInfo | URL, init?: RequestInit) => fetch(input, init),
        log: (...xs: unknown[]) => {
          logs.push(xs.map(String).join(" "));
        },
      };
      const argsObj = JSON.parse(s.args || "{}");
      const fn = new Function(
        "args",
        "ctx",
        `"use strict";\nreturn (async function(args, ctx) {\n${tool.code}\n})(args, ctx);`,
      );
      const out = await fn(argsObj, ctx);
      const logPart = logs.length ? `[日志]\n${logs.join("\n")}\n\n` : "";
      result =
        logPart +
        (typeof out === "string"
          ? out
          : out === undefined
            ? "（工具无返回值）"
            : JSON.stringify(out, null, 2));
    } catch (e: unknown) {
      ok = false;
      result = errText(e) || String(e);
    }
    const patch = {
      status: ok ? ("ok" as const) : ("err" as const),
      result,
      duration_ms: Math.round(performance.now() - t0),
      retried: true,
    };
    if (msgIndex === null) {
      // 当前回合的实时步骤
      agentSteps = agentSteps.map((x) => (x.id === s.id ? { ...x, ...patch } : x));
    } else {
      // 历史步骤：更新消息并重新落盘
      messages = messages.map((m, i) => {
        if (i !== msgIndex || !m.tool_steps) return m;
        return {
          ...m,
          tool_steps: m.tool_steps.map((x) => (x.id === s.id ? { ...x, ...patch } : x)),
        };
      });
      const msg = messages[msgIndex];
      llmApi
        .saveAgentToolSteps(selectedId, modelId, assistantSeqOf(msgIndex), msg.tool_steps ?? [])
        .catch(() => {});
    }
    retryingStep = null;
  }

  /** 计算 messages 中第 msgIndex 条消息的助手序号（用于步骤落盘） */
  function assistantSeqOf(msgIndex: number): number {
    let seq = 0;
    for (let i = 0; i < msgIndex; i++) {
      if (messages[i].role === "assistant") seq++;
    }
    return seq;
  }

  async function approveAgent(id: string, remember = false) {
    const a = pendingApprovals.find((x) => x.id === id);
    if (remember && a && selectedId && modelId) {
      try {
        await llmApi.trustAgentTool(selectedId, modelId, a.tool);
      } catch {
        /* 信任记录失败不影响审批 */
      }
    }
    pendingApprovals = pendingApprovals.filter((x) => x.id !== id);
    try {
      await llmApi.approveAgentTool(id);
    } catch {
      /* 审批可能已超时 */
    }
  }

  async function rejectAgent(id: string) {
    pendingApprovals = pendingApprovals.filter((a) => a.id !== id);
    try {
      await llmApi.rejectAgentTool(id);
    } catch {
      /* 审批可能已超时 */
    }
  }

  // ─── 动态插件（DSH 插件模型迁移） ───
  let agentPlugins = $state<AgentPlugin[]>([]);
  let pluginDrawerOpen = $state(false);
  let pluginEditing = $state(false);
  let pluginDraft = $state<{
    id: string;
    name: string;
    description: string;
    enabled: boolean;
    toolName: string;
    toolDesc: string;
    toolApproval: boolean;
    toolCode: string;
  } | null>(null);
  let pluginSaving = $state(false);
  let pluginError = $state("");

  async function loadPlugins() {
    try {
      agentPlugins = await llmApi.listAgentPlugins();
    } catch {
      agentPlugins = [];
    }
  }

  function startNewPlugin() {
    pluginDraft = {
      id: "",
      name: "",
      description: "",
      enabled: true,
      toolName: "",
      toolDesc: "",
      toolApproval: false,
      toolCode: "// 函数体（async）：args 为工具参数对象，ctx.fetch / ctx.log 可用\n// 返回值：字符串（或可 JSON 序列化的对象）\nreturn String(args.expression);",
    };
    pluginEditing = true;
    pluginError = "";
  }

  function startEditPlugin(p: AgentPlugin) {
    const t = p.tools[0];
    pluginDraft = {
      id: p.id,
      name: p.name,
      description: p.description,
      enabled: p.enabled,
      toolName: t?.name ?? "",
      toolDesc: t?.description ?? "",
      toolApproval: t?.requires_approval ?? false,
      toolCode: t?.code ?? "",
    };
    pluginEditing = true;
    pluginError = "";
  }

  async function savePluginDraft() {
    if (!pluginDraft) return;
    if (!pluginDraft.name.trim() || !pluginDraft.toolName.trim() || !pluginDraft.toolCode.trim()) {
      pluginError = "请填写插件名称、工具名与工具代码";
      return;
    }
    pluginSaving = true;
    pluginError = "";
    try {
      const p: AgentPlugin = {
        id: pluginDraft.id,
        name: pluginDraft.name.trim(),
        description: pluginDraft.description.trim(),
        enabled: pluginDraft.enabled,
        tools: [
          {
            name: pluginDraft.toolName.trim(),
            description: pluginDraft.toolDesc.trim() || pluginDraft.toolName.trim(),
            parameters: {
              type: "object",
              properties: {},
            },
            requires_approval: pluginDraft.toolApproval,
            code: pluginDraft.toolCode,
          },
        ],
        versions: [],
        created_at: "",
        updated_at: "",
      };
      await llmApi.saveAgentPlugin(p);
      pluginEditing = false;
      pluginDraft = null;
      await loadPlugins();
    } catch (e: unknown) {
      pluginError = errText(e);
    } finally {
      pluginSaving = false;
    }
  }

  async function togglePluginEnabled(p: AgentPlugin) {
    try {
      await llmApi.setAgentPluginEnabled(p.id, !p.enabled);
      await loadPlugins();
    } catch (e: unknown) {
      pluginError = errText(e);
    }
  }

  async function deletePlugin(p: AgentPlugin) {
    try {
      await llmApi.deleteAgentPlugin(p.id);
      await loadPlugins();
    } catch (e: unknown) {
      pluginError = errText(e);
    }
  }

  /** 执行插件工具（后端请求 → 前端 WebView 运行 JS → 回传结果） */
  async function execPluginTool(payload: {
    id: string;
    name: string;
    args: string;
    code: string;
  }) {
    let ok = true;
    let result = "";
    const logs: string[] = [];
    try {
      const ctx = {
        fetch: (input: RequestInfo | URL, init?: RequestInit) => fetch(input, init),
        log: (...xs: unknown[]) => {
          logs.push(xs.map(String).join(" "));
        },
      };
      const argsObj = JSON.parse(payload.args || "{}");
      const fn = new Function(
        "args",
        "ctx",
        `"use strict";\nreturn (async function(args, ctx) {\n${payload.code}\n})(args, ctx);`,
      );
      const out = await fn(argsObj, ctx);
      const logPart = logs.length ? `[日志]\n${logs.join("\n")}\n\n` : "";
      result =
        logPart +
        (typeof out === "string"
          ? out
          : out === undefined
            ? "（工具无返回值）"
            : JSON.stringify(out));
    } catch (e: unknown) {
      ok = false;
      result = errText(e) || String(e);
    }
    await llmApi.submitAgentToolResult(payload.id, ok, result).catch(() => {});
  }

  // 语音合成（TTS）参数（音色/语速记住用户偏好）；安全读写见共享 storage.ts
  // 音色列表：硅基流动 CosyVoice2（OpenAI 兼容 /audio/speech，实测可用
  // 的音色子集），支持语速 speed 参数——自然度远高于系统 SAPI 兜底
  const SPEECH_VOICES: { value: string; label: string }[] = [
    { value: "FunAudioLLM/CosyVoice2-0.5B:anna", label: "anna · 女声 · 温柔" },
    { value: "FunAudioLLM/CosyVoice2-0.5B:bella", label: "bella · 女声 · 清亮" },
    { value: "FunAudioLLM/CosyVoice2-0.5B:alex", label: "alex · 男声 · 沉稳" },
    { value: "FunAudioLLM/CosyVoice2-0.5B:benjamin", label: "benjamin · 男声 · 磁性" },
    { value: "FunAudioLLM/CosyVoice2-0.5B:charles", label: "charles · 男声 · 成熟" },
    { value: "FunAudioLLM/CosyVoice2-0.5B:david", label: "david · 男声 · 阳光" },
  ];
  const SPEECH_VOICE_DEFAULT = SPEECH_VOICES[0].value;
  let speechVoice = $state(lsGet("st.speechVoice") || SPEECH_VOICE_DEFAULT);
  // 语速倍率（字符串态便于 select 绑定；调用时 Number() 转换）
  let speechSpeed = $state(lsGet("st.speechSpeed") || "1.0");
  let speechFormat = $state("mp3");
  // 旧版存储的 OpenAI 音色（alloy 等）在 CosyVoice2 下不可用 → 自动迁移默认音色
  $effect(() => {
    if (!SPEECH_VOICES.some((v) => v.value === speechVoice)) {
      speechVoice = SPEECH_VOICE_DEFAULT;
    }
  });
  // 历史存储的语速值可能不在选项列表内（如 "1"）→ 归一化到默认档
  $effect(() => {
    if (!["0.75", "0.9", "1.0", "1.15", "1.3"].includes(speechSpeed)) {
      speechSpeed = "1.0";
    }
  });
  // ─── 语音对话模式 ───
  let voiceMode = $state(false);
  let voiceReply = $state(lsGet("st.voiceReply") !== "0");
  let voiceLoop = $state(lsGet("st.voiceLoop") !== "0");
  let voiceCfgOpen = $state(false);
  let voiceStatus = $state("");
  let micError = $state("");
  let mediaStream = $state<MediaStream | null>(null);
  let audioCtxRef: AudioContext | null = null;
  let bargeTimer: number | null = null;
  let bargeSource: MediaStreamAudioSourceNode | null = null;
  let bargeHits = 0;
  let sttEngine = $state("云端转写");
  let ttsEngine = $state("");
  let pendingVoiceReply = $state(false);
  // ─── 流式语音回复（边生成边说）───
  // 重排序查询语句（文档使用主输入框按行填写）
  let rerankQuery = $state("");

  // ─── AI 角色（跨模块：由 Agent 模块定义，经此处检索并调用）───
  let aiRoles = $state<AiRole[]>([]);
  let roleSearch = $state("");
  let selectedRole = $state<AiRole | null>(null);
  let roleDrawerOpen = $state(false);

  async function loadAiRoles() {
    try {
      aiRoles = await llmApi.getAiRoles();
    } catch {
      aiRoles = [];
    }
  }

  function filteredAiRoles(): AiRole[] {
    const base = aiRoles.filter((r) => r.enabled);
    return filterByAnyKeyword(
      base,
      roleSearch,
      (r) => r.name || "",
      (r) => r.description || "",
      (r) => r.capabilities || [],
      (r) => r.system_prompt || "",
    );
  }

  function applyRole(role: AiRole) {
    selectedRole = role;
    roleDrawerOpen = false;
    // 若角色指定了偏好提供方 / 模型，则尝试在全局调用中切换
    if (role.preferred_provider_name) {
      const hit = config.providers.find(
        (p) => p.name.toLowerCase() === role.preferred_provider_name!.toLowerCase()
      );
      if (hit) selectedId = hit.id;
    }
    if (role.preferred_model) modelId = role.preferred_model;
  }

  function clearRole() {
    selectedRole = null;
  }

  // 组合系统提示词（与 Agent 模块 RoleManager 预览逻辑一致）
  onMount(() => {
    loadAiRoles();
    // 代理模式：工具目录 + 审批事件监听
    llmApi
      .getAgentTools()
      .then((t) => (agentTools = t))
      .catch(() => {});
    let unlistenApproval: (() => void) | null = null;
    listen<{ id: string; tool: string; description: string; arguments?: string }>(
      "agent-approval-requested",
      (e) => {
        pendingApprovals = [...pendingApprovals, e.payload];
      },
    ).then((f) => (unlistenApproval = f));
    // 插件工具执行桥：后端请求 → 前端运行 JS → 回传结果
    let unlistenToolExec: (() => void) | null = null;
    listen<{ id: string; name: string; args: string; code: string }>(
      "agent-tool-exec-request",
      (e) => {
        execPluginTool(e.payload);
      },
    ).then((f) => (unlistenToolExec = f));
    loadPlugins();
    setTtsPlayerHooks({
      onStatus: (text) => (voiceStatus = text),
      onMicError: (text) => (micError = text),
      onBargeStart: () => startBargeInMonitor(),
      onBargeStop: () => stopBargeInMonitor(),
    });
    setSpeechSynthHooks({
      tryProvider: (text) => trySpeech(text),
      // 原生兜底也按用户语速设置 SAPI Rate（-10~10）
      synthesizeNative: (text) =>
        llmApi.synthesizeNativeSpeech(text, Math.round((Number(speechSpeed) - 1) * 10)),
      onEngine: (label) => (ttsEngine = label),
      onError: (text) => (micError = text),
    });
    // 监听 AI 角色面板「使用」事件，实现跨模块角色一键调用
    const handler = (e: CustomEvent<AiRole>) => applyRole(e.detail);
    window.addEventListener("role-selected", handler as EventListener);
    return () => {
      window.removeEventListener("role-selected", handler as EventListener);
      unlistenApproval?.();
      unlistenToolExec?.();
      stopBargeInMonitor();
      abortStreamSpeech(false);
      releaseVoiceRecorder();
      mediaStream?.getTracks().forEach((t) => t.stop());
      audioCtxRef?.close().catch(() => {});
      audioCtxRef = null;
    };
  });

  let attachments = $state<Attachment[]>([]);
  let dragOver = $state(false);
  let fileInput = $state<HTMLInputElement | null>(null);
  let attSeq = 0;

  // ─── 输入框自适应高度 / 消息操作（复制）───
  let textareaEl = $state<HTMLTextAreaElement | null>(null);
  function autoGrow() {
    const el = textareaEl;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = Math.min(el.scrollHeight, 200) + "px";
  }
  $effect(() => {
    input;
    autoGrow();
  });
  let copiedIdx = $state<number | null>(null);
  async function copyMessage(i: number, text: string) {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      // 剪贴板 API 受限（如 WebView2 权限策略）时回退 execCommand
      try {
        const ta = document.createElement("textarea");
        ta.value = text;
        ta.style.position = "fixed";
        ta.style.opacity = "0";
        document.body.appendChild(ta);
        ta.select();
        document.execCommand("copy");
        ta.remove();
      } catch {
        return;
      }
    }
    copiedIdx = i;
    window.setTimeout(() => {
      if (copiedIdx === i) copiedIdx = null;
    }, 1500);
  }

  // 空态推荐问题（点击即填入并发送）
  const SUGGESTIONS = [
    "帮我写一段自我介绍，突出沟通能力",
    "用通俗的语言解释什么是大语言模型",
    "制定一个为期一周的健身计划",
    "写一封请假邮件：明天发烧需要休息",
  ];
  function useSuggestion(s: string) {
    input = s;
    if (canSend) send();
  }

  const modelKind = $derived(
    classifyModelType(selected?.model_meta?.[modelId]?.model_type),
  );
  const isImageGen = $derived(modelKind === "image");
  const isVideoGen = $derived(modelKind === "video");
  const isSpeech = $derived(modelKind === "speech");
  const isEmbed = $derived(modelKind === "embed");
  const isRerank = $derived(modelKind === "rerank");
  // 仅「对话」类型支持附件上传
  const isChat = $derived(modelKind === "chat");

  const sendLabel = $derived(sending ? "生成中…" : modelSendLabel(modelKind));

  const canSend = $derived.by(() => {
    if (sending || !selected) return false;
    if (isRerank) return rerankQuery.trim().length > 0 && input.trim().length > 0;
    if (isImageGen || isVideoGen || isSpeech || isEmbed)
      return input.trim().length > 0;
    return input.trim().length > 0 || attachments.length > 0;
  });
  let imgSize = $state("1024x1024");

  // 自动滚动到底部，保证最新消息始终完整可见
  function scrollToBottom() {
    if (chatWindow) chatWindow.scrollTop = chatWindow.scrollHeight;
  }
  $effect(() => {
    messages;
    scrollToBottom();
  });

  // 切换提供方/模型时加载已持久化的聊天记录
  async function loadHistory() {
    if (!selectedId || !modelId) {
      messages = [];
      attachments = [];
      return;
    }
    loadingHistory = true;
    try {
      const hist = await llmApi.getChatHistory(selectedId, modelId);
      // 实时工具步骤为「当前回合」状态，重载历史时重置，
      // 避免残留的实时面板盖住历史面板
      agentSteps = [];
      pendingApprovals = [];
      expandedStep = null;
      // 工具调用历史：按助手消息序号挂载（代理模式的思考过程随对话恢复）
      try {
        const steps = await llmApi.getAgentToolSteps(selectedId, modelId);
        if (steps.length > 0) {
          const bySeq = new Map(steps.map(([idx, list]) => [idx, list as AgentStepRecord[]]));
          let seq = 0;
          for (const m of hist) {
            if (m.role === "assistant") {
              const list = bySeq.get(seq);
              if (list && list.length > 0) m.tool_steps = list;
              seq++;
            }
          }
        }
      } catch {
        /* 步骤读取失败不影响历史加载 */
      }
      messages = hist;
    } catch (e) {
      logError("loadHistory", e);
    } finally {
      loadingHistory = false;
    }
  }

  // 提供方或模型变化时重新加载对应会话
  $effect(() => {
    selectedId;
    modelId;
    loadHistory();
  });

  // 切换到非对话模型时自动退出语音对话模式，避免残留麦克风占用
  $effect(() => {
    if (!isChat && voiceMode) {
      voiceMode = false;
      abortStreamSpeech(false);
      mediaStream?.getTracks().forEach((t) => t.stop());
      mediaStream = null;
      voiceStatus = "";
    }
  });

  // 记住语音偏好（语音回复 / 连续对话 / 音色）
  $effect(() => {
    lsSet("st.voiceReply", voiceReply ? "1" : "0");
  });
  $effect(() => {
    lsSet("st.voiceLoop", voiceLoop ? "1" : "0");
  });
  $effect(() => {
    lsSet("st.speechVoice", speechVoice);
  });
  $effect(() => {
    lsSet("st.speechSpeed", speechSpeed);
  });

  // 默认使用最后一次聊天的 提供方/模型（若存在），否则用全局默认提供方
  $effect(() => {
    const lastP = config.last_chat_provider_id;
    const lastM = config.last_chat_model;
    if (!selectedId) {
      if (lastP && config.providers.some((p) => p.id === lastP)) {
        selectedId = lastP;
      } else {
        selectedId = config.default_provider_id ?? config.providers[0]?.id ?? "";
      }
    }
    selected = config.providers.find((p) => p.id === selectedId) ?? null;
    if (selected && !modelId) {
      modelId = selectedId === lastP && lastM ? lastM : selected.default_model;
    }
  });

  // 选择变化时记住当前会话，重启后自动恢复
  $effect(() => {
    const pid = selectedId;
    const mid = modelId;
    if (pid && mid) {
      llmApi.setLastChat(pid, mid).catch(() => {});
    }
  });

  // 切换提供方时同步默认模型
  function onProviderChange() {
    selected = config.providers.find((p) => p.id === selectedId) ?? null;
    modelId = selected?.default_model ?? "";
    error = "";
  }

  function logError(ctx: string, e: unknown) {
    console.error(`[GlobalChatTab] ${ctx}:`, e);
  }

  async function handleFiles(list: FileList | null) {
    // 附件仅「对话」类型使用
    if (!isChat || !list || list.length === 0) return;
    const files = Array.from(list);
    for (const f of files) {
      const att = await fileToAttachment(f, () => `att-${++attSeq}`);
      attachments = [...attachments, att];
    }
  }

  function removeAttachment(id: string) {
    attachments = attachments.filter((a) => a.id !== id);
  }

  async function send() {
    if (sending) return;
    if (isImageGen) {
      await sendImageGen();
      return;
    }
    if (isVideoGen) {
      await sendVideoGen();
      return;
    }
    if (isSpeech) {
      await sendSpeech();
      return;
    }
    if (isEmbed) {
      await sendEmbedding();
      return;
    }
    if (isRerank) {
      await sendRerank();
      return;
    }
    const text = input.trim();
    if (!text || !selectedId || !modelId) {
      if (!selectedId || !modelId) error = "请先选择提供方与模型";
      return;
    }
    const parts = attachmentsToParts(attachments);
    const userMsg: ChatMessage = {
      role: "user",
      content: text,
      parts: parts.length ? parts : undefined,
    };
    const history = [...messages, userMsg];
    messages = history;
    const trimmed = trimContext(history);
    ctxTrimmed = trimmed.trimmed;
    input = "";
    attachments = [];
    error = "";
    sending = true;
    agentSteps = [];

    // 立即持久化用户消息（助手消息由后端流式完成后写入，避免重复）
    try {
      await llmApi.appendChatMessages(selectedId, modelId, [userMsg]);
    } catch (e) {
      logError("appendUser", e);
      error = "聊天记录保存失败：" + (e?.toString?.() ?? "");
    }

    // 创建助手占位，流式填充；记录其助手序号（用于工具步骤随消息持久化）
    const assistantSeq = messages.filter((m) => m.role === "assistant").length;
    const assistantMsg: ChatMessage = { role: "assistant", content: "" };
    messages = [...messages, assistantMsg];
    const assistantIndex = messages.length - 1;
    let assistantContent = "";
    scrollToBottom();

    // 若已选用 AI 角色，将角色 ID 透传给后端；后端从共享角色库读取并注入系统提示词，
    // 同时采用角色定义的采样参数（跨模块角色复用与统一调度）。
    const req: ChatRequest = {
      provider_id: selectedId,
      model: modelId,
      role_id: selectedRole?.id ?? null,
      // 上下文被裁剪时注入系统说明，避免模型脱离当前窗口主题作答
      messages: trimmed.trimmed
        ? [{ role: "system", content: TRIMMED_CONTEXT_NOTE }, ...trimmed.messages]
        : trimmed.messages,
      max_tokens: selectedRole ? selectedRole.max_tokens : null,
      temperature: selectedRole ? selectedRole.temperature : 0.7,
      top_p: selectedRole ? selectedRole.top_p : null,
      presence_penalty: selectedRole ? selectedRole.presence_penalty : null,
      frequency_penalty: selectedRole ? selectedRole.frequency_penalty : null,
    };

    try {
      if (agentMode && isChat) {
        // 代理模式：模型可调用本地工具（联网搜索 / 知识库检索 / 文件读写 / 命令执行），
        // 工具执行过程以事件流回传，界面实时展示；
        // 发送前刷新插件列表（插件可能在其他入口新建/更新）
        loadPlugins().catch(() => {});
        await llmApi.chatAgentStream(req, (ev: AgentStreamEvent) => {
          if (ev.type === "delta") {
            assistantContent += ev.content;
            messages[assistantIndex] = { role: "assistant", content: assistantContent };
            messages = [...messages];
            scrollToBottom();
          } else if (ev.type === "tool_start") {
            agentSteps = [
              ...agentSteps,
              { id: ev.id, name: ev.name, args: ev.arguments, status: "running" },
            ];
          } else if (ev.type === "tool_done") {
            agentSteps = agentSteps.map((s) =>
              s.id === ev.id
                ? {
                    ...s,
                    status: ev.ok ? "ok" : "err",
                    result: ev.result,
                    duration_ms: ev.duration_ms,
                  }
                : s,
            );
          } else if (ev.type === "done") {
            messages[assistantIndex] = {
              role: "assistant",
              content: ev.content,
              tool_steps: agentSteps,
            };
            messages = [...messages];
            lastResult = {
              content: ev.content,
              model: ev.model,
              provider_id: selectedId,
              provider_name: selected?.name ?? "",
              prompt_tokens: ev.prompt_tokens,
              completion_tokens: ev.completion_tokens,
              total_tokens: ev.total_tokens,
              cost: ev.cost,
            };
            scrollToBottom();
            // 代理流后端不负责持久化助手消息，由前端落盘一次；
            // 工具调用步骤随助手消息一并持久化（重新打开会话仍可见）
            llmApi
              .appendChatMessages(selectedId, modelId, [
                { role: "assistant", content: ev.content },
              ])
              .catch(() => {});
            if (agentSteps.length > 0) {
              llmApi
                .saveAgentToolSteps(selectedId, modelId, assistantSeq, agentSteps)
                .catch(() => {});
            }
          } else if (ev.type === "error") {
            error = `代理调用失败：${ev.message}`;
            pendingVoiceReply = false;
          }
        });
      } else {
        await llmApi.chatStream(req, (chunk) => {
          if (chunk.type === "delta") {
            assistantContent += chunk.content;
            messages[assistantIndex] = { role: "assistant", content: assistantContent };
            messages = [...messages];
            // 语音对话模式：边生成边按句子入队播报
            if (pendingVoiceReply && voiceReply) {
              if (!speechFlow.active) speechFlow.active = true;
              feedStreamSpeech(chunk.content);
              drainSpeech();
            }
            scrollToBottom();
          } else if (chunk.type === "done") {
            messages[assistantIndex] = { role: "assistant", content: chunk.content };
            messages = [...messages];
            lastResult = {
              content: chunk.content,
              model: chunk.model,
              provider_id: selectedId,
              provider_name: selected?.name ?? "",
              prompt_tokens: chunk.prompt_tokens,
              completion_tokens: chunk.completion_tokens,
              total_tokens: chunk.total_tokens,
              cost: chunk.cost,
            };
            scrollToBottom();
            // 语音对话模式：回复完成后把最后一句也入队播报
            if (pendingVoiceReply && voiceReply) {
              pendingVoiceReply = false;
              speechFlow.active = true;
              finishStreamSpeech();
              drainSpeech();
            }
          } else if (chunk.type === "error") {
            error = `调用失败：${chunk.message}`;
            pendingVoiceReply = false;
            abortStreamSpeech(false);
          }
        });
      }
    } catch (e: unknown) {
      error = `调用失败：${errText(e)}`;
      pendingVoiceReply = false;
      abortStreamSpeech(false);
      logError("send", e);
      // 移除空白助手占位
      messages = messages.filter((_, i) => i !== assistantIndex);
    } finally {
      sending = false;
      scrollToBottom();
    }
  }

  // ─── 语音对话 ───

  /** 依次尝试当前模型与「语音」类模型，返回第一个可用的 TTS 结果 */
  async function trySpeech(inputText: string): Promise<SpeechResult | null> {
    const attempts = buildSpeechAttempts(
      { provider_id: selectedId, model: modelId },
      config.providers,
    );
    let lastErr = "";
    for (const a of attempts) {
      if (!a.provider_id || !a.model) continue;
      try {
        return await llmApi.generateSpeech({
          provider_id: a.provider_id,
          model: a.model,
          input: inputText,
          voice: speechVoice,
          response_format: speechFormat,
          speed: Number(speechSpeed),
        });
      } catch (e: unknown) {
        lastErr = errText(e);
      }
    }
    if (lastErr) throw new Error(lastErr);
    return null;
  }

  function stopSpeaking() {
    stopBargeInMonitor();
    stopTtsPlayer();
  }

  /** 终止当前流式播报会话（打断/停止时调用） */
  function abortStreamSpeech(autoListen: boolean) {
    resetSpeechFlow();
    stopSpeaking();
    if (autoListen && voiceMode) {
      if (voiceLoop) scheduleAutoListen();
      else voiceStatus = "已停止播报";
    }
  }

  /** 播放一段语音（提供方音频或系统语音）；直到播完或被 stopSpeaking 打断才 resolve */
  function playSpeechChunk(chunk: SpeechChunk, msgIndex: number | null = null): Promise<void> {
    return playTtsAudio(ttsDataUrl(chunk.res), msgIndex, {
      viaNative: chunk.viaNative,
      voiceMode,
      voiceLoop,
    });
  }

  /** 流式播报工作线程：逐句合成播放，并预取下一条减少句间停顿 */
  function drainSpeech() {
    return drainSpeechFlow({
      synth: synthOneSpeech,
      play: playSpeechChunk,
      isActive: () => voiceMode,
      onStatus: (text) => (voiceStatus = text),
      onDone: () => {
        if (voiceMode && voiceLoop && !ttsPlayer.speaking && !ttsPlayer.audioPlayer) {
          scheduleAutoListen();
        } else {
          voiceStatus = voiceMode ? "一轮对话完成，可继续说…" : "";
        }
      },
    });
  }

  /** AI 说话期间监听麦克风：检测到连续说话即打断播报（barge-in） */
  function startBargeInMonitor() {
    if (!voiceMode || !voiceLoop || !mediaStream || !ttsPlayer.speaking) return;
    try {
      if (!audioCtxRef) {
        const Ctx =
          window.AudioContext ||
          (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
        audioCtxRef = new Ctx();
      }
      const ctx = audioCtxRef;
      if (ctx.state === "suspended") ctx.resume().catch(() => {});
      bargeSource = ctx.createMediaStreamSource(mediaStream);
      const an = ctx.createAnalyser();
      an.fftSize = 1024;
      bargeSource.connect(an);
      bargeHits = 0;
      const tick = () => {
        if (!ttsPlayer.speaking || !voiceMode) return;
        const buf = new Uint8Array(an.frequencyBinCount);
        an.getByteTimeDomainData(buf);
        const rms = rmsLevel(buf);
        if (rms > 0.035) {
          bargeHits++;
          if (bargeHits >= 3) {
            abortStreamSpeech(true);
            return;
          }
        } else {
          bargeHits = 0;
        }
        bargeTimer = window.setTimeout(tick, 120);
      };
      tick();
    } catch {
      /* 打断监听不可用（如无麦克风数据）时忽略 */
    }
  }

  function stopBargeInMonitor() {
    if (bargeTimer) {
      clearTimeout(bargeTimer);
      bargeTimer = null;
    }
    if (bargeSource) {
      try {
        bargeSource.disconnect();
      } catch {
        /* 忽略 */
      }
      bargeSource = null;
    }
    bargeHits = 0;
  }

  /** 连续对话：一轮结束后自动重新开始聆听 */
  function scheduleAutoListen(message = "可继续说…") {
    voiceStatus = message;
    window.setTimeout(() => {
      if (
        voiceMode &&
        voiceLoop &&
        !voiceRecorder.recording &&
        !ttsPlayer.speaking &&
        !ttsPlayer.audioPlayer
      ) {
        startRecording();
      }
    }, 600);
  }

  /** 合成并播报一段文本；消息旁喇叭与语音对话自动回复共用 */
  async function speakText(text: string, msgIndex: number | null = null) {
    // 朗读前剥离 Markdown 标记（**加粗**、# 标题、代码块等），避免读出符号
    const content = plainTextForSpeech(text ?? "");
    if (!content) return;
    // 手动重听会终止进行中的流式播报会话
    abortStreamSpeech(false);
    const sid = speechSessionId();
    if (voiceMode) voiceStatus = "正在合成语音…";
    const chunk = await synthOneSpeech(content);
    if (!chunk || !isCurrentSpeechSession(sid)) return;
    await playSpeechChunk(chunk, msgIndex);
    if (isCurrentSpeechSession(sid) && voiceMode && voiceLoop) {
      scheduleAutoListen();
    } else if (isCurrentSpeechSession(sid) && voiceMode) {
      voiceStatus = "可继续说…";
    }
  }

  async function toggleVoiceMode() {
    if (voiceMode) {
      voiceMode = false;
      abortStreamSpeech(false);
      await stopVoiceRecorder(false);
      mediaStream?.getTracks().forEach((t) => t.stop());
      mediaStream = null;
      voiceStatus = "";
      ttsEngine = "";
      return;
    }
    if (!isChat) {
      error = "语音对话仅支持对话类模型";
      return;
    }
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      mediaStream = stream;
      voiceMode = true;
      speechSynth.providerTtsFailed = false;
      ttsEngine = "";
      micError = "";
      voiceStatus = voiceLoop
        ? "点击麦克风开始说话；连续对话开启时，AI 说完会自动继续聆听"
        : "点击麦克风开始说话，说完静音约 1.6 秒自动识别";
      try {
        const stt = await getLocalSttStatus();
        const ready = Boolean(stt?.enabled && stt?.model_exists);
        const asrRe = /sensevoice|whisper|telespeech|speechasr|fun_audio|audio-transcri/i;
        const hasCloudAsr = config.providers.some(
          (p) =>
            p.enabled &&
            ((p.models ?? []).some((m) => asrRe.test(m)) || asrRe.test(p.default_model ?? "")),
        );
        sttEngine = ready
          ? hasCloudAsr
            ? "本地 Whisper + 云端转写"
            : "本地 Whisper（离线）"
          : hasCloudAsr
            ? "云端转写"
            : "云端转写（未配置，将回退本地）";
      } catch {
        sttEngine = "云端转写";
      }
    } catch (e: unknown) {
      micError = `麦克风权限被拒绝：${errText(e)}`;
    }
  }

  function startRecording() {
    if (voiceRecorder.recording || !mediaStream) return;
    // 用户开始说话时打断 AI 播报（手动打断）
    abortStreamSpeech(false);
    startVoiceRecorder(mediaStream, {
      onBlob: handleRecordedBlob,
      onStatus: (text) => (voiceStatus = text),
    });
  }

  /** 输入行麦克风一键三态：未开启 → 开启语音；已开启 → 开始/停止录音 */
  async function micClick() {
    if (!voiceMode) {
      await toggleVoiceMode();
      return;
    }
    if (voiceRecorder.recording) {
      stopRecording(false);
    } else {
      startRecording();
    }
  }

  async function stopRecording(auto = false) {
    stopVoiceRecorder(auto);
  }

  /** 录音结束：转写 → 作为用户消息发送 → 回复完成后自动语音播报 */
  async function handleRecordedBlob(blob: Blob) {
    if (blob.size < 2000) {
      voiceStatus = "未检测到语音，请再说一次";
      return;
    }
    const wav = await blobToWav16kMono(blob);
    const audio = wav ?? new Uint8Array(await blob.arrayBuffer());
    const ext = wav ? "wav" : "webm";
    try {
      const res = await llmApi.transcribeVoiceAudio(audio, ext);
      sttEngine = res.engine;
      const text = res.text.trim();
      if (!text) {
        voiceStatus = "未识别到内容，请再说一次";
        return;
      }
      voiceStatus = `识别完成（${res.engine}），发送中…`;
      // 新一轮语音对话：清空上一轮残留的播报队列
      resetSpeechFlow();
      pendingVoiceReply = voiceReply;
      input = `🎙️ ${text}`;
      await send();
      if (voiceMode && !speechFlow.active) {
        if (voiceLoop && !ttsPlayer.speaking && !ttsPlayer.audioPlayer) {
          scheduleAutoListen();
        } else {
          voiceStatus = "一轮对话完成，可继续说…";
        }
      }
    } catch (e: unknown) {
      voiceStatus = "";
      micError = `转写失败：${errText(e)}`;
    }
  }

  // 生图模型：调用 /images/generations，把结果作为助手消息的图片渲染
  async function sendImageGen() {
    const prompt = input.trim();
    if (!prompt || !selectedId || !modelId) {
      if (!selectedId || !modelId) error = "请先选择提供方与模型";
      return;
    }
    input = "";
    attachments = [];
    error = "";
    sending = true;

    const userMsg: ChatMessage = { role: "user", content: `🎨 生图提示词：${prompt}` };
    messages = [...messages, userMsg];
    const assistantMsg: ChatMessage = { role: "assistant", content: "" };
    messages = [...messages, assistantMsg];
    const assistantIndex = messages.length - 1;
    scrollToBottom();

    try {
      const res = await llmApi.generateImage({
        provider_id: selectedId,
        model: modelId,
        prompt,
        n: 1,
        size: imgSize,
      });
      const md = res.urls.map((u) => `![${modelId}](${u})`).join("\n\n");
      messages[assistantIndex] = { role: "assistant", content: md };
      messages = [...messages];
      scrollToBottom();
      // 持久化（图片以 markdown 链接形式保存）
      try {
        await llmApi.appendChatMessages(selectedId, modelId, [userMsg]);
        await llmApi.appendChatMessages(selectedId, modelId, [
          { role: "assistant", content: md },
        ]);
      } catch (e) {
        logError("appendImage", e);
      }
    } catch (e: unknown) {
      error = `图像生成失败：${errText(e)}`;
      messages = messages.filter((_, i) => i !== assistantIndex && i !== assistantIndex - 1);
      logError("sendImageGen", e);
    } finally {
      sending = false;
      scrollToBottom();
    }
  }

  // 视频生成模型：调用 /videos/generations，把结果作为助手消息的视频渲染
  async function sendVideoGen() {
    const prompt = input.trim();
    if (!prompt || !selectedId || !modelId) {
      if (!selectedId || !modelId) error = "请先选择提供方与模型";
      return;
    }
    input = "";
    attachments = [];
    error = "";
    sending = true;

    const userMsg: ChatMessage = { role: "user", content: `🎬 视频生成提示词：${prompt}` };
    messages = [...messages, userMsg];
    const assistantMsg: ChatMessage = { role: "assistant", content: "" };
    messages = [...messages, assistantMsg];
    const assistantIndex = messages.length - 1;
    scrollToBottom();

    try {
      const res = await llmApi.generateVideo({
        provider_id: selectedId,
        model: modelId,
        prompt,
        n: 1,
      });
      const md = res.urls.map((u) => `![🎬 ${modelId}](${u})`).join("\n\n");
      messages[assistantIndex] = { role: "assistant", content: md };
      messages = [...messages];
      scrollToBottom();
      // 持久化
      try {
        await llmApi.appendChatMessages(selectedId, modelId, [userMsg]);
        await llmApi.appendChatMessages(selectedId, modelId, [
          { role: "assistant", content: md },
        ]);
      } catch (e) {
        logError("appendVideo", e);
      }
    } catch (e: unknown) {
      error = `视频生成失败：${errText(e)}`;
      messages = messages.filter((_, i) => i !== assistantIndex && i !== assistantIndex - 1);
      logError("sendVideoGen", e);
    } finally {
      sending = false;
      scrollToBottom();
    }
  }

  // 语音合成（TTS）：调用 /audio/speech，把音频以 data URL 形式内联渲染
  async function sendSpeech() {
    const text = input.trim();
    if (!text || !selectedId || !modelId) {
      if (!selectedId || !modelId) error = "请先选择提供方与模型";
      return;
    }
    input = "";
    attachments = [];
    error = "";
    statusMsg = "";
    lastResult = null;
    sending = true;

    const userMsg: ChatMessage = { role: "user", content: `🔊 语音合成：${text}` };
    messages = [...messages, userMsg];
    const assistantMsg: ChatMessage = { role: "assistant", content: "" };
    messages = [...messages, assistantMsg];
    const assistantIndex = messages.length - 1;
    scrollToBottom();

    try {
      const res = await llmApi.generateSpeech({
        provider_id: selectedId,
        model: modelId,
        input: text,
        voice: speechVoice,
        response_format: speechFormat,
        speed: Number(speechSpeed),
      });
      const mime = audioMime(res.format);
      const src = `data:${mime};base64,${res.audio_data}`;
      const md = `![🎙️ ${modelId}](${src})`;
      messages[assistantIndex] = { role: "assistant", content: md };
      messages = [...messages];
      scrollToBottom();
      statusMsg = `语音合成完成 · 音色 ${res.voice} · 格式 ${res.format} · 语速 ${speechSpeed}x`;
      try {
        await llmApi.appendChatMessages(selectedId, modelId, [userMsg]);
        await llmApi.appendChatMessages(selectedId, modelId, [
          { role: "assistant", content: md },
        ]);
      } catch (e) {
        logError("appendSpeech", e);
      }
    } catch (e: unknown) {
      error = `语音合成失败：${errText(e)}`;
      messages = messages.filter((_, i) => i !== assistantIndex && i !== assistantIndex - 1);
      logError("sendSpeech", e);
    } finally {
      sending = false;
      scrollToBottom();
    }
  }

  // 文本嵌入：调用 /embeddings，把向量维度与预览作为助手消息渲染
  async function sendEmbedding() {
    const text = input.trim();
    if (!text || !selectedId || !modelId) {
      if (!selectedId || !modelId) error = "请先选择提供方与模型";
      return;
    }
    input = "";
    attachments = [];
    error = "";
    statusMsg = "";
    lastResult = null;
    sending = true;

    const userMsg: ChatMessage = { role: "user", content: `🔢 嵌入文本：${text}` };
    messages = [...messages, userMsg];
    const assistantMsg: ChatMessage = { role: "assistant", content: "" };
    messages = [...messages, assistantMsg];
    const assistantIndex = messages.length - 1;
    scrollToBottom();

    try {
      const res = await llmApi.generateEmbedding({
        provider_id: selectedId,
        model: modelId,
        input: text,
      });
      const count = res.embeddings.length;
      const first = res.embeddings[0] || [];
      const shown = Math.min(16, first.length);
      const preview = first
        .slice(0, shown)
        .map((x) => x.toFixed(6))
        .join(", ");
      const md = [
        `### 🔢 嵌入生成完成`,
        `- 模型：${res.model}`,
        `- 向量条数：${count}`,
        `- 维度：${res.dimensions}`,
        `- 输入 token：${res.prompt_tokens}`,
        `- 总 token：${res.total_tokens}`,
        ``,
        `**向量预览（首条前 ${shown} 维）：**`,
        "```",
        `[${preview}${first.length > 16 ? ", …" : ""}]`,
        "```",
        first.length > 16 ? `完整向量共 ${first.length} 维，已省略。` : "",
      ].join("\n");
      messages[assistantIndex] = { role: "assistant", content: md };
      messages = [...messages];
      scrollToBottom();
      statusMsg = `嵌入生成完成 · ${count} 条 · 维度 ${res.dimensions} · 输入 ${res.prompt_tokens} token`;
      try {
        await llmApi.appendChatMessages(selectedId, modelId, [userMsg]);
        await llmApi.appendChatMessages(selectedId, modelId, [
          { role: "assistant", content: md },
        ]);
      } catch (e) {
        logError("appendEmbed", e);
      }
    } catch (e: unknown) {
      error = `嵌入生成失败：${errText(e)}`;
      messages = messages.filter((_, i) => i !== assistantIndex && i !== assistantIndex - 1);
      logError("sendEmbedding", e);
    } finally {
      sending = false;
      scrollToBottom();
    }
  }

  // 重排序：调用 /rerank，把相关性得分列表作为助手消息渲染
  async function sendRerank() {
    const query = rerankQuery.trim();
    const docs = input
      .split("\n")
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    if (!query || docs.length === 0 || !selectedId || !modelId) {
      if (!selectedId || !modelId) error = "请先选择提供方与模型";
      else error = "请填写查询语句与至少一条文档";
      return;
    }
    input = "";
    rerankQuery = "";
    attachments = [];
    error = "";
    statusMsg = "";
    lastResult = null;
    sending = true;

    const userMsg: ChatMessage = {
      role: "user",
      content: `🔁 重排序\n查询：${query}\n文档数：${docs.length}`,
    };
    messages = [...messages, userMsg];
    const assistantMsg: ChatMessage = { role: "assistant", content: "" };
    messages = [...messages, assistantMsg];
    const assistantIndex = messages.length - 1;
    scrollToBottom();

    try {
      const res = await llmApi.rerank({
        provider_id: selectedId,
        model: modelId,
        query,
        documents: docs,
        top_n: null,
      });
      const lines = res.results.map((r, i) => {
        const docPreview =
          r.document.length > 120 ? r.document.slice(0, 120) + "…" : r.document;
        return `${i + 1}. **#${r.index}** · 得分 ${r.score.toFixed(4)} — ${docPreview}`;
      });
      const md = [
        `### 🔁 重排序结果`,
        `- 模型：${res.model}`,
        `- 查询：${query}`,
        `- 结果数：${res.results.length} / 文档数 ${docs.length}`,
        ``,
        ...lines,
      ].join("\n");
      messages[assistantIndex] = { role: "assistant", content: md };
      messages = [...messages];
      scrollToBottom();
      statusMsg = `重排序完成 · 返回 ${res.results.length} 条`;
      try {
        await llmApi.appendChatMessages(selectedId, modelId, [userMsg]);
        await llmApi.appendChatMessages(selectedId, modelId, [
          { role: "assistant", content: md },
        ]);
      } catch (e) {
        logError("appendRerank", e);
      }
    } catch (e: unknown) {
      error = `重排序失败：${errText(e)}`;
      messages = messages.filter((_, i) => i !== assistantIndex && i !== assistantIndex - 1);
      logError("sendRerank", e);
    } finally {
      sending = false;
      scrollToBottom();
    }
  }

  async function clearChat() {
    if (selectedId && modelId) {
      try {
        await llmApi.clearChatHistory(selectedId, modelId);
      } catch (e) {
        logError("clearChat", e);
      }
      // 「记住批准」的信任记录随会话一并清空
      llmApi.clearAgentTrust(selectedId, modelId).catch(() => {});
    }
    messages = [];
    attachments = [];
    lastResult = null;
    error = "";
    statusMsg = "";
    rerankQuery = "";
    ctxTrimmed = false;
    agentSteps = [];
    pendingApprovals = [];
    expandedStep = null;
  }
</script>

<div class="llm-chat">
  {#if config.providers.length === 0}
    <div class="llm-empty">请先在「接入配置」中添加并启用提供方。</div>
  {/if}
  {#if error}<div class="llm-error">{error}</div>{/if}
  {#if ctxTrimmed}
    <div class="llm-ctx-hint">长对话已自动裁剪更早上下文（保留最近 24 条），避免超出模型上下文窗口；完整历史仍保存在会话中。</div>
  {/if}

  <div class="llm-chat-window" bind:this={chatWindow}>
    {#if loadingHistory}
      <div class="llm-chat-placeholder">加载历史记录…</div>
    {:else if messages.length === 0}
      <div class="llm-hero">
        <div class="llm-hero-logo"><SparklesIcon class="size-6" /></div>
        <h2 class="llm-hero-title">有什么可以帮你？</h2>
        <p class="llm-hero-sub">与「{selected?.name ?? "AI 助手"}」对话 · 支持图片、文件与语音输入</p>
        <div class="llm-hero-sugs">
          {#each SUGGESTIONS as s (s)}
            <button class="llm-hero-sug" onclick={() => useSuggestion(s)}>{s}</button>
          {/each}
        </div>
      </div>
    {:else}
      {#snippet stepRows(steps: AgentStepRecord[], msgIndex: number | null)}
        {#each steps.slice(-8) as s (s.id)}
          <div
            class="llm-agent-step"
            class:ok={s.status === "ok"}
            class:err={s.status === "err"}
            class:open={expandedStep === s.id}
          >
            <button
              class="llm-agent-step-head"
              onclick={() => (expandedStep = expandedStep === s.id ? null : s.id)}
              title={expandedStep === s.id ? "收起详情" : "展开参数与结果"}
            >
              <WrenchIcon class="size-3.5 llm-agent-ico" />
              <span class="llm-agent-step-name">{s.name}</span>
              {#if s.args}
                <span class="llm-agent-step-args" title={s.args}>{s.args.length > 50 ? s.args.slice(0, 50) + "…" : s.args}</span>
              {/if}
              <span class="llm-agent-step-status">
                {#if s.status === "running"}
                  <span class="llm-agent-running">执行中…</span>
                {:else if s.status === "err"}
                  <XIcon class="size-3" />失败
                {:else}
                  <CheckIcon class="size-3" />完成
                {/if}
              </span>
              {#if s.duration_ms != null}
                <span class="llm-agent-step-dur" title="执行耗时">{fmtDuration(s.duration_ms)}</span>
              {/if}
              {#if s.retried}<span class="llm-agent-step-retried" title="用户手动重试（仅查看）">已重试</span>{/if}
              <span class="llm-agent-step-chevron">{expandedStep === s.id ? "▾" : "▸"}</span>
            </button>
            {#if expandedStep === s.id}
              <div class="llm-agent-step-detail">
                <div class="llm-agent-step-field">
                  <div class="llm-agent-step-field-head">
                    <span>参数</span>
                    {#if s.args}
                      <button class="llm-agent-step-copy" onclick={() => copyStepText(prettyText(s.args))}>
                        {#if copiedText === s.args.slice(0, 20)}<CheckIcon class="size-3" />已复制{:else}复制{/if}
                      </button>
                    {/if}
                  </div>
                  <pre class="llm-agent-step-pre">{prettyText(s.args)}</pre>
                </div>
                {#if s.result}
                  <div class="llm-agent-step-field">
                    <div class="llm-agent-step-field-head">
                      <span>{s.status === "err" ? "错误" : "结果"}</span>
                      <button class="llm-agent-step-copy" onclick={() => copyStepText(s.result ?? "")}>
                        {#if copiedText === (s.result ?? "").slice(0, 20)}<CheckIcon class="size-3" />已复制{:else}复制{/if}
                      </button>
                    </div>
                    <pre class="llm-agent-step-pre">{s.result}</pre>
                  </div>
                {/if}
                {#if s.status === "err" && isPluginTool(s.name)}
                  <button
                    class="llm-agent-step-retry"
                    disabled={retryingStep === s.id}
                    onclick={() => retryStep(s, msgIndex)}
                    title="用相同参数在本地重新执行该插件工具（结果仅供参考，不回传模型）"
                  >
                    {#if retryingStep === s.id}
                      <RefreshCwIcon class="size-3 llm-spin" />重试中…
                    {:else}
                      <RefreshCwIcon class="size-3" />重试（仅本地查看）
                    {/if}
                  </button>
                {/if}
              </div>
            {/if}
          </div>
        {/each}
      {/snippet}
      <div class="llm-chat-col">
        {#each messages as m, i (i)}
          {#if i === messages.length - 1 && messages.length > 1 && (agentSteps.length > 0 || pendingApprovals.length > 0)}
            <!-- 工具调用/审批：内嵌在对话流中，位于当前 AI 回复之前（思考位置） -->
            <div class="llm-agent-panel">
              <div class="llm-agent-panel-head">
                <span class="llm-agent-panel-title">工具调用</span>
                <span class="llm-agent-panel-sub">{agentSteps.length} 步</span>
              </div>
              {@render stepRows(agentSteps, null)}
              {#each pendingApprovals as a (a.id)}
                <div class="llm-agent-approval">
                  <div class="llm-agent-approval-head">
                    <ShieldAlertIcon class="size-3.5 llm-agent-ico" />
                    <span class="llm-agent-approval-text" title={a.description}>{a.tool} 需要批准</span>
                    {#if a.arguments}
                      <code class="llm-agent-approval-args" title={prettyText(a.arguments)}>{a.arguments.length > 60 ? a.arguments.slice(0, 60) + "…" : a.arguments}</code>
                      <button class="llm-agent-step-copy" onclick={() => copyStepText(prettyText(a.arguments ?? ""))}>复制</button>
                    {/if}
                    <span class="llm-agent-approval-actions">
                      <button class="llm-agent-approve" onclick={() => approveAgent(a.id, true)} title="同一工具在本会话有效期内不再询问">记住并批准</button>
                      <button class="llm-agent-approve" onclick={() => approveAgent(a.id)}>批准</button>
                      <button class="llm-agent-reject" onclick={() => rejectAgent(a.id)}>拒绝</button>
                    </span>
                  </div>
                </div>
              {/each}
            </div>
          {:else if m.role === "assistant" && (m.tool_steps?.length ?? 0) > 0}
            <!-- 历史工具调用：随对话持久化，重新打开会话仍可见 -->
            <div class="llm-agent-panel llm-agent-panel-history">
              <div class="llm-agent-panel-head">
                <span class="llm-agent-panel-title">工具调用</span>
                <span class="llm-agent-panel-sub">历史 · {m.tool_steps!.length} 步</span>
              </div>
              {@render stepRows(m.tool_steps!, i)}
            </div>
          {/if}
          {#if m.role === "user"}
            <div class="llm-msg llm-msg-user">
              <div class="llm-msg-bubble"><MessageBody msg={m} /></div>
            </div>
          {:else}
            <div class="llm-msg llm-msg-bot">
              <div class="llm-msg-avatar" aria-hidden="true"><BotIcon class="size-4" /></div>
              <div class="llm-msg-body">
                <div class="llm-msg-name">{selected?.name ?? "AI"}</div>
                <div class="llm-msg-bubble">
                  {#if !m.content && sending}
                    <div class="llm-typing"><span></span><span></span><span></span></div>
                  {:else}
                    <MessageBody msg={m} />
                    {#if sending && i === messages.length - 1}<span class="llm-caret"></span>{/if}
                  {/if}
                </div>
                {#if m.content && isChat && !(sending && i === messages.length - 1)}
                  <div class="llm-msg-actions">
                    <button
                      class="llm-msg-act"
                      class:active={ttsPlayer.speaking && ttsPlayer.speakingIndex === i}
                      onclick={() => speakText(typeof m.content === "string" ? m.content : "", i)}
                      title="语音播报这条回复"
                    >
                      <Volume2Icon class="size-3.5" />播报
                    </button>
                    <button
                      class="llm-msg-act"
                      onclick={() => copyMessage(i, typeof m.content === "string" ? m.content : "")}
                      title="复制回复内容"
                    >
                      {#if copiedIdx === i}<CheckIcon class="size-3.5" />已复制{:else}<CopyIcon class="size-3.5" />复制{/if}
                    </button>
                  </div>
                {/if}
              </div>
            </div>
          {/if}
        {/each}
      </div>
    {/if}
  </div>

  <!-- 工具调用/审批面板已内嵌在对话流中（AI 回复位置） -->

  <div
    class="llm-chat-input"
    role="group"
    aria-label="对话输入区"
    class:drag={dragOver}
    ondragover={(e) => {
      e.preventDefault();
      dragOver = true;
    }}
    ondragleave={(e) => {
      if (e.currentTarget === e.target) dragOver = false;
    }}
    ondrop={(e) => {
      e.preventDefault();
      dragOver = false;
      handleFiles(e.dataTransfer?.files ?? null);
    }}
  >
    {#if isImageGen}
      <div class="llm-img-gen-hint">当前为「生图」模型，将根据提示词直接生成图像</div>
    {:else if isVideoGen}
      <div class="llm-img-gen-hint">当前为「视频」模型，将根据提示词直接生成视频</div>
    {:else if isSpeech}
      <div class="llm-img-gen-hint">当前为「语音」模型，将根据文本合成语音（支持选择音色与格式）</div>
    {:else if isEmbed}
      <div class="llm-img-gen-hint">当前为「嵌入」模型，将文本转换为向量（多行文本按多条处理）</div>
    {:else if isRerank}
      <div class="llm-img-gen-hint">当前为「重排序」模型，请填写上方查询语句，并在下方每行粘贴一条待排序文档</div>
    {/if}

    {#if isChat && attachments.length}
      <div class="llm-att-previews">
        {#each attachments as a (a.id)}
          <div class="llm-att-prev" class:tooBig={a.tooBig} title={a.name}>
            {#if a.kind === "image" && a.url}
              <img src={a.url} alt={a.name} />
            {:else}
              <div class="llm-att-prev-icon">
                {#if a.kind === "text"}<FileTextIcon class="size-5" />{:else}<PaperclipIcon class="size-5" />{/if}
              </div>
            {/if}
            <div class="llm-att-prev-name">{a.name}</div>
            {#if a.tooBig}<div class="llm-att-prev-warn">过大</div>{/if}
            <button class="llm-att-remove" onclick={() => removeAttachment(a.id)} title="移除"><XIcon class="size-3" /></button>
          </div>
        {/each}
      </div>
    {/if}

    {#if voiceMode && isChat}
      <div class="llm-voice-line" class:recording={voiceRecorder.recording}>
        <div
          class="llm-voice-status"
          class:err={!!(micError || voiceRecorder.micError)}
          title={micError || voiceRecorder.micError || ""}
        >
          {micError || voiceRecorder.micError || voiceStatus || (voiceRecorder.recording ? "正在聆听…" : "点击麦克风开始说话，静音约 1.6 秒自动识别")}
        </div>
        {#if ttsPlayer.speaking}
          <button class="llm-voice-chip llm-voice-stop" onclick={stopSpeaking} title="打断播报">停止播报</button>
        {/if}
        <button class="llm-voice-chip" class:on={voiceReply} onclick={() => (voiceReply = !voiceReply)} title="AI 回复用语音播报">语音回复</button>
        <button class="llm-voice-chip" class:on={voiceLoop} onclick={() => (voiceLoop = !voiceLoop)} title="AI 说完自动继续聆听">连续对话</button>
        <button class="llm-voice-chip llm-voice-gear" class:on={voiceCfgOpen} onclick={() => (voiceCfgOpen = !voiceCfgOpen)} title="音色与语速设置">
          <SlidersHorizontalIcon class="size-3.5" />
        </button>
        <button class="llm-voice-chip llm-voice-exit" onclick={toggleVoiceMode} title="退出语音对话">
          <XIcon class="size-3.5" />
        </button>
        {#if voiceCfgOpen}
          <div class="llm-voice-pop">
            <div class="llm-voice-pop-row">
              <span class="llm-voice-pop-label">音色</span>
              <NativeSelect class="h-8" bind:value={speechVoice}>
                {#each SPEECH_VOICES as v (v.value)}
                  <NativeSelectOption value={v.value}>{v.label}</NativeSelectOption>
                {/each}
              </NativeSelect>
            </div>
            <div class="llm-voice-pop-row">
              <span class="llm-voice-pop-label">语速</span>
              <NativeSelect class="h-8" bind:value={speechSpeed}>
                <NativeSelectOption value="0.75">0.75x 舒缓</NativeSelectOption>
                <NativeSelectOption value="0.9">0.9x 稍慢</NativeSelectOption>
                <NativeSelectOption value="1.0">1.0x 正常</NativeSelectOption>
                <NativeSelectOption value="1.15">1.15x 稍快</NativeSelectOption>
                <NativeSelectOption value="1.3">1.3x 快速</NativeSelectOption>
              </NativeSelect>
            </div>
            <div class="llm-voice-pop-meta">转写：{sttEngine} · 播报：{ttsEngine || "—"}</div>
          </div>
        {/if}
      </div>
    {/if}

    <div class="llm-input-row">
      {#if isChat}
        <button class="llm-ico-btn" title="上传图片 / 文件" onclick={(e) => { e.stopPropagation(); fileInput?.click(); }}>
          <PaperclipIcon class="size-4" />
        </button>
        <button
          class="llm-ico-btn llm-mic-btn"
          class:voice-on={voiceMode}
          class:recording={voiceRecorder.recording}
          onclick={micClick}
          title={voiceMode ? (voiceRecorder.recording ? "停止录音" : "开始说话") : "语音对话"}
        >
          {#if voiceRecorder.recording}<SquareIcon class="size-4" />{:else}<MicIcon class="size-4" />{/if}
        </button>
      {/if}
      <input
        bind:this={fileInput}
        type="file"
        multiple
        hidden
        accept="image/*,.txt,.md,.json,.csv,.log,.xml,.html,.js,.ts,.py,.rs,.go,.java,.c,.cpp,.h,.sh,.yml,.yaml,.toml,.srt,.vtt,.pdf,.doc,.docx,.xls,.xlsx,.ppt,.pptx,.zip"
        onchange={(e) => {
          handleFiles((e.target as HTMLInputElement).files);
          (e.target as HTMLInputElement).value = "";
        }}
      />
      {#if isRerank}
        <input
          class="llm-rerank-query"
          bind:value={rerankQuery}
          placeholder="输入查询语句（query）…"
          onkeydown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); send(); } }}
        />
      {/if}
      <textarea
        bind:this={textareaEl}
        bind:value={input}
        rows="1"
        placeholder={
          isImageGen
            ? "输入生图提示词…"
            : isVideoGen
              ? "输入视频生成提示词…"
              : isSpeech
                ? "输入要转换为语音的文本…"
                : isEmbed
                  ? "输入文本（或多行，每行一条）以生成嵌入…"
                  : isRerank
                    ? "粘贴待排序文档，每行一条…"
                    : "给「AI」发送消息…"
        }
        oninput={autoGrow}
        onkeydown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            send();
          }
        }}
      ></textarea>
      {#if isImageGen}
        <label class="llm-size-sel" title="生成尺寸">
          <NativeSelect class="h-14" bind:value={imgSize}>
            <NativeSelectOption value="1024x1024">1024×1024</NativeSelectOption>
            <NativeSelectOption value="1024x1792">1024×1792</NativeSelectOption>
            <NativeSelectOption value="1792x1024">1792×1024</NativeSelectOption>
          </NativeSelect>
        </label>
      {/if}
      {#if isSpeech}
        <label class="llm-size-sel" title="音色">
          <NativeSelect class="h-14" bind:value={speechVoice}>
            {#each SPEECH_VOICES as v (v.value)}
              <NativeSelectOption value={v.value}>{v.label}</NativeSelectOption>
            {/each}
          </NativeSelect>
        </label>
        <label class="llm-size-sel" title="音频格式">
          <NativeSelect class="h-14" bind:value={speechFormat}>
            <NativeSelectOption value="mp3">mp3</NativeSelectOption>
            <NativeSelectOption value="wav">wav</NativeSelectOption>
            <NativeSelectOption value="opus">opus</NativeSelectOption>
            <NativeSelectOption value="aac">aac</NativeSelectOption>
            <NativeSelectOption value="flac">flac</NativeSelectOption>
          </NativeSelect>
        </label>
        <label class="llm-size-sel" title="语速（倍率，1.0 为正常）">
          <NativeSelect class="h-14" bind:value={speechSpeed}>
            <NativeSelectOption value="0.75">0.75x</NativeSelectOption>
            <NativeSelectOption value="0.9">0.9x</NativeSelectOption>
            <NativeSelectOption value="1.0">1.0x</NativeSelectOption>
            <NativeSelectOption value="1.15">1.15x</NativeSelectOption>
            <NativeSelectOption value="1.3">1.3x</NativeSelectOption>
          </NativeSelect>
        </label>
      {/if}
      {#if isChat}
        <RippleButton
          onclick={send}
          disabled={!canSend}
          rippleColor="rgba(255,255,255,.55)"
          class="llm-send-btn"
          title={sending ? "生成中…" : "发送"}
          aria-label="发送消息"
        >
          <SendIcon class="size-4" />
        </RippleButton>
      {:else}
        <RippleButton
          onclick={send}
          disabled={!canSend}
          rippleColor="#a5f3fc"
          class="llm-btn-primary h-14 rounded-md border-0 bg-[var(--primary)] px-4 text-sm font-medium text-[var(--primary-foreground)] hover:opacity-90"
        >
          <SendIcon class="size-4" />{sendLabel}
        </RippleButton>
      {/if}
    </div>

    <div class="llm-input-foot">
      <div class="llm-input-foot-left">
        {#if isChat}
          <button
            class="llm-agent-toggle"
            class:on={agentMode}
            onclick={() => (agentMode = !agentMode)}
            title={agentToolsTitle}
          >
            <WrenchIcon class="size-3" />代理
          </button>
        {/if}
        <span>{isChat ? "AI 生成内容仅供参考 · Enter 发送 / Shift+Enter 换行" : "Enter 发送"}</span>
      </div>
      {#if isChat && selected}<span class="llm-input-foot-model">{selected.name} · {modelId}</span>{/if}
    </div>

    {#if dragOver}
      <div class="llm-drop-overlay">松开以添加图片 / 文件</div>
    {/if}
  </div>

  <!-- 模型/角色工具栏：置于输入框下方，轻量底栏风格 -->
  <div class="llm-chat-toolbar">
    <div class="llm-toolbar-left">
      <ModelSelect
        providerClass="min-w-[150px]"
        modelClass="min-w-[150px]"
        bind:providerId={selectedId}
        bind:model={modelId}
        onProviderChange={onProviderChange}
        optionSuffix={(m) => {
          const meta = selected?.model_meta?.[m];
          return meta?.tags && meta.tags.length > 0 ? `  · ${meta.tags.join(" · ")}` : "";
        }}
      />
      {#if selected?.model_meta?.[modelId]?.model_type}
        <span
          class="llm-model-tag"
          class:img={isImageGen}
          class:vid={isVideoGen}
          class:sp={isSpeech}
          class:emb={isEmbed}
          class:re={isRerank}
        >{selected?.model_meta?.[modelId]?.model_type}</span>
      {/if}
    </div>
    <div class="llm-toolbar-right">
      {#if selectedRole}
        <span class="llm-role-chip" title={composeSystemPrompt(selectedRole)}>
          <span class="llm-role-chip-emoji">{selectedRole.emoji || '🎭'}</span>
          <span class="llm-role-chip-name">{selectedRole.name}</span>
          <button class="llm-role-chip-clear" onclick={clearRole} title="取消角色"><XIcon class="size-3" /></button>
        </span>
      {/if}
      <button class="llm-btn llm-role-btn" onclick={() => (roleDrawerOpen = true)}>
        <SparklesIcon class="size-3.5" />AI 角色
      </button>
      <button class="llm-btn llm-role-btn" onclick={() => { loadPlugins(); pluginDrawerOpen = true; }} title="管理动态插件（为代理模式注册自定义工具）">
        <PuzzleIcon class="size-3.5" />插件
      </button>
      <button class="llm-btn" onclick={clearChat} title="清空当前会话">
        <EraserIcon class="size-3.5" />清空对话
      </button>
    </div>
  </div>

  {#if lastResult}
    <div class="llm-usage-line">
      本次消耗：{lastResult.total_tokens} tokens（输入 {lastResult.prompt_tokens} / 输出 {lastResult.completion_tokens}）·
      估算成本 ${lastResult.cost.toFixed(6)} · 模型 {lastResult.model}
    </div>
  {:else if statusMsg}
    <div class="llm-usage-line llm-status-line">{statusMsg}</div>
  {/if}

  <!-- AI 角色选择抽屉（跨模块检索 Agent 模块定义的角色） -->
  {#if roleDrawerOpen}
    <div
      class="llm-role-overlay"
      role="button"
      aria-label="关闭角色抽屉"
      tabindex="-1"
      onclick={(e) => { if (e.target === e.currentTarget) roleDrawerOpen = false; }}
      onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ' || e.key === 'Escape') { e.preventDefault(); roleDrawerOpen = false; } }}
    >
      <div class="llm-role-drawer">
        <div class="llm-role-drawer-hd">
          <div>
            <h3 class="llm-role-title"><BotIcon class="size-4" />选择 AI 角色</h3>
            <p class="llm-role-drawer-sub">检索 Agent 模块「AI 角色定位」中定义的角色，调用时自动注入其系统提示词与行为约束</p>
          </div>
          <button class="llm-role-close" onclick={() => (roleDrawerOpen = false)} aria-label="关闭"><XIcon class="size-4" /></button>
        </div>
        <div class="llm-role-search">
          <input type="text" bind:value={roleSearch} placeholder="搜索角色名称 / 能力 / 提示词…" />
          <button class="llm-btn" onclick={loadAiRoles} title="刷新"><RefreshCwIcon class="size-3.5" /></button>
        </div>
        <div class="llm-role-list">
          {#if filteredAiRoles().length === 0}
            <div class="llm-role-empty">暂无可用角色（请在 Agent 模块「AI 角色定位」中启用并保存角色）</div>
          {:else}
            {#each filteredAiRoles() as role (role.id)}
              <button class="llm-role-item" class:llm-role-item-active={selectedRole?.id === role.id} onclick={() => applyRole(role)}>
                <span class="llm-role-item-emoji">{role.emoji || '🎭'}</span>
                <span class="llm-role-item-body">
                  <span class="llm-role-item-name">{role.name}</span>
                  <span class="llm-role-item-desc">{role.description || '（无简介）'}</span>
                  <span class="llm-role-item-tags">
                    {#each role.capabilities.slice(0, 5) as cap}<span class="llm-role-item-tag">{cap}</span>{/each}
                    {#if role.preferred_model}<span class="llm-role-item-tag llm-role-item-model">🧩 {role.preferred_model}</span>{/if}
                  </span>
                </span>
                {#if selectedRole?.id === role.id}<span class="llm-role-item-check">✓</span>{/if}
              </button>
            {/each}
          {/if}
        </div>
      </div>
    </div>
  {/if}

  <!-- 插件管理抽屉（DSH 动态插件：新建/编辑/启用/删除 + 版本历史） -->
  {#if pluginDrawerOpen}
    <div
      class="llm-role-overlay"
      role="button"
      aria-label="关闭插件抽屉"
      tabindex="-1"
      onclick={(e) => { if (e.target === e.currentTarget) { pluginDrawerOpen = false; pluginEditing = false; } }}
      onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ' || e.key === 'Escape') { e.preventDefault(); pluginDrawerOpen = false; pluginEditing = false; } }}
    >
      <div class="llm-role-drawer llm-plugin-drawer">
        <div class="llm-role-drawer-hd">
          <div>
            <h3 class="llm-role-title"><PuzzleIcon class="size-4" />动态插件</h3>
            <p class="llm-role-drawer-sub">为代理模式注册自定义工具：工具代码为 JavaScript，在应用内执行（与 DSH Client 插件同信任级别）。更新插件会生成新版本，历史不可变。</p>
          </div>
          <button class="llm-role-close" onclick={() => { pluginDrawerOpen = false; pluginEditing = false; }} aria-label="关闭"><XIcon class="size-4" /></button>
        </div>

        <div class="llm-plugin-body">
          {#if pluginEditing && pluginDraft}
            <div class="llm-plugin-form">
              <div class="llm-plugin-row">
                <span class="llm-plugin-label">插件名称 *</span>
                <input type="text" bind:value={pluginDraft.name} placeholder="例如：计算器插件" />
              </div>
              <div class="llm-plugin-row">
                <span class="llm-plugin-label">插件描述</span>
                <input type="text" bind:value={pluginDraft.description} placeholder="一句话说明插件用途" />
              </div>
              <div class="llm-plugin-row">
                <span class="llm-plugin-label">工具名 *</span>
                <input type="text" bind:value={pluginDraft.toolName} placeholder="例如：calculator（模型将按此名调用）" />
              </div>
              <div class="llm-plugin-row">
                <span class="llm-plugin-label">工具描述</span>
                <input type="text" bind:value={pluginDraft.toolDesc} placeholder="告诉模型这个工具做什么" />
              </div>
              <div class="llm-plugin-row">
                <label class="llm-plugin-check">
                  <input type="checkbox" bind:checked={pluginDraft.toolApproval} />
                  <span>执行前需要用户审批</span>
                </label>
              </div>
              <div class="llm-plugin-row llm-plugin-code">
                <span class="llm-plugin-label">工具代码（JS 函数体）*</span>
                <textarea bind:value={pluginDraft.toolCode} rows="10" spellcheck="false" placeholder="async function(args, ctx) … 的函数体"></textarea>
              </div>
              {#if pluginError}<div class="llm-plugin-err">{pluginError}</div>{/if}
              <div class="llm-plugin-actions">
                <button class="llm-btn" onclick={() => { pluginEditing = false; pluginDraft = null; pluginError = ""; }}>取消</button>
                <button class="llm-btn llm-plugin-save" onclick={savePluginDraft} disabled={pluginSaving}>
                  {pluginSaving ? "保存中…" : pluginDraft.id ? "保存新版本" : "创建插件"}
                </button>
              </div>
            </div>
          {:else}
            <div class="llm-plugin-list">
              {#if agentPlugins.length === 0}
                <div class="llm-role-empty">暂无插件。点击右上角「新建插件」为代理模式添加自定义工具。</div>
              {:else}
                {#each agentPlugins as p (p.id)}
                  <div class="llm-plugin-item" class:off={!p.enabled}>
                    <div class="llm-plugin-item-hd">
                      <span class="llm-plugin-item-name">{p.name}</span>
                      <span class="llm-plugin-item-ver">v{p.versions.at(-1)?.version ?? 1}</span>
                      <span class="llm-plugin-item-state">{p.enabled ? "运行中" : "已停止"}</span>
                      <span class="llm-plugin-item-tools">
                        {p.tools.map((t) => t.name + (t.requires_approval ? "🔒" : "")).join(" · ")}
                      </span>
                    </div>
                    {#if p.description}<div class="llm-plugin-item-desc">{p.description}</div>{/if}
                    <div class="llm-plugin-item-actions">
                      <button class="llm-btn" onclick={() => togglePluginEnabled(p)}>{p.enabled ? "停止" : "运行"}</button>
                      <button class="llm-btn" onclick={() => startEditPlugin(p)}>编辑</button>
                      <button class="llm-btn llm-plugin-del" onclick={() => deletePlugin(p)}>删除</button>
                    </div>
                  </div>
                {/each}
              {/if}
              {#if pluginError}<div class="llm-plugin-err">{pluginError}</div>{/if}
              <div class="llm-plugin-actions">
                <button class="llm-btn llm-plugin-save" onclick={startNewPlugin}>
                  <PuzzleIcon class="size-3.5" />新建插件
                </button>
              </div>
            </div>
          {/if}
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  /* flex:1 占据父容器（含 PanelHeader 之外的）剩余空间；min-height:0 允许
     内部滚动而不是把底部工具栏挤出可视区域（此前 height:100% 会与
     头部叠加导致底栏被裁掉） */
  .llm-chat { display: flex; flex-direction: column; gap: 10px; flex: 1; min-height: 0; }
  /* 底栏工具栏：置于输入框下方，透明轻量、居中窄栏 */
  .llm-chat-toolbar {
    display: flex; align-items: center; justify-content: space-between; gap: 10px; flex-wrap: wrap;
    width: 100%; max-width: 800px; margin: 0 auto;
    padding: 0 4px;
    background: transparent; border: none; box-shadow: none;
  }
  .llm-chat-toolbar .llm-btn {
    padding: 4px 10px; font-size: 12px; border-radius: 999px;
    background: transparent; color: var(--app-color-muted);
  }
  .llm-chat-toolbar .llm-btn:hover {
    background: var(--app-color-surface-alt); color: var(--app-color-text);
  }
  .llm-toolbar-left { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; min-width: 0; }
  .llm-toolbar-right { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .llm-model-tag {
    font-size: 11.5px; color: var(--app-color-accent);
    background: color-mix(in srgb, var(--app-color-accent) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--app-color-accent) 35%, transparent);
    border-radius: 999px; padding: 3px 9px; white-space: nowrap;
  }
  .llm-model-tag.img {
    color: var(--app-gold); background: var(--app-gold-soft);
    border-color: color-mix(in srgb, var(--app-gold) 35%, transparent);
  }
  .llm-model-tag.vid {
    color: var(--app-purple, #a78bfa); background: color-mix(in srgb, var(--app-purple, #a78bfa) 12%, transparent);
    border-color: color-mix(in srgb, var(--app-purple, #a78bfa) 35%, transparent);
  }
  .llm-model-tag.sp {
    color: var(--app-cyan, #22d3ee); background: color-mix(in srgb, var(--app-cyan, #22d3ee) 12%, transparent);
    border-color: color-mix(in srgb, var(--app-cyan, #22d3ee) 35%, transparent);
  }
  .llm-model-tag.emb {
    color: var(--app-green, #34d399); background: color-mix(in srgb, var(--app-green, #34d399) 12%, transparent);
    border-color: color-mix(in srgb, var(--app-green, #34d399) 35%, transparent);
  }
  .llm-model-tag.re {
    color: var(--app-orange, #fb923c); background: color-mix(in srgb, var(--app-orange, #fb923c) 12%, transparent);
    border-color: color-mix(in srgb, var(--app-orange, #fb923c) 35%, transparent);
  }
  .llm-rerank-query {
    width: 100%; box-sizing: border-box; resize: vertical;
    background: var(--app-color-surface); color: var(--app-color-text);
    border: 1px solid var(--app-color-border); border-radius: 8px; padding: 8px 10px; font-size: 13px;
    font-family: var(--app-font-sans);
  }
  .llm-status-line { color: var(--app-color-accent); background: color-mix(in srgb, var(--app-color-accent) 8%, transparent); }

  /* ─── AI 角色 ─── */
  .llm-role-btn { white-space: nowrap; }
  .llm-role-chip {
    display: inline-flex; align-items: center; gap: 6px;
    background: color-mix(in srgb, var(--app-purple, #a78bfa) 16%, transparent);
    border: 1px solid color-mix(in srgb, var(--app-purple, #a78bfa) 45%, transparent);
    color: var(--app-color-text); border-radius: 999px; padding: 3px 8px 3px 6px; font-size: 12px;
    max-width: 220px;
  }
  .llm-role-chip-emoji { font-size: 14px; }
  .llm-role-chip-name { font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .llm-role-chip-clear {
    background: none; border: none; color: var(--app-color-muted); cursor: pointer; font-size: 15px; line-height: 1; padding: 0 2px;
  }
  .llm-role-chip-clear:hover { color: #f87171; }

  .llm-role-overlay {
    position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); z-index: 60;
    display: flex; align-items: center; justify-content: center; padding: 24px;
  }
  .llm-role-drawer {
    width: min(560px, 92vw); max-height: 80vh; display: flex; flex-direction: column;
    background: var(--app-color-surface, #161a22); border: 1px solid var(--app-color-border, #232833);
    border-radius: 14px; box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5); overflow: hidden;
  }
  .llm-role-drawer-hd {
    display: flex; align-items: flex-start; justify-content: space-between; gap: 12px;
    padding: 16px 18px; border-bottom: 1px solid var(--app-color-border, #232833);
  }
  .llm-role-drawer-hd h3 { margin: 0; font-size: 16px; display: inline-flex; align-items: center; gap: 8px; }
  .llm-role-drawer-sub { margin: 4px 0 0; font-size: 12px; color: var(--app-color-muted, #8b93a7); }
  .llm-role-close { background: none; border: none; color: var(--app-color-muted); font-size: 18px; cursor: pointer; }
  .llm-role-close:hover { color: var(--app-color-text); }
  .llm-role-search { display: flex; gap: 8px; padding: 12px 18px; }
  .llm-role-search input {
    flex: 1; background: var(--app-color-card-bg, #141821); border: 1px solid var(--app-color-border, #232833);
    color: var(--app-color-text, #e6e9ef); border-radius: 8px; padding: 9px 12px; outline: none; font-size: 13px;
  }
  .llm-role-search input:focus { border-color: var(--app-color-accent, #4f8cff); }
  .llm-role-list { overflow-y: auto; padding: 4px 18px 18px; display: flex; flex-direction: column; gap: 8px; }
  .llm-role-empty { color: var(--app-color-muted, #8b93a7); font-size: 13px; text-align: center; padding: 28px 8px; }
  .llm-role-item {
    display: flex; align-items: center; gap: 12px; text-align: left; width: 100%;
    background: var(--app-color-card-bg, #141821); border: 1px solid var(--app-color-border, #232833);
    border-radius: 12px; padding: 12px; cursor: pointer; transition: border-color 0.15s; color: var(--app-color-text, #e6e9ef);
  }
  .llm-role-item:hover { border-color: var(--app-color-accent, #4f8cff); }
  .llm-role-item-active { border-color: var(--app-purple, #a78bfa); box-shadow: 0 0 0 2px color-mix(in srgb, var(--app-purple, #a78bfa) 30%, transparent); }
  .llm-role-item-emoji { font-size: 26px; }
  .llm-role-item-body { display: flex; flex-direction: column; gap: 3px; flex: 1; min-width: 0; }
  .llm-role-item-name { font-weight: 600; font-size: 14px; }
  .llm-role-item-desc { font-size: 12px; color: var(--app-color-muted, #8b93a7); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .llm-role-item-tags { display: flex; flex-wrap: wrap; gap: 5px; margin-top: 2px; }
  .llm-role-item-tag {
    font-size: 11.5px; padding: 2px 8px; border-radius: 999px;
    background: var(--app-color-surface-alt, #1d2230); color: var(--app-color-muted, #8b93a7);
    border: 1px solid var(--app-color-border, #232833);
  }
  .llm-role-item-model { color: var(--app-cyan, #38bdf8); border-color: rgba(56, 189, 248, 0.4); }
  .llm-role-item-check { color: var(--app-purple, #a78bfa); font-size: 18px; font-weight: 700; }
  .llm-btn {
    display: inline-flex; align-items: center; gap: 5px; white-space: nowrap;
    background: var(--app-color-surface-alt); color: var(--app-color-text);
    border: 1px solid var(--app-color-border); border-radius: 7px; padding: 7px 12px; font-size: 13px; cursor: pointer;
  }
  .llm-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  /* ─── 语音：与普通聊天共用一个输入行，仅多一条细状态行 ─── */
  .llm-mic-btn.voice-on {
    color: var(--app-color-accent);
    background: color-mix(in srgb, var(--app-color-accent) 14%, transparent);
  }
  .llm-mic-btn.recording {
    color: #fff; background: #ef4444;
    animation: llm-pulse 1.1s infinite;
  }
  .llm-voice-line {
    position: relative;
    display: flex; align-items: center; gap: 6px; flex-wrap: wrap;
    background: color-mix(in srgb, var(--app-color-accent) 7%, transparent);
    border: 1px solid color-mix(in srgb, var(--app-color-accent) 22%, var(--app-color-border));
    border-radius: 999px; padding: 4px 6px 4px 12px;
  }
  .llm-voice-line.recording {
    border-color: color-mix(in srgb, #f87171 55%, var(--app-color-border));
    background: color-mix(in srgb, #f87171 8%, transparent);
  }
  .llm-voice-status {
    flex: 1; min-width: 140px;
    font-size: 12px; color: var(--app-color-text);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .llm-voice-status.err { color: #f87171; }
  .llm-voice-chip {
    display: inline-flex; align-items: center; gap: 4px; flex: none;
    background: transparent; color: var(--app-color-muted);
    border: 1px solid transparent; border-radius: 999px;
    padding: 2px 8px; font-size: 11.5px; cursor: pointer;
    transition: color 0.15s, background 0.15s, border-color 0.15s;
  }
  .llm-voice-chip:hover { color: var(--app-color-text); background: var(--app-color-surface-alt); }
  .llm-voice-chip.on {
    color: var(--app-color-accent);
    background: color-mix(in srgb, var(--app-color-accent) 12%, transparent);
    border-color: color-mix(in srgb, var(--app-color-accent) 30%, transparent);
  }
  .llm-voice-chip.llm-voice-stop { color: #f87171; }
  .llm-voice-chip.llm-voice-gear, .llm-voice-chip.llm-voice-exit { padding: 2px 6px; }
  .llm-voice-chip.llm-voice-exit:hover { color: #f87171; }
  /* 语音设置浮层：音色 + 语速 + 引擎信息 */
  .llm-voice-pop {
    position: absolute; right: 0; bottom: calc(100% + 8px); z-index: 30;
    width: 280px; display: flex; flex-direction: column; gap: 8px;
    background: var(--app-color-card-bg, #141821);
    border: 1px solid var(--app-color-border);
    border-radius: 12px; padding: 12px;
    box-shadow: 0 18px 44px -16px rgba(0, 0, 0, 0.6);
  }
  .llm-voice-pop-row { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
  .llm-voice-pop-label { font-size: 12px; color: var(--app-color-muted); flex: none; }
  .llm-voice-pop-meta { font-size: 11px; color: var(--app-color-muted); border-top: 1px solid var(--app-color-border); padding-top: 8px; }
  .llm-empty { padding: 24px; text-align: center; color: var(--app-color-muted); border: 1px dashed var(--app-color-border); border-radius: 10px; }
  .llm-error { background: #ef44441a; color: #f87171; border: 1px solid #ef444433; padding: 8px 10px; border-radius: 7px; font-size: 13px; }
  .llm-ctx-hint { background: color-mix(in oklab, var(--primary) 9%, transparent); color: var(--muted-foreground); border: 1px solid color-mix(in oklab, var(--primary) 22%, transparent); padding: 6px 10px; border-radius: 7px; font-size: 11.5px; margin-bottom: 8px; line-height: 1.6; }
  .llm-chat-window {
    flex: 1; min-height: 0; overflow-y: auto;
    display: flex; flex-direction: column;
  }
  .llm-chat-placeholder { color: var(--app-color-muted); font-size: 13px; margin: auto; text-align: center; }
  /* 对话列：居中窄栏，主流 AI 聊天界面布局 */
  .llm-chat-col {
    width: 100%; max-width: 800px; margin: 0 auto;
    padding: 20px 20px 10px;
    display: flex; flex-direction: column; gap: 24px;
  }
  /* ─── 空态首屏 ─── */
  .llm-hero {
    margin: auto; max-width: 640px; width: 100%;
    padding: 40px 24px; display: flex; flex-direction: column; align-items: center; gap: 10px;
    text-align: center;
  }
  .llm-hero-logo {
    width: 60px; height: 60px; border-radius: 18px;
    display: grid; place-items: center; color: #fff;
    background: linear-gradient(135deg, color-mix(in srgb, var(--app-color-accent) 90%, #fff 10%), color-mix(in srgb, var(--app-color-accent) 55%, var(--app-purple, #a78bfa)));
    box-shadow: 0 10px 34px -12px color-mix(in srgb, var(--app-color-accent) 65%, transparent);
  }
  .llm-hero-title { margin: 6px 0 0; font-size: 22px; font-weight: 700; color: var(--app-color-text); }
  .llm-hero-sub { margin: 0; font-size: 13px; color: var(--app-color-muted); }
  .llm-hero-sugs { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; width: 100%; margin-top: 14px; }
  .llm-hero-sug {
    text-align: left; padding: 12px 14px; font-size: 13px; line-height: 1.5;
    background: var(--app-color-surface-alt); color: var(--app-color-text);
    border: 1px solid var(--app-color-border); border-radius: 12px; cursor: pointer;
    transition: border-color 0.15s, background 0.15s, transform 0.1s;
  }
  .llm-hero-sug:hover {
    border-color: color-mix(in srgb, var(--app-color-accent) 45%, var(--app-color-border));
    background: color-mix(in srgb, var(--app-color-accent) 7%, var(--app-color-surface-alt));
    transform: translateY(-1px);
  }
  /* ─── 消息行 ─── */
  .llm-msg { display: flex; gap: 12px; align-items: flex-start; min-width: 0; }
  .llm-msg-user { justify-content: flex-end; }
  .llm-msg-bot { justify-content: flex-start; }
  .llm-msg-avatar {
    flex-shrink: 0; width: 30px; height: 30px; border-radius: 50%;
    display: grid; place-items: center; margin-top: 2px;
    color: var(--app-color-accent);
    background: color-mix(in srgb, var(--app-color-accent) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--app-color-accent) 30%, transparent);
  }
  .llm-msg-body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 6px; }
  .llm-msg-name { font-size: 12.5px; font-weight: 600; color: var(--app-color-text); }
  .llm-msg-bubble {
    font-size: 13.5px; color: var(--app-color-text);
    white-space: pre-wrap; line-height: 1.65; word-break: break-word;
    padding: 10px 14px;
  }
  /* 助手：无气泡，透明底铺满栏宽 */
  .llm-msg-bot .llm-msg-bubble { padding: 0; }
  /* 用户：右侧圆角气泡 */
  .llm-msg-user .llm-msg-bubble {
    max-width: 76%;
    background: var(--app-color-surface-alt);
    border: 1px solid var(--app-color-border);
    border-radius: 16px 16px 4px 16px;
  }
  /* 流式光标 */
  .llm-caret {
    display: inline-block; width: 7px; height: 15px; margin-left: 2px; vertical-align: -2px;
    background: var(--app-color-accent); border-radius: 1px;
    animation: llm-caret-blink 0.9s steps(2) infinite;
  }
  @keyframes llm-caret-blink { 0%, 100% { opacity: 1; } 50% { opacity: 0; } }
  /* 消息操作（悬停显示） */
  .llm-msg-actions { display: flex; gap: 6px; opacity: 0; transition: opacity 0.15s; }
  .llm-msg-bot:hover .llm-msg-actions, .llm-msg-actions:focus-within { opacity: 1; }
  .llm-msg-act {
    display: inline-flex; align-items: center; gap: 4px;
    background: none; border: none; padding: 3px 7px; border-radius: 6px;
    font-size: 11.5px; color: var(--app-color-muted); cursor: pointer;
    transition: color 0.15s, background 0.15s;
  }
  .llm-msg-act:hover { color: var(--app-color-text); background: var(--app-color-surface-alt); }
  .llm-msg-act.active {
    color: #fff; background: var(--app-color-accent);
    animation: llm-pulse 1.1s infinite;
  }
  @keyframes llm-pulse {
    0%, 100% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--app-color-accent) 45%, transparent); }
    50% { box-shadow: 0 0 0 5px transparent; }
  }
  .llm-typing { display: inline-flex; gap: 3px; padding: 6px 0; }
  .llm-typing span { width: 6px; height: 6px; border-radius: 50%; background: var(--app-color-muted); animation: llm-blink 1.2s infinite; }
  .llm-typing span:nth-child(2) { animation-delay: 0.2s; }
  .llm-typing span:nth-child(3) { animation-delay: 0.4s; }
  @keyframes llm-blink { 0%, 100% { opacity: 0.2; } 50% { opacity: 1; } }
  .llm-chat-input {
    position: relative; display: flex; flex-direction: column; gap: 8px;
    width: 100%; max-width: 800px; margin: 0 auto;
    background: var(--app-color-surface-alt); border: 1px solid var(--app-color-border);
    border-radius: 16px; padding: 10px 12px;
    box-shadow: 0 16px 44px -20px rgba(0, 0, 0, 0.55);
    transition: border-color 0.15s, box-shadow 0.15s, background 0.15s;
  }
  .llm-chat-input:focus-within {
    border-color: color-mix(in srgb, var(--app-color-accent) 45%, var(--app-color-border));
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--app-color-accent) 20%, transparent), 0 16px 44px -20px rgba(0, 0, 0, 0.55);
  }
  .llm-chat-input.drag { border-color: var(--app-color-accent); background: color-mix(in srgb, var(--app-color-accent) 10%, transparent); }
  .llm-img-gen-hint { font-size: 11.5px; color: var(--app-color-accent); background: color-mix(in srgb, var(--app-color-accent) 10%, transparent); border-radius: 6px; padding: 5px 8px; }
  .llm-input-row { display: flex; gap: 8px; align-items: flex-end; }
  .llm-ico-btn {
    flex: none; width: 34px; height: 34px; border-radius: 50%;
    display: inline-flex; align-items: center; justify-content: center;
    background: none; border: none; color: var(--app-color-muted); cursor: pointer;
    transition: color 0.15s, background 0.15s;
  }
  .llm-ico-btn:hover { color: var(--app-color-text); background: var(--app-color-surface); }
  .llm-chat-input textarea {
    flex: 1; resize: none; min-height: 24px; max-height: 200px;
    background: transparent; color: var(--app-color-text);
    border: none; padding: 5px 4px; font-size: 13.5px; font-family: inherit;
    line-height: 1.55;
  }
  .llm-chat-input textarea:focus { outline: none; }
  .llm-chat-input textarea::placeholder { color: var(--app-color-muted); }
  /* 圆形发送按钮（元素在 RippleButton 内部，需 :global） */
  :global(.llm-send-btn) {
    flex: none; width: 34px; height: 34px; border-radius: 50%; padding: 0;
    display: inline-flex; align-items: center; justify-content: center;
    background: var(--app-color-accent); color: #fff; border: none;
    transition: opacity 0.15s, transform 0.1s;
  }
  :global(.llm-send-btn:hover:not(:disabled)) { opacity: 0.9; }
  :global(.llm-send-btn:active:not(:disabled)) { transform: scale(0.94); }
  :global(.llm-send-btn:disabled) { background: var(--app-color-surface); color: var(--app-color-muted); cursor: not-allowed; }
  /* 输入框脚注 */
  .llm-input-foot {
    display: flex; align-items: center; justify-content: space-between; gap: 10px;
    padding: 0 4px; font-size: 11px; color: var(--app-color-muted);
  }
  .llm-input-foot-left { display: flex; align-items: center; gap: 8px; min-width: 0; flex-wrap: wrap; }
  .llm-input-foot-model { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  /* ─── 代理模式 ─── */
  .llm-agent-toggle {
    display: inline-flex; align-items: center; gap: 4px; flex: none;
    background: transparent; color: var(--app-color-muted);
    border: 1px solid var(--app-color-border); border-radius: 999px;
    padding: 1px 8px; font-size: 11px; cursor: pointer;
    transition: color 0.15s, background 0.15s, border-color 0.15s;
  }
  .llm-agent-toggle:hover { color: var(--app-color-text); }
  .llm-agent-toggle.on {
    color: var(--app-color-accent);
    background: color-mix(in srgb, var(--app-color-accent) 12%, transparent);
    border-color: color-mix(in srgb, var(--app-color-accent) 40%, transparent);
  }
  .llm-agent-panel {
    display: flex; flex-direction: column; gap: 5px;
    /* 对齐 AI 消息正文（头像 30px + 间距 12px），位于 AI 回复之前（思考位置）；
       下负边距让后续 AI 气泡上靠，面板与回复视觉上连成一组 */
    width: calc(100% - 42px); margin: 0 0 -14px 42px;
    flex-shrink: 0;
    background: color-mix(in srgb, var(--app-color-accent) 4%, transparent);
    border: 1px solid color-mix(in srgb, var(--app-color-accent) 16%, var(--app-color-border));
    border-radius: 12px; padding: 7px 10px;
  }
  .llm-agent-panel-history { opacity: 0.92; }
  .llm-agent-panel-head {
    display: flex; align-items: center; gap: 8px; min-width: 0;
    font-size: 11px; color: var(--app-color-muted); user-select: none;
  }
  .llm-agent-panel-title { font-weight: 600; letter-spacing: 0.4px; }
  .llm-agent-panel-sub { flex: 1; }
  .llm-agent-step {
    display: flex; flex-direction: column; min-width: 0;
    font-size: 11.5px; color: var(--app-color-text);
    border-radius: 8px;
  }
  .llm-agent-step.open { background: color-mix(in srgb, var(--app-color-surface-alt) 55%, transparent); }
  .llm-agent-step-head {
    display: flex; align-items: center; gap: 6px; min-width: 0; width: 100%;
    background: none; border: none; padding: 3px 4px; border-radius: 6px;
    font-size: 11.5px; color: var(--app-color-text); cursor: pointer; text-align: left;
    transition: background 0.12s;
  }
  .llm-agent-step-head:hover { background: color-mix(in srgb, var(--app-color-surface-alt) 80%, transparent); }
  :global(.llm-agent-ico) { flex: none; color: var(--app-color-muted); }
  .llm-agent-step.ok :global(.llm-agent-ico) { color: var(--app-green, #34d399); }
  .llm-agent-step.err :global(.llm-agent-ico) { color: #f87171; }
  .llm-agent-approval :global(.llm-agent-ico) { color: #f59e0b; }
  .llm-agent-step-name { flex: none; font-weight: 600; }
  .llm-agent-step-args {
    flex: none; color: var(--app-color-muted); font-size: 11px;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 40%;
  }
  .llm-agent-step-status {
    flex: none; display: inline-flex; align-items: center; gap: 3px;
    color: var(--app-color-muted); font-size: 11px;
  }
  .llm-agent-step.ok .llm-agent-step-status { color: var(--app-green, #34d399); }
  .llm-agent-step.err .llm-agent-step-status { color: #f87171; }
  .llm-agent-running { color: var(--app-color-accent); animation: llm-caret-blink 1s steps(2) infinite; }
  .llm-agent-step-dur {
    flex: none; color: var(--app-color-muted); font-size: 10.5px;
    font-variant-numeric: tabular-nums;
  }
  .llm-agent-step-retried {
    flex: none; color: #f59e0b; font-size: 10.5px;
    border: 1px solid color-mix(in srgb, #f59e0b 45%, transparent); border-radius: 999px; padding: 0 5px;
  }
  .llm-agent-step-chevron { flex: none; color: var(--app-color-muted); font-size: 10px; margin-left: auto; }
  .llm-agent-step-detail {
    display: flex; flex-direction: column; gap: 7px;
    padding: 5px 8px 7px 26px; min-width: 0;
  }
  .llm-agent-step-field { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .llm-agent-step-field-head {
    display: flex; align-items: center; justify-content: space-between; gap: 8px;
    font-size: 10.5px; color: var(--app-color-muted); user-select: none;
  }
  .llm-agent-step-pre {
    margin: 0; padding: 6px 8px; min-width: 0; max-height: 220px; overflow: auto;
    background: color-mix(in srgb, var(--app-color-surface) 70%, transparent);
    border: 1px solid var(--app-color-border); border-radius: 6px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 10.5px; line-height: 1.5; color: var(--app-color-text);
    white-space: pre-wrap; word-break: break-all;
  }
  .llm-agent-step-copy {
    flex: none; display: inline-flex; align-items: center; gap: 3px;
    background: none; border: none; padding: 1px 6px; border-radius: 5px;
    font-size: 10.5px; color: var(--app-color-muted); cursor: pointer;
  }
  .llm-agent-step-copy:hover { color: var(--app-color-text); background: var(--app-color-surface-alt); }
  .llm-agent-step-retry {
    align-self: flex-start; display: inline-flex; align-items: center; gap: 4px;
    background: color-mix(in srgb, #f59e0b 12%, transparent);
    border: 1px solid color-mix(in srgb, #f59e0b 40%, transparent);
    color: #fbbf24; border-radius: 6px; padding: 3px 9px; font-size: 11px; cursor: pointer;
    transition: background 0.12s;
  }
  .llm-agent-step-retry:hover:not(:disabled) { background: color-mix(in srgb, #f59e0b 22%, transparent); }
  .llm-agent-step-retry:disabled { opacity: 0.6; cursor: wait; }
  :global(.llm-spin) { animation: llm-spin 0.9s linear infinite; }
  @keyframes llm-spin { to { transform: rotate(360deg); } }
  .llm-agent-approval {
    display: flex; flex-direction: column; gap: 4px;
    font-size: 12px; color: var(--app-color-text);
    background: color-mix(in srgb, #f59e0b 10%, transparent);
    border: 1px solid color-mix(in srgb, #f59e0b 45%, transparent);
    border-radius: 8px; padding: 6px 9px;
  }
  .llm-agent-approval-head { display: flex; align-items: center; gap: 8px; min-width: 0; flex-wrap: wrap; }
  .llm-agent-approval-text { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 600; }
  .llm-agent-approval-args {
    flex: none; max-width: 45%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    background: color-mix(in srgb, var(--app-color-surface) 70%, transparent);
    border: 1px solid var(--app-color-border); border-radius: 5px; padding: 1px 6px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 10.5px; color: var(--app-color-muted);
  }
  .llm-agent-approval-actions { flex: none; display: inline-flex; align-items: center; gap: 6px; margin-left: auto; }
  .llm-agent-approve, .llm-agent-reject {
    flex: none; border-radius: 6px; border: none; padding: 3px 10px;
    font-size: 12px; cursor: pointer;
  }
  .llm-agent-approve { background: var(--app-color-accent); color: #fff; }
  .llm-agent-approve:hover { opacity: 0.9; }
  .llm-agent-reject { background: transparent; color: #f87171; border: 1px solid #f8717155; }
  /* ─── 插件管理抽屉 ─── */
  .llm-plugin-drawer { width: min(640px, 92vw); }
  .llm-plugin-body { overflow-y: auto; padding: 12px 18px 18px; display: flex; flex-direction: column; gap: 10px; }
  .llm-plugin-form { display: flex; flex-direction: column; gap: 10px; }
  .llm-plugin-row { display: flex; flex-direction: column; gap: 5px; }
  .llm-plugin-row label, .llm-plugin-row .llm-plugin-label { font-size: 12px; color: var(--app-color-muted); }
  .llm-plugin-row input, .llm-plugin-row textarea {
    background: var(--app-color-card-bg, #141821); border: 1px solid var(--app-color-border, #232833);
    color: var(--app-color-text, #e6e9ef); border-radius: 8px; padding: 8px 10px;
    font-size: 12.5px; outline: none; font-family: inherit; resize: vertical;
  }
  .llm-plugin-row input:focus, .llm-plugin-row textarea:focus { border-color: var(--app-color-accent, #4f8cff); }
  .llm-plugin-code textarea { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; line-height: 1.5; }
  .llm-plugin-check { flex-direction: row !important; align-items: center; gap: 8px; cursor: pointer; }
  .llm-plugin-check input { accent-color: var(--app-color-accent); }
  .llm-plugin-err { color: #f87171; font-size: 12px; }
  .llm-plugin-actions { display: flex; gap: 8px; justify-content: flex-end; }
  .llm-plugin-save { color: #fff; background: var(--app-color-accent); border-color: var(--app-color-accent); }
  .llm-plugin-list { display: flex; flex-direction: column; gap: 8px; }
  .llm-plugin-item {
    display: flex; flex-direction: column; gap: 6px;
    background: var(--app-color-card-bg, #141821); border: 1px solid var(--app-color-border, #232833);
    border-radius: 12px; padding: 10px 12px;
  }
  .llm-plugin-item.off { opacity: 0.62; }
  .llm-plugin-item-hd { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .llm-plugin-item-name { font-weight: 600; font-size: 13.5px; }
  .llm-plugin-item-ver {
    font-size: 10.5px; padding: 1px 7px; border-radius: 999px;
    background: var(--app-color-surface-alt, #1d2230); color: var(--app-color-muted);
  }
  .llm-plugin-item-state { font-size: 11px; color: var(--app-color-accent); }
  .llm-plugin-item.off .llm-plugin-item-state { color: var(--app-color-muted); }
  .llm-plugin-item-tools { margin-left: auto; font-size: 11px; color: var(--app-color-muted); }
  .llm-plugin-item-desc { font-size: 12px; color: var(--app-color-muted); }
  .llm-plugin-item-actions { display: flex; gap: 6px; }
  .llm-plugin-item-actions .llm-btn { padding: 3px 9px; font-size: 12px; }
  .llm-plugin-del { color: #f87171; }

  /* 附件预览 */
  .llm-att-previews { display: flex; flex-wrap: wrap; gap: 8px; }
  .llm-att-prev {
    position: relative; width: 72px; border: 1px solid var(--app-color-border);
    border-radius: 8px; padding: 4px; background: var(--app-color-surface-alt);
  }
  .llm-att-prev img { width: 100%; height: 52px; object-fit: cover; border-radius: 5px; display: block; }
  .llm-att-prev-icon { height: 52px; display: grid; place-items: center; color: var(--app-color-muted); }
  .llm-att-prev-name {
    font-size: 11.5px; color: var(--app-color-muted); margin-top: 3px;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .llm-att-prev.tooBig { opacity: 0.85; }
  .llm-att-prev-warn { font-size: 11.5px; color: #f87171; }
  .llm-att-remove {
    position: absolute; top: -6px; right: -6px; width: 18px; height: 18px;
    border-radius: 50%; border: none; background: #ef4444; color: #fff;
    font-size: 12px; line-height: 1; cursor: pointer; display: flex; align-items: center; justify-content: center;
  }
  .llm-drop-overlay {
    position: absolute; inset: 0; display: flex; align-items: center; justify-content: center;
    background: color-mix(in srgb, var(--app-color-accent) 14%, transparent);
    border-radius: 16px; color: var(--app-color-text); font-size: 14px; pointer-events: none;
  }
  .llm-usage-line { font-size: 12px; color: var(--app-color-muted); width: 100%; max-width: 800px; margin: 0 auto; padding: 0 4px; }
</style>

