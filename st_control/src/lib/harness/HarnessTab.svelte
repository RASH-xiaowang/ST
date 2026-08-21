<script lang="ts">
  // ============================================================
  // Harness — 会话界面（DSH 纯原生迁移 · 阶段 1+2）
  // 左侧会话列表（新建/重命名/删除），右侧对话流（渲染与回放同源：
  // UI 由后端从会话日志投影）。工具步骤随助手回复展示（可展开详情），
  // 危险工具经审批卡批准（支持会话内记住批准）。模型接入复用全局配置，
  // 最近使用的提供方/模型持久化到 Harness 设置。
  // ============================================================
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { errText } from "../format";
  import { harnessApi } from "./services/ipc";
  import { llmApi } from "../llm/services/ipc";
  import type { LlmConfig, AiRole, AgentPlugin } from "../llm/types";
  import { composeSystemPrompt } from "../llm/roleUtils";
  import type {
    HarnessSessionMeta,
    HarnessDisplayMessage,
    HarnessToolStepView,
    HarnessToolInfo,
    HarnessApprovalPayload,
    HarnessSettings,
    HarnessPreset,
    HarnessHook,
    HarnessUsageSummary,
    HarnessHookFired,
    HarnessSessionState,
    HarnessSchedule,
    HarnessWorkflow,
    TerminalSession,
    TerminalLogEntry,
    AttachmentMeta,
    McpServerConfig,
    SkillInfo,
    SearchHit,
    CredentialView,
    LspServerConfig,
    HarnessJobRecord,
    WorkspaceEntity,
    HarnessTrajectory,
    TurnFileView,
    ContextMeterView,
    SubagentNode,
    TrajectoryEntry,
  } from "./types";
  import TrajectoryView from "./components/TrajectoryView.svelte";
  import ToolCard from "./components/ToolCard.svelte";
  import SubagentRow from "./components/SubagentRow.svelte";
  import ModelSelect from "./components/ModelSelect.svelte";
  import MessageBody from "../llm/components/MessageBody.svelte";
  import {
    buildSpeechAttempts,
    plainTextForSpeech,
    blobToWav16kMono,
  } from "../llm/services/voice";
  import { playTtsAudio, stopTtsPlayer, ttsDataUrl, ttsPlayer } from "../llm/services/ttsPlayer.svelte";
  import {
    releaseVoiceRecorder,
    startVoiceRecorder,
    stopVoiceRecorder,
    voiceRecorder,
  } from "../llm/services/voiceRecorder.svelte";
  import PlusIcon from "@lucide/svelte/icons/plus";
  import SendIcon from "@lucide/svelte/icons/send";
  import Trash2Icon from "@lucide/svelte/icons/trash-2";
  import EraserIcon from "@lucide/svelte/icons/eraser";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import SparklesIcon from "@lucide/svelte/icons/sparkles";
  import ArchiveIcon from "@lucide/svelte/icons/archive";
  import ArchiveRestoreIcon from "@lucide/svelte/icons/archive-restore";
  import FolderIcon from "@lucide/svelte/icons/folder";
  import SlidersHorizontalIcon from "@lucide/svelte/icons/sliders-horizontal";
  import MessageSquarePlusIcon from "@lucide/svelte/icons/message-square-plus";
  import CheckIcon from "@lucide/svelte/icons/check";
  import XIcon from "@lucide/svelte/icons/x";
  import WrenchIcon from "@lucide/svelte/icons/wrench";
  import ShieldAlertIcon from "@lucide/svelte/icons/shield-alert";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import SettingsIcon from "@lucide/svelte/icons/settings";
  import PaperclipIcon from "@lucide/svelte/icons/paperclip";
  import ThumbsUpIcon from "@lucide/svelte/icons/thumbs-up";
  import ThumbsDownIcon from "@lucide/svelte/icons/thumbs-down";
  import SearchIcon from "@lucide/svelte/icons/search";
  import GitForkIcon from "@lucide/svelte/icons/git-fork";
  import DownloadIcon from "@lucide/svelte/icons/download";
  import UploadIcon from "@lucide/svelte/icons/upload";
  import Volume2Icon from "@lucide/svelte/icons/volume-2";
  import SquareIcon from "@lucide/svelte/icons/square";
  import MicIcon from "@lucide/svelte/icons/mic";
  // 治理中心 tab 图标
  import BellIcon from "@lucide/svelte/icons/bell";
  import PuzzleIcon from "@lucide/svelte/icons/puzzle";
  import ClockIcon from "@lucide/svelte/icons/clock";
  import TerminalIcon from "@lucide/svelte/icons/terminal";
  import BoxesIcon from "@lucide/svelte/icons/boxes";
  import LightbulbIcon from "@lucide/svelte/icons/lightbulb";
  import Code2Icon from "@lucide/svelte/icons/code-2";
  import KeyRoundIcon from "@lucide/svelte/icons/key-round";
  import BracesIcon from "@lucide/svelte/icons/braces";
  import PlugIcon from "@lucide/svelte/icons/plug";
  import ListTodoIcon from "@lucide/svelte/icons/list-todo";
  import WorkflowIcon from "@lucide/svelte/icons/workflow";
  import FolderOpenIcon from "@lucide/svelte/icons/folder-open";
  import PanelRightIcon from "@lucide/svelte/icons/panel-right";
  import PanelLeftCloseIcon from "@lucide/svelte/icons/panel-left-close";
  import PanelLeftOpenIcon from "@lucide/svelte/icons/panel-left-open";
  import ChevronRightIcon from "@lucide/svelte/icons/chevron-right";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";

  let sessions = $state<HarnessSessionMeta[]>([]);
  let activeId = $state<string | null>(null);
  let messages = $state<HarnessDisplayMessage[]>([]);
  /** 侧栏折叠（DSH 56px 竖轨等价：折叠后仅保留头部图标） */
  let sideCollapsed = $state(false);
  /** 悬浮展开（rail 模式：鼠标移入临时展开，移出自动折叠） */
  let sideHover = $state(false);
  /** 实际展开状态：手动展开 或（手动折叠且悬浮中） */
  const sideExpanded = $derived(sideCollapsed ? sideHover : true);
  /** 折叠/展开切换：悬浮展开中点击 = 固定展开；展开中点击 = 折叠为 rail */
  function toggleSidebar() {
    if (sideCollapsed) {
      sideCollapsed = false;
      sideHover = false;
    } else {
      sideCollapsed = true;
    }
  }
  // ─── 会话视图标签页（DSH 对话|轨迹）：轨迹台账按需加载 ───
  let viewTab = $state<"chat" | "trajectory">("chat");
  let trajectory = $state<HarnessTrajectory | null>(null);
  let trajectoryLoading = $state(false);
  let trajectoryError = $state("");
  /** 回合产物文件（DSH ProducedFiles：编辑/写入的文件 chips） */
  let turnFiles = $state<TurnFileView[]>([]);
  /** 产物按回合归属（user 消息 seq → 该回合产物；DSH turn-tail 语义） */
  const turnFilesByUser = $derived.by(() => {
    const userSeqs = messages
      .filter((m) => m.role === "user" && m.seq > 0)
      .map((m) => m.seq)
      .sort((a, b) => a - b);
    const map = new Map<number, TurnFileView[]>();
    for (const f of turnFiles) {
      const owner = [...userSeqs].reverse().find((s) => s <= f.seq);
      if (owner === undefined) continue;
      const arr = map.get(owner) ?? [];
      arr.push(f);
      map.set(owner, arr);
    }
    return map;
  });
  /** 助手消息所属回合的 user seq（最近的前置 user 消息） */
  function turnUserSeq(assistantSeq: number): number | undefined {
    return [...turnFilesByUser.keys()]
      .filter((s) => s <= assistantSeq)
      .sort((a, b) => b - a)[0];
  }
  /** 详情面板（DSH DetailsPanel 迁移：右侧列显示选中工具调用的输入/输出；
   * running = 工具执行中（实时详情，运行中态）） */
  let detailCall = $state<{
    name: string;
    args: string;
    result: string;
    ok: boolean;
    duration_ms?: number | null;
    running?: boolean;
  } | null>(null);
  /** 会话拖拽排序（DSH 手动排序：交换双方 order 并刷新列表） */
  let dragId: string | null = $state(null);
  let dragOverId: string | null = $state(null);
  async function reorderSessions(fromId: string, toId: string) {
    try {
      await harnessApi.swapSessionOrder(fromId, toId);
      await refreshSessions();
    } catch (e) {
      error = errText(e);
    }
  }

  /** 会话祖先链（DSH 面包屑：近→远 [(id, title)]） */
  let lineage = $state<Array<[string, string]>>([]);
  /** 子代理目录树（DSH SubagentCatalog：会话头树目录弹层） */
  let subagentTree = $state<SubagentNode[]>([]);
  let subagentOpen = $state(false);
  /** 子代理计数（含后代） */
  const subagentTotal = $derived.by(() => {
    let n = 0;
    const walk = (nodes: SubagentNode[]) => {
      for (const nd of nodes) {
        n += 1;
        walk(nd.children);
      }
    };
    walk(subagentTree);
    return n;
  });
  const subagentRunning = $derived.by(() => {
    let n = 0;
    const walk = (nodes: SubagentNode[]) => {
      for (const nd of nodes) {
        if (nd.activity === "running") n += 1;
        walk(nd.children);
      }
    };
    walk(subagentTree);
    return n;
  });
  /** 上下文占用（DSH ContextMeter：输入区环形仪表） */
  let contextMeter = $state<ContextMeterView | null>(null);
  let meterOpen = $state(false);
  // ─── 输入排队（DSH ui-conversation queue 迁移） ───
  let queue = $state<Array<{ id: string; text: string }>>([]);
  let busyEnter = $state<"queue" | "steer">("queue");
  let queueSeq = 0;
  /** 忙碌时按 Enter：入队（queue）或插话到队首（steer） */
  function enqueueInput(text: string) {
    queueSeq += 1;
    const item = { id: `q-${Date.now()}-${queueSeq}`, text };
    queue = busyEnter === "steer" ? [item, ...queue] : [...queue, item];
  }
  function removeQueued(id: string) {
    queue = queue.filter((q) => q.id !== id);
  }
  function editQueued(id: string, text: string) {
    queue = queue.map((q) => (q.id === id ? { ...q, text } : q));
  }
  /** 插话：把某条排队消息排到队首（DSH queue steer 语义） */
  function steerQueued(id: string) {
    const item = queue.find((q) => q.id === id);
    if (!item) return;
    queue = [item, ...queue.filter((q) => q.id !== id)];
  }
  /** 回合结束后自动发送队首消息（DSH queue drain；延迟到 sending 清理后） */
  function drainQueue() {
    if (queue.length === 0) return;
    const next = queue[0];
    queue = queue.slice(1);
    window.setTimeout(() => {
      input = next.text;
      send();
    }, 300);
  }
  let input = $state("");
  let sending = $state(false);
  /** M2：回合令牌。每次发送递增；切换会话时递增作废。过期回合的回调
   *  （streamBuf/liveTools/messages 写入与 finally 收尾）不再触碰当前视图，
   *   防止回合中切换会话把旧回合残文写进新会话（幻影气泡）或 sending 全局
   *   卡死新会话输入。 */
  let turnToken = $state(0);
  let streamBuf = $state("");
  /** 当前回合的实时推理文本（Think 折叠行，DSH ReasoningRow 迁移） */
  let streamReasoning = $state("");
  /** Think 行折叠状态（key = "h"+消息 seq 或 "live"） */
  let thinkOpen = $state<Record<string, boolean>>({});
  function toggleThink(key: string) {
    thinkOpen = { ...thinkOpen, [key]: !thinkOpen[key] };
  }
  /** 当前回合的实时工具步骤 */
  let liveTools = $state<HarnessToolStepView[]>([]);
  let pendingApprovals = $state<HarnessApprovalPayload[]>([]);
  /** 用户提问卡（DSH user-questions 接缝） */
  let pendingQuestions = $state<
    Array<{ id: string; session_id: string; question: string; options: string[]; multi_select?: boolean }>
  >([]);
  let questionDrafts = $state<Record<string, string>>({});
  /** 多选勾选状态（key = question id） */
  let questionChecks = $state<Record<string, Set<string>>>({});
  /** 提问卡翻页（DSH QuestionFlow：多题分页 + 进度） */
  let questionIndex = $state(0);
  function prevQuestion() {
    questionIndex = Math.max(0, questionIndex - 1);
  }
  function nextQuestion() {
    questionIndex = Math.min(pendingQuestions.length - 1, questionIndex + 1);
  }
  function toggleQuestionCheck(qId: string, option: string) {
    const next = new Set(questionChecks[qId] ?? []);
    if (next.has(option)) next.delete(option);
    else next.add(option);
    questionChecks = { ...questionChecks, [qId]: next };
  }
  let error = $state("");
  let loading = $state(true);
  let config = $state<LlmConfig | null>(null);
  let providerId = $state("");
  let modelId = $state("");
  let editingId = $state<string | null>(null);
  let editingTitle = $state("");
  let expandedStep = $state<string | null>(null);
  let expandedMeta = $state<number | null>(null);
  let copiedText = $state("");
  let toolsCatalog = $state<HarnessToolInfo[]>([]);
  let toolsOpen = $state(false);
  /** 工具目录搜索与 schema 展开（重设计后的目录交互） */
  let toolSearch = $state("");
  let openToolSchema = $state<string | null>(null);

  /** 工具目录分组（按功能族） */
  function toolCategory(name: string): string {
    if (name.startsWith("session_")) return "会话管理";
    if (["read_file", "write_file", "edit_file", "list_dir", "glob", "grep", "read_image", "spill_read", "attachment_list"].includes(name)) return "文件与内容";
    if (["exec_command", "job_list", "job_output", "job_kill", "terminal_open", "terminal_send", "terminal_read", "terminal_signal", "terminal_close", "terminal_list", "workspace_list", "workspace_create", "workspace_switch", "shell_run"].includes(name)) return "执行环境";
    if (["web_search", "fetch_web_page", "search_knowledge_base", "get_current_time"].includes(name)) return "信息检索";
    if (["todo_write", "plan_enter", "plan_exit", "goal_set", "goal_get", "subagent", "send_message", "subagent_list", "subagent_output", "schedule_list", "schedule_create", "schedule_delete", "schedule_run", "workflow_list", "workflow_run"].includes(name)) return "编排与协作";
    if (["skill_list", "skill_load", "lsp_hover", "lsp_definition", "lsp_references", "lsp_implementation"].includes(name)) return "技能与语言服务";
    return "系统与集成";
  }
  const toolGroups = $derived.by(() => {
    const q = toolSearch.trim().toLowerCase();
    const filtered = toolsCatalog.filter(
      (t) =>
        !q ||
        t.name.toLowerCase().includes(q) ||
        (t.description ?? "").toLowerCase().includes(q),
    );
    const groups = new Map<string, HarnessToolInfo[]>();
    for (const t of filtered) {
      const c = toolCategory(t.name);
      const arr = groups.get(c) ?? [];
      arr.push(t);
      groups.set(c, arr);
    }
    return [...groups.entries()];
  });
  const toolTotalCount = $derived(toolsCatalog.length);
  const toolApprovalCount = $derived(toolsCatalog.filter((t) => t.requires_approval).length);
  let unlistenApproval: (() => void) | null = null;
  /** 顶部轻提示（分叉/导出/预设切换反馈，3 秒自动消失） */
  let notice = $state("");
  let noticeTimer: ReturnType<typeof window.setTimeout> | null = null;
  /** 当前会话的预设覆盖值（"" = 跟随全局） */
  let sessionPresetId = $state("");
  // ─── 设置 / 钩子 / 预设 抽屉 ───
  let drawerOpen = $state(false);
  let drawerTab = $state<
    | "settings"
    | "hooks"
    | "presets"
    | "schedule"
    | "workflow"
    | "terminal"
    | "skill"
    | "cli"
    | "credentials"
    | "lsp"
    | "mcp"
    | "plugins"
    | "jobs"
  >("settings");
  let settingsForm = $state<HarnessSettings>({ last_provider_id: "", last_model: "" });
  /** 会话级推理等级（DSH reasoningEffort；"" = 跟随提供方默认） */
  let effortId = $state("");
  let settingsMsg = $state("");
  let hooks = $state<HarnessHook[]>([]);
  let hooksMsg = $state("");
  let hookFiredLog = $state<HarnessHookFired[]>([]);
  let presets = $state<HarnessPreset[]>([]);
  let presetMsg = $state("");
  let presetDraft = $state<{
    id: string;
    name: string;
    description: string;
    disabled: string[];
    prompt: string;
  } | null>(null);
  let usage = $state<HarnessUsageSummary | null>(null);
  // ─── AI 角色注入（原「AI 聊天」角色功能迁移：会话级持久化，日志投影） ───
  let aiRoles = $state<AiRole[]>([]);
  let roleId = $state("");
  let roleMsg = $state("");
  // ─── 编排（todo / plan / goal / schedule / workflow） ───
  let sessionState = $state<HarnessSessionState | null>(null);
  let schedules = $state<HarnessSchedule[]>([]);
  let scheduleMsg = $state("");
  let scheduleDraft = $state<{
    id: string;
    name: string;
    prompt: string;
    interval: number;
    enabled: boolean;
  } | null>(null);
  let workflows = $state<HarnessWorkflow[]>([]);
  let workflowMsg = $state("");
  let workflowDraft = $state<{
    id: string;
    name: string;
    description: string;
    stages: string;
  } | null>(null);
  // ─── 终端（执行世界） ───
  let terminals = $state<TerminalSession[]>([]);
  let terminalMsg = $state("");
  let terminalInputs = $state<Record<string, string>>({});
  let terminalLogs = $state<Record<string, TerminalLogEntry[]>>({});
  let terminalBusy = $state<string | null>(null);
  /** PTY 运行状态（真终端；false 时走普通命令模式） */
  let ptyRunning = $state<Record<string, boolean>>({});
  // ─── 附件 ───
  let attachments = $state<AttachmentMeta[]>([]);
  let attachBusy = $state(false);
  // ─── 会话查询 / 技能 / CLI / 反馈 ───
  let searchQuery = $state("");
  let searchHits = $state<SearchHit[]>([]);
  let skills = $state<SkillInfo[]>([]);
  let skillMsg = $state("");
  let skillDraft = $state<{ id: string; content: string } | null>(null);
  // ─── 动态插件（DSH extensions / code-runtime：模型自修改 + 代码执行） ───
  let plugins = $state<AgentPlugin[]>([]);
  let pluginMsg = $state("");
  let pluginDraft = $state<{
    id: string;
    name: string;
    description: string;
    enabled: boolean;
    tools: string;
  } | null>(null);
  let cliInput = $state("");
  let cliOutput = $state("");
  let cliBusy = $state(false);
  // ─── 凭据 / LSP ───
  let credentials = $state<CredentialView[]>([]);
  let credentialMsg = $state("");
  let credentialDraft = $state<{ key: string; value: string; storeEnv: boolean } | null>(null);
  let lspServers = $state<LspServerConfig[]>([]);
  let lspMsg = $state("");
  let lspDraft = $state<{ id: string; name: string; command: string; args: string; extensions: string; enabled: boolean } | null>(null);
  // ─── MCP（管理 UI + 配置束导入导出） ───
  let mcpServers = $state<McpServerConfig[]>([]);
  let mcpMsg = $state("");
  let mcpDraft = $state<{ id: string; name: string; command: string; args: string; env: string; cwd: string; enabled: boolean } | null>(null);
  let mcpImportJson = $state("");
  let portMsg = $state("");
  // ─── 后台作业（DSH jobs） ───
  let jobs = $state<HarnessJobRecord[]>([]);
  let jobsMsg = $state("");
  let jobOutputs = $state<Record<string, string>>({});
  let jobExpanded = $state<string | null>(null);
  // ─── 工作区（DSH workspace） ───
  let workspaces = $state<WorkspaceEntity[]>([]);
  let workspaceMsg = $state("");
  let workspaceNewTitle = $state("");
  // ─── 语音（TTS 朗读 + STT 输入） ───
  let speakingIdx = $state<number | null>(null);
  let voiceStatus = $state("");
  let micStream: MediaStream | null = null;

  const activeSession = $derived(sessions.find((s) => s.id === activeId) ?? null);
  /** 当前模型的元数据（能力标签 / 推理等级 / 上下文窗口；DSH 模型元数据） */
  const currentModelMeta = $derived.by(() => {
    const p = config?.providers.find((x) => x.id === providerId);
    return p?.model_meta?.[modelId] ?? null;
  });
  /** 当前模型声明的推理等级（空 = 未声明，不展示选择器） */
  const modelEfforts = $derived(currentModelMeta?.reasoning_efforts ?? []);
  /** 保存会话级推理等级（DSH U11 两级选择器：模型 + effort） */
  async function changeEffort(effort: string) {
    effortId = effort;
    settingsForm.reasoning_effort = effort || null;
    await saveSettingsForm();
    notify(effort ? `推理等级已设为 ${effort}` : "推理等级跟随提供方默认");
  }
  /** 侧栏会话按工作区分组（DSH WorkspaceBrowser 轻量版：组头 + 折叠；
   * 归档会话独立「已归档」分组，DSH archiveSession 语义） */
  const workspaceGroups = $derived.by(() => {    const groups: { id: string; title: string; sessions: HarnessSessionMeta[] }[] = [];
    const active = sessions.filter((s) => !s.archived);
    const def = active.filter((s) => !s.workspace_id);
    if (def.length > 0) groups.push({ id: "", title: "默认工作区", sessions: def });
    for (const w of workspaces) {
      const wsSessions = active.filter((s) => s.workspace_id === w.id);
      if (wsSessions.length > 0) groups.push({ id: w.id, title: w.title, sessions: wsSessions });
    }
    const archived = sessions.filter((s) => s.archived);
    if (archived.length > 0) groups.push({ id: "archived", title: "已归档", sessions: archived });
    return groups;
  });
  let collapsedWs = $state<Set<string>>(new Set());
  /** 当前激活工作区标题（hero 工作区 chip；DSH WorkspacePicker） */
  const activeWorkspaceTitle = $derived(
    workspaces.find((w) => w.id === activeSession?.workspace_id)?.title ?? "",
  );
  /** 当前会话预设标题（hero Agent 预设座位；DSH AgentPresetSeat） */
  const sessionPresetTitle = $derived(
    presets.find((p) => p.id === sessionPresetId)?.name ?? "",
  );
  function toggleWsGroup(id: string) {
    const next = new Set(collapsedWs);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    collapsedWs = next;
  }
  const providers = $derived(config?.providers.filter((p) => p.enabled) ?? []);
  const models = $derived(providers.find((p) => p.id === providerId)?.models ?? []);
  const canSend = $derived(input.trim().length > 0 && !sending && !!activeId);

  function fmtDuration(ms?: number): string {
    if (ms == null) return "";
    return ms >= 1000 ? `${(ms / 1000).toFixed(1)}s` : `${Math.round(ms)}ms`;
  }

  /** 统计条墙钟格式（DSH 风格：895m59s；不足 1 分钟显示秒） */
  function fmtWall(ms: number): string {
    const s = Math.round(ms / 1000);
    if (s < 60) return `${s}s`;
    const m = Math.floor(s / 60);
    const r = s % 60;
    return `${m}m${r.toString().padStart(2, "0")}s`;
  }

  /** 秒格式（首 token 平均）：3.1s */
  function fmtSec(ms: number): string {
    return `${(ms / 1000).toFixed(1)}s`;
  }

  /** token 紧凑格式（DSH 风格：3.3M / 2857M / 512K） */
  function fmtTok(n: number): string {
    if (n >= 1e9) return `${(n / 1e9).toFixed(1)}B`;
    if (n >= 1e6) return `${(n / 1e6).toFixed(1)}M`;
    if (n >= 1e3) return `${(n / 1e3).toFixed(0)}K`;
    return String(n);
  }

  function prettyText(s?: string): string {
    if (!s) return "";
    try {
      return JSON.stringify(JSON.parse(s), null, 2);
    } catch {
      return s;
    }
  }

  async function copyText(text: string) {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      /* 剪贴板不可用时静默忽略 */
    }
    copiedText = text.slice(0, 20);
    window.setTimeout(() => {
      if (copiedText === text.slice(0, 20)) copiedText = "";
    }, 1500);
  }

  function notify(text: string) {
    notice = text;
    if (noticeTimer) window.clearTimeout(noticeTimer);
    noticeTimer = window.setTimeout(() => {
      notice = "";
      noticeTimer = null;
    }, 3000);
  }

  async function refreshSessions() {
    try {
      sessions = await harnessApi.listSessions();
      // 列表刷新后同步当前会话的预设覆盖显示
      if (activeId) {
        sessionPresetId = sessions.find((s) => s.id === activeId)?.preset_id ?? "";
      }
    } catch {
      /* 列表刷新失败保持现状 */
    }
  }

  async function selectSession(id: string) {
    if (id !== activeId) {
      // M2：切换到不同会话 → 作废进行中回合的令牌并解除 sending 占用，
      // 新会话立即可发送（旧回合继续在后台完成，日志为准；切回时重载可见）
      turnToken++;
      sending = false;
    }
    activeId = id;
    error = "";
    streamBuf = "";
    liveTools = [];
    pendingApprovals = [];
    expandedStep = null;
    shownCount = MSG_PAGE;
    viewTab = "chat";
    trajectory = null;
    trajectoryError = "";
    turnFiles = [];
    sessionPresetId = sessions.find((s) => s.id === id)?.preset_id ?? "";
    // 面包屑（DSH 会话头：祖先链加载；近→远）
    lineage = [];
    harnessApi.sessionLineage(id).then((chain) => (lineage = chain)).catch(() => {});
    // 子代理目录（DSH SubagentCatalog：树数据加载）
    subagentTree = [];
    harnessApi.subagentCatalog(id).then((tree) => (subagentTree = tree)).catch(() => {});
    // 回显会话级 AI 角色（日志投影；name 匹配角色表，否则视为自定义）
    roleId = "";
    harnessApi
      .getSessionRole(id)
      .then((r) => {
        if (r.name) {
          roleId = aiRoles.find((x) => x.name === r.name)?.id ?? "";
        }
      })
      .catch(() => {});
    try {
      messages = await harnessApi.displayMessages(id);
      loadUsage().catch(() => {});
      loadSessionState().catch(() => {});
      loadAttachments().catch(() => {});
      loadTurnFiles(id).catch(() => {});
      loadContextMeter(id).catch(() => {});
    } catch (e) {
      error = errText(e);
      messages = [];
    }
  }

  /** 切换视图标签页：轨迹台账按需加载（日志投影，渲染与回放同源；
   *  每次切换刷新，保证与最新日志一致） */
  async function switchView(tab: "chat" | "trajectory") {
    viewTab = tab;
    if (tab === "trajectory" && activeId) {
      await loadTrajectory(activeId);
    }
  }

  async function loadTrajectory(id: string) {
    trajectoryLoading = true;
    trajectoryError = "";
    // 超时保护：invoke 若 10 秒未返回，放弃等待并提示（避免无限「加载中」）
    const timer = window.setTimeout(() => {
      trajectoryLoading = false;
      trajectoryError = "轨迹加载超时（10 秒），请重试或刷新页面";
    }, 10000);
    try {
      trajectory = await harnessApi.trajectory(id);
    } catch (e) {
      trajectoryError = errText(e);
    } finally {
      window.clearTimeout(timer);
      trajectoryLoading = false;
    }
  }

  /** 回合产物文件（DSH ProducedFiles：变更类工具路径，日志投影） */
  async function loadTurnFiles(id: string) {
    try {
      turnFiles = await harnessApi.turnFiles(id);
    } catch {
      turnFiles = [];
    }
  }

  /** 打开文件/目录（产物 chip 点击，系统默认程序） */
  async function openHarnessPath(path: string) {
    try {
      await harnessApi.openPath(path);
    } catch (e) {
      notify(`打开失败：${errText(e)}`);
    }
  }

  /** 上下文占用刷新（会话切换 / 回合完成后调用） */
  async function loadContextMeter(id: string) {
    try {
      contextMeter = await harnessApi.contextMeter(id);
    } catch {
      contextMeter = null;
    }
  }

  /** token 紧凑格式（与统计条一致） */
  function fmtCtxTok(n: number): string {
    if (n >= 1e6) return `${(n / 1e6).toFixed(1)}M`;
    if (n >= 1e3) return `${(n / 1e3).toFixed(0)}K`;
    return String(n);
  }

  /** 详情面板：展示工具调用的输入/输出（DSH DetailsPanel 迁移；
   * running = 执行中态，输出区显示「运行中…」） */
  function openDetail(
    name: string,
    args: string,
    result: string | undefined,
    ok: boolean,
    durationMs?: number | null,
    running?: boolean,
  ) {
    detailCall = {
      name,
      args: args ?? "",
      result: result ?? "",
      ok,
      duration_ms: durationMs ?? null,
      running: running ?? false,
    };
  }

  /** Inspect 轨迹行 → 右侧详情面板（DSH Inspect 语义：请求/消息/工具检查器；
   * 轨迹行「检查」按钮入口，详情面板含 Timing/Usage 遥测区块） */
  function inspectTrajectoryEntry(e: TrajectoryEntry) {
    if (e.kind === "tool") {
      openDetail(e.name, e.args, e.result, e.ok, e.duration_ms);
    } else if (e.kind === "user") {
      openDetail("用户消息", "", e.content, true);
    } else if (e.kind === "assistant") {
      openDetail("助手消息", "", e.content, true);
    } else {
      openDetail(
        `系统更新（${e.event}）`,
        "",
        `${e.summary}\n${e.detail}`.trim(),
        true,
      );
    }
  }

  // ─── 斜杠命令菜单（DSH ui-input-trigger / ui-commands 迁移） ───
  const SLASH_COMMANDS: { name: string; desc: string }[] = [
    { name: "plan", desc: "进入计划模式（可带方案文本）；/plan off 退出" },
    { name: "exit", desc: "退出计划模式" },
    { name: "goal", desc: "设置会话目标：/goal <目标文本>" },
    { name: "feedback", desc: "提交反馈：/feedback <内容>" },
    { name: "compact", desc: "立即压缩上下文" },
    { name: "skill", desc: "加载技能：/skill <技能id>" },
    { name: "model", desc: "切换模型：/model <模型名>" },
    { name: "permission", desc: "切换访问模式：/permission <模式>" },
    { name: "help", desc: "显示全部命令帮助" },
  ];
  let slashOpen = $state(false);
  let slashFilter = $state("");
  let slashIndex = $state(0);
  let inputRef: HTMLTextAreaElement | null = $state(null);
  const slashMatches = $derived.by(() => {
    const q = slashFilter.toLowerCase();
    return SLASH_COMMANDS.filter((c) => !q || c.name.startsWith(q));
  });
  // @ 提及菜单（DSH ui-input-trigger @ 源：技能引用；子代理目录未接入时仅技能）
  let atOpen = $state(false);
  let atFilter = $state("");
  let atIndex = $state(0);
  const atMatches = $derived.by(() => {
    const q = atFilter.toLowerCase();
    return skills.filter(
      (s) => !q || s.id.toLowerCase().includes(q) || s.name.toLowerCase().includes(q),
    );
  });
  /** textarea 输入检测：行首 / + 字母（未含空格）时弹出命令菜单；@ + 名称时弹出提及菜单 */
  function onInputKeydown(e: KeyboardEvent) {
    if (slashOpen) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        slashIndex = Math.min(slashIndex + 1, slashMatches.length - 1);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        slashIndex = Math.max(slashIndex - 1, 0);
      } else if (e.key === "Enter" || e.key === "Tab") {
        const pick = slashMatches[slashIndex];
        if (pick) {
          e.preventDefault();
          pickSlashCommand(pick.name);
        }
      } else if (e.key === "Escape") {
        slashOpen = false;
      }
      return;
    }
    if (atOpen) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        atIndex = Math.min(atIndex + 1, atMatches.length - 1);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        atIndex = Math.max(atIndex - 1, 0);
      } else if (e.key === "Enter" || e.key === "Tab") {
        const pick = atMatches[atIndex];
        if (pick) {
          e.preventDefault();
          pickAtSkill(pick);
        }
      } else if (e.key === "Escape") {
        atOpen = false;
      }
    }
  }
  function onInputValueChange() {
    const m = input.match(/^\/[a-z]*$/i);
    if (m) {
      slashFilter = m[0].slice(1).toLowerCase();
      slashIndex = 0;
      slashOpen = true;
      atOpen = false;
      return;
    }
    const at = input.match(/^@[a-z0-9_-]*$/i);
    if (at) {
      atFilter = at[0].slice(1).toLowerCase();
      atIndex = 0;
      atOpen = true;
      slashOpen = false;
      return;
    }
    slashOpen = false;
    atOpen = false;
  }
  /** 选择命令：插入 "/name " 并保持焦点继续输入参数 */
  function pickSlashCommand(name: string) {
    input = `/${name} `;
    slashOpen = false;
    window.setTimeout(() => inputRef?.focus(), 0);
  }
  /** 选择技能提及：插入 "/skill <id> "（DSH /skill 手势等价：加载技能指令） */
  function pickAtSkill(skill: SkillInfo) {
    input = `/skill ${skill.id} `;
    atOpen = false;
    window.setTimeout(() => inputRef?.focus(), 0);
  }

  /** 人工目标操作（DSH GoalBar：暂停/恢复/完成/清除/阻塞/编辑） */
  async function goalAction(action: string, objective?: string) {
    if (!activeId) return;
    try {
      await harnessApi.goalAction(activeId, action, null, objective ?? null);
      await loadSessionState();
    } catch (e) {
      notify(`目标操作失败：${errText(e)}`);
    }
  }
  // GoalBar 内联编辑（DSH GoalBar 编辑态：Enter 保存 / Esc 取消）
  let goalEditing = $state(false);
  let goalDraft = $state("");
  function startGoalEdit() {
    goalDraft = sessionState?.goal ?? "";
    goalEditing = true;
  }
  function cancelGoalEdit() {
    goalEditing = false;
  }
  async function saveGoalEdit() {
    const text = goalDraft.trim();
    if (!text) return;
    goalEditing = false;
    await goalAction("edit", text);
  }

  // ─── 图片灯箱（DSH ImageLightbox 迁移：附件图片点击查看原图） ───
  let lightboxSrc = $state<string | null>(null);
  let lightboxName = $state("");
  function openImageLightbox(path: string, name: string) {
    lightboxSrc = convertFileSrc(path);
    lightboxName = name;
  }

  // ─── 会话级权限芯片（DSH PermissionSelect 迁移） ───
  const SANDBOX_LABELS: Record<string, string> = {
    "read-only": "只读",
    "workspace-write": "工作区写入",
    "danger-full-access": "完全访问",
  };
  /** 切换沙箱模式：完整访问需风险确认（DSH RiskConfirmation 语义） */
  async function changeSandboxMode(mode: string) {
    const current = settingsForm.sandbox_mode ?? "workspace-write";
    if (mode === current) return;
    if (mode === "danger-full-access" && current !== "danger-full-access") {
      if (!window.confirm("确认启用「完全访问」？\n\n代理将可读取和修改任意路径的文件、执行任意命令。\n\n请确认你已了解风险并愿意继续。")) {
        return;
      }
    }
    settingsForm.sandbox_mode = mode;
    await saveSettingsForm();
    notify(`访问模式已切换为「${SANDBOX_LABELS[mode] ?? mode}」`);
  }

  /** plan 输入芯片（DSH PlanChip 迁移）：点击退出计划模式 */
  function exitPlanFromChip() {
    if (!activeId || sending) return;
    input = "/plan off";
    send();
  }

  // ─── AI 角色注入（原「AI 聊天」角色功能迁移） ───
  async function loadAiRoles() {
    try {
      const all = await llmApi.getAiRoles();
      aiRoles = (all ?? []).filter((r) => r.enabled);
    } catch {
      aiRoles = [];
    }
  }

  async function applyRole(nextId: string) {
    roleId = nextId;
    if (!activeId) return;
    const role = aiRoles.find((r) => r.id === nextId);
    const name = role?.name ?? "";
    const prompt = role ? composeSystemPrompt(role) : "";
    try {
      await harnessApi.setSessionRole(activeId, name, prompt);
      roleMsg = role ? `已应用角色「${role.name}」（本会话后续回合生效）` : "已清除角色";
      // 角色注入落日志后重载投影：meta 行（渲染与回放同源）立即可见
      if (activeId) {
        harnessApi
          .displayMessages(activeId)
          .then((msgs) => {
            messages = msgs;
          })
          .catch(() => {});
      }
    } catch (e) {
      roleMsg = "应用角色失败：" + errText(e);
    }
    notice = roleMsg;
    if (noticeTimer) clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (notice = ""), 3000);
  }

  async function createSession() {
    // 新建会话归属当前激活会话所在工作区（无则默认工作区），
    // 与 DSH「在工作区下新建会话」语义一致
    await createSessionIn(activeSession?.workspace_id ?? "");
  }

  async function createSessionIn(workspaceId: string) {
    try {
      const meta = await harnessApi.createSession(workspaceId);
      sessions = [meta, ...sessions];
      activeId = meta.id;
      messages = [];
      error = "";
    } catch (e) {
      error = errText(e);
    }
  }

  function startRename(s: HarnessSessionMeta) {
    editingId = s.id;
    editingTitle = s.title || "新会话";
  }

  async function commitRename(id: string) {
    const title = editingTitle.trim();
    editingId = null;
    if (!title || title === (sessions.find((s) => s.id === id)?.title ?? "")) return;
    try {
      await harnessApi.renameSession(id, title);
      await refreshSessions();
    } catch (e) {
      error = errText(e);
    }
  }

  /** B19：LLM 生成会话标题（手动触发，消耗一次模型调用） */
  async function generateSessionTitle(s: HarnessSessionMeta) {
    if (s.message_count === 0) {
      notify("会话暂无消息，无法生成标题");
      return;
    }
    try {
      const title = await harnessApi.generateTitle(s.id);
      notify(`已生成标题：${title}`);
      await refreshSessions();
    } catch (e) {
      notify(`标题生成失败：${errText(e)}`);
    }
  }

  async function deleteSession(id: string) {
    // 删除前确认：误删即丢失整段会话日志
    if (!window.confirm("确认删除该会话？会话日志将一并删除。")) return;
    try {
      await harnessApi.deleteSession(id);
      sessions = sessions.filter((s) => s.id !== id);
      if (activeId === id) {
        const next = sessions[0];
        if (next) await selectSession(next.id);
        else {
          activeId = null;
          messages = [];
          await createSession();
        }
      }
    } catch (e) {
      error = errText(e);
    }
  }

  /** 清空会话聊天记录（维护会话：消息/工具日志清空，会话与预设/角色保留） */
  async function clearSession(id: string) {
    if (!window.confirm("确认清空该会话的聊天记录？消息与工具日志将删除（会话本身保留）。")) return;
    try {
      await harnessApi.clearSession(id);
      if (activeId === id) {
        messages = await harnessApi.displayMessages(id);
        streamBuf = "";
        liveTools = [];
        usage = null;
        notice = "聊天记录已清空";
        if (noticeTimer) clearTimeout(noticeTimer);
        noticeTimer = setTimeout(() => (notice = ""), 3000);
      }
      await refreshSessions();
    } catch (e) {
      error = errText(e);
    }
  }

  /** 归档/恢复会话（DSH workspace.archiveSession：归档保留日志，移入「已归档」分组） */
  async function setSessionArchived(id: string, archived: boolean) {
    try {
      await harnessApi.setSessionArchived(id, archived);
      await refreshSessions();
    } catch (e) {
      error = errText(e);
    }
  }

  // ─── 分叉 / 回放导出 / 会话预设 ───
  async function forkSessionAt(seq: number) {
    if (!activeId || seq <= 0) return;
    try {
      const meta = await harnessApi.forkSession(activeId, seq);
      sessions = [meta, ...sessions];
      await selectSession(meta.id);
      notify(`已分叉：${meta.title}`);
    } catch (e) {
      error = errText(e);
    }
  }

  async function exportSessionMd() {
    if (!activeId) return;
    try {
      const base = (activeSession?.title || "harness-session").replace(/[\\/:*?"<>|]/g, "_");
      const path = await saveDialog({
        title: "导出会话转写（Markdown 回放）",
        defaultPath: `${base}.md`,
        filters: [{ name: "Markdown", extensions: ["md"] }],
      });
      if (!path) return;
      const out = await harnessApi.exportSession(activeId, path);
      notify(`会话转写已导出：${out}`);
    } catch (e) {
      error = errText(e);
    }
  }

  async function onSessionPresetChange() {
    if (!activeId) return;
    try {
      await harnessApi.setSessionPreset(activeId, sessionPresetId);
      notify(sessionPresetId ? "会话预设已切换（仅本会话生效）" : "会话预设已重置为全局默认");
      await refreshSessions();
    } catch (e) {
      error = errText(e);
    }
  }

  // ─── 审批 ───
  async function approvePending(a: HarnessApprovalPayload, remember = false) {
    // 信任写入审批所属会话（L6：不能写当前 activeId——子代理审批/切换会话
    // 后会把信任错误记到别的会话）；M8：携带参数指纹，仅相同参数命令免审批
    if (remember && a.session_id) {
      try {
        await harnessApi.trustTool(a.session_id, a.tool, a.arguments);
      } catch {
        /* 信任记录失败不影响审批 */
      }
    }
    pendingApprovals = pendingApprovals.filter((x) => x.id !== a.id);
    try {
      await harnessApi.approveTool(a.id);
    } catch {
      /* 审批可能已超时 */
    }
  }

  async function rejectPending(a: HarnessApprovalPayload) {
    pendingApprovals = pendingApprovals.filter((x) => x.id !== a.id);
    try {
      await harnessApi.rejectTool(a.id);
    } catch {
      /* 审批可能已超时 */
    }
  }

  // ─── 用户提问（ask_user_question 接缝） ───
  async function answerQuestion(
    q: { id: string; session_id: string; question: string; options: string[]; multi_select?: boolean },
    answer: string,
  ) {
    const text = answer.trim();
    if (!text) return;
    pendingQuestions = pendingQuestions.filter((x) => x.id !== q.id);
    questionIndex = Math.max(0, questionIndex - 1);
    try {
      await harnessApi.answerQuestion(q.id, text);
    } catch {
      /* 提问可能已超时 */
    }
  }
  /** 提交多选答案（DSH QuestionFlow 多选：勾选项以「, 」拼接） */
  async function answerQuestionMulti(
    q: { id: string; session_id: string; question: string; options: string[]; multi_select?: boolean },
    selected: Set<string>,
  ) {
    const text = [...selected].join(", ");
    if (!text) return;
    await answerQuestion(q, text);
  }

  // ─── 发送 ───
  async function send() {
    if (!canSend) return;
    if (!activeId) {
      await createSession();
      if (!activeId) return;
    }
    // 前端拦截的人工命令（DSH interaction/commands 语义，不消耗模型回合）：
    // /model <名称> 切换模型；/permission <模式> 切换访问模式（U27）
    const frontCmd = input.trim().match(/^\/(model|permission)\s+(.+)$/);
    if (frontCmd) {
      const cmd = frontCmd[1];
      const arg = frontCmd[2].trim();
      input = "";
      if (cmd === "model") {
        if (models.includes(arg)) {
          modelId = arg;
          persistSelection();
          notify(`模型已切换为 ${arg}`);
        } else {
          notify(`未知模型：${arg}`);
        }
      } else if (cmd === "permission") {
        if (arg in SANDBOX_LABELS) {
          await changeSandboxMode(arg);
        } else {
          notify(`未知访问模式：${arg}（read-only / workspace-write / danger-full-access）`);
        }
      }
      return;
    }
    // 输入排队（DSH busyEnter）：回合进行中按 Enter → 入队（steer 插队首），
    // 当前回合结束后自动发送（drainQueue）
    if (sending) {
      const pending = input.trim();
      if (pending) {
        enqueueInput(pending);
        input = "";
        notify(busyEnter === "steer" ? "已插话到队首，当前回合结束后发送" : "已排队，当前回合结束后自动发送");
      }
      return;
    }
    // 图像模态发送前校验（DSH 图片模态声明）：当前模型未声明「视觉」时
    // 拒绝图片附件并点名模型（附件图片无法被该模型读取）
    const hasImage = attachments.some((a) => a.kind === "image");
    if (hasImage && !(currentModelMeta?.tags ?? []).includes("视觉")) {
      notify(`当前模型 ${modelId || "(未选择)"} 未声明图片输入能力（模型元数据缺「视觉」标签），无法发送图片附件`);
      return;
    }
    const text = input.trim();
    input = "";
    // M2：记录本回合归属会话与令牌（会话切换/新回合接管时失效）
    const turnSession = activeId;
    const token = ++turnToken;
    sending = true;
    error = "";
    streamBuf = "";
    streamReasoning = "";
    liveTools = [];
    expandedStep = null;
    // 乐观展示：后端落日志后的投影与此一致（seq 未知，先用 0 占位）
    messages = [...messages, { role: "user", content: text, seq: 0 }];
    try {
      await harnessApi.chatStream(
        activeId,
        providerId || null,
        modelId || null,
        text,
        (ev) => {
          // M2：会话已切换 → 过期回调不写当前视图
          if (turnSession !== activeId) return;
          // 新回合已接管：仅允许「完成/错误」按日志重载（用户切回原会话时
          // 仍能同步已完成回合；不追加本地状态、不排空队列防循环）
          if (token !== turnToken) {
            if (ev.type === "done" || ev.type === "error") {
              const aid = activeId;
              if (aid)
                harnessApi
                  .displayMessages(aid)
                  .then((msgs) => (messages = msgs))
                  .catch(() => {});
            }
            return;
          }
          if (ev.type === "assistant_chunk") {
            if (ev.reasoning_delta) {
              // 推理增量：Think 行实时追加（不进入正文流）
              streamReasoning += ev.reasoning_delta;
            } else {
              streamBuf += ev.delta;
            }
          } else if (ev.type === "assistant_tool_calls") {
            liveTools = [
              ...liveTools,
              ...ev.calls.map((c) => ({
                id: c.id,
                name: c.name,
                args: c.arguments,
                status: "running",
              })),
            ];
          } else if (ev.type === "tool_result") {
            liveTools = liveTools.map((s) =>
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
            streamBuf = "";
            streamReasoning = "";
            messages = [
              ...messages,
              { role: "assistant", content: ev.content, tools: liveTools, seq: ev.seq },
            ];
            liveTools = [];
            refreshSessions().catch(() => {});
            loadUsage().catch(() => {});
            loadSessionState().catch(() => {});
            const cmId = activeId;
            if (cmId) loadContextMeter(cmId).catch(() => {});
            // 日志为准重载消息：模型端 session_clear/session_delete 等维护操作
            // 后，界面与日志投影保持一致（渲染与回放同源）
            const aid = activeId;
            if (!aid) return;
            harnessApi
              .displayMessages(aid)
              .then((msgs) => {
                messages = msgs;
                // 当前会话被模型删除（或会话列表变化）：自动落到可用会话
                harnessApi.listSessions().then((list) => {
                  if (!list.some((s) => s.id === activeId)) {
                    if (list[0]) selectSession(list[0].id);
                    else createSession();
                  }
                }).catch(() => {});
              })
              .catch(() => {});
            // 队列排空：自动发送下一条排队消息（DSH queue drain）
            drainQueue();
          } else if (ev.type === "error") {
            error = ev.message;
            drainQueue();
            // M1：错误回合以日志为准重载（乐观 user 消息若未落日志则移除，
            // 防止幻影消息长期残留且被 retryLastTurn 双写）
            const aid = activeId;
            if (aid)
              harnessApi
                .displayMessages(aid)
                .then((msgs) => (messages = msgs))
                .catch(() => {});
          } else if (ev.type === "goal_auto_round") {
            // 目标自动续跑提示（DSH goal-round-driver）
            notify(`目标自动续跑 ${ev.round}/${ev.max}`);
            loadSessionState().catch(() => {});
          }
        },
      );
    } catch (e) {
      // 用户主动停止时静默（已生成内容保留），其余错误照常提示
      if (!stoppedByUser) error = errText(e);
      // M1：失败路径以日志为准重载（提供方解析/配额等失败发生在用户消息
      // 落日志之前，乐观 user 消息不在日志里，需移除避免 UI 与日志分叉）
      const aid = activeId;
      if (aid)
        harnessApi
          .displayMessages(aid)
          .then((msgs) => (messages = msgs))
          .catch(() => {});
    } finally {
      // M2：过期回合（会话已切换或新回合接管）不向当前视图写局部状态
      if (turnSession !== activeId || token !== turnToken) return;
      sending = false;
      stoppedByUser = false;
      // 停止中断的回合：把已生成的部分以本地消息保留（seq=0 占位，
      // 后续以日志投影为准重载）；正常完成时 done 已清空缓冲
      if (streamBuf) {
        messages = [...messages, { role: "assistant", content: streamBuf, tools: liveTools, seq: 0 }];
        streamBuf = "";
        liveTools = [];
      } else {
        streamBuf = "";
      }
    }
  }

  /** UI「停止」：请求中断当前回合（工具循环下一检查点生效，已生成内容保留） */
  let stoppedByUser = false;
  async function stopTurn() {
    stoppedByUser = true;
    if (!activeId) return;
    try {
      await harnessApi.cancelTurn(activeId);
    } catch {
      /* 取消请求本身失败则忽略（回合继续） */
    }
  }

  /** 重试上一回合（DSH 回合失败重试卡）：取最后一条用户消息重新发送 */
  async function retryLastTurn() {
    if (sending || !activeId) return;
    const lastUser = [...messages].reverse().find((m) => m.role === "user");
    if (!lastUser?.content) return;
    input = lastUser.content;
    await send();
  }

  // ─── 设置持久化（最近使用的提供方/模型；合并保留 guard/preset 配置） ───
  let currentSettings = $state<HarnessSettings>({ last_provider_id: "", last_model: "" });

  function persistSelection() {
    if (!providerId) return;
    currentSettings = {
      ...currentSettings,
      last_provider_id: providerId,
      last_model: modelId,
    };
    harnessApi.saveSettings(currentSettings).catch(() => {});
  }

  // ─── 模型座（DSH ModelSelect：弹层菜单回调用；替代头部三个原生下拉） ───
  function seatProviderChange(id: string) {
    providerId = id;
    const p = providers.find((x) => x.id === id);
    modelId = p?.default_model ?? p?.models[0] ?? "";
    persistSelection();
  }

  function seatModelChange(model: string) {
    modelId = model;
    persistSelection();
  }

  // ─── 设置 / 钩子 / 预设 ───
  async function loadUsage() {
    if (!activeId) {
      usage = null;
      return;
    }
    try {
      usage = await harnessApi.usageSummary(activeId);
    } catch {
      usage = null;
    }
  }

  async function loadSessionState() {
    if (!activeId) {
      sessionState = null;
      return;
    }
    try {
      sessionState = await harnessApi.sessionState(activeId);
    } catch {
      sessionState = null;
    }
  }

  // ─── 附件 ───
  async function loadAttachments() {
    if (!activeId) {
      attachments = [];
      return;
    }
    try {
      attachments = await harnessApi.listAttachments(activeId);
    } catch {
      attachments = [];
    }
  }

  async function attachFromDialog() {
    if (!activeId || attachBusy) return;
    attachBusy = true;
    try {
      const picked = await openDialog({ multiple: false, title: "选择要附加的文件" });
      const path = typeof picked === "string" ? picked : picked?.[0];
      if (!path) return;
      await harnessApi.attachFile(activeId, path);
      await loadAttachments();
    } catch (e) {
      error = errText(e);
    } finally {
      attachBusy = false;
    }
  }

  // ─── 整页拖放遮罩（DSH DropOverlay：拖入文件即显示，放下后附加） ───
  let dragOver = $state(false);
  let dragDepth = 0;
  function onDocDragEnter(e: DragEvent) {
    if (!activeId) return;
    if (e.dataTransfer?.types.includes("Files")) {
      dragDepth += 1;
      dragOver = true;
    }
  }
  function onDocDragOver(e: DragEvent) {
    if (!activeId) return;
    if (e.dataTransfer?.types.includes("Files")) e.preventDefault();
  }
  function onDocDragLeave() {
    dragDepth = Math.max(0, dragDepth - 1);
    if (dragDepth === 0) dragOver = false;
  }
  async function onDocDrop(e: DragEvent) {
    dragDepth = 0;
    dragOver = false;
    if (!activeId) return;
    const files = e.dataTransfer?.files;
    if (!files || files.length === 0) return;
    e.preventDefault();
    let okCount = 0;
    for (const f of Array.from(files)) {
      // WebView2（Tauri 2）提供 File.path 扩展属性
      const path = (f as File & { path?: string }).path;
      if (!path) continue;
      try {
        await harnessApi.attachFile(activeId, path);
        okCount += 1;
      } catch {
        /* 单文件失败继续其余 */
      }
    }
    if (okCount > 0) {
      await loadAttachments();
      notify(`已附加 ${okCount} 个文件`);
    }
  }

  // ─── 会话查询 ───
  async function runSearch() {
    const q = searchQuery.trim();
    if (!q) {
      searchHits = [];
      return;
    }
    try {
      searchHits = await harnessApi.searchSessions(q);
    } catch {
      searchHits = [];
    }
  }

  // ─── 反馈（DSH ui-message-feedback：👍👎 + 补充说明备注） ───
  let feedbackComment = $state<Record<string, string>>({});
  let feedbackDraftOpen = $state<number | null>(null);
  async function sendFeedback(rating: "good" | "bad", messageSeq?: number, comment?: string) {
    if (!activeId) return;
    try {
      await harnessApi.submitFeedback(activeId, rating, comment, messageSeq);
    } catch {
      /* 反馈失败静默 */
    }
  }
  /** 打开补充说明输入（DSH MessageFeedbackActions 备注：textarea + 保存/取消） */
  function openFeedbackNote(seq: number) {
    feedbackDraftOpen = feedbackDraftOpen === seq ? null : seq;
    feedbackComment[seq] = feedbackComment[seq] ?? "";
  }
  /** 保存补充说明（与最近一次评价关联；无评价时按 current 保存） */
  async function saveFeedbackNote(seq: number) {
    const comment = (feedbackComment[seq] ?? "").trim();
    if (!comment) return;
    await sendFeedback("good", seq, comment);
    feedbackDraftOpen = null;
  }

  // ─── 语音：TTS 朗读（提供方 TTS → 系统语音兜底） ───
  async function speakMessage(m: HarnessDisplayMessage, i: number) {
    if (!m.content) return;
    if (ttsPlayer.speaking) {
      // 再点一次 = 停止播报
      stopTtsPlayer();
      speakingIdx = null;
      voiceStatus = "";
      return;
    }
    const text = plainTextForSpeech(m.content);
    const attempts = buildSpeechAttempts(
      { provider_id: providerId, model: modelId },
      providers,
    );
    let lastErr = "";
    for (const a of attempts) {
      if (!a.provider_id || !a.model) continue;
      try {
        const res = await llmApi.generateSpeech({
          provider_id: a.provider_id,
          model: a.model,
          input: text,
          voice: "",
          response_format: "mp3",
          speed: 1,
        });
        speakingIdx = i;
        await playTtsAudio(ttsDataUrl(res), i);
        speakingIdx = null;
        return;
      } catch (e) {
        lastErr = errText(e);
      }
    }
    // 全部提供方失败 → Windows SAPI 系统语音兜底（零配置）
    try {
      const res = await llmApi.synthesizeNativeSpeech(text, -2);
      speakingIdx = i;
      await playTtsAudio(ttsDataUrl(res), i, { viaNative: true });
      speakingIdx = null;
    } catch (e2) {
      error = `语音合成失败：${lastErr || errText(e2)}`;
    }
  }

  // ─── 语音：STT 输入（麦克风 → 本地/云端转写 → 输入框） ───
  async function toggleVoiceInput() {
    if (voiceRecorder.recording) {
      stopVoiceRecorder(false);
      return;
    }
    try {
      micStream = await navigator.mediaDevices.getUserMedia({ audio: true });
      startVoiceRecorder(micStream, {
        onBlob: async (blob) => {
          if (micStream) {
            micStream.getTracks().forEach((t) => t.stop());
            micStream = null;
          }
          voiceStatus = "识别中…";
          try {
            const wav = await blobToWav16kMono(blob);
            if (!wav) throw new Error("音频转码失败");
            const r = await llmApi.transcribeVoiceAudio(wav, "wav");
            input = (input + (input ? "\n" : "") + r.text.trim()).slice(0, 20000);
            voiceStatus = "";
            notify(`语音识别完成（${r.engine}）`);
          } catch (e) {
            voiceStatus = "";
            error = `语音识别失败：${errText(e)}`;
          }
        },
        onStatus: (text) => (voiceStatus = text),
      });
    } catch (e) {
      error = `麦克风不可用：${errText(e)}`;
    }
  }

  // ─── 技能 ───
  async function loadSkills() {
    try {
      skills = await harnessApi.listSkills();
    } catch {
      skills = [];
    }
  }

  function startNewSkill() {
    skillDraft = { id: "", content: "# 技能名\n\n技能说明与操作步骤。" };
  }

  function startEditSkill(s: SkillInfo) {
    skillDraft = { id: s.id, content: s.content };
  }

  async function saveSkillDraft() {
    if (!skillDraft) return;
    skillMsg = "";
    const content = skillDraft.content.trim();
    if (!content) {
      skillMsg = "技能内容不能为空";
      return;
    }
    const id = skillDraft.id.trim() || `skill-${Date.now().toString(36)}`;
    const name = content.split("\n").find((l) => l.startsWith("# "))?.slice(2).trim() ?? id;
    const description = content.split("\n").find((l) => l.trim() && !l.startsWith("#"))?.trim() ?? "";
    try {
      await harnessApi.saveSkill({ id, name, description, content });
      skillDraft = null;
      await loadSkills();
      skillMsg = `已保存「${name}」`;
      window.setTimeout(() => {
        if (skillMsg?.startsWith("已保存")) skillMsg = "";
      }, 1500);
    } catch (e) {
      skillMsg = errText(e);
    }
  }

  async function deleteSkill(id: string) {
    try {
      await harnessApi.deleteSkill(id);
      await loadSkills();
    } catch (e) {
      skillMsg = errText(e);
    }
  }

  // ─── 动态插件（DSH extensions / code-runtime） ───
  async function loadPlugins() {
    try {
      plugins = await llmApi.listAgentPlugins();
    } catch {
      plugins = [];
    }
  }

  function startNewPlugin() {
    pluginDraft = {
      id: "",
      name: "",
      description: "",
      enabled: true,
      tools: '[{"name":"","description":"","code":"return args;"}]',
    };
    pluginMsg = "";
  }

  function startEditPlugin(p: AgentPlugin) {
    pluginDraft = {
      id: p.id,
      name: p.name,
      description: p.description,
      enabled: p.enabled,
      tools: JSON.stringify(p.tools, null, 2),
    };
    pluginMsg = "";
  }

  async function savePluginDraft() {
    if (!pluginDraft) return;
    pluginMsg = "";
    try {
      const tools = JSON.parse(pluginDraft.tools || "[]") as AgentPlugin["tools"];
      if (!Array.isArray(tools) || tools.length === 0) {
        pluginMsg = "至少需要一个工具定义";
        return;
      }
      const saved = await llmApi.saveAgentPlugin({
        id: pluginDraft.id,
        name: pluginDraft.name,
        description: pluginDraft.description,
        enabled: pluginDraft.enabled,
        tools,
        versions: [],
        created_at: "",
        updated_at: "",
      });
      pluginDraft = null;
      await loadPlugins();
      pluginMsg = `已保存插件「${saved.name}」`;
      window.setTimeout(() => {
        if (pluginMsg?.startsWith("已保存")) pluginMsg = "";
      }, 1500);
    } catch (e) {
      pluginMsg = errText(e);
    }
  }

  async function deletePlugin(p: AgentPlugin) {
    try {
      await llmApi.deleteAgentPlugin(p.id);
      await loadPlugins();
      pluginMsg = `已删除「${p.name}」`;
      window.setTimeout(() => {
        if (pluginMsg?.startsWith("已删除")) pluginMsg = "";
      }, 1500);
    } catch (e) {
      pluginMsg = errText(e);
    }
  }

  async function togglePlugin(p: AgentPlugin) {
    try {
      await llmApi.setAgentPluginEnabled(p.id, !p.enabled);
      await loadPlugins();
    } catch (e) {
      pluginMsg = errText(e);
    }
  }

  /** 前端沙箱执行模型编写的代码（插件工具 / run_code），回传结果给后端 */
  async function execPluginTool(payload: {
    id: string;
    name: string;
    args: string;
    code: string;
    session_id?: string;
  }) {
    let ok = true;
    let result = "";
    const logs: string[] = [];
    try {
      // B23：ctx.tools——脚本内可调用其它 Harness 工具（经会话派发，
      // 遵守审批/沙箱/预设作用域）。用无锁 IPC：外层 run_code/插件派发
      // 已持有会话锁，嵌套调用再取锁会死锁
      const tools =
        payload.session_id
          ? new Proxy(
              {},
              {
                get: (_t, toolName: string) =>
                  async (toolArgs?: unknown) => {
                    const r = await harnessApi.executeToolNoLock(
                      payload.session_id!,
                      toolName,
                      JSON.stringify(toolArgs ?? {}),
                    );
                    return r?.ok === false ? { __err: r.result } : r?.result;
                  },
              },
            )
          : undefined;
      const ctx = {
        fetch: (input: RequestInfo | URL, init?: RequestInit) => fetch(input, init),
        log: (...xs: unknown[]) => {
          logs.push(xs.map(String).join(" "));
        },
        tools,
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

  /** B2：workflow JS 编排执行器（DSH workflow 组合子）。
   * 与 run_code 同沙箱，但 ctx 额外提供 agent/parallel/pipeline：
   * - ctx.agent(prompt): 派生子代理（后端 fork 子会话 + 一轮对话），返回结论
   * - ctx.parallel(thunks): 并发执行（Promise.all）
   * - ctx.pipeline(items, ...stages): 逐阶段流水线（每阶段对当前数组做 map） */
  async function execWorkflowJs(payload: {
    id: string;
    name: string;
    args: string;
    code: string;
    session_id?: string;
  }) {
    let ok = true;
    let result = "";
    const logs: string[] = [];
    try {
      const agent = async (prompt: string) => {
        if (!payload.session_id) throw new Error("缺少 session_id，无法派生子代理");
        return await harnessApi.workflowAgent(payload.session_id, String(prompt));
      };
      const parallel = async (thunks: Array<() => Promise<unknown>>) =>
        await Promise.all(thunks.map((t) => t()));
      const pipeline = async (
        items: unknown[],
        ...stages: Array<(v: unknown) => Promise<unknown>>
      ) => {
        let cur = items;
        for (const stage of stages) {
          cur = await Promise.all(cur.map((x) => stage(x)));
        }
        return cur;
      };
      const ctx = {
        fetch: (input: RequestInfo | URL, init?: RequestInit) => fetch(input, init),
        log: (...xs: unknown[]) => {
          logs.push(xs.map(String).join(" "));
        },
        agent,
        parallel,
        pipeline,
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

  // ─── CLI ───
  async function runCli() {
    const input = cliInput.trim();
    if (!input || cliBusy) return;
    cliBusy = true;
    try {
      const out = await harnessApi.cli(input);
      cliOutput = `$ ${input}\n${out}\n\n${cliOutput}`.slice(0, 4000);
      cliInput = "";
      refreshSessions().catch(() => {});
      if (activeId) {
        refreshOrchestration().catch(() => {});
      }
    } catch (e) {
      cliOutput = `$ ${input}\n错误：${errText(e)}\n\n${cliOutput}`.slice(0, 4000);
    } finally {
      cliBusy = false;
    }
  }

  // ─── 凭据 / LSP ───
  async function loadCredentials() {
    try {
      credentials = await harnessApi.credentialList();
    } catch {
      credentials = [];
    }
  }

  async function putCredential() {
    if (!credentialDraft) return;
    credentialMsg = "";
    try {
      await harnessApi.credentialPut(
        credentialDraft.key.trim(),
        credentialDraft.value,
        credentialDraft.storeEnv,
      );
      credentialDraft = null;
      await loadCredentials();
      credentialMsg = "已保存（值已掩码）";
      window.setTimeout(() => {
        if (credentialMsg === "已保存（值已掩码）") credentialMsg = "";
      }, 1500);
    } catch (e) {
      credentialMsg = errText(e);
    }
  }

  async function deleteCredential(key: string) {
    try {
      await harnessApi.credentialDelete(key);
      await loadCredentials();
    } catch (e) {
      credentialMsg = errText(e);
    }
  }

  async function loadLspServers() {
    try {
      lspServers = await harnessApi.listLspServers();
    } catch {
      lspServers = [];
    }
  }

  async function saveLspDraft() {
    if (!lspDraft) return;
    lspMsg = "";
    if (!lspDraft.name.trim() || !lspDraft.command.trim()) {
      lspMsg = "请填写名称与命令";
      return;
    }
    const server: LspServerConfig = {
      id: lspDraft.id || `lsp-${Date.now().toString(36)}`,
      name: lspDraft.name.trim(),
      command: lspDraft.command.trim(),
      args: lspDraft.args.split(",").map((s) => s.trim()).filter((s) => s.length > 0),
      extensions: lspDraft.extensions.split(",").map((s) => s.trim().toLowerCase()).filter((s) => s.length > 0),
      enabled: lspDraft.enabled,
    };
    try {
      const list = lspServers.filter((s) => s.id !== server.id);
      list.push(server);
      lspServers = await harnessApi.saveLspServers(list);
      lspDraft = null;
      lspMsg = `已保存「${server.name}」`;
      window.setTimeout(() => {
        if (lspMsg?.startsWith("已保存")) lspMsg = "";
      }, 1500);
    } catch (e) {
      lspMsg = errText(e);
    }
  }

  async function deleteLsp(id: string) {
    try {
      lspServers = await harnessApi.saveLspServers(lspServers.filter((s) => s.id !== id));
    } catch (e) {
      lspMsg = errText(e);
    }
  }

  // ─── 后台作业（DSH jobs） ───
  async function loadJobs() {
    if (!activeId) {
      jobs = [];
      return;
    }
    try {
      jobs = await harnessApi.jobList(activeId);
    } catch (e) {
      jobsMsg = errText(e);
    }
  }

  async function toggleJobOutput(id: string) {
    if (jobExpanded === id) {
      jobExpanded = null;
      return;
    }
    jobExpanded = id;
    jobsMsg = "";
    try {
      jobOutputs = { ...jobOutputs, [id]: await harnessApi.jobOutput(id) };
    } catch (e) {
      jobOutputs = { ...jobOutputs, [id]: `读取失败：${errText(e)}` };
    }
  }

  async function killJob(id: string) {
    jobsMsg = "";
    try {
      await harnessApi.jobKill(id);
      await loadJobs();
      jobsMsg = `已请求终止 ${id}`;
    } catch (e) {
      jobsMsg = errText(e);
    }
  }

  // ─── 工作区（DSH workspace） ───
  async function loadWorkspaces() {
    try {
      workspaces = await harnessApi.listWorkspaces();
    } catch (e) {
      workspaceMsg = errText(e);
    }
  }

  async function createWorkspace() {
    const title = workspaceNewTitle.trim();
    if (!title) {
      workspaceMsg = "请输入工作区名称";
      return;
    }
    workspaceMsg = "";
    try {
      await harnessApi.createWorkspace(title);
      workspaceNewTitle = "";
      await loadWorkspaces();
      workspaceMsg = `已创建工作区「${title}」`;
    } catch (e) {
      workspaceMsg = errText(e);
    }
  }

  async function deleteWorkspace(id: string) {
    if (!window.confirm("删除工作区将连同其目录内容一并删除，确认？")) return;
    workspaceMsg = "";
    try {
      await harnessApi.deleteWorkspace(id);
      await loadWorkspaces();
    } catch (e) {
      workspaceMsg = errText(e);
    }
  }

  async function switchWorkspace(id: string) {
    workspaceMsg = "";
    settingsForm.workspace_id = id;
    try {
      await saveSettingsForm();
      workspaceMsg = id === "default" ? "已切换到默认工作区" : `已切换到 ${id}`;
    } catch (e) {
      workspaceMsg = errText(e);
    }
  }

  // ─── MCP 管理 + 配置束导入导出 ───
  async function loadMcpServers() {    try {
      mcpServers = await harnessApi.listMcpServers();
    } catch {
      mcpServers = [];
    }
  }

  async function saveMcpDraft() {
    if (!mcpDraft) return;
    mcpMsg = "";
    if (!mcpDraft.name.trim() || !mcpDraft.command.trim()) {
      mcpMsg = "请填写名称与命令";
      return;
    }
    const envMap: Record<string, string> = {};
    for (const pair of mcpDraft.env.split(",").map((s) => s.trim()).filter((s) => s.length > 0)) {
      const eq = pair.indexOf("=");
      if (eq > 0) envMap[pair.slice(0, eq).trim()] = pair.slice(eq + 1).trim();
    }
    const server: McpServerConfig = {
      id: mcpDraft.id || `mcp-${Date.now().toString(36)}`,
      name: mcpDraft.name.trim(),
      command: mcpDraft.command.trim(),
      args: mcpDraft.args.split(",").map((s) => s.trim()).filter((s) => s.length > 0),
      enabled: mcpDraft.enabled,
      env: envMap,
      cwd: mcpDraft.cwd.trim() || null,
    };
    try {
      const list = mcpServers.filter((s) => s.id !== server.id);
      list.push(server);
      mcpServers = await harnessApi.saveMcpServers(list);
      mcpDraft = null;
      mcpMsg = `已保存「${server.name}」（工具已注册）`;
      window.setTimeout(() => {
        if (mcpMsg?.startsWith("已保存")) mcpMsg = "";
      }, 1500);
    } catch (e) {
      mcpMsg = errText(e);
    }
  }

  async function deleteMcp(id: string) {
    try {
      mcpServers = await harnessApi.saveMcpServers(mcpServers.filter((s) => s.id !== id));
    } catch (e) {
      mcpMsg = errText(e);
    }
  }

  /** 配置束导出到文件（预设+技能+MCP+LSP+钩子） */
  async function exportBundleFile() {
    portMsg = "";
    try {
      const path = await saveDialog({
        title: "导出 Harness 配置束",
        defaultPath: "harness-bundle.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!path) return;
      const out = await harnessApi.exportBundle(path);
      portMsg = `已导出配置束：${out}`;
    } catch (e) {
      portMsg = errText(e);
    }
  }

  /** 配置束导出为 JSON 文本（复制到剪贴板） */
  async function copyBundleJson() {
    portMsg = "";
    try {
      const json = await harnessApi.exportBundle(null);
      await navigator.clipboard.writeText(json);
      portMsg = "配置束 JSON 已复制到剪贴板";
    } catch (e) {
      portMsg = errText(e);
    }
  }

  /** 从文件导入配置束 */
  async function importBundleFile() {
    portMsg = "";
    try {
      const path = await openDialog({
        title: "导入 Harness 配置束",
        filters: [{ name: "JSON", extensions: ["json"] }],
        multiple: false,
      });
      if (!path || Array.isArray(path)) return;
      const count = await harnessApi.importBundle(path, null);
      portMsg = `已导入 ${count} 条配置`;
      await Promise.all([loadMcpServers(), loadSkills(), loadLspServers()]);
      try {
        presets = await harnessApi.listPresets();
      } catch {
        /* 刷新失败保持现状 */
      }
      try {
        hooks = await harnessApi.listHooks();
      } catch {
        /* 刷新失败保持现状 */
      }
    } catch (e) {
      portMsg = errText(e);
    }
  }

  /** 粘贴 JSON 文本导入配置束 */
  async function importBundleJson() {
    portMsg = "";
    const text = mcpImportJson.trim();
    if (!text) {
      portMsg = "请粘贴配置束 JSON";
      return;
    }
    try {
      const count = await harnessApi.importBundle(null, text);
      portMsg = `已导入 ${count} 条配置`;
      mcpImportJson = "";
      await Promise.all([loadMcpServers(), loadSkills(), loadLspServers()]);
      try {
        presets = await harnessApi.listPresets();
      } catch {
        /* 刷新失败保持现状 */
      }
      try {
        hooks = await harnessApi.listHooks();
      } catch {
        /* 刷新失败保持现状 */
      }
    } catch (e) {
      portMsg = errText(e);
    }
  }

  async function refreshOrchestration() {
    loadUsage().catch(() => {});
    loadSessionState().catch(() => {});
    if (activeId) {
      // 定时/工作流可能在后台追加了回合：重新投影消息
      try {
        messages = await harnessApi.displayMessages(activeId);
      } catch {
        /* 刷新失败保持现状 */
      }
    }
  }

  async function openDrawer(
    tab:
      | "settings"
      | "hooks"
      | "presets"
      | "schedule"
      | "workflow"
      | "terminal"
      | "skill"
      | "cli"
      | "credentials"
      | "lsp"
      | "mcp"
      | "jobs",
  ) {
    drawerTab = tab;
    drawerOpen = true;
    settingsMsg = "";
    hooksMsg = "";
    presetMsg = "";
    scheduleMsg = "";
    workflowMsg = "";
    terminalMsg = "";
    try {
      const s = await harnessApi.getSettings();
      currentSettings = {
        last_provider_id: s.last_provider_id,
        last_model: s.last_model,
        tool_timeout_secs: s.tool_timeout_secs ?? null,
        max_agent_rounds: s.max_agent_rounds ?? null,
        preset_id: s.preset_id ?? null,
        allow_workspace_escape: s.allow_workspace_escape ?? false,
        sandbox_mode: s.sandbox_mode ?? "workspace-write",
        workspace_id: s.workspace_id ?? "",
        context_budget_tokens: s.context_budget_tokens ?? null,
        enable_compaction: s.enable_compaction ?? true,
        busy_enter: s.busy_enter ?? null,
        reasoning_effort: s.reasoning_effort ?? null,
        web_search_provider: s.web_search_provider ?? null,
      };
      settingsForm = { ...currentSettings };
      busyEnter = (s.busy_enter ?? "queue") === "steer" ? "steer" : "queue";
      effortId = s.reasoning_effort ?? "";
    } catch {
      /* 设置读取失败保持现状 */
    }
    try {
      workspaces = await harnessApi.listWorkspaces();
    } catch {
      workspaces = [];
    }
    try {
      hooks = await harnessApi.listHooks();
    } catch {
      hooks = [];
    }
    try {
      presets = await harnessApi.listPresets();
    } catch {
      presets = [];
    }
    try {
      schedules = await harnessApi.listSchedules();
    } catch {
      schedules = [];
    }
    try {
      workflows = await harnessApi.listWorkflows();
    } catch {
      workflows = [];
    }
    try {
      terminals = await harnessApi.listTerminals();
      if (terminals.length > 0) await loadTerminalLogs(terminals[0].id);
      for (const t of terminals) {
        refreshPtyStatus(t.id).catch(() => {});
      }
    } catch {
      terminals = [];
    }
    try {
      skills = await harnessApi.listSkills();
    } catch {
      skills = [];
    }
    try {
      credentials = await harnessApi.credentialList();
    } catch {
      credentials = [];
    }
    try {
      await loadLspServers();
    } catch {
      lspServers = [];
    }
    try {
      await loadMcpServers();
    } catch {
      mcpServers = [];
    }
    try {
      await loadJobs();
    } catch {
      jobs = [];
    }
  }

  // ─── 终端 ───
  async function loadTerminalLogs(id: string) {
    try {
      terminalLogs = { ...terminalLogs, [id]: await harnessApi.terminalLogs(id) };
    } catch {
      terminalLogs = { ...terminalLogs, [id]: [] };
    }
  }

  async function createTerminal() {
    try {
      const t = await harnessApi.createTerminal("终端");
      terminals = [t, ...terminals];
      await loadTerminalLogs(t.id);
    } catch (e) {
      terminalMsg = errText(e);
    }
  }

  async function deleteTerminal(id: string) {
    try {
      await harnessApi.deleteTerminal(id);
      terminals = terminals.filter((x) => x.id !== id);
      ptyRunning = { ...ptyRunning };
      delete ptyRunning[id];
    } catch (e) {
      terminalMsg = errText(e);
    }
  }

  async function terminalSend(id: string) {
    const input = (terminalInputs[id] ?? "").trim();
    if (!input || terminalBusy) return;
    terminalBusy = id;
    terminalMsg = "";
    try {
      // PTY 运行时走真终端；否则退回非 PTY 状态保持执行
      if (ptyRunning[id]) {
        await harnessApi.terminalSendPty(id, input);
      } else {
        await harnessApi.terminalSend(id, input);
      }
      terminalInputs = { ...terminalInputs, [id]: "" };
      await loadTerminalLogs(id);
      const list = await harnessApi.listTerminals();
      terminals = list;
      if (ptyRunning[id]) await refreshPtyStatus(id);
    } catch (e) {
      terminalMsg = errText(e);
    } finally {
      terminalBusy = null;
    }
  }

  // ─── PTY 真终端（ConPTY） ───
  async function refreshPtyStatus(id: string) {
    try {
      const s = await harnessApi.terminalPtyStatus(id);
      ptyRunning = { ...ptyRunning, [id]: s.running };
    } catch {
      ptyRunning = { ...ptyRunning, [id]: false };
    }
  }

  async function startPty(id: string) {
    terminalMsg = "";
    try {
      await harnessApi.terminalStartPty(id, 30, 120);
      ptyRunning = { ...ptyRunning, [id]: true };
      terminalMsg = "PTY 已启动（powershell 交互终端，保持进程内状态）";
      await loadTerminalLogs(id);
      const list = await harnessApi.listTerminals();
      terminals = list;
    } catch (e) {
      terminalMsg = `PTY 启动失败（已保留普通命令模式）：${errText(e)}`;
      ptyRunning = { ...ptyRunning, [id]: false };
    }
  }

  async function stopPty(id: string) {
    try {
      await harnessApi.terminalStopPty(id);
      ptyRunning = { ...ptyRunning, [id]: false };
      terminalMsg = "PTY 已停止（可继续使用普通命令模式）";
    } catch (e) {
      terminalMsg = errText(e);
    }
  }

  async function saveSettingsForm() {
    settingsMsg = "";
    const timeout = settingsForm.tool_timeout_secs;
    if (timeout != null && (timeout < 5 || timeout > 300)) {
      settingsMsg = "工具超时需在 5~300 秒之间";
      return;
    }
    const rounds = settingsForm.max_agent_rounds;
    if (rounds != null && (rounds < 1 || rounds > 12)) {
      settingsMsg = "最大工具轮次需在 1~12 之间";
      return;
    }
    try {
      const saved = await harnessApi.saveSettings({
        ...settingsForm,
        preset_id: settingsForm.preset_id || null,
        busy_enter: busyEnter,
        reasoning_effort: effortId || null,
      });
      currentSettings = saved;
      settingsMsg = "已保存";
      window.setTimeout(() => {
        if (settingsMsg === "已保存") settingsMsg = "";
      }, 1500);
    } catch (e) {
      settingsMsg = errText(e);
    }
  }

  function addHook() {
    hooks = [
      ...hooks,
      {
        id: `hook-${Date.now().toString(36)}`,
        event: "turn_end",
        matcher: "",
        command: "",
        enabled: false,
      },
    ];
  }

  async function saveHooksList() {
    hooksMsg = "";
    try {
      hooks = await harnessApi.saveHooks(hooks);
      hooksMsg = "已保存";
      window.setTimeout(() => {
        if (hooksMsg === "已保存") hooksMsg = "";
      }, 1500);
    } catch (e) {
      hooksMsg = errText(e);
    }
  }

  function startNewPreset() {
    presetDraft = { id: "", name: "", description: "", disabled: [], prompt: "" };
  }

  function startEditPreset(p: HarnessPreset) {
    presetDraft = {
      id: p.id,
      name: p.name,
      description: p.description,
      disabled: [...p.disabled_tools],
      prompt: p.prompt_sections.map((s) => s.content).join("\n\n"),
    };
  }

  async function savePresetDraft() {
    if (!presetDraft) return;
    presetMsg = "";
    if (!presetDraft.name.trim()) {
      presetMsg = "请填写预设名称";
      return;
    }
    const p: HarnessPreset = {
      id: presetDraft.id,
      name: presetDraft.name.trim(),
      description: presetDraft.description.trim(),
      disabled_tools: presetDraft.disabled,
      overrides: {},
      prompt_sections: presetDraft.prompt.trim()
        ? [{ order: 10, title: "preset", content: presetDraft.prompt.trim() }]
        : [],
      created_at: "",
      updated_at: "",
    };
    try {
      const saved = await harnessApi.savePreset(p);
      presetDraft = null;
      presets = await harnessApi.listPresets();
      presetMsg = `已保存「${saved.name}」`;
      window.setTimeout(() => {
        if (presetMsg?.startsWith("已保存")) presetMsg = "";
      }, 1500);
    } catch (e) {
      presetMsg = errText(e);
    }
  }

  async function deletePreset(id: string) {
    if (!window.confirm("确认删除该预设？")) return;
    try {
      await harnessApi.deletePreset(id);
      presets = await harnessApi.listPresets();
      // 若删除的是当前默认预设，回退设置
      if (settingsForm.preset_id === id) {
        settingsForm.preset_id = null;
        await harnessApi.saveSettings({ ...settingsForm, preset_id: null });
      }
    } catch (e) {
      presetMsg = errText(e);
    }
  }

  // ─── 定时任务 ───
  function startNewSchedule() {
    scheduleDraft = { id: "", name: "", prompt: "", interval: 30, enabled: true };
  }

  async function saveScheduleDraft() {
    if (!scheduleDraft) return;
    scheduleMsg = "";
    if (!scheduleDraft.name.trim() || !scheduleDraft.prompt.trim()) {
      scheduleMsg = "请填写名称与提示词";
      return;
    }
    const s: HarnessSchedule = {
      id: scheduleDraft.id,
      name: scheduleDraft.name.trim(),
      session_id: activeId ?? "",
      prompt: scheduleDraft.prompt.trim(),
      interval_minutes: scheduleDraft.interval,
      enabled: scheduleDraft.enabled,
      next_run_at: 0,
      last_run_at: null,
      created_at: "",
    };
    try {
      const saved = await harnessApi.saveSchedule(s);
      scheduleDraft = null;
      schedules = await harnessApi.listSchedules();
      scheduleMsg = `已保存「${saved.name}」`;
      window.setTimeout(() => {
        if (scheduleMsg?.startsWith("已保存")) scheduleMsg = "";
      }, 1500);
    } catch (e) {
      scheduleMsg = errText(e);
    }
  }

  async function deleteSchedule(id: string) {
    try {
      await harnessApi.deleteSchedule(id);
      schedules = await harnessApi.listSchedules();
    } catch (e) {
      scheduleMsg = errText(e);
    }
  }

  async function runScheduleNow(id: string) {
    scheduleMsg = "已触发运行，稍候刷新会话…";
    try {
      await harnessApi.runScheduleNow(id);
      window.setTimeout(() => {
        refreshOrchestration().catch(() => {});
        if (scheduleMsg === "已触发运行，稍候刷新会话…") scheduleMsg = "";
      }, 6000);
    } catch (e) {
      scheduleMsg = errText(e);
    }
  }

  // ─── 工作流 ───
  function startNewWorkflow() {
    workflowDraft = {
      id: "",
      name: "",
      description: "",
      stages: "阶段一 | 请输出 STAGE_ONE_DONE",
    };
  }

  function startEditWorkflow(w: HarnessWorkflow) {
    workflowDraft = {
      id: w.id,
      name: w.name,
      description: w.description,
      stages: w.stages.map((s) => `${s.name} | ${s.prompt}`).join("\n"),
    };
  }

  async function saveWorkflowDraft() {
    if (!workflowDraft) return;
    workflowMsg = "";
    if (!workflowDraft.name.trim()) {
      workflowMsg = "请填写工作流名称";
      return;
    }
    const stages = workflowDraft.stages
      .split("\n")
      .map((l) => l.trim())
      .filter((l) => l.length > 0)
      .map((l) => {
        const idx = l.indexOf("|");
        const name = (idx > 0 ? l.slice(0, idx) : `阶段 ${l.length}`).trim();
        const prompt = (idx > 0 ? l.slice(idx + 1) : l).trim();
        return { name, prompt };
      })
      .filter((s) => s.prompt.length > 0);
    if (stages.length === 0) {
      workflowMsg = "至少需要一个阶段（每行「名称 | 提示词」）";
      return;
    }
    const w: HarnessWorkflow = {
      id: workflowDraft.id,
      name: workflowDraft.name.trim(),
      description: workflowDraft.description.trim(),
      stages,
      created_at: "",
      updated_at: "",
    };
    try {
      const saved = await harnessApi.saveWorkflow(w);
      workflowDraft = null;
      workflows = await harnessApi.listWorkflows();
      workflowMsg = `已保存「${saved.name}」（${saved.stages.length} 阶段）`;
      window.setTimeout(() => {
        if (workflowMsg?.startsWith("已保存")) workflowMsg = "";
      }, 1500);
    } catch (e) {
      workflowMsg = errText(e);
    }
  }

  async function deleteWorkflow(id: string) {
    try {
      await harnessApi.deleteWorkflow(id);
      workflows = await harnessApi.listWorkflows();
    } catch (e) {
      workflowMsg = errText(e);
    }
  }

  async function runWorkflowNow(id: string) {
    if (!activeId) return;
    workflowMsg = "工作流运行中…";
    try {
      const r = await harnessApi.runWorkflow(id, activeId);
      workflowMsg = `完成：${r.stages.filter((s) => s.ok).length}/${r.stages.length} 阶段成功`;
      refreshOrchestration().catch(() => {});
    } catch (e) {
      workflowMsg = errText(e);
    }
  }

  function scrollBottom() {
    requestAnimationFrame(() => {
      const el = document.querySelector(".hns-msgs");
      if (el) el.scrollTop = el.scrollHeight;
    });
  }
  $effect(() => {
    messages;
    streamBuf;
    liveTools;
    scrollBottom();
  });

  // ─── 历史分页（DSH「加载更早」：仅渲染尾部 N 条，顶部按钮加载更早；
  // 流式/发送期间始终渲染全部） ───
  const MSG_PAGE = 50;
  let shownCount = $state(MSG_PAGE);
  /** 当前可见消息数（流式时全量） */
  const visibleCount = $derived(sending || streamBuf ? messages.length : shownCount);
  function loadEarlier() {
    shownCount = Math.min(messages.length, shownCount + MSG_PAGE);
  }

  onMount(() => {
    // 整页拖放附加（DSH DropOverlay：拖入文件 → 遮罩 → 放下附加）
    document.addEventListener("dragenter", onDocDragEnter);
    document.addEventListener("dragover", onDocDragOver);
    document.addEventListener("dragleave", onDocDragLeave);
    document.addEventListener("drop", onDocDrop);
    // 审批事件监听（interaction）。子代理（fork 子会话）等非当前会话的审批
    // 也照常展示：后台子代理的 exec_command 会向父界面推事件，过滤掉会让
    // 子代理挂 10 分钟审批超时（M4）
    listen<HarnessApprovalPayload>("harness-approval-requested", (e) => {
      pendingApprovals = [...pendingApprovals, e.payload];
    }).then((f) => (unlistenApproval = f));
    // 用户提问监听（DSH user-questions 接缝；multi_select = 多选模式）
    let unlistenQuestion: (() => void) | null = null;
    listen<{ id: string; session_id: string; question: string; options: string[]; multi_select?: boolean }>(
      "harness-question-requested",
      (e) => {
        pendingQuestions = [...pendingQuestions, e.payload];
      },
    ).then((f) => (unlistenQuestion = f));
    // 钩子触发回传（hooks 桥）
    let unlistenHook: (() => void) | null = null;
    listen<HarnessHookFired>("harness-hook-fired", (e) => {
      hookFiredLog = [e.payload, ...hookFiredLog].slice(0, 20);
    }).then((f) => (unlistenHook = f));
    // 插件工具 / run_code 执行桥（DSH extensions + code-runtime）：
    // 后端请求 → 前端 WebView 运行 async 函数体 → submit 回传
    let unlistenToolExec: (() => void) | null = null;
    listen<{ id: string; name: string; args: string; code: string; session_id?: string }>(
      "harness-tool-exec-request",
      (e) => {
        void execPluginTool(e.payload);
      },
    ).then((f) => (unlistenToolExec = f));
    // B2：workflow JS 编排执行桥（DSH workflow 组合子：agent/parallel/pipeline）
    let unlistenWfExec: (() => void) | null = null;
    listen<{ id: string; name: string; args: string; code: string; session_id?: string }>(
      "harness-workflow-exec-request",
      (e) => {
        void execWorkflowJs(e.payload);
      },
    ).then((f) => (unlistenWfExec = f));
    harnessApi
      .getTools()
      .then((t) => (toolsCatalog = t))
      .catch(() => {});
    // 预设列表：会话头部的作用域选择与抽屉共用
    harnessApi
      .listPresets()
      .then((p) => (presets = p))
      .catch(() => {});
    void initTab();
    return () => {
      document.removeEventListener("dragenter", onDocDragEnter);
      document.removeEventListener("dragover", onDocDragOver);
      document.removeEventListener("dragleave", onDocDragLeave);
      document.removeEventListener("drop", onDocDrop);
      unlistenApproval?.();
      unlistenQuestion?.();
      unlistenHook?.();
      unlistenToolExec?.();
      unlistenWfExec?.();
      // 组件卸载：释放录音与播报资源
      releaseVoiceRecorder();
      stopTtsPlayer();
      if (micStream) {
        micStream.getTracks().forEach((t) => t.stop());
        micStream = null;
      }
    };
  });

  async function initTab() {
    try {
      config = await llmApi.getConfig();
      // 优先恢复 Harness 设置中的最近选择，否则回退全局默认
      let restored = false;
      try {
        const s = await harnessApi.getSettings();
        currentSettings = {
          last_provider_id: s.last_provider_id,
          last_model: s.last_model,
          tool_timeout_secs: s.tool_timeout_secs ?? null,
          max_agent_rounds: s.max_agent_rounds ?? null,
          preset_id: s.preset_id ?? null,
          allow_workspace_escape: s.allow_workspace_escape ?? false,
          sandbox_mode: s.sandbox_mode ?? "workspace-write",
          workspace_id: s.workspace_id ?? "",
          context_budget_tokens: s.context_budget_tokens ?? null,
          enable_compaction: s.enable_compaction ?? true,
          busy_enter: s.busy_enter ?? null,
          reasoning_effort: s.reasoning_effort ?? null,
          web_search_provider: s.web_search_provider ?? null,
        };
        if (
          s.last_provider_id &&
          config.providers.some((p) => p.id === s.last_provider_id)
        ) {
          providerId = s.last_provider_id;
          const p = config.providers.find((x) => x.id === providerId);
          modelId =
            p?.models.includes(s.last_model) && s.last_model
              ? s.last_model
              : (p?.default_model ?? p?.models[0] ?? "");
          restored = true;
        }
      } catch {
        /* 设置读取失败回退 */
      }
      if (!restored) {
        const last = config.last_chat_provider_id;
        providerId =
          last && config.providers.some((p) => p.id === last)
            ? last
            : (config.default_provider_id ?? config.providers[0]?.id ?? "");
        const p = config.providers.find((x) => x.id === providerId);
        modelId = p?.default_model ?? p?.models[0] ?? "";
      }
    } catch {
      /* 未配置提供方时列表为空，界面提示前往「大模型」配置 */
    }
    loadAiRoles().catch(() => {});
    try {
      const list = await harnessApi.listSessions();
      sessions = list;
      if (list.length > 0) {
        await selectSession(list[0].id);
      } else {
        await createSession();
      }
    } catch (e) {
      error = errText(e);
    } finally {
      loading = false;
    }
  }
</script>

<div class="hns">
  <!-- 整页拖放遮罩（DSH DropOverlay：拖入文件显示，放下附加） -->
  {#if dragOver}
    <div class="hns-drop-overlay" aria-hidden="true">
      <div class="hns-drop-inner">
        <span class="hns-drop-icon">📎</span>
        <span>图片拖动到此处即可添加</span>
      </div>
    </div>
  {/if}
  <!-- ─── 会话侧栏（折叠为 rail：悬浮自动展开、移开自动折叠） ─── -->
  <aside
    class="hns-side"
    class:collapsed={!sideExpanded}
    onmouseenter={() => (sideHover = true)}
    onmouseleave={() => (sideHover = false)}
  >
    <div class="hns-side-head">
      {#if sideExpanded}<span class="hns-side-title">Harness 会话</span>{/if}
      <span class="hns-side-head-actions">
        <button
          class="hns-side-collapse"
          onclick={toggleSidebar}
          title={sideExpanded ? "收起侧栏（rail 模式，悬浮展开）" : "展开侧栏"}
        >
          {#if sideExpanded}<PanelLeftCloseIcon class="size-3.5" />{:else}<PanelLeftOpenIcon class="size-3.5" />{/if}
        </button>
        <button class="hns-new" onclick={createSession} title="新建会话">
          <PlusIcon class="size-3.5" />{#if sideExpanded}新建{/if}
        </button>
      </span>
    </div>
    {#if sideExpanded}
    <div class="hns-side-search">
      <input
        placeholder="搜索会话内容…"
        aria-label="搜索会话"
        bind:value={searchQuery}
        onkeydown={(e) => {
          if (e.key === "Enter") runSearch();
        }}
      />
      <button class="hns-session-act" onclick={runSearch} title="搜索"><SearchIcon class="size-3" /></button>
    </div>
    {#if searchHits.length > 0}
      <div class="hns-search-results">
        {#each searchHits.slice(0, 8) as h, i (i + h.session_id + h.snippet.slice(0, 20))}
          <button
            class="hns-search-hit"
            onclick={() => {
              selectSession(h.session_id);
              searchHits = [];
              searchQuery = "";
            }}
            title={h.snippet}
          >
            <span class="hns-search-type">{h.event_type === "user_message" ? "问" : "答"}</span>
            {h.snippet.slice(0, 60)}
          </button>
        {/each}
      </div>
    {/if}
    {/if}
    <div class="hns-side-list">
      {#if !sideExpanded}
        {#each sessions as s (s.id)}
          <div
            class="hns-session collapsed"
            class:active={s.id === activeId}
            onclick={() => selectSession(s.id)}
            role="button"
            tabindex="0"
            onkeydown={(e) => {
              if (e.key === "Enter") selectSession(s.id);
            }}
            title={s.title || "新会话"}
          >
            <span class="hns-session-dot" class:active-dot={s.id === activeId} aria-hidden="true"></span>
          </div>
        {/each}
      {:else if sessions.length === 0 && !loading}
        <div class="hns-side-empty">暂无会话，点击「新建」开始</div>
      {:else}
        {#each workspaceGroups as g (g.id)}
          <div class="hns-ws-group">
            <button
              class="hns-ws-head"
              onclick={() => toggleWsGroup(g.id)}
              title={collapsedWs.has(g.id) ? "展开" : "折叠"}
            >
              {#if collapsedWs.has(g.id)}
                <ChevronRightIcon class="size-3.5" />
              {:else}
                <ChevronDownIcon class="size-3.5" />
              {/if}
              <FolderOpenIcon class="size-3" />
              <span class="hns-ws-name">{g.title}</span>
              <span class="hns-ws-count">{g.sessions.length}</span>
            </button>
            {#if !collapsedWs.has(g.id)}
              {#each g.sessions as s (s.id)}
                <div
                  class="hns-session"
                  class:active={s.id === activeId}
                  class:dragover={dragOverId === s.id}
                  draggable={!editingId}
                  onclick={() => selectSession(s.id)}
                  role="button"
                  tabindex="0"
                  onkeydown={(e) => {
                    if (e.key === "Enter") selectSession(s.id);
                  }}
                  ondragstart={(e) => {
                    dragId = s.id;
                    if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
                  }}
                  ondragover={(e) => {
                    e.preventDefault();
                    if (s.id !== dragId) dragOverId = s.id;
                  }}
                  ondragleave={() => {
                    if (dragOverId === s.id) dragOverId = null;
                  }}
                  ondrop={(e) => {
                    e.preventDefault();
                    const from = dragId;
                    const to = s.id;
                    dragOverId = null;
                    dragId = null;
                    if (from && to && from !== to) reorderSessions(from, to);
                  }}
                  ondragend={() => {
                    dragOverId = null;
                    dragId = null;
                  }}
                  title={s.title || "新会话"}
                >
                  {#if editingId === s.id}
                    <input
                      class="hns-session-edit"
                      bind:value={editingTitle}
                      onkeydown={(e) => {
                        if (e.key === "Enter") commitRename(s.id);
                        if (e.key === "Escape") editingId = null;
                      }}
                      onclick={(e) => e.stopPropagation()}
                    />
                    <button class="hns-session-act" onclick={(e) => { e.stopPropagation(); commitRename(s.id); }} title="确认">
                      <CheckIcon class="size-3" />
                    </button>
                    <button class="hns-session-act" onclick={(e) => { e.stopPropagation(); editingId = null; }} title="取消">
                      <XIcon class="size-3" />
                    </button>
                  {:else}
                    <span class="hns-session-title">{s.title || "新会话"}</span>
                    {#if s.message_count > 0}<span class="hns-session-count">{s.message_count}</span>{/if}
                    <span class="hns-session-acts">
                      <button class="hns-session-act" onclick={(e) => { e.stopPropagation(); startRename(s); }} title="重命名">
                        <PencilIcon class="size-3" />
                      </button>
                      <button class="hns-session-act" onclick={(e) => { e.stopPropagation(); generateSessionTitle(s); }} title="AI 生成标题（LLM 摘要，消耗一次模型调用）">
                        <SparklesIcon class="size-3" />
                      </button>
                      <button class="hns-session-act" onclick={(e) => { e.stopPropagation(); clearSession(s.id); }} title="清空聊天记录（保留会话）">
                        <EraserIcon class="size-3" />
                      </button>
                      <button
                        class="hns-session-act"
                        onclick={(e) => { e.stopPropagation(); setSessionArchived(s.id, !s.archived); }}
                        title={s.archived ? "取消归档" : "归档会话"}
                      >
                        {#if s.archived}<ArchiveRestoreIcon class="size-3" />{:else}<ArchiveIcon class="size-3" />{/if}
                      </button>
                      <button class="hns-session-act" onclick={(e) => { e.stopPropagation(); deleteSession(s.id); }} title="删除会话">
                        <Trash2Icon class="size-3" />
                      </button>
                    </span>
                  {/if}
                </div>
              {/each}
            {/if}
          </div>
        {/each}
      {/if}
    </div>
  </aside>

  <!-- ─── 对话区 ─── -->
  <main class="hns-main">
    <header class="hns-bar">
      <!-- 面包屑（DSH 会话头：子代理谱系祖先链，近→远；祖先可点击跳转） -->
      {#if lineage.length > 0}
        <nav class="hns-crumbs" aria-label="会话层级">
          {#each [...lineage].reverse() as [ancId, ancTitle] (ancId)}
            <button class="hns-crumb" onclick={() => selectSession(ancId)} title={ancTitle}>
              {ancTitle || "会话"}
            </button>
            <span class="hns-crumb-sep">/</span>
          {/each}
        </nav>
      {/if}
      <div class="hns-bar-title" title={activeSession?.title}>
        {activeSession?.title || "新会话"}
      </div>
      {#if notice}<span class="hns-notice">{notice}</span>{/if}
      <div class="hns-bar-right">
        {#if subagentTotal > 0}
          <!-- 子代理目录（DSH SubagentCatalog：会话头树目录弹层） -->
          <div class="hns-subagent-wrap">
            <button
              class="hns-bar-icon"
              class:on={subagentOpen}
              onclick={() => (subagentOpen = !subagentOpen)}
              aria-haspopup="tree"
              title={subagentRunning > 0 ? `${subagentRunning} 个子代理，正在运行` : `${subagentTotal} 个子代理`}
            >
              <GitForkIcon class="size-3.5" />
            </button>
            {#if subagentOpen}
              <div class="hns-subagent-pop" role="tree" aria-label="子代理目录">
                <div class="hns-subagent-head">子代理</div>
                {#each subagentTree as node (node.id)}
                  <SubagentRow {node} onOpen={(id) => { subagentOpen = false; selectSession(id); }} />
                {/each}
              </div>
            {/if}
          </div>
        {/if}
        {#if providers.length === 0}
          <span class="hns-no-provider">请先在「大模型」中配置提供方</span>
        {:else}
          <!-- 模型座（DSH ModelSelect：提供方 / 模型 / 推理等级 三级菜单） -->
          <ModelSelect
            providers={providers}
            models={models}
            modelEfforts={modelEfforts}
            providerId={providerId}
            modelId={modelId}
            effortId={effortId}
            onProviderChange={seatProviderChange}
            onModelChange={seatModelChange}
            onEffortChange={(e) => void changeEffort(e)}
          />
        {/if}
        {#if aiRoles.length > 0}
          <select
            class="hns-bar-compact"
            bind:value={roleId}
            onchange={() => void applyRole(roleId)}
            title="AI 角色注入（会话级持久化，后续回合生效）"
          >
            <option value="">无角色</option>
            {#each aiRoles as r (r.id)}
              <option value={r.id}>{r.emoji} {r.name}</option>
            {/each}
          </select>
        {/if}
        {#if activeSession}
          <select
            class="hns-bar-compact"
            bind:value={sessionPresetId}
            onchange={() => void onSessionPresetChange()}
            title="会话预设作用域（仅本会话；空 = 跟随全局默认）"
          >
            <option value="">预设：全局默认</option>
            {#each presets as p (p.id)}
              <option value={p.id}>预设：{p.name}</option>
            {/each}
          </select>
        {/if}
        <button
          class="hns-bar-icon"
          class:on={toolsOpen}
          onclick={() => (toolsOpen = !toolsOpen)}
          title="工具目录（{toolsCatalog.length}）"
        >
          <WrenchIcon class="size-3.5" />
        </button>
        <button
          class="hns-bar-icon"
          class:on={drawerOpen}
          onclick={() => {
            if (drawerOpen) drawerOpen = false;
            else openDrawer("settings");
          }}
          title="设置 / 钩子 / 预设"
        >
          <SettingsIcon class="size-3.5" />
        </button>
        {#if activeSession}
          <button class="hns-bar-icon" onclick={exportSessionMd} title="导出会话转写（Markdown 回放）">
            <DownloadIcon class="size-3.5" />
          </button>
        {/if}
      </div>
    </header>

    {#if usage && usage.turns > 0}
      <div class="hns-stats" title="会话遥测（DSH 统计条）">
        <span>{usage.turns} 轮 · {usage.steps} 步</span>
        <span class="hns-stats-sep" aria-hidden="true">|</span>
        <span>LLM {fmtWall(usage.llm_wall_ms)} · 工具调用 {fmtWall(usage.tool_wall_ms)}</span>
        <span class="hns-stats-sep" aria-hidden="true">|</span>
        <span>首 token 平均 {fmtSec(usage.first_token_avg_ms)} · {Math.round(usage.tokens_per_sec)} tok/s</span>
        <span class="hns-stats-sep" aria-hidden="true">|</span>
        <span>缓存命中 {Math.round(usage.cache_hit_rate * 100)}%</span>
        <span class="hns-stats-sep" aria-hidden="true">|</span>
        <span>输入 {fmtTok(usage.input_tokens)} tok · 输出 {fmtTok(usage.output_tokens)} tok</span>
        {#if usage.cost > 0}
          <span class="hns-stats-sep" aria-hidden="true">|</span>
          <span>成本 ${usage.cost.toFixed(4)}</span>
        {/if}
      </div>
    {/if}

    {#if toolsOpen}
      <div class="hns-tools-panel">
        <div class="hns-tools-head">
          <span class="hns-tools-title">工具目录</span>
          <span class="hns-tools-count">{toolTotalCount} 个工具 · {toolApprovalCount} 需审批</span>
          <span class="hns-tools-search">
            <SearchIcon class="size-3" />
            <input
              bind:value={toolSearch}
              placeholder="搜索工具名 / 说明…"
              aria-label="搜索工具"
            />
          </span>
        </div>
        <div class="hns-tools-scroll">
          {#if toolGroups.length === 0}
            <div class="hns-tools-empty">{toolSearch ? "无匹配工具" : "暂无工具"}</div>
          {/if}
          {#each toolGroups as [cat, list] (cat)}
            <div class="hns-tool-group">
              <div class="hns-tool-group-head">{cat}<span class="hns-tool-group-count">{list.length}</span></div>
              {#each list as t (t.name)}
                <div class="hns-tool-item">
                  <button
                    class="hns-tool-main"
                    onclick={() => (openToolSchema = openToolSchema === t.name ? null : t.name)}
                    title="点击展开参数 schema"
                  >
                    <span class="hns-tool-name">{t.name}</span>
                    {#if t.requires_approval}
                      <span class="hns-tool-lock" title="执行前需要审批">🔒 需审批</span>
                    {/if}
                    <span class="hns-tool-desc">{t.description}</span>
                    <span class="hns-tool-chevron">{openToolSchema === t.name ? "▾" : "▸"}</span>
                  </button>
                  {#if openToolSchema === t.name && t.parameters}
                    <pre class="hns-tool-pre hns-tool-schema">{prettyText(t.parameters)}</pre>
                  {/if}
                </div>
              {/each}
            </div>
          {/each}
        </div>
      </div>
    {/if}

    {#if drawerOpen}
      <div class="hns-drawer">
        <div class="hns-drawer-head">
          <div class="hns-drawer-title">
            <SettingsIcon class="size-4" />
            <span>治理中心</span>
          </div>
          <span class="hns-drawer-sub">设置 · 钩子 · 预设 · 执行 · 内容 · 集成</span>
          <button class="hns-drawer-close" onclick={() => (drawerOpen = false)} title="关闭">
            <XIcon class="size-3.5" />
          </button>
        </div>
        <div class="hns-drawer-tabs">
          <div class="hns-drawer-group">基础</div>
          <button class:on={drawerTab === "settings"} onclick={() => (drawerTab = "settings")} title="会话设置 / 沙箱模式 / 工作区"><SettingsIcon class="size-3.5" /><span>设置</span></button>
          <button class:on={drawerTab === "hooks"} onclick={() => (drawerTab = "hooks")} title="钩子桥（事件 → 命令）"><BellIcon class="size-3.5" /><span>钩子</span></button>
          <button class:on={drawerTab === "presets"} onclick={() => (drawerTab = "presets")} title="预设（工具作用域 / 提示词分区）"><PuzzleIcon class="size-3.5" /><span>预设</span></button>
          <button class:on={drawerTab === "schedule"} onclick={() => (drawerTab = "schedule")} title="定时任务"><ClockIcon class="size-3.5" /><span>定时</span></button>
          <button class:on={drawerTab === "workflow"} onclick={() => (drawerTab = "workflow")} title="工作流（阶段流水线）"><WorkflowIcon class="size-3.5" /><span>工作流</span></button>
          <div class="hns-drawer-group">执行</div>
          <button class:on={drawerTab === "terminal"} onclick={() => (drawerTab = "terminal")} title="终端会话 / PTY 真终端"><TerminalIcon class="size-3.5" /><span>终端</span></button>
          <button class:on={drawerTab === "jobs"} onclick={() => (drawerTab = "jobs")} title="后台作业"><ListTodoIcon class="size-3.5" /><span>作业</span></button>
          <div class="hns-drawer-group">内容</div>
          <button class:on={drawerTab === "skill"} onclick={() => (drawerTab = "skill")} title="技能（frontmatter 门控）"><LightbulbIcon class="size-3.5" /><span>技能</span></button>
          <button class:on={drawerTab === "cli"} onclick={() => (drawerTab = "cli")} title="Harness CLI"><Code2Icon class="size-3.5" /><span>CLI</span></button>
          <div class="hns-drawer-group">集成</div>
          <button class:on={drawerTab === "credentials"} onclick={() => (drawerTab = "credentials")} title="凭据引用"><KeyRoundIcon class="size-3.5" /><span>凭据</span></button>
          <button class:on={drawerTab === "lsp"} onclick={() => (drawerTab = "lsp")} title="LSP 语言服务器"><BracesIcon class="size-3.5" /><span>LSP</span></button>
          <button class:on={drawerTab === "mcp"} onclick={() => (drawerTab = "mcp")} title="MCP 外部工具服务器"><PlugIcon class="size-3.5" /><span>MCP</span></button>
          <button
            class:on={drawerTab === "plugins"}
            onclick={() => {
              drawerTab = "plugins";
              loadPlugins();
            }}
            title="动态插件（extensions / run_code）"
          ><BoxesIcon class="size-3.5" /><span>插件</span></button>
        </div>

        {#if drawerTab === "settings"}
          <div class="hns-drawer-body">
            <div class="hns-field">
              <span class="hns-field-label">工具执行超时（秒，5~300，默认 30）</span>
              <input
                type="number"
                min="5"
                max="300"
                placeholder="30"
                aria-label="工具执行超时（秒）"
                value={settingsForm.tool_timeout_secs ?? ""}
                oninput={(e) => {
                  const v = (e.currentTarget as HTMLInputElement).value;
                  settingsForm.tool_timeout_secs = v === "" ? null : Number(v);
                }}
              />
            </div>
            <div class="hns-field">
              <span class="hns-field-label">最大工具轮次（1~12，默认 6）</span>
              <input
                type="number"
                min="1"
                max="12"
                placeholder="6"
                aria-label="最大工具轮次"
                value={settingsForm.max_agent_rounds ?? ""}
                oninput={(e) => {
                  const v = (e.currentTarget as HTMLInputElement).value;
                  settingsForm.max_agent_rounds = v === "" ? null : Number(v);
                }}
              />
            </div>
            <div class="hns-field">
              <span class="hns-field-label">默认预设（作用于新对话的工具组合）</span>
              <select
                aria-label="默认预设"
                value={settingsForm.preset_id ?? ""}
                onchange={(e) => {
                  settingsForm.preset_id =
                    (e.currentTarget as HTMLSelectElement).value || null;
                }}
              >
                <option value="">（不使用预设）</option>
                {#each presets as p (p.id)}
                  <option value={p.id}>{p.name}</option>
                {/each}
              </select>
            </div>
            <div class="hns-field">
              <span class="hns-field-label">受限执行世界</span>
              <label class="hns-hook-enable">
                <input
                  type="checkbox"
                  checked={settingsForm.allow_workspace_escape ?? false}
                  onchange={(e) => {
                    settingsForm.allow_workspace_escape = (e.currentTarget as HTMLInputElement).checked;
                  }}
                />允许访问 agent_workspace 之外（fs/shell/终端；默认关闭）
              </label>
            </div>
            <div class="hns-field">
              <span class="hns-field-label">沙箱模式（DSH 三模式）</span>
              <select
                aria-label="沙箱模式"
                value={settingsForm.sandbox_mode ?? "workspace-write"}
                onchange={(e) => {
                  settingsForm.sandbox_mode = (e.currentTarget as HTMLSelectElement).value;
                }}
              >
                <option value="workspace-write">workspace-write（工作区内读写）</option>
                <option value="read-only">read-only（仅只读工具）</option>
                <option value="danger-full-access">danger-full-access（工作区外全权）</option>
              </select>
            </div>
            <div class="hns-field">
              <span class="hns-field-label">繁忙时 Enter 键行为（DSH busyEnter）</span>
              <select
                aria-label="繁忙时 Enter 键行为"
                value={busyEnter}
                onchange={(e) => {
                  busyEnter = (e.currentTarget as HTMLSelectElement).value as "queue" | "steer";
                }}
              >
                <option value="queue">排队发送（当前回合结束后自动发送）</option>
                <option value="steer">插话发送（新消息排到队首）</option>
              </select>
            </div>
            <div class="hns-field">
              <span class="hns-field-label">联网搜索提供商（DSH web 提供商缝）</span>
              <select
                aria-label="联网搜索提供商"
                value={settingsForm.web_search_provider ?? "bing"}
                onchange={(e) => {
                  settingsForm.web_search_provider = (e.currentTarget as HTMLSelectElement).value;
                }}
              >
                <option value="bing">Bing（双域兜底，无需密钥）</option>
                <option value="deepseek">DeepSeek（原生 web_search，需 DeepSeek 提供方密钥）</option>
              </select>
            </div>
            <div class="hns-field">
              <span class="hns-field-label">当前工作区（终端/Shell 默认目录 + fs 锚点）</span>
              <select
                aria-label="当前工作区"
                value={settingsForm.workspace_id ?? ""}
                onchange={(e) => {
                  switchWorkspace((e.currentTarget as HTMLSelectElement).value).catch(() => {});
                }}
              >
                {#each workspaces as w (w.id)}
                  {#if w.status === "active" || w.id === settingsForm.workspace_id}
                    <option value={w.id}>{w.title}{#if w.id === "default"}（默认）{/if}</option>
                  {/if}
                {/each}
              </select>
              {#if workspaceMsg}<span class="hns-msg-note">{workspaceMsg}</span>{/if}
            </div>
            <div class="hns-field">
              <span class="hns-field-label">工作区管理</span>
              <div class="hns-terminal-input">
                <input
                  placeholder="新工作区名称"
                  aria-label="新工作区名称"
                  bind:value={workspaceNewTitle}
                  onkeydown={(e) => {
                    if (e.key === "Enter") createWorkspace();
                  }}
                />
                <button class="hns-primary" onclick={createWorkspace}>+ 创建工作区</button>
              </div>
              {#each workspaces.filter((w) => w.id !== "default") as w (w.id)}
                <div class="hns-preset-item">
                  <div class="hns-preset-main">
                    <span class="hns-preset-name">{w.title}</span>
                    <span class="hns-preset-meta">{w.dir} · {w.status === "active" ? "使用中" : "已归档"}</span>
                  </div>
                  <span class="hns-session-acts">
                    <button
                      class="hns-session-act"
                      onclick={() => harnessApi.setWorkspaceStatus(w.id, w.status === "active" ? "archived" : "active").then(() => loadWorkspaces())}
                      title={w.status === "active" ? "归档" : "恢复"}
                    >
                      {w.status === "active" ? "归档" : "恢复"}
                    </button>
                    <button class="hns-session-act" onclick={() => deleteWorkspace(w.id)} title="删除">
                      <Trash2Icon class="size-3" />
                    </button>
                  </span>
                </div>
              {/each}
            </div>
            <div class="hns-field">
              <span class="hns-field-label">上下文压缩</span>
              <label class="hns-hook-enable">
                <input
                  type="checkbox"
                  checked={settingsForm.enable_compaction ?? true}
                  onchange={(e) => {
                    settingsForm.enable_compaction = (e.currentTarget as HTMLInputElement).checked;
                  }}
                />启用超长历史自动压缩
              </label>
              <input
                type="number"
                min="4000"
                max="128000"
                placeholder="24000"
                aria-label="压缩预算（token 估算）"
                value={settingsForm.context_budget_tokens ?? ""}
                oninput={(e) => {
                  const v = (e.currentTarget as HTMLInputElement).value;
                  settingsForm.context_budget_tokens = v === "" ? null : Number(v);
                }}
              />
            </div>
            <div class="hns-field-actions">
              {#if settingsMsg}<span class="hns-msg-note">{settingsMsg}</span>{/if}
              <button class="hns-primary" onclick={saveSettingsForm}>保存设置</button>
            </div>
          </div>
        {:else if drawerTab === "hooks"}
          <div class="hns-drawer-body">
            <div class="hns-drawer-hint">
              钩子：会话事件触发本机命令（≤10 秒），环境变量 HARNESS_EVENT / HARNESS_SESSION 可用。
              PreToolUse 钩子可输出 JSON 决策（decision 为 deny 或 ask）拦截/转审批工具调用；
              匹配器为空时全部命中，非空时载荷包含该子串才触发。
            </div>
            {#each hooks as h, i (h.id)}
              <div class="hns-hook-row">
                <select
                  aria-label="钩子事件"
                  value={h.event}
                  onchange={(e) => {
                    hooks[i] = { ...h, event: (e.currentTarget as HTMLSelectElement).value };
                    hooks = [...hooks];
                  }}
                >
                  <option value="turn_start">turn_start</option>
                  <option value="turn_end">turn_end</option>
                  <option value="tool_executed">tool_executed</option>
                  <option value="SessionStart">SessionStart</option>
                  <option value="UserPromptSubmit">UserPromptSubmit</option>
                  <option value="PreToolUse">PreToolUse</option>
                  <option value="PostToolUse">PostToolUse</option>
                  <option value="Stop">Stop</option>
                  <option value="SubagentStart">SubagentStart</option>
                  <option value="SubagentStop">SubagentStop</option>
                </select>
                <input
                  placeholder="匹配器（可空）"
                  aria-label="钩子匹配器"
                  value={h.matcher ?? ""}
                  oninput={(e) => {
                    hooks[i] = { ...h, matcher: (e.currentTarget as HTMLInputElement).value };
                    hooks = [...hooks];
                  }}
                />
                <input
                  placeholder="PowerShell 命令"
                  aria-label="钩子命令"
                  value={h.command}
                  oninput={(e) => {
                    hooks[i] = { ...h, command: (e.currentTarget as HTMLInputElement).value };
                    hooks = [...hooks];
                  }}
                />
                <label class="hns-hook-enable">
                  <input
                    type="checkbox"
                    checked={h.enabled}
                    onchange={(e) => {
                      hooks[i] = { ...h, enabled: (e.currentTarget as HTMLInputElement).checked };
                      hooks = [...hooks];
                    }}
                  />启用
                </label>
                <button class="hns-session-act" onclick={() => { hooks = hooks.filter((x) => x.id !== h.id); }} title="删除">
                  <Trash2Icon class="size-3" />
                </button>
              </div>
            {/each}
            <div class="hns-field-actions">
              <button class="hns-plain" onclick={addHook}>+ 添加钩子</button>
              {#if hooksMsg}<span class="hns-msg-note">{hooksMsg}</span>{/if}
              <button class="hns-primary" onclick={saveHooksList}>保存钩子</button>
            </div>
            {#if hookFiredLog.length > 0}
              <div class="hns-hook-log-head">触发记录（最近 20 条）</div>
              {#each hookFiredLog as f (f.id + f.event)}
                <div class="hns-hook-log" class:err={!f.ok}>
                  [{f.event}] {f.ok ? "✓" : "✗"} {f.output || "（无输出）"}
                </div>
              {/each}
            {/if}
          </div>
        {:else if drawerTab === "presets"}
          <div class="hns-drawer-body">
            {#if presetDraft}
              <div class="hns-preset-form">
                <div class="hns-field">
                  <span class="hns-field-label">预设名称</span>
                  <input bind:value={presetDraft!.name} placeholder="如：只读办公预设" aria-label="预设名称" />
                </div>
                <div class="hns-field">
                  <span class="hns-field-label">描述</span>
                  <input bind:value={presetDraft!.description} placeholder="用途说明" aria-label="预设描述" />
                </div>
                <div class="hns-field">
                  <span class="hns-field-label">禁用工具（会话内不注入模型）</span>
                  <div class="hns-preset-tools">
                    {#each toolsCatalog as t (t.name)}
                      <label class="hns-preset-tool">
                        <input
                          type="checkbox"
                          checked={presetDraft!.disabled.includes(t.name)}
                          onchange={(e) => {
                            const on = (e.currentTarget as HTMLInputElement).checked;
                            presetDraft!.disabled = on
                              ? [...presetDraft!.disabled, t.name]
                              : presetDraft!.disabled.filter((x) => x !== t.name);
                          }}
                        />{t.name}
                      </label>
                    {/each}
                  </div>
                </div>
                <div class="hns-field">
                  <span class="hns-field-label">附加提示词分区（随系统提示词注入）</span>
                  <textarea bind:value={presetDraft!.prompt} rows="3" aria-label="附加提示词分区" placeholder="例如：只回答与文档相关的问题，不使用命令执行工具……"></textarea>
                </div>
                <div class="hns-field-actions">
                  {#if presetMsg}<span class="hns-msg-note">{presetMsg}</span>{/if}
                  <button class="hns-plain" onclick={() => (presetDraft = null)}>取消</button>
                  <button class="hns-primary" onclick={savePresetDraft}>保存预设</button>
                </div>
              </div>
            {:else}
              <div class="hns-field-actions">
                {#if presetMsg}<span class="hns-msg-note">{presetMsg}</span>{/if}
                <button class="hns-plain" onclick={startNewPreset}>+ 新建预设</button>
              </div>
              {#each presets as p (p.id)}
                <div class="hns-preset-item">
                  <div class="hns-preset-main">
                    <span class="hns-preset-name">{p.name}</span>
                    {#if p.disabled_tools.length > 0}
                      <span class="hns-preset-meta">禁用 {p.disabled_tools.length} 个工具</span>
                    {/if}
                    {#if p.prompt_sections.length > 0}
                      <span class="hns-preset-meta">含提示词分区</span>
                    {/if}
                  </div>
                  <span class="hns-session-acts">
                    <button class="hns-session-act" onclick={() => startEditPreset(p)} title="编辑"><PencilIcon class="size-3" /></button>
                    <button class="hns-session-act" onclick={() => deletePreset(p.id)} title="删除"><Trash2Icon class="size-3" /></button>
                  </span>
                </div>
              {:else}
                <div class="hns-tools-empty">暂无预设</div>
              {/each}
            {/if}
          </div>
        {:else if drawerTab === "schedule"}
          <div class="hns-drawer-body">
            <div class="hns-drawer-hint">
              定时任务：按间隔（分钟）在目标会话自动发送提示词并执行一轮代理对话（结果照常落日志/用量/钩子）。
            </div>
            {#if scheduleDraft}
              <div class="hns-preset-form">
                <div class="hns-field">
                  <span class="hns-field-label">名称</span>
                  <input bind:value={scheduleDraft!.name} placeholder="如：每日早报" aria-label="定时名称" />
                </div>
                <div class="hns-field">
                  <span class="hns-field-label">提示词（在目标会话发送）</span>
                  <textarea bind:value={scheduleDraft!.prompt} rows="2" aria-label="定时提示词"></textarea>
                </div>
                <div class="hns-field">
                  <span class="hns-field-label">间隔（分钟，1~10080）</span>
                  <input
                    type="number"
                    min="1"
                    max="10080"
                    aria-label="间隔分钟"
                    value={scheduleDraft!.interval}
                    oninput={(e) => {
                      scheduleDraft!.interval = Number((e.currentTarget as HTMLInputElement).value) || 30;
                    }}
                  />
                </div>
                <label class="hns-hook-enable">
                  <input
                    type="checkbox"
                    checked={scheduleDraft!.enabled}
                    onchange={(e) => {
                      scheduleDraft!.enabled = (e.currentTarget as HTMLInputElement).checked;
                    }}
                  />启用
                </label>
                <div class="hns-field-actions">
                  {#if scheduleMsg}<span class="hns-msg-note">{scheduleMsg}</span>{/if}
                  <button class="hns-plain" onclick={() => (scheduleDraft = null)}>取消</button>
                  <button class="hns-primary" onclick={saveScheduleDraft}>保存定时</button>
                </div>
              </div>
            {:else}
              <div class="hns-field-actions">
                {#if scheduleMsg}<span class="hns-msg-note">{scheduleMsg}</span>{/if}
                <button class="hns-plain" onclick={startNewSchedule}>+ 新建定时</button>
              </div>
              {#each schedules as s (s.id)}
                <div class="hns-preset-item">
                  <div class="hns-preset-main">
                    <span class="hns-preset-name">{s.name}</span>
                    <span class="hns-preset-meta">{s.interval_minutes} 分钟</span>
                    {#if !s.enabled}<span class="hns-preset-meta">已停用</span>{/if}
                  </div>
                  <span class="hns-session-acts">
                    <button class="hns-session-act" onclick={() => runScheduleNow(s.id)} title="立即运行一次">▶</button>
                    <button class="hns-session-act" onclick={() => deleteSchedule(s.id)} title="删除"><Trash2Icon class="size-3" /></button>
                  </span>
                </div>
              {:else}
                <div class="hns-tools-empty">暂无定时任务</div>
              {/each}
            {/if}
          </div>
        {:else if drawerTab === "workflow"}
          <div class="hns-drawer-body">
            <div class="hns-drawer-hint">
              工作流：有序阶段依次执行一轮对话，前序阶段输出注入后序提示词（同一会话日志）。
            </div>
            {#if workflowDraft}
              <div class="hns-preset-form">
                <div class="hns-field">
                  <span class="hns-field-label">名称</span>
                  <input bind:value={workflowDraft!.name} placeholder="如：周报生成" aria-label="工作流名称" />
                </div>
                <div class="hns-field">
                  <span class="hns-field-label">描述</span>
                  <input bind:value={workflowDraft!.description} aria-label="工作流描述" />
                </div>
                <div class="hns-field">
                  <span class="hns-field-label">阶段（每行「名称 | 提示词」）</span>
                  <textarea bind:value={workflowDraft!.stages} rows="5" aria-label="工作流阶段"></textarea>
                </div>
                <div class="hns-field-actions">
                  {#if workflowMsg}<span class="hns-msg-note">{workflowMsg}</span>{/if}
                  <button class="hns-plain" onclick={() => (workflowDraft = null)}>取消</button>
                  <button class="hns-primary" onclick={saveWorkflowDraft}>保存工作流</button>
                </div>
              </div>
            {:else}
              <div class="hns-field-actions">
                {#if workflowMsg}<span class="hns-msg-note">{workflowMsg}</span>{/if}
                <button class="hns-plain" onclick={startNewWorkflow}>+ 新建工作流</button>
              </div>
              {#each workflows as w (w.id)}
                <div class="hns-preset-item">
                  <div class="hns-preset-main">
                    <span class="hns-preset-name">{w.name}</span>
                    <span class="hns-preset-meta">{w.stages.length} 阶段</span>
                  </div>
                  <span class="hns-session-acts">
                    <button class="hns-session-act" onclick={() => runWorkflowNow(w.id)} title="在当前会话运行">▶</button>
                    <button class="hns-session-act" onclick={() => startEditWorkflow(w)} title="编辑"><PencilIcon class="size-3" /></button>
                    <button class="hns-session-act" onclick={() => deleteWorkflow(w.id)} title="删除"><Trash2Icon class="size-3" /></button>
                  </span>
                </div>
              {:else}
                <div class="hns-tools-empty">暂无工作流</div>
              {/each}
            {/if}
          </div>
        {:else if drawerTab === "terminal"}
          <div class="hns-drawer-body">
            <div class="hns-drawer-hint">
              终端：持久会话（保持工作目录 cwd 与输入/输出日志）。PTY 真终端
              （ConPTY + powershell）保留进程内状态（变量/REPL）；未启动 PTY 时
              每次命令独立执行（状态仅保持 cwd）。
            </div>
            {#if terminalMsg}<span class="hns-msg-note">{terminalMsg}</span>{/if}
            <div class="hns-field-actions">
              <button class="hns-plain" onclick={createTerminal}>+ 新建终端</button>
            </div>
            {#each terminals as t (t.id)}
              <div class="hns-terminal">
                <div class="hns-terminal-head">
                  <span class="hns-preset-name">{t.name}</span>
                  <code class="hns-terminal-cwd" title={t.cwd}>{t.cwd}</code>
                  {#if ptyRunning[t.id]}
                    <span class="hns-pty-badge" title="真终端运行中（保留进程状态）">PTY ●</span>
                    <button class="hns-session-act" onclick={() => stopPty(t.id)} title="停止 PTY">停止 PTY</button>
                  {:else}
                    <button class="hns-session-act" onclick={() => startPty(t.id)} title="启动 PTY 真终端（powershell）">启动 PTY</button>
                  {/if}
                  <button class="hns-session-act" onclick={() => deleteTerminal(t.id)} title="删除"><Trash2Icon class="size-3" /></button>
                </div>
                <div class="hns-terminal-out">
                  {#each (terminalLogs[t.id] ?? []) as l, i (i)}
                    <div class="hns-terminal-line"><span class="hns-terminal-in">$ {l.input}</span>{#if l.output}<pre class="hns-terminal-pre">{l.output}</pre>{/if}</div>
                  {/each}
                  {#if terminalBusy === t.id}<div class="hns-terminal-line">执行中…</div>{/if}
                </div>
                <div class="hns-terminal-input">
                  <input
                    placeholder={ptyRunning[t.id] ? "PTY：输入命令，Enter 执行" : "输入命令，Enter 执行"}
                    aria-label="终端命令"
                    value={terminalInputs[t.id] ?? ""}
                    oninput={(e) => {
                      terminalInputs = {
                        ...terminalInputs,
                        [t.id]: (e.currentTarget as HTMLInputElement).value,
                      };
                    }}
                    onkeydown={(e) => {
                      if (e.key === "Enter") terminalSend(t.id);
                    }}
                  />
                  <button class="hns-primary" disabled={terminalBusy !== null} onclick={() => terminalSend(t.id)}>执行</button>
                </div>
              </div>
            {:else}
              <div class="hns-tools-empty">暂无终端会话</div>
            {/each}
          </div>
        {:else if drawerTab === "skill"}
          <div class="hns-drawer-body">
            <div class="hns-drawer-hint">
              技能：目录约定（data/harness/skills/&lt;id&gt;/SKILL.md）。模型可经 skill_list /
              skill_load 工具读取并执行技能说明。
            </div>
            {#if skillDraft}
              <div class="hns-preset-form">
                <div class="hns-field">
                  <span class="hns-field-label">技能 id（留空自动生成）</span>
                  <input bind:value={skillDraft!.id} placeholder="如：weekly-report" aria-label="技能 id" />
                </div>
                <div class="hns-field">
                  <span class="hns-field-label">SKILL.md 内容（首行 # 名称）</span>
                  <textarea bind:value={skillDraft!.content} rows="8" aria-label="技能内容"></textarea>
                </div>
                <div class="hns-field-actions">
                  {#if skillMsg}<span class="hns-msg-note">{skillMsg}</span>{/if}
                  <button class="hns-plain" onclick={() => (skillDraft = null)}>取消</button>
                  <button class="hns-primary" onclick={saveSkillDraft}>保存技能</button>
                </div>
              </div>
            {:else}
              <div class="hns-field-actions">
                {#if skillMsg}<span class="hns-msg-note">{skillMsg}</span>{/if}
                <button class="hns-plain" onclick={startNewSkill}>+ 新建技能</button>
              </div>
              {#each skills as s (s.id)}
                <div class="hns-preset-item">
                  <div class="hns-preset-main">
                    <span class="hns-preset-name">{s.name}</span>
                    <span class="hns-preset-meta" title={s.description}>{s.id}</span>
                  </div>
                  <span class="hns-session-acts">
                    <button class="hns-session-act" onclick={() => startEditSkill(s)} title="编辑"><PencilIcon class="size-3" /></button>
                    <button class="hns-session-act" onclick={() => deleteSkill(s.id)} title="删除"><Trash2Icon class="size-3" /></button>
                  </span>
                </div>
              {:else}
                <div class="hns-tools-empty">暂无技能</div>
              {/each}
            {/if}
          </div>
        {:else if drawerTab === "cli"}
          <div class="hns-drawer-body">
            <div class="hns-drawer-hint">
              Harness CLI（DSH CLI 等价物）：sessions list / session create /
              session chat &lt;id&gt; &lt;文本&gt; / session show &lt;id&gt; / tools list / usage &lt;id&gt;
            </div>
            <div class="hns-terminal-input">
              <input
                placeholder="输入命令，如：tools list"
                aria-label="CLI 命令"
                bind:value={cliInput}
                onkeydown={(e) => {
                  if (e.key === "Enter") runCli();
                }}
              />
              <button class="hns-primary" disabled={cliBusy} onclick={runCli}>执行</button>
            </div>
            {#if cliOutput}
              <pre class="hns-tool-pre">{cliOutput}</pre>
            {/if}
          </div>
        {:else if drawerTab === "credentials"}
          <div class="hns-drawer-body">
            <div class="hns-drawer-hint">
              凭据引用：键值凭据（本地持久化，展示掩码）。子进程（钩子/MCP/LSP/终端）经
              HARNESS_CREDENTIAL_&lt;KEY&gt; 环境变量消费；.env 提供者写入 data/harness/.env。
            </div>
            {#if credentialDraft}
              <div class="hns-preset-form">
                <div class="hns-field">
                  <span class="hns-field-label">键名</span>
                  <input bind:value={credentialDraft!.key} placeholder="如：API_TOKEN" aria-label="凭据键名" />
                </div>
                <div class="hns-field">
                  <span class="hns-field-label">值</span>
                  <input type="password" bind:value={credentialDraft!.value} aria-label="凭据值" />
                </div>
                <label class="hns-hook-enable">
                  <input type="checkbox" bind:checked={credentialDraft!.storeEnv} />写入 .env 提供者（而非凭据存储）
                </label>
                <div class="hns-field-actions">
                  {#if credentialMsg}<span class="hns-msg-note">{credentialMsg}</span>{/if}
                  <button class="hns-plain" onclick={() => (credentialDraft = null)}>取消</button>
                  <button class="hns-primary" onclick={putCredential}>保存凭据</button>
                </div>
              </div>
            {:else}
              <div class="hns-field-actions">
                {#if credentialMsg}<span class="hns-msg-note">{credentialMsg}</span>{/if}
                <button class="hns-plain" onclick={() => (credentialDraft = { key: "", value: "", storeEnv: false })}>+ 添加凭据</button>
              </div>
              {#each credentials as c (c.key)}
                <div class="hns-preset-item">
                  <div class="hns-preset-main">
                    <span class="hns-preset-name">{c.key}</span>
                    <code class="hns-terminal-cwd">{c.masked}</code>
                  </div>
                  <span class="hns-session-acts">
                    <button class="hns-session-act" onclick={() => deleteCredential(c.key)} title="删除"><Trash2Icon class="size-3" /></button>
                  </span>
                </div>
              {:else}
                <div class="hns-tools-empty">暂无凭据</div>
              {/each}
            {/if}
          </div>
        {:else if drawerTab === "mcp"}
          <div class="hns-drawer-body">
            <div class="hns-drawer-hint">
              MCP 服务器（stdio：命令 + 参数）。启用后经 list_tools 拉取工具并注册进 Harness
              工具目录（名称形如 mcp_&lt;服务器id&gt;_&lt;工具&gt;），模型可直接调用。
            </div>
            {#if mcpDraft}
              <div class="hns-preset-form">
                <div class="hns-field">
                  <span class="hns-field-label">名称</span>
                  <input bind:value={mcpDraft!.name} placeholder="如：filesystem" aria-label="MCP 名称" />
                </div>
                <div class="hns-field">
                  <span class="hns-field-label">命令</span>
                  <input bind:value={mcpDraft!.command} placeholder="如：npx" aria-label="MCP 命令" />
                </div>
                <div class="hns-field">
                  <span class="hns-field-label">参数（逗号分隔）</span>
                  <input bind:value={mcpDraft!.args} placeholder="如：-y,@modelcontextprotocol/server-filesystem,E:\\ST" aria-label="MCP 参数" />
                </div>
                <div class="hns-field">
                  <span class="hns-field-label">环境变量（KEY=VALUE，逗号分隔；与凭据注入合并）</span>
                  <input bind:value={mcpDraft!.env} placeholder="如：MCP_MEMORY_PATH=E:\\ST\\memory.json" aria-label="MCP 环境变量" />
                </div>
                <div class="hns-field">
                  <span class="hns-field-label">工作目录（空 = 继承应用）</span>
                  <input bind:value={mcpDraft!.cwd} placeholder="如：E:\\ST" aria-label="MCP 工作目录" />
                </div>
                <label class="hns-hook-enable">
                  <input type="checkbox" bind:checked={mcpDraft!.enabled} />启用
                </label>
                <div class="hns-field-actions">
                  {#if mcpMsg}<span class="hns-msg-note">{mcpMsg}</span>{/if}
                  <button class="hns-plain" onclick={() => (mcpDraft = null)}>取消</button>
                  <button class="hns-primary" onclick={saveMcpDraft}>保存服务器</button>
                </div>
              </div>
            {:else}
              <div class="hns-field-actions">
                {#if mcpMsg}<span class="hns-msg-note">{mcpMsg}</span>{/if}
                <button class="hns-plain" onclick={() => (mcpDraft = { id: "", name: "", command: "", args: "", env: "", cwd: "", enabled: true })}>+ 添加服务器</button>
              </div>
              {#each mcpServers as s (s.id)}
                <div class="hns-preset-item">
                  <div class="hns-preset-main">
                    <span class="hns-preset-name">{s.name}</span>
                    <span class="hns-preset-meta">{s.command} {s.args.join(" ")}</span>
                    {#if !s.enabled}<span class="hns-preset-meta">已停用</span>{/if}
                  </div>
                  <span class="hns-session-acts">
                    <button class="hns-session-act" onclick={() => (mcpDraft = { id: s.id, name: s.name, command: s.command, args: s.args.join(", "), env: Object.entries(s.env ?? {}).map(([k, v]) => `${k}=${v}`).join(","), cwd: s.cwd ?? "", enabled: s.enabled })} title="编辑"><PencilIcon class="size-3" /></button>
                    <button class="hns-session-act" onclick={() => deleteMcp(s.id)} title="删除"><Trash2Icon class="size-3" /></button>
                  </span>
                </div>
              {:else}
                <div class="hns-tools-empty">暂无 MCP 服务器</div>
              {/each}
            {/if}
            <div class="hns-port">
              <div class="hns-port-head">配置束导入 / 导出（预设 + 技能 + MCP + LSP + 钩子）</div>
              {#if portMsg}<span class="hns-msg-note">{portMsg}</span>{/if}
              <div class="hns-field-actions">
                <button class="hns-plain" onclick={exportBundleFile} title="导出配置束到 JSON 文件">
                  <DownloadIcon class="size-3.5" />导出到文件
                </button>
                <button class="hns-plain" onclick={copyBundleJson} title="导出配置束 JSON 到剪贴板">
                  <CopyIcon class="size-3.5" />复制 JSON
                </button>
                <button class="hns-plain" onclick={importBundleFile} title="从 JSON 文件导入并合并">
                  <UploadIcon class="size-3.5" />从文件导入
                </button>
              </div>
              <div class="hns-field">
                <span class="hns-field-label">或粘贴配置束 JSON 文本导入</span>
                <textarea bind:value={mcpImportJson} rows="4" placeholder="粘贴配置束 JSON（含 presets / skills / mcp_servers / lsp_servers / hooks 字段）" aria-label="配置束 JSON"></textarea>
              </div>
              <div class="hns-field-actions">
                <button class="hns-primary" disabled={!mcpImportJson.trim()} onclick={importBundleJson}>粘贴导入</button>
              </div>
            </div>
          </div>
        {:else if drawerTab === "plugins"}
          <div class="hns-drawer-body">
            <div class="hns-drawer-hint">
              动态插件（DSH extensions）：模型可经 plugin_list / plugin_define 定义、
              plugin_enable / plugin_disable 启停、plugin_delete 移除；插件工具与 run_code
              在前端沙箱执行（async 函数体，可用 args 与 ctx.fetch / ctx.log）。
            </div>
            {#if pluginDraft}
              <div class="hns-preset-form">
                <div class="hns-field">
                  <span class="hns-field-label">插件名称</span>
                  <input bind:value={pluginDraft!.name} placeholder="如：计算助手" aria-label="插件名称" />
                </div>
                <div class="hns-field">
                  <span class="hns-field-label">说明</span>
                  <textarea bind:value={pluginDraft!.description} rows="2" aria-label="插件说明"></textarea>
                </div>
                <div class="hns-field">
                  <span class="hns-field-label">工具 JSON（name/description/parameters/code/requires_approval）</span>
                  <textarea bind:value={pluginDraft!.tools} rows="8" class="font-mono" aria-label="工具 JSON"></textarea>
                </div>
                <label class="hns-field">
                  <input type="checkbox" bind:checked={pluginDraft!.enabled} /> 保存后启用
                </label>
                <div class="hns-field-actions">
                  {#if pluginMsg}<span class="hns-msg-note">{pluginMsg}</span>{/if}
                  <button class="hns-plain" onclick={() => (pluginDraft = null)}>取消</button>
                  <button class="hns-primary" onclick={savePluginDraft}>保存插件</button>
                </div>
              </div>
            {:else}
              <div class="hns-field-actions">
                {#if pluginMsg}<span class="hns-msg-note">{pluginMsg}</span>{/if}
                <button class="hns-plain" onclick={startNewPlugin}>+ 新建插件</button>
              </div>
              {#each plugins as p (p.id)}
                <div class="hns-preset-item">
                  <div class="hns-preset-main">
                    <span class="hns-preset-name">{p.name}</span>
                    <span class="hns-preset-meta" title={p.description}>
                      {p.id} · {p.enabled ? "启用" : "停用"} · 工具：{p.tools.map((t) => t.name).join(", ") || "（无）"} · v{p.versions.length}
                    </span>
                  </div>
                  <span class="hns-session-acts">
                    <button class="hns-session-act" onclick={() => togglePlugin(p)} title={p.enabled ? "停用" : "启用"}>{p.enabled ? "停用" : "启用"}</button>
                    <button class="hns-session-act" onclick={() => startEditPlugin(p)} title="编辑"><PencilIcon class="size-3" /></button>
                    <button class="hns-session-act" onclick={() => deletePlugin(p)} title="删除"><Trash2Icon class="size-3" /></button>
                  </span>
                </div>
              {:else}
                <div class="hns-tools-empty">暂无动态插件</div>
              {/each}
            {/if}
          </div>
        {:else if drawerTab === "jobs"}
          <div class="hns-drawer-body">
            <div class="hns-drawer-hint">
              后台作业：exec_command 以 run_in_background=true 启动的命令在此运行
              （进程后台执行，输出可取回、可终止；模型可经 job_list/job_output/job_kill 管理）。
            </div>
            {#if jobsMsg}<span class="hns-msg-note">{jobsMsg}</span>{/if}
            <div class="hns-field-actions">
              <button class="hns-plain" onclick={loadJobs} title="刷新作业列表">刷新</button>
            </div>
            {#each jobs as j (j.id)}
              <div class="hns-preset-item">
                <div class="hns-preset-main">
                  <span class="hns-preset-name">{j.name}</span>
                  <span
                    class="hns-preset-meta"
                    class:ok={j.status === "done"}
                    class:err={j.status === "error" || j.status === "killed"}
                  >
                    {j.status === "running" ? "运行中" : j.status === "done" ? "完成" : j.status === "killed" ? "已终止" : "错误"} · {j.id}
                  </span>
                  <span class="hns-preset-meta" title={j.created_at}>{j.created_at.slice(11, 19)}</span>
                </div>
                <span class="hns-session-acts">
                  <button class="hns-session-act" onclick={() => toggleJobOutput(j.id)} title={jobExpanded === j.id ? "收起输出" : "查看输出"}>
                    {jobExpanded === j.id ? "收起" : "输出"}
                  </button>
                  {#if j.status === "running"}
                    <button class="hns-session-act" onclick={() => killJob(j.id)} title="终止作业">
                      <Trash2Icon class="size-3" />
                    </button>
                  {/if}
                </span>
              </div>
              {#if jobExpanded === j.id}
                <pre class="hns-tool-pre hns-job-out">{jobOutputs[j.id] ?? "读取中…"}</pre>
              {/if}
            {:else}
              <div class="hns-tools-empty">当前会话暂无后台作业</div>
            {/each}
          </div>
        {:else}
          <div class="hns-drawer-body">
            <div class="hns-drawer-hint">
              语言服务器（LSP）：stdio 服务器（命令 + 参数）。模型工具 lsp_hover 查询
              工作区文件位置的类型/文档信息；未配置时优雅报错。
            </div>
            {#if lspDraft}
              <div class="hns-preset-form">
                <div class="hns-field">
                  <span class="hns-field-label">名称</span>
                  <input bind:value={lspDraft!.name} placeholder="如：rust-analyzer" aria-label="LSP 名称" />
                </div>
                <div class="hns-field">
                  <span class="hns-field-label">命令</span>
                  <input bind:value={lspDraft!.command} placeholder="如：rust-analyzer.exe" aria-label="LSP 命令" />
                </div>
                <div class="hns-field">
                  <span class="hns-field-label">参数（逗号分隔）</span>
                  <input bind:value={lspDraft!.args} placeholder="如：--log-file,nul" aria-label="LSP 参数" />
                </div>
                <div class="hns-field">
                  <span class="hns-field-label">文件扩展名映射（逗号分隔；查询按扩展名路由服务器）</span>
                  <input bind:value={lspDraft!.extensions} placeholder="如：rs,toml" aria-label="LSP 扩展名映射" />
                </div>
                <label class="hns-hook-enable">
                  <input type="checkbox" bind:checked={lspDraft!.enabled} />启用
                </label>
                <div class="hns-field-actions">
                  {#if lspMsg}<span class="hns-msg-note">{lspMsg}</span>{/if}
                  <button class="hns-plain" onclick={() => (lspDraft = null)}>取消</button>
                  <button class="hns-primary" onclick={saveLspDraft}>保存服务器</button>
                </div>
              </div>
            {:else}
              <div class="hns-field-actions">
                {#if lspMsg}<span class="hns-msg-note">{lspMsg}</span>{/if}
                <button class="hns-plain" onclick={() => (lspDraft = { id: "", name: "", command: "", args: "", extensions: "", enabled: true })}>+ 添加服务器</button>
              </div>
              {#each lspServers as s (s.id)}
                <div class="hns-preset-item">
                  <div class="hns-preset-main">
                    <span class="hns-preset-name">{s.name}</span>
                    <span class="hns-preset-meta">{s.command} {s.args.join(" ")}</span>
                    {#if (s.extensions?.length ?? 0) > 0}<span class="hns-preset-meta">扩展名：{s.extensions.join(",")}</span>{/if}
                    {#if !s.enabled}<span class="hns-preset-meta">已停用</span>{/if}
                  </div>
                  <span class="hns-session-acts">
                    <button class="hns-session-act" onclick={() => (lspDraft = { id: s.id, name: s.name, command: s.command, args: s.args.join(", "), extensions: (s.extensions ?? []).join(","), enabled: s.enabled })} title="编辑"><PencilIcon class="size-3" /></button>
                    <button class="hns-session-act" onclick={() => deleteLsp(s.id)} title="删除"><Trash2Icon class="size-3" /></button>
                  </span>
                </div>
              {:else}
                <div class="hns-tools-empty">暂无 LSP 服务器</div>
              {/each}
            {/if}
          </div>
        {/if}
      </div>
    {/if}

    {#if sessionState?.goal}
      <div class="hns-goal" title="本会话目标">
        {#if goalEditing}
          <input
            class="hns-goal-edit"
            bind:value={goalDraft}
            onkeydown={(e) => {
              if (e.key === "Enter") saveGoalEdit();
              if (e.key === "Escape") cancelGoalEdit();
            }}
            onblur={cancelGoalEdit}
          />
          <span class="hns-goal-actions">
            <button class="hns-goal-act" onclick={saveGoalEdit} title="保存目标">✓ 保存</button>
            <button class="hns-goal-act" onclick={cancelGoalEdit} title="取消编辑">✕ 取消</button>
          </span>
        {:else}
          🎯 {sessionState.goal}
          {#if sessionState.goal_status !== "active"}
            <span class="hns-goal-status">
              （{sessionState.goal_status === "paused" ? "已暂停" : sessionState.goal_status === "blocked" ? `已阻塞：${sessionState.goal_blocked_reason}` : "已完成"}）
            </span>
          {/if}
          <span class="hns-goal-actions">
            {#if sessionState.goal_status === "paused"}
              <button class="hns-goal-act" onclick={() => goalAction("resume")} title="恢复目标">▶ 恢复</button>
            {:else if sessionState.goal_status === "active"}
              <button class="hns-goal-act" onclick={() => goalAction("pause")} title="暂停目标">⏸ 暂停</button>
            {/if}
            {#if sessionState.goal_status === "active" || sessionState.goal_status === "paused" || sessionState.goal_status === "blocked"}
              <button class="hns-goal-act" onclick={startGoalEdit} title="编辑目标">✎ 编辑</button>
              <button class="hns-goal-act" onclick={() => goalAction("complete")} title="标记完成">✓ 完成</button>
              <button class="hns-goal-act danger" onclick={() => goalAction("clear")} title="清除目标">✕ 清除</button>
            {/if}
          </span>
        {/if}
      </div>
    {/if}
    {#if sessionState?.plan_mode}
      <div class="hns-plan" title="计划模式：仅只读工具可用">
        计划模式 {#if sessionState.plan_text}· {sessionState.plan_text.slice(0, 80)}{/if} —— 仅只读工具可用
      </div>
    {/if}
    {#if (sessionState?.todos?.length ?? 0) > 0}
      <div class="hns-todos">
        <div class="hns-todos-head">待办（{sessionState!.todos.length}）</div>
        {#each sessionState!.todos as t (t.id)}
          <div class="hns-todo" class:done={t.status === "completed"} class:doing={t.status === "in_progress"}>
            <span class="hns-todo-status">
              {#if t.status === "completed"}✓{:else if t.status === "in_progress"}▶{:else}○{/if}
            </span>
            <span class="hns-todo-text">{t.content}</span>
          </div>
        {/each}
      </div>
    {/if}

    <!-- 视图切换条（DSH 会话头视图标签页：对话 | 轨迹；贴近内容区常驻） -->
    {#if activeId}
      <div class="hns-view-switch" role="tablist" aria-label="会话视图">
        <button
          role="tab"
          aria-selected={viewTab === "chat"}
          class:on={viewTab === "chat"}
          onclick={() => switchView("chat")}
        >
          对话
        </button>
        <button
          role="tab"
          aria-selected={viewTab === "trajectory"}
          class:on={viewTab === "trajectory"}
          onclick={() => switchView("trajectory")}
        >
          轨迹
        </button>
      </div>
    {/if}

    {#if viewTab === "chat"}
    <div class="hns-msgs" class:empty={messages.length === 0}>
      {#if error}
        <div class="hns-turn-error" role="alert">
          <span class="hns-turn-error-ico" aria-hidden="true">⚠️</span>
          <span class="hns-turn-error-text">{error}</span>
          <button class="hns-turn-retry" onclick={retryLastTurn} disabled={sending}>重试本轮</button>
        </div>
      {/if}
      {#if messages.length === 0 && !streamBuf}
        <div class="hns-hero">
          <div class="hns-hero-logo" aria-hidden="true">🐋</div>
          <h2>探索未至之境</h2>
          <span class="hns-hero-badge">预览版</span>
          <p>代理运行时会话界面。模型可调用本地工具（联网搜索 / 知识库检索 / 文件读写 / 命令执行），
             工具过程随会话日志持久化，重新打开仍可回放。</p>
          <div class="hns-hero-seats">
            <!-- 工作区 chip（DSH WorkspacePicker：新会话归属选择） -->
            <button
              class="hns-hero-chip"
              onclick={() => { drawerTab = "settings"; drawerOpen = true; }}
              title="选择工作区"
            >
              <FolderIcon class="size-3.5" />
              {activeWorkspaceTitle || "选择工作区"}
            </button>
            <!-- Agent 预设座位（DSH AgentPresetSeat：新会话预设选择） -->
            <button
              class="hns-hero-chip"
              onclick={() => { drawerTab = "presets"; drawerOpen = true; }}
              title="选择 Agent 预设"
            >
              <SlidersHorizontalIcon class="size-3.5" />
              {sessionPresetTitle || "Agent 预设"}
            </button>
          </div>
        </div>
      {/if}
      {#if !sending && !streamBuf && messages.length > visibleCount}
        <div class="hns-load-earlier">
          <button onclick={loadEarlier} title="加载更早的消息">
            加载更早（还有 {messages.length - visibleCount} 条）
          </button>
        </div>
      {/if}
      {#each messages as m, i (i)}
        {#if i >= messages.length - visibleCount}
        {#if m.role === "meta"}
          {#if m.kind === "workflow" && m.workflow}
            <!-- 工作流运行面板（DSH WorkflowRunPanel 迁移：运行头 + 阶段进度点 + 输出） -->
            <div class="hns-workflow-run" data-run-status="completed">
              <div class="hns-wf-head">
                <span class="hns-wf-title">📋 工作流「{m.workflow.name}」</span>
                <span class="hns-wf-dots">
                  {#each Array(m.workflow.total) as _, di (di)}
                    <span class="hns-wf-dot" class:done={di < m.workflow.stage}></span>
                  {/each}
                </span>
                <span class="hns-wf-status">
                  {m.workflow.stage}/{m.workflow.total} 阶段{m.workflow.stage === m.workflow.total ? " · 已完成" : ""}
                </span>
                {#if m.detail}
                  <button
                    class="hns-meta-toggle"
                    onclick={() => (expandedMeta = expandedMeta === m.seq ? null : m.seq)}
                    title="展开 / 收起阶段输出"
                  >
                    {expandedMeta === m.seq ? "收起" : "输出"}
                  </button>
                {/if}
              </div>
              {#if m.detail && expandedMeta === m.seq}
                <div class="hns-wf-phase">
                  <div class="hns-wf-phase-head">
                    <span>阶段 {m.workflow.stage}</span>
                    <span class="hns-wf-phase-status">已完成</span>
                  </div>
                  <pre class="hns-wf-output">{m.detail}</pre>
                </div>
              {/if}
            </div>
          {:else}
          <div class="hns-meta" class:compaction={m.kind === "compaction"}>
            <span class="hns-meta-title">
              {#if m.kind === "compaction"}🗜️{/if}
              {#if m.kind === "context"}📄{/if}
              {#if m.kind === "skill"}🧩{/if}
              {m.title}
            </span>
            {#if m.detail}
              <button
                class="hns-meta-toggle"
                onclick={() => (expandedMeta = expandedMeta === m.seq ? null : m.seq)}
                title="展开 / 收起详情"
              >
                {expandedMeta === m.seq ? "收起" : "详情"}
              </button>
            {/if}
            {#if m.detail && expandedMeta === m.seq}
              <span class="hns-meta-detail">{m.detail}</span>
            {/if}
          </div>
          {/if}
        {:else if m.role === "user"}
          <div class="hns-msg hns-msg-user" class:cmd={m.content.startsWith("/goal")}>
            {#if m.content.startsWith("/goal")}
              <!-- /goal 命令输入视图（DSH GoalCommandInputView：命令气泡，右对齐等宽） -->
              <div class="hns-cmd-bubble" role="group">{m.content}</div>
            {:else}
              <div class="hns-bubble"><MessageBody msg={{ role: "user", content: m.content }} /></div>
            {/if}
            {#if m.content}
              <button
                class="hns-session-act hns-copy-btn"
                onclick={() => copyText(m.content)}
                title="复制消息"
              >
                {#if copiedText === m.content.slice(0, 20)}<CheckIcon class="size-3" />{:else}<CopyIcon class="size-3" />{/if}
              </button>
            {/if}
            {#if m.seq > 0}
              <button class="hns-fork-btn" onclick={() => forkSessionAt(m.seq)} title="从此消息分叉新会话（复制此前全部日志）">
                <GitForkIcon class="size-3" />分叉
              </button>
            {/if}
          </div>
        {:else}
          <div class="hns-msg hns-msg-bot">
            <div class="hns-bot-col">
              {#if (m.tools?.length ?? 0) > 0}
                <div class="hns-tool-timeline">
                  {#each m.tools! as s (s.id)}
                    <div
                      class="hns-tool-step"
                      class:ok={s.status === "ok"}
                      class:err={s.status === "err"}
                      class:open={expandedStep === s.id}
                    >
                      <span class="hns-tool-node" aria-hidden="true">
                        {#if s.status === "running"}
                          <span class="hns-tool-node-dot"></span>
                        {:else if s.status === "err"}
                          <XIcon class="size-3" />
                        {:else}
                          <CheckIcon class="size-3" />
                        {/if}
                      </span>
                      <button
                        class="hns-tool-head"
                        onclick={() => (expandedStep = expandedStep === s.id ? null : s.id)}
                        title={expandedStep === s.id ? "收起详情" : "展开参数与结果"}
                      >
                        <span class="hns-tool-name">{s.name}</span>
                        {#if s.args}
                          <span class="hns-tool-args" title={s.args}>{s.args.length > 40 ? s.args.slice(0, 40) + "…" : s.args}</span>
                        {/if}
                        <span class="hns-tool-status">
                          {#if s.status === "running"}
                            <span class="hns-tool-running">执行中…</span>
                          {:else if s.status === "err"}
                            失败
                          {:else}
                            完成
                          {/if}
                        </span>
                        {#if s.duration_ms != null}
                          <span class="hns-tool-dur">{fmtDuration(s.duration_ms)}</span>
                        {/if}
                        <span class="hns-tool-chevron">{expandedStep === s.id ? "▾" : "▸"}</span>
                      </button>
                      {#if expandedStep === s.id}
                        <div class="hns-tool-detail">
                          <div class="hns-tool-detail-actions">
                            <button class="hns-tool-copy" onclick={() => openDetail(s.name, s.args, s.result, s.status === "ok", s.duration_ms, s.status === "running")} title="在右侧详情面板打开">
                              <PanelRightIcon class="size-3" />面板
                            </button>
                          </div>
                          <ToolCard
                            name={s.name}
                            args={s.args}
                            result={s.result ?? ""}
                            ok={s.status === "ok"}
                          />
                        </div>
                      {/if}
                    </div>
                  {/each}
                </div>
              {/if}
              {#if m.reasoning}
                <div class="hns-think" class:open={thinkOpen["h" + m.seq]}>
                  <button class="hns-think-head" onclick={() => toggleThink("h" + m.seq)} title="展开 / 收起推理过程">
                    <span class="hns-think-icon" aria-hidden="true">💭</span>
                    <span class="hns-think-label">Think</span>
                    <span class="hns-think-chevron">{thinkOpen["h" + m.seq] ? "▾" : "▸"}</span>
                  </button>
                  {#if thinkOpen["h" + m.seq]}
                    <div class="hns-think-body">{m.reasoning}</div>
                  {/if}
                </div>
              {/if}
              <div class="hns-bubble"><MessageBody msg={{ role: "assistant", content: m.content }} /></div>
              <div class="hns-feedback">
                {#if m.seq > 0}
                  <button class="hns-fork-btn" onclick={() => forkSessionAt(m.seq)} title="从此回复分叉新会话（复制此前全部日志）">
                    <GitForkIcon class="size-3" />分叉
                  </button>
                {/if}
                {#if m.content}
                  <button
                    class="hns-session-act"
                    class:speaking={speakingIdx === i}
                    onclick={() => speakMessage(m, i)}
                    title={speakingIdx === i ? "停止朗读" : "朗读此回复"}
                  >
                    {#if speakingIdx === i}<SquareIcon class="size-3" />{:else}<Volume2Icon class="size-3" />{/if}
                  </button>
                  <button class="hns-session-act" onclick={() => copyText(m.content)} title="复制回复">
                    {#if copiedText === m.content.slice(0, 20)}<CheckIcon class="size-3" />{:else}<CopyIcon class="size-3" />{/if}
                  </button>
                  <button class="hns-session-act" onclick={() => sendFeedback("good", m.seq)} title="回答有帮助">
                    <ThumbsUpIcon class="size-3" />
                  </button>
                  <button class="hns-session-act" onclick={() => sendFeedback("bad", m.seq)} title="回答需改进">
                    <ThumbsDownIcon class="size-3" />
                  </button>
                  <button class="hns-session-act" onclick={() => openFeedbackNote(m.seq)} title="补充说明">
                    <MessageSquarePlusIcon class="size-3" />
                  </button>
                {/if}
              </div>
              {#if feedbackDraftOpen === m.seq}
                <!-- 反馈补充说明（DSH MessageFeedbackActions：备注 + 保存/取消） -->
                <div class="hns-feedback-note">
                  <textarea
                    class="hns-feedback-note-input"
                    placeholder="补充说明…"
                    value={feedbackComment[m.seq] ?? ""}
                    oninput={(e) => {
                      feedbackComment = {
                        ...feedbackComment,
                        [m.seq]: (e.currentTarget as HTMLTextAreaElement).value,
                      };
                    }}
                  ></textarea>
                  <span class="hns-feedback-note-actions">
                    <button class="hns-session-act" onclick={() => saveFeedbackNote(m.seq)} title="保存备注">
                      <CheckIcon class="size-3" />保存
                    </button>
                    <button class="hns-session-act" onclick={() => (feedbackDraftOpen = null)} title="取消">
                      <XIcon class="size-3" />取消
                    </button>
                  </span>
                </div>
              {/if}
              {#if m.seq > 0 && (turnFilesByUser.get(turnUserSeq(m.seq) ?? -1) ?? []).length > 0}
                <!-- 回合尾产物（DSH TurnTail + ProducedFiles：本轮编辑/写入的文件） -->
                {@const ownerFiles = turnFilesByUser.get(turnUserSeq(m.seq) ?? -1) ?? []}
                <div class="hns-turn-files" title="本轮编辑/写入的文件（点击打开）">
                  <span class="hns-turn-files-label">产物</span>
                  {#each ownerFiles as f (f.path)}
                    <button class="hns-file-chip" onclick={() => openHarnessPath(f.path)} title={f.path}>
                      <FolderOpenIcon class="size-3" />{f.path.split(/[\\/]/).pop()}
                    </button>
                  {/each}
                </div>
              {/if}
            </div>
          </div>
        {/if}
        {/if}
      {/each}
      {#if messages.length > 0}
        <button class="hns-scroll-bottom" onclick={scrollBottom} title="回到底部" aria-label="回到底部">
          ↓
        </button>
      {/if}
      {#if liveTools.length > 0 || streamBuf || (sending && messages.length === 0)}
        <div class="hns-msg hns-msg-bot">
          <div class="hns-bot-col">
            {#if liveTools.length > 0}
              <div class="hns-tool-timeline">
                {#each liveTools as s (s.id)}
                  <div
                    class="hns-tool-step"
                    class:ok={s.status === "ok"}
                    class:err={s.status === "err"}
                    class:open={expandedStep === s.id}
                  >
                    <span class="hns-tool-node" aria-hidden="true">
                      {#if s.status === "running"}
                        <span class="hns-tool-node-dot"></span>
                      {:else if s.status === "err"}
                        <XIcon class="size-3" />
                      {:else}
                        <CheckIcon class="size-3" />
                      {/if}
                    </span>
                    <button
                      class="hns-tool-head"
                      onclick={() => (expandedStep = expandedStep === s.id ? null : s.id)}
                      title={expandedStep === s.id ? "收起详情" : "展开参数与结果"}
                    >
                      <span class="hns-tool-name">{s.name}</span>
                      {#if s.args}
                        <span class="hns-tool-args" title={s.args}>{s.args.length > 40 ? s.args.slice(0, 40) + "…" : s.args}</span>
                      {/if}
                      <span class="hns-tool-status">
                        {#if s.status === "running"}
                          <span class="hns-tool-running">执行中…</span>
                        {:else if s.status === "err"}
                          失败
                        {:else}
                          完成
                        {/if}
                      </span>
                      {#if s.duration_ms != null}
                        <span class="hns-tool-dur">{fmtDuration(s.duration_ms)}</span>
                      {/if}
                      <span class="hns-tool-chevron">{expandedStep === s.id ? "▾" : "▸"}</span>
                    </button>
                    {#if expandedStep === s.id}
                      <div class="hns-tool-detail">
                        <div class="hns-tool-detail-actions">
                          <button class="hns-tool-copy" onclick={() => openDetail(s.name, s.args, s.result, s.status === "ok", s.duration_ms, s.status === "running")} title="在右侧详情面板打开">
                            <PanelRightIcon class="size-3" />面板
                          </button>
                        </div>
                        <ToolCard
                          name={s.name}
                          args={s.args}
                          result={s.result ?? ""}
                          ok={s.status === "ok"}
                        />
                      </div>
                    {/if}
                  </div>
                {/each}
              </div>
            {/if}
            {#if streamReasoning}
              <div class="hns-think" class:open={thinkOpen["live"]}>
                <button class="hns-think-head" onclick={() => toggleThink("live")} title="展开 / 收起推理过程">
                  <span class="hns-think-icon" aria-hidden="true">💭</span>
                  <span class="hns-think-label">Think</span>
                  <span class="hns-think-running">思考中…</span>
                  <span class="hns-think-chevron">{thinkOpen["live"] ? "▾" : "▸"}</span>
                </button>
                {#if thinkOpen["live"]}
                  <div class="hns-think-body">{streamReasoning}</div>
                {/if}
              </div>
            {/if}
            <div class="hns-bubble">
              <MessageBody msg={{ role: "assistant", content: streamBuf }} />
              {#if !streamBuf}<span class="hns-caret"></span>{/if}
            </div>
          </div>
        </div>
      {/if}
      {#if sending && streamBuf}
        <span class="hns-stream-hint">生成中…</span>
      {/if}
    </div>
    {:else}
      <div class="hns-trajectory-wrap">
        {#if trajectoryLoading}
          <div class="hns-traj-loading">轨迹加载中…</div>
        {:else if trajectoryError}
          <div class="hns-traj-loading hns-traj-error" title={trajectoryError}>轨迹加载失败：{trajectoryError}</div>
        {:else if trajectory}
          <TrajectoryView
            entries={trajectory.entries}
            turnCount={trajectory.turn_count}
            toolCallCount={trajectory.tool_call_count}
            onOpenPath={openHarnessPath}
            onInspect={inspectTrajectoryEntry}
          />
        {:else}
          <div class="hns-traj-loading">暂无轨迹</div>
        {/if}
      </div>
    {/if}

    {#if pendingApprovals.length > 0}
      <div class="hns-approvals">
        {#each pendingApprovals as a (a.id)}
          <div class="hns-approval">
            <div class="hns-approval-head">
              <ShieldAlertIcon class="size-3.5" />
              <span class="hns-approval-text" title={a.description}>{a.tool} 需要批准</span>
              {#if a.arguments}
                <code class="hns-approval-args" title={prettyText(a.arguments)}>{a.arguments.length > 60 ? a.arguments.slice(0, 60) + "…" : a.arguments}</code>
                <button class="hns-tool-copy" onclick={() => copyText(prettyText(a.arguments))}>复制</button>
              {/if}
              <span class="hns-approval-actions">
                <button class="hns-approve" onclick={() => approvePending(a, true)} title="同一工具在本会话有效期内不再询问">记住并批准</button>
                <button class="hns-approve" onclick={() => approvePending(a)}>批准</button>
                <button class="hns-reject" onclick={() => rejectPending(a)}>拒绝</button>
              </span>
            </div>
          </div>
        {/each}
      </div>
    {/if}

    {#if pendingQuestions.length > 0}
      {@const q = pendingQuestions[Math.min(questionIndex, pendingQuestions.length - 1)]}
      <div class="hns-approvals">
        {#if q.question.startsWith("方案评审")}
          <!-- 计划待审（DSH PlanReviewPanel：确认执行 / 拒绝 / 去聊天里说） -->
          <div class="hns-plan-review" role="alertdialog" aria-label="计划待审">
            <div class="hns-plan-review-head">
              <span class="hns-plan-review-title">📋 计划待审</span>
              {#if pendingQuestions.length > 1}
                <span class="hns-q-progress">{questionIndex + 1} / {pendingQuestions.length}</span>
              {/if}
            </div>
            <div class="hns-plan-review-body">
              {q.question.slice("方案评审（计划模式退出）：".length)}
            </div>
            <div class="hns-plan-review-actions">
              {#each q.options as o (o)}
                <button
                  class:primary={o === "确认执行"}
                  onclick={() => {
                    answerQuestion(q, o);
                    if (o === "去聊天里说") window.setTimeout(() => inputRef?.focus(), 50);
                  }}
                >
                  {o}
                </button>
              {/each}
            </div>
          </div>
        {:else if q.multi_select}
          <!-- 多选提问（DSH QuestionFlow：复选框 + 勾选提交） -->
          <div class="hns-approval">
            <div class="hns-approval-head">
              <span class="hns-approval-text" title={q.question}>{q.question}</span>
              <span class="hns-approval-actions">
                <span class="hns-q-checks">
                  {#each q.options as o (o)}
                    <label class="hns-q-check">
                      <input
                        type="checkbox"
                        checked={(questionChecks[q.id] ?? new Set()).has(o)}
                        onchange={() => toggleQuestionCheck(q.id, o)}
                      />
                      {o}
                    </label>
                  {/each}
                </span>
                <button
                  class="hns-approve"
                  disabled={(questionChecks[q.id] ?? new Set()).size === 0}
                  onclick={() => answerQuestionMulti(q, questionChecks[q.id] ?? new Set())}
                >
                  提交
                </button>
              </span>
            </div>
          </div>
        {:else}
        <div class="hns-approval">
          <div class="hns-approval-head">
            <span class="hns-approval-text" title={q.question}>{q.question}</span>
            <span class="hns-approval-actions">
              {#each q.options as o (o)}
                <button class="hns-approve" onclick={() => answerQuestion(q, o)}>{o}</button>
              {/each}
              <input
                class="hns-question-input"
                placeholder="自由输入回答…"
                aria-label="问题回答"
                value={questionDrafts[q.id] ?? ""}
                oninput={(e) => {
                  questionDrafts = {
                    ...questionDrafts,
                    [q.id]: (e.currentTarget as HTMLInputElement).value,
                  };
                }}
                onkeydown={(e) => {
                  if (e.key === "Enter") answerQuestion(q, questionDrafts[q.id] ?? "");
                }}
              />
              <button class="hns-approve" disabled={!(questionDrafts[q.id] ?? "").trim()} onclick={() => answerQuestion(q, questionDrafts[q.id] ?? "")}>回答</button>
            </span>
          </div>
        </div>
        {/if}
        {#if pendingQuestions.length > 1}
          <!-- 多题分页（DSH QuestionFlow：上一题 / 下一题 / 跳过本题） -->
          <div class="hns-q-nav">
            <button class="hns-q-nav-btn" disabled={questionIndex === 0} onclick={prevQuestion}>上一题</button>
            <span class="hns-q-progress">{questionIndex + 1} / {pendingQuestions.length}</span>
            <button
              class="hns-q-nav-btn"
              disabled={questionIndex >= pendingQuestions.length - 1}
              onclick={nextQuestion}
            >
              下一题
            </button>
            <button
              class="hns-q-nav-btn"
              onclick={() => answerQuestion(q, "（跳过本题）")}
              title="跳过本题"
            >
              跳过本题
            </button>
          </div>
        {/if}
      </div>
    {/if}

    {#if attachments.length > 0}
      <div class="hns-attachments">
        {#each attachments as a (a.id)}
          {#if a.kind === "image"}
            <button
              class="hns-attachment hns-attachment-img"
              title={`${a.path}\n点击查看原图`}
              onclick={() => openImageLightbox(a.path, a.name)}
            >
              🖼️ {a.name}
            </button>
          {:else}
            <span class="hns-attachment" title={a.path}>📎 {a.name}{#if a.kind === "text"}（已注入上下文预览）{/if}</span>
          {/if}
        {/each}
      </div>
    {/if}
    {#if lightboxSrc}
      <div
        class="hns-lightbox"
        role="dialog"
        aria-modal="true"
        tabindex="-1"
        onclick={() => (lightboxSrc = null)}
        onkeydown={(e) => {
          if (e.key === "Escape" || e.key === "Enter" || e.key === " ") lightboxSrc = null;
        }}
      >
        <button class="hns-lightbox-close" title="关闭" aria-label="关闭图片预览">✕</button>
        <img src={lightboxSrc} alt={lightboxName} />
        <span class="hns-lightbox-name">{lightboxName}</span>
      </div>
    {/if}
    {#if queue.length > 0}
      <div class="hns-queue" data-queue-dock="">
        <div class="hns-queue-head">
          <ListTodoIcon class="size-3.5" />
          <span>{queue.length} 条排队消息</span>
          <span class="hns-queue-hint">{busyEnter === "steer" ? "插话模式" : "回合结束后自动发送"}</span>
        </div>
        {#each queue as q (q.id)}
          <div class="hns-queue-row">
            <span class="hns-queue-text" title={q.text}>{q.text}</span>
            <span class="hns-session-acts">
              <button class="hns-session-act" onclick={() => steerQueued(q.id)} title="插话（排到队首）">
                <SendIcon class="size-3" />
              </button>
              <button
                class="hns-session-act"
                onclick={() => {
                  const next = window.prompt("编辑排队消息", q.text);
                  if (next && next.trim()) editQueued(q.id, next.trim());
                }}
                title="编辑"
              >
                <PencilIcon class="size-3" />
              </button>
              <button class="hns-session-act" onclick={() => removeQueued(q.id)} title="删除">
                <Trash2Icon class="size-3" />
              </button>
            </span>
          </div>
        {/each}
      </div>
    {/if}
    <div class="hns-input" role="group" aria-label="Harness 对话输入区">
      {#if sessionState?.plan_mode}
        <button
          class="hns-plan-chip"
          onclick={exitPlanFromChip}
          title="计划模式已开启，点击退出（/plan off）"
        >
          Plan ×
        </button>
      {/if}
      <select
        class="hns-perm-chip"
        value={settingsForm.sandbox_mode ?? "workspace-write"}
        onchange={(e) => changeSandboxMode((e.currentTarget as HTMLSelectElement).value)}
        title="访问模式（当前：{SANDBOX_LABELS[settingsForm.sandbox_mode ?? 'workspace-write']}）"
      >
        <option value="read-only">只读</option>
        <option value="workspace-write">工作区写入</option>
        <option value="danger-full-access">完全访问</option>
      </select>
      <button
        class="hns-attach-btn"
        onclick={attachFromDialog}
        disabled={attachBusy || sending}
        title="附加文件（复制进工作区，文本内容预览注入上下文）"
      >
        <PaperclipIcon class="size-3.5" />
      </button>
      <textarea
        bind:this={inputRef}
        bind:value={input}
        placeholder="输入消息，Enter 发送，Shift+Enter 换行；/ 命令菜单，@ 提及技能"
        rows="1"
        disabled={sending}
        oninput={onInputValueChange}
        onkeydown={(e) => {
          onInputKeydown(e);
          if (e.key === "Enter" && !e.shiftKey && !slashOpen && !atOpen) {
            e.preventDefault();
            send();
          }
        }}
      ></textarea>
      {#if slashOpen && slashMatches.length > 0}
        <div class="hns-slash-menu" role="listbox" aria-label="命令菜单">
          <div class="hns-slash-head">命令</div>
          {#each slashMatches as c, i (c.name)}
            <button
              role="option"
              aria-selected={i === slashIndex}
              class:on={i === slashIndex}
              onclick={() => pickSlashCommand(c.name)}
              onmouseenter={() => (slashIndex = i)}
            >
              <span class="hns-slash-name">/{c.name}</span>
              <span class="hns-slash-desc">{c.desc}</span>
            </button>
          {/each}
        </div>
      {/if}
      {#if atOpen && atMatches.length > 0}
        <div class="hns-slash-menu" role="listbox" aria-label="提及菜单">
          <div class="hns-slash-head">技能</div>
          {#each atMatches as s, i (s.id)}
            <button
              role="option"
              aria-selected={i === atIndex}
              class:on={i === atIndex}
              onclick={() => pickAtSkill(s)}
              onmouseenter={() => (atIndex = i)}
            >
              <span class="hns-slash-name">@{s.id}</span>
              <span class="hns-slash-desc">{s.description || s.name}</span>
            </button>
          {/each}
        </div>
      {/if}
      {#if voiceStatus}
        <span class="hns-voice-status" title="语音状态">{voiceStatus}</span>
      {/if}
      <button
        class="hns-attach-btn"
        class:rec={voiceRecorder.recording}
        onclick={toggleVoiceInput}
        disabled={sending}
        title={voiceRecorder.recording ? "停止录音" : "语音输入（麦克风）"}
      >
        {#if voiceRecorder.recording}<SquareIcon class="size-3.5" />{:else}<MicIcon class="size-3.5" />{/if}
      </button>
      {#if contextMeter && contextMeter.budget_tokens > 0}
        <span class="hns-meter-wrap">
          <button
            class="hns-meter"
            onclick={() => (meterOpen = !meterOpen)}
            title="上下文占用：{Math.round(contextMeter.percent * 100)}%（{fmtCtxTok(contextMeter.used_tokens)} / {fmtCtxTok(contextMeter.budget_tokens)} token）"
            aria-label="上下文占用仪表"
          >
            <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
              <circle cx="10" cy="10" r="8" fill="none" stroke="var(--hns-border)" stroke-width="2.5" />
              <circle
                cx="10"
                cy="10"
                r="8"
                fill="none"
                stroke={contextMeter.percent >= 0.9 ? "#d73a49" : contextMeter.percent >= 0.7 ? "#b08800" : "var(--hns-accent)"}
                stroke-width="2.5"
                stroke-linecap="round"
                stroke-dasharray="{2 * Math.PI * 8}"
                stroke-dashoffset={2 * Math.PI * 8 * (1 - contextMeter.percent)}
                transform="rotate(-90 10 10)"
              />
            </svg>
          </button>
          {#if meterOpen}
            <div class="hns-meter-panel">
              <div class="hns-meter-head">
                <span>上下文已用 {Math.round(contextMeter.percent * 100)}%</span>
                <span class="hns-meter-sub">约 {fmtCtxTok(contextMeter.used_tokens)} / {fmtCtxTok(contextMeter.budget_tokens)} token</span>
              </div>
              <div class="hns-meter-rows">
                <div class="hns-meter-row"><span class="hns-meter-dot sys"></span>系统提示词 <b>{fmtCtxTok(contextMeter.system_tokens)}</b></div>
                <div class="hns-meter-row"><span class="hns-meter-dot tools"></span>工具 schema <b>{fmtCtxTok(contextMeter.tools_tokens)}</b></div>
                <div class="hns-meter-row"><span class="hns-meter-dot msgs"></span>对话消息 <b>{fmtCtxTok(contextMeter.messages_tokens)}</b></div>
              </div>
            </div>
          {/if}
        </span>
      {/if}
      {#if sending}
        <button
          class="hns-stop"
          onclick={stopTurn}
          title="停止生成（中断当前回合；已生成内容保留）"
        >
          <SquareIcon class="size-3.5" />
          停止
        </button>
      {:else}
        <button
          class="hns-send"
          disabled={!canSend}
          onclick={send}
          title="发送"
        >
          <SendIcon class="size-4" />
        </button>
      {/if}
    </div>

    {#if detailCall}
      {@const dc = detailCall}
      <div class="hns-details">
        <div class="hns-details-head">
          <div class="hns-details-title">
            <WrenchIcon class="size-3.5" />
            <span>{dc.name}</span>
            {#if dc.running}
              <span class="hns-details-status running">运行中…</span>
            {:else if dc.ok}
              <span class="hns-details-status ok">完成</span>
            {:else}
              <span class="hns-details-status err">失败</span>
            {/if}
            {#if dc.duration_ms != null}
              <span class="hns-details-dur">{fmtDuration(dc.duration_ms)}</span>
            {/if}
          </div>
          <button class="hns-drawer-close" onclick={() => (detailCall = null)} title="关闭详情">
            <XIcon class="size-3.5" />
          </button>
        </div>
        <div class="hns-details-body">
          <div class="hns-tool-field">
            <div class="hns-tool-field-head">
              <span>输入</span>
              <button class="hns-tool-copy" onclick={() => copyText(prettyText(dc.args))}>
                {#if copiedText === dc.args.slice(0, 20)}<CheckIcon class="size-3" />已复制{:else}<CopyIcon class="size-3" />复制{/if}
              </button>
            </div>
            <pre class="hns-tool-pre">{prettyText(dc.args) || "（无参数）"}</pre>
          </div>
          <div class="hns-tool-field">
            <div class="hns-tool-field-head">
              <span>输出</span>
              {#if !dc.running}
                <button class="hns-tool-copy" onclick={() => copyText(dc.result)}>
                  <CopyIcon class="size-3" />复制
                </button>
              {/if}
            </div>
            <pre class="hns-tool-pre">{dc.running ? "运行中…" : (dc.result || "（无输出）")}</pre>
          </div>
          {#if usage}
            <!-- 计时 / 用量（DSH 检查器 Timing/Usage 面板等价：会话级遥测聚合） -->
            <div class="hns-tool-field">
              <div class="hns-tool-field-head"><span>计时</span></div>
              <div class="hns-tool-metrics">
                <span>LLM {fmtWall(usage.llm_wall_ms)}</span>
                <span>首 token 平均 {fmtDuration(usage.first_token_avg_ms)}</span>
                <span>{usage.tokens_per_sec.toFixed(0)} tok/s</span>
                <span>工具 {fmtWall(usage.tool_wall_ms)}</span>
              </div>
            </div>
            <div class="hns-tool-field">
              <div class="hns-tool-field-head"><span>用量</span></div>
              <div class="hns-tool-metrics">
                <span>输入 {fmtTok(usage.input_tokens)}</span>
                <span>输出 {fmtTok(usage.output_tokens)}</span>
                <span>缓存命中 {Math.round(usage.cache_hit_rate * 100)}%</span>
                <span>成本 ${usage.cost.toFixed(4)}</span>
              </div>
            </div>
          {/if}
        </div>
      </div>
    {/if}
  </main>
</div>

<style>
  /* ═══════════════════════════════════════════════════════
     Harness — 界面设计系统（重设计）
     层次：玻璃卡片 / 强调色轨 / 分隔语义色；
     沿用应用令牌 --app-color-*，Harness 内部派生 --hns-*。
     ═══════════════════════════════════════════════════════ */
  .hns {
    --hns-accent: var(--app-color-accent);
    --hns-accent-soft: color-mix(in srgb, var(--app-color-accent) 12%, transparent);
    --hns-card: var(--app-color-card-bg);
    --hns-card-2: color-mix(in srgb, var(--app-color-card-bg) 96%, var(--app-color-text));
    --hns-border: var(--app-color-border);
    --hns-border-light: var(--app-color-border-light);
    --hns-muted: var(--app-color-muted);
    --hns-text: var(--app-color-text);
    --hns-surface: color-mix(in srgb, var(--app-color-bg-subtle) 78%, transparent);
    --hns-amber: #f59e0b;
    --hns-amber-soft: color-mix(in srgb, #f59e0b 11%, transparent);
    --hns-green: #34d399;
    --hns-red: #f87171;
    --hns-radius: 12px;
    display: flex; height: 100%; min-height: 0;
    background:
      radial-gradient(1200px 500px at 15% -10%, color-mix(in srgb, var(--app-color-accent) 5%, transparent), transparent 60%),
      radial-gradient(900px 420px at 110% 110%, color-mix(in srgb, var(--app-color-accent) 4%, transparent), transparent 55%),
      var(--app-color-bg-subtle);
  }
  /* ─── 整页拖放遮罩（DSH DropOverlay 迁移） ─── */
  .hns-drop-overlay {
    position: fixed;
    inset: 0;
    z-index: 9999;
    display: flex;
    align-items: center;
    justify-content: center;
    background: color-mix(in srgb, var(--hns-card, #fff) 78%, transparent);
    backdrop-filter: blur(3px);
    pointer-events: none;
  }
  .hns-drop-inner {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    font-size: 14px;
    color: var(--hns-accent, #4176e6);
    border: 2px dashed color-mix(in srgb, var(--hns-accent, #4176e6) 45%, transparent);
    border-radius: 16px;
    padding: 36px 60px;
    background: color-mix(in srgb, var(--hns-accent, #4176e6) 5%, transparent);
  }
  .hns-drop-icon { font-size: 30px; }
  /* ─── 会话侧栏 ─── */
  .hns-side {
    width: 260px; flex: none; display: flex; flex-direction: column;
    border-right: 1px solid var(--hns-border);
    background: color-mix(in srgb, var(--hns-card) 88%, transparent);
    backdrop-filter: blur(8px);
    transition: width .18s ease;
    overflow: hidden;
  }
  .hns-side.collapsed { width: 52px; }
  .hns-side-head {
    display: flex; align-items: center; justify-content: space-between; gap: 8px;
    padding: 14px 14px 10px;
  }
  .hns-side-head-actions {
    display: inline-flex; align-items: center; gap: 6px; flex: none;
  }
  /* rail 态：头部按钮竖排（折叠开关在上、新建在下），32px 方形统一居中 */
  .hns-side.collapsed .hns-side-head {
    flex-direction: column;
    padding: 12px 0 8px;
    gap: 8px;
  }
  .hns-side.collapsed .hns-side-head-actions {
    flex-direction: column;
    gap: 8px;
    width: 100%;
  }
  .hns-side-collapse {
    display: inline-flex; align-items: center; justify-content: center;
    width: 30px; height: 30px; flex: none;
    background: var(--hns-surface); border: 1px solid var(--hns-border);
    color: var(--hns-muted); border-radius: 8px; cursor: pointer;
    transition: color .15s, border-color .15s;
  }
  .hns-side-collapse:hover { color: var(--hns-text); border-color: color-mix(in srgb, var(--hns-accent) 40%, var(--hns-border)); }
  .hns-side.collapsed .hns-side-collapse { order: 0; width: 32px; height: 32px; }
  .hns-side.collapsed .hns-new {
    order: 1;
    width: 32px; height: 32px;
    padding: 0;
    justify-content: center;
  }
  .hns-session-dot {
    width: 8px; height: 8px; flex: none; border-radius: 50%;
    background: var(--hns-border);
    margin: 0 auto;
  }
  .hns-session-dot.active-dot { background: var(--hns-accent); box-shadow: 0 0 6px color-mix(in srgb, var(--hns-accent) 60%, transparent); }
  .hns-side-title {
    display: inline-flex; align-items: center; gap: 6px;
    flex: 1; min-width: 0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-size: 12.5px; font-weight: 700; letter-spacing: .02em; color: var(--hns-text);
  }
  .hns-new {
    display: inline-flex; align-items: center; gap: 4px;
    background: linear-gradient(135deg, var(--hns-accent), color-mix(in srgb, var(--hns-accent) 72%, #0ea5e9));
    color: #fff; border: none;
    border-radius: 8px; padding: 5px 11px; font-size: 12px; font-weight: 600; cursor: pointer;
    box-shadow: 0 6px 16px -8px color-mix(in srgb, var(--hns-accent) 70%, transparent);
    transition: transform .1s, box-shadow .15s;
  }
  .hns-new:hover { transform: translateY(-1px); box-shadow: 0 10px 20px -8px color-mix(in srgb, var(--hns-accent) 80%, transparent); }
  .hns-new:active { transform: translateY(0); }
  .hns-side-search { display: flex; align-items: center; gap: 5px; padding: 0 12px 10px; }
  .hns-side-search input {
    flex: 1; min-width: 0;
    background: var(--hns-surface); color: var(--hns-text);
    border: 1px solid var(--hns-border); border-radius: 8px;
    padding: 6px 9px; font-size: 12px; outline: none; transition: border-color .15s;
  }
  .hns-side-search input:focus { border-color: color-mix(in srgb, var(--hns-accent) 55%, var(--hns-border)); }
  .hns-side-list { flex: 1; overflow-y: auto; padding: 0 10px 12px; display: flex; flex-direction: column; gap: 2px; }
  .hns-side-empty { padding: 16px 10px; font-size: 12px; color: var(--hns-muted); text-align: center; }
  /* ─── 工作区分组（DSH WorkspaceBrowser 轻量版） ─── */
  .hns-ws-group { display: flex; flex-direction: column; gap: 2px; }
  .hns-ws-head {
    display: flex; align-items: center; gap: 5px;
    width: 100%;
    background: transparent; border: 0;
    color: var(--hns-muted); font-size: 11px; font-weight: 600;
    letter-spacing: .03em;
    padding: 5px 4px 3px;
    cursor: pointer;
    border-radius: 6px;
  }
  .hns-ws-head:hover { color: var(--hns-text); background: color-mix(in srgb, var(--hns-card-2) 45%, transparent); }
  .hns-ws-head > :global(svg) { color: var(--hns-accent); flex: none; }
  .hns-ws-name {
    flex: 1; min-width: 0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .hns-ws-count {
    flex: none; font-size: 10px;
    background: var(--hns-surface); border-radius: 999px; padding: 0 6px;
    font-variant-numeric: tabular-nums;
  }
  .hns-session {
    display: flex; align-items: center; gap: 6px; min-width: 0;
    padding: 8px 9px; border-radius: 9px; cursor: pointer;
    color: var(--hns-text); font-size: 12.5px;
    border: 1px solid transparent;
    transition: background .12s, border-color .12s;
  }
  .hns-session.collapsed {
    justify-content: center;
    padding: 8px 0;
  }
  .hns-side.collapsed .hns-side-list { padding: 0 8px 12px; }
  .hns-session:hover { background: color-mix(in srgb, var(--hns-card-2) 60%, transparent); border-color: var(--hns-border-light); }
  .hns-session.active {
    background: var(--hns-accent-soft);
    border-color: color-mix(in srgb, var(--hns-accent) 32%, transparent);
    box-shadow: inset 3px 0 0 var(--hns-accent);
  }
  .hns-session.dragover {
    outline: 2px dashed var(--hns-accent, #4176e6);
    outline-offset: -2px;
    background: color-mix(in srgb, var(--hns-accent, #4176e6) 8%, transparent);
  }
  .hns-session-title {
    flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .hns-session.active .hns-session-title { font-weight: 600; }
  .hns-session-count {
    flex: none; font-size: 10.5px; color: var(--hns-muted);
    background: var(--hns-surface); border-radius: 999px; padding: 0 7px;
    font-variant-numeric: tabular-nums;
  }
  .hns-session-acts { flex: none; display: none; gap: 2px; }
  .hns-session:hover .hns-session-acts, .hns-session.active .hns-session-acts { display: inline-flex; }
  .hns-session-act {
    display: inline-flex; align-items: center; justify-content: center;
    background: none; border: none; border-radius: 6px; padding: 3px;
    color: var(--hns-muted); cursor: pointer; transition: color .12s, background .12s;
  }
  .hns-session-act:hover { color: var(--hns-text); background: color-mix(in srgb, var(--hns-card-2) 70%, transparent); }
  .hns-session-edit {
    flex: 1; min-width: 0; font-size: 12px;
    background: var(--hns-surface); color: var(--hns-text);
    border: 1px solid var(--hns-accent); border-radius: 6px; padding: 2px 6px; outline: none;
  }
  .hns-search-results {
    display: flex; flex-direction: column; gap: 2px;
    padding: 0 12px 8px; max-height: 200px; overflow-y: auto;
  }
  .hns-search-hit {
    display: flex; align-items: center; gap: 6px; min-width: 0;
    background: none; border: none; text-align: left;
    font-size: 11.5px; color: var(--hns-text);
    padding: 5px 7px; border-radius: 7px; cursor: pointer;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    transition: background .12s;
  }
  .hns-search-hit:hover { background: color-mix(in srgb, var(--hns-card-2) 60%, transparent); }
  .hns-search-type {
    flex: none; font-size: 10px; color: var(--hns-muted);
    background: var(--hns-surface); border-radius: 5px; padding: 0 5px;
  }
  /* ─── 对话区 ─── */
  .hns-main { flex: 1; min-width: 0; display: flex; flex-direction: column; position: relative; }
  .hns-bar {
    display: flex; align-items: center; gap: 10px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--hns-border);
    background: color-mix(in srgb, var(--hns-card) 86%, transparent);
    backdrop-filter: blur(8px);
    min-height: 48px;
  }
  .hns-bar-title {
    display: inline-flex; align-items: center; gap: 8px;
    flex: none;
    max-width: 260px;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-size: 13.5px; font-weight: 700; color: var(--hns-text);
  }
  .hns-bar-title::before {
    content: ""; flex: none; width: 8px; height: 8px; border-radius: 50%;
    background: linear-gradient(135deg, var(--hns-accent), #0ea5e9);
    box-shadow: 0 0 0 3px var(--hns-accent-soft);
  }
  /* ─── 会话头面包屑（DSH 子代理谱系） ─── */
  .hns-crumbs {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    min-width: 0;
    flex: 1;
    overflow: hidden;
    white-space: nowrap;
  }
  .hns-crumb {
    border: 0;
    background: transparent;
    font-size: 12px;
    color: var(--hns-muted, #888);
    cursor: pointer;
    max-width: 130px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 2px 4px;
    border-radius: 5px;
  }
  .hns-crumb:hover { color: var(--hns-accent, #4176e6); background: color-mix(in srgb, var(--hns-accent, #4176e6) 8%, transparent); }
  .hns-crumb-sep { color: var(--hns-muted, #888); font-size: 11px; flex: none; }
  /* ─── 子代理目录（DSH SubagentCatalog） ─── */
  .hns-subagent-wrap { position: relative; flex: none; }
  .hns-subagent-pop {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 60;
    min-width: 260px;
    max-width: 340px;
    max-height: 360px;
    overflow: auto;
    background: var(--hns-card);
    border: 1px solid var(--hns-border);
    border-radius: 10px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, .18);
    padding: 6px;
  }
  .hns-subagent-head {
    font-size: 11px;
    font-weight: 700;
    color: var(--hns-muted, #888);
    padding: 4px 8px 6px;
    border-bottom: 1px solid var(--hns-border-light, rgba(128, 128, 128, .14));
    margin-bottom: 4px;
  }
  /* ─── 会话头右侧（重新分配：模型座 + 紧凑下拉 + 图标按钮组，不换行） ─── */
  .hns-bar-right {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
    min-width: 0;
    flex: none;
    flex-wrap: nowrap;
    margin-left: auto;
  }
  .hns-bar-right select.hns-bar-compact {
    background: var(--hns-surface);
    color: var(--hns-text);
    border: 1px solid var(--hns-border);
    border-radius: 8px;
    padding: 4px 8px;
    font-size: 11.5px;
    height: 30px;
    max-width: 130px;
    outline: none;
    cursor: pointer;
    transition: border-color .15s;
  }
  .hns-bar-right select.hns-bar-compact:focus {
    border-color: color-mix(in srgb, var(--hns-accent) 55%, var(--hns-border));
  }
  .hns-bar-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    flex: none;
    background: var(--hns-surface);
    border: 1px solid var(--hns-border);
    border-radius: 8px;
    color: var(--hns-muted);
    cursor: pointer;
    transition: color .12s, border-color .12s, background .12s;
  }
  .hns-bar-icon:hover {
    color: var(--hns-text);
    border-color: color-mix(in srgb, var(--hns-accent) 40%, var(--hns-border));
  }
  .hns-bar-icon.on {
    color: var(--hns-accent);
    background: var(--hns-accent-soft);
    border-color: color-mix(in srgb, var(--hns-accent) 40%, transparent);
  }
  .hns-no-provider { font-size: 12px; color: var(--hns-muted); }
  .hns-notice {
    font-size: 11.5px; color: var(--hns-accent);
    background: var(--hns-accent-soft); border-radius: 999px; padding: 3px 11px;
    border: 1px solid color-mix(in srgb, var(--hns-accent) 26%, transparent);
    flex: none;
  }
  /* ─── 工具目录（重设计：搜索 + 分组 + schema） ─── */
  .hns-tools-panel {
    margin: 0 16px; display: flex; flex-direction: column;
    background: color-mix(in srgb, var(--hns-card) 94%, transparent);
    border: 1px solid var(--hns-border); border-top: none;
    border-radius: 0 0 14px 14px;
    box-shadow: 0 18px 40px -22px rgba(0, 0, 0, .35);
    backdrop-filter: blur(10px);
    max-height: 46%; overflow: hidden;
  }
  .hns-tools-head {
    flex: none; display: flex; align-items: center; gap: 10px; flex-wrap: wrap;
    padding: 10px 12px 8px; border-bottom: 1px solid var(--hns-border-light);
  }
  .hns-tools-title { font-size: 12.5px; font-weight: 700; color: var(--hns-text); }
  .hns-tools-count {
    font-size: 11px; color: var(--hns-muted); font-variant-numeric: tabular-nums;
    background: var(--hns-surface); border-radius: 999px; padding: 1px 8px;
  }
  .hns-tools-search {
    margin-left: auto; display: inline-flex; align-items: center; gap: 5px;
    background: var(--hns-surface); border: 1px solid var(--hns-border);
    border-radius: 8px; padding: 4px 9px; min-width: 200px; color: var(--hns-muted);
  }
  .hns-tools-search input {
    flex: 1; min-width: 0; background: none; border: none; outline: none;
    font-size: 12px; color: var(--hns-text); font-family: inherit;
  }
  .hns-tools-scroll { flex: 1; min-height: 0; overflow-y: auto; padding: 8px 10px 12px; display: flex; flex-direction: column; gap: 10px; }
  .hns-tool-group { display: flex; flex-direction: column; gap: 3px; }
  .hns-tool-group-head {
    display: flex; align-items: center; gap: 6px;
    font-size: 10.5px; font-weight: 700; letter-spacing: .06em;
    color: var(--hns-muted); text-transform: uppercase;
    padding: 4px 2px 2px;
  }
  .hns-tool-group-head::after { content: ""; flex: 1; height: 1px; background: var(--hns-border-light); }
  .hns-tool-group-count {
    flex: none; font-size: 10px; color: var(--hns-muted);
    background: var(--hns-surface); border-radius: 999px; padding: 0 6px;
  }
  .hns-tool-item { display: flex; flex-direction: column; min-width: 0; }
  .hns-tool-main {
    display: flex; align-items: center; gap: 8px; min-width: 0; width: 100%;
    background: none; border: none; text-align: left; cursor: pointer;
    padding: 6px 8px; border-radius: 8px; font-size: 12px; color: var(--hns-text);
    transition: background .12s;
  }
  .hns-tool-main:hover { background: color-mix(in srgb, var(--hns-card-2) 55%, transparent); }
  .hns-tool-name { flex: none; font-weight: 700; color: var(--hns-text); font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 11.5px; }
  .hns-tool-lock {
    flex: none; font-size: 10px; color: var(--hns-amber);
    border: 1px solid color-mix(in srgb, var(--hns-amber) 45%, transparent); border-radius: 999px; padding: 0 6px;
    background: var(--hns-amber-soft);
  }
  .hns-tool-desc {
    flex: 1; min-width: 0; color: var(--hns-muted); font-size: 11.5px; line-height: 1.5;
    display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
    line-clamp: 2;
  }
  .hns-tool-chevron { flex: none; color: var(--hns-muted); font-size: 10px; }
  .hns-tool-schema {
    margin: 0 8px 6px 8px; max-height: 180px; overflow: auto;
  }
  .hns-tools-empty {
    font-size: 12px; color: var(--hns-muted); text-align: center; padding: 18px 0;
  }
  /* ─── 治理中心（重设计：全高右侧面板 + 图标分组 tab） ─── */
  .hns-drawer {
    position: absolute; inset: 0 0 0 auto; z-index: 30;
    width: min(470px, 92%); height: 100%;
    display: flex; flex-direction: column;
    background:
      radial-gradient(600px 300px at 100% 0%, color-mix(in srgb, var(--hns-accent) 6%, transparent), transparent 60%),
      color-mix(in srgb, var(--hns-card) 96%, transparent);
    border: none; border-left: 1px solid var(--hns-border);
    border-radius: 16px 0 0 16px;
    box-shadow: -24px 0 60px -24px rgba(0, 0, 0, 0.45);
    backdrop-filter: blur(12px);
    animation: hns-drawer-in .18s ease-out;
  }
  /* ─── 详情面板（DSH DetailsPanel 迁移：工具调用输入/输出右侧列） ─── */
  .hns-details {
    position: absolute; inset: 0 0 0 auto; z-index: 29;
    width: min(380px, 85%); height: 100%;
    display: flex; flex-direction: column;
    background:
      radial-gradient(600px 300px at 100% 0%, color-mix(in srgb, var(--hns-accent) 5%, transparent), transparent 60%),
      color-mix(in srgb, var(--hns-card) 96%, transparent);
    border-left: 1px solid var(--hns-border);
    border-radius: 16px 0 0 16px;
    box-shadow: -24px 0 60px -24px rgba(0, 0, 0, 0.4);
    backdrop-filter: blur(12px);
    animation: hns-drawer-in .18s ease-out;
  }
  .hns-details-head {
    flex: none; display: flex; align-items: center; gap: 8px;
    padding: 12px 14px 10px; border-bottom: 1px solid var(--hns-border-light);
  }
  .hns-details-title {
    display: inline-flex; align-items: center; gap: 7px;
    font-size: 13px; font-weight: 700; color: var(--hns-text);
    min-width: 0;
  }
  .hns-details-title > :global(svg) { color: var(--hns-accent); flex: none; }
  .hns-details-title > span:nth-child(2) {
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-family: ui-monospace, Consolas, monospace;
  }
  .hns-details-status {
    flex: none; font-size: 10px; font-weight: 600;
    border-radius: 4px; padding: 0 5px;
  }
  .hns-details-status.ok { color: #2ea043; background: rgba(46, 160, 67, .12); }
  .hns-details-status.err { color: #d73a49; background: rgba(215, 58, 73, .12); }
  .hns-details-status.running { color: var(--hns-accent, #4176e6); background: color-mix(in srgb, var(--hns-accent, #4176e6) 12%, transparent); }
  .hns-details-dur {
    flex: none; font-size: 10.5px; color: var(--hns-muted);
    font-variant-numeric: tabular-nums;
  }
  .hns-details-body {
    flex: 1; min-height: 0; overflow-y: auto;
    padding: 12px 14px 20px;
    display: flex; flex-direction: column; gap: 10px;
  }
  @keyframes hns-drawer-in { from { transform: translateX(18px); opacity: .4; } to { transform: none; opacity: 1; } }
  .hns-drawer-head {
    flex: none; display: flex; align-items: center; gap: 8px; flex-wrap: wrap;
    padding: 12px 14px 10px; border-bottom: 1px solid var(--hns-border-light);
  }
  .hns-drawer-title {
    display: inline-flex; align-items: center; gap: 7px;
    font-size: 13.5px; font-weight: 700; color: var(--hns-text);
  }
  .hns-drawer-title > :global(svg) { color: var(--hns-accent); }
  .hns-drawer-sub { font-size: 10.5px; color: var(--hns-muted); letter-spacing: .03em; }
  .hns-drawer-close {
    margin-left: auto; display: inline-flex; align-items: center; justify-content: center;
    background: var(--hns-surface); border: 1px solid var(--hns-border);
    color: var(--hns-muted); border-radius: 8px; width: 26px; height: 26px; cursor: pointer;
    transition: color .12s, border-color .12s;
  }
  .hns-drawer-close:hover { color: var(--hns-text); border-color: color-mix(in srgb, var(--hns-accent) 40%, var(--hns-border)); }
  .hns-drawer-tabs {
    flex: none; display: grid; grid-template-columns: repeat(6, 1fr); gap: 4px;
    padding: 10px 12px 8px;
    border-bottom: 1px solid var(--hns-border-light);
  }
  .hns-drawer-group {
    grid-column: 1 / -1; display: flex; align-items: center; gap: 6px;
    font-size: 10px; font-weight: 700; letter-spacing: .08em; text-transform: uppercase;
    color: var(--hns-muted); margin-top: 2px;
  }
  .hns-drawer-group:first-child { margin-top: 0; }
  .hns-drawer-group::after { content: ""; flex: 1; height: 1px; background: var(--hns-border-light); }
  .hns-drawer-tabs button {
    display: inline-flex; flex-direction: column; align-items: center; justify-content: center; gap: 3px;
    background: none; border: 1px solid transparent; border-radius: 9px;
    padding: 6px 2px; font-size: 10.5px; color: var(--hns-muted); cursor: pointer;
    transition: color .12s, background .12s, border-color .12s;
  }
  .hns-drawer-tabs button > :global(svg) { width: 14px; height: 14px; }
  .hns-drawer-tabs button:hover {
    color: var(--hns-text);
    background: color-mix(in srgb, var(--hns-card-2) 55%, transparent);
  }
  .hns-drawer-tabs button.on {
    color: var(--hns-accent);
    background: var(--hns-accent-soft);
    border-color: color-mix(in srgb, var(--hns-accent) 30%, transparent);
    font-weight: 600;
  }
  .hns-drawer-body {
    flex: 1; min-height: 0; overflow-y: auto;
    padding: 12px 14px 18px; display: flex; flex-direction: column; gap: 10px;
  }
  .hns-drawer-hint {
    font-size: 11.5px; color: var(--hns-muted); line-height: 1.65;
    background: var(--hns-surface); border: 1px solid var(--hns-border-light);
    border-radius: 9px; padding: 8px 10px;
  }
  .hns-field {
    display: flex; flex-direction: column; gap: 5px;
    background: var(--hns-surface); border: 1px solid var(--hns-border-light);
    border-radius: 10px; padding: 9px 11px;
    transition: border-color .15s;
  }
  .hns-field:focus-within { border-color: color-mix(in srgb, var(--hns-accent) 40%, var(--hns-border)); }
  .hns-field-label { font-size: 11px; font-weight: 600; color: var(--hns-muted); letter-spacing: .02em; }
  .hns-field input, .hns-field select, .hns-field textarea {
    background: var(--hns-card); color: var(--hns-text);
    border: 1px solid var(--hns-border); border-radius: 8px;
    padding: 6px 10px; font-size: 12.5px; font-family: inherit; outline: none;
    transition: border-color .15s;
  }
  .hns-field input:focus, .hns-field select:focus, .hns-field textarea:focus {
    border-color: color-mix(in srgb, var(--hns-accent) 55%, var(--hns-border));
  }
  .hns-field-actions {
    display: flex; align-items: center; gap: 8px; flex-wrap: wrap; margin-top: 2px;
  }
  .hns-msg-note { font-size: 11.5px; color: var(--hns-accent); flex: 1; }
  .hns-primary {
    background: linear-gradient(135deg, var(--hns-accent), color-mix(in srgb, var(--hns-accent) 72%, #0ea5e9));
    color: #fff; border: none;
    border-radius: 8px; padding: 5px 13px; font-size: 12px; font-weight: 600; cursor: pointer;
    box-shadow: 0 6px 14px -8px color-mix(in srgb, var(--hns-accent) 60%, transparent);
    transition: transform .1s, box-shadow .15s;
  }
  .hns-primary:hover { transform: translateY(-1px); }
  .hns-plain {
    background: var(--hns-surface); color: var(--hns-text);
    border: 1px solid var(--hns-border); border-radius: 8px;
    padding: 5px 12px; font-size: 12px; cursor: pointer; transition: border-color .12s, color .12s;
  }
  .hns-plain:hover { border-color: color-mix(in srgb, var(--hns-accent) 40%, var(--hns-border)); color: var(--hns-accent); }
  .hns-hook-row { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .hns-hook-row select {
    flex: none; width: 118px;
    background: var(--hns-card); color: var(--hns-text);
    border: 1px solid var(--hns-border); border-radius: 8px; padding: 5px 8px; font-size: 12px;
  }
  .hns-hook-row input {
    flex: 1; min-width: 0;
    background: var(--hns-card); color: var(--hns-text);
    border: 1px solid var(--hns-border); border-radius: 8px; padding: 5px 9px; font-size: 12px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  }
  .hns-hook-enable {
    flex: none; display: inline-flex; align-items: center; gap: 3px;
    font-size: 11.5px; color: var(--hns-muted);
  }
  .hns-hook-log-head { font-size: 11px; font-weight: 700; color: var(--hns-muted); letter-spacing: .04em; }
  .hns-hook-log {
    font-size: 11px; color: var(--hns-muted);
    background: var(--hns-surface); border: 1px solid var(--hns-border-light); border-radius: 8px; padding: 6px 9px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    white-space: pre-wrap; word-break: break-all;
  }
  .hns-hook-log.err { color: var(--hns-red); }
  .hns-preset-form { display: flex; flex-direction: column; gap: 10px; }
  .hns-preset-tools { display: flex; flex-wrap: wrap; gap: 5px 10px; max-height: 130px; overflow-y: auto; }
  .hns-preset-tool {
    display: inline-flex; align-items: center; gap: 4px; font-size: 11.5px;
    color: var(--hns-text); cursor: pointer;
    background: var(--hns-surface); border: 1px solid var(--hns-border-light);
    border-radius: 999px; padding: 2px 9px; transition: border-color .12s;
  }
  .hns-preset-tool:hover { border-color: color-mix(in srgb, var(--hns-accent) 40%, var(--hns-border)); }
  .hns-preset-item {
    display: flex; align-items: center; gap: 8px; min-width: 0;
    background: var(--hns-surface); border: 1px solid var(--hns-border-light);
    border-radius: 10px; padding: 8px 11px;
    transition: border-color .12s, transform .1s;
  }
  .hns-preset-item:hover { border-color: color-mix(in srgb, var(--hns-accent) 32%, var(--hns-border)); transform: translateY(-1px); }
  .hns-preset-main { flex: 1; min-width: 0; display: flex; align-items: center; gap: 8px; }
  .hns-preset-name { font-weight: 600; font-size: 12.5px; color: var(--hns-text); }
  .hns-preset-meta {
    font-size: 10.5px; color: var(--hns-muted);
    background: var(--hns-card); border-radius: 999px; padding: 0 7px;
    border: 1px solid var(--hns-border-light);
  }
  /* ─── 会话遥测统计条（DSH 统计条等价） ─── */
  .hns-stats {
    flex: none; display: flex; align-items: center; flex-wrap: wrap; gap: 6px 10px;
    padding: 6px 14px; font-size: 11.5px; font-variant-numeric: tabular-nums;
    color: var(--hns-muted);
    border-bottom: 1px solid var(--hns-border-light);
    background: color-mix(in srgb, var(--hns-card) 50%, transparent);
  }
  .hns-stats > span:not(.hns-stats-sep) {
    display: inline-flex; align-items: center; gap: 4px;
  }
  .hns-stats-sep { color: var(--hns-border); }
  /* ─── 目标 / 计划 / 待办横幅 ─── */
  .hns-goal, .hns-plan, .hns-todos {
    margin: 10px 16px 0; max-width: 760px; align-self: center; width: calc(100% - 32px);
  }
  .hns-goal {
    display: flex; align-items: center; gap: 8px; flex-wrap: wrap;
    font-size: 12px; color: var(--hns-accent);
    background: var(--hns-accent-soft);
    border: 1px solid color-mix(in srgb, var(--hns-accent) 32%, transparent);
    border-left: 3px solid var(--hns-accent);
    border-radius: 10px; padding: 7px 12px;
  }
  .hns-goal-status { font-size: 11px; color: var(--hns-muted); }
  .hns-goal-edit {
    flex: 1; min-width: 0;
    font-size: 12px;
    border: 1px solid var(--hns-accent, #4176e6);
    border-radius: 6px;
    padding: 3px 8px;
    background: transparent;
    color: var(--hns-text, inherit);
  }
  .hns-goal-actions { margin-left: auto; display: inline-flex; gap: 6px; }
  .hns-goal-act {
    font-size: 10.5px;
    color: var(--hns-text);
    background: var(--hns-surface);
    border: 1px solid var(--hns-border);
    border-radius: 6px;
    padding: 1px 7px;
    cursor: pointer;
  }
  .hns-goal-act:hover { border-color: var(--hns-accent); color: var(--hns-accent); }
  .hns-goal-act.danger:hover { border-color: #d73a49; color: #d73a49; }
  .hns-plan {
    font-size: 12px; color: var(--hns-amber);
    background: var(--hns-amber-soft);
    border: 1px solid color-mix(in srgb, var(--hns-amber) 40%, transparent);
    border-left: 3px solid var(--hns-amber);
    border-radius: 10px; padding: 7px 12px;
  }
  .hns-todos {
    display: flex; flex-direction: column; gap: 4px;
    background: color-mix(in srgb, var(--hns-card) 92%, transparent);
    border: 1px solid var(--hns-border); border-radius: 11px; padding: 8px 12px;
  }
  .hns-todos-head { font-size: 11px; font-weight: 700; color: var(--hns-muted); letter-spacing: .04em; }
  .hns-todo { display: flex; align-items: center; gap: 7px; font-size: 12px; color: var(--hns-text); }
  .hns-todo-status { flex: none; width: 14px; color: var(--hns-muted); }
  .hns-todo.done .hns-todo-text { text-decoration: line-through; color: var(--hns-muted); }
  .hns-todo.doing .hns-todo-status { color: var(--hns-accent); }
  .hns-todo.done .hns-todo-status { color: var(--hns-green); }
  /* ─── 终端 ─── */
  .hns-terminal {
    display: flex; flex-direction: column; gap: 6px;
    background: var(--hns-surface); border: 1px solid var(--hns-border-light);
    border-radius: 11px; padding: 9px 11px;
  }
  .hns-terminal-head { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .hns-terminal-cwd {
    flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-size: 10.5px; color: var(--hns-muted); font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  }
  .hns-terminal-out {
    max-height: 200px; overflow-y: auto; display: flex; flex-direction: column; gap: 4px;
    background: var(--hns-card); border: 1px solid var(--hns-border);
    border-radius: 8px; padding: 7px 9px;
  }
  .hns-terminal-line { font-size: 11.5px; color: var(--hns-text); }
  .hns-terminal-in { color: var(--hns-accent); font-weight: 600; }
  .hns-terminal-pre {
    margin: 2px 0 0; font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 11px; white-space: pre-wrap; word-break: break-all; color: var(--hns-text);
  }
  .hns-terminal-input { display: flex; align-items: center; gap: 6px; }
  .hns-terminal-input input {
    flex: 1; min-width: 0;
    background: var(--hns-card); color: var(--hns-text);
    border: 1px solid var(--hns-border); border-radius: 8px; padding: 5px 9px; font-size: 12px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  }
  /* ─── 消息流 ─── */
  .hns-msgs {
    flex: 1; min-height: 0; overflow-y: auto;
    padding: 18px 24px 10px; display: flex; flex-direction: column; gap: 16px;
    position: relative;
  }
  .hns-msgs.empty { justify-content: center; }
  .hns-load-earlier {
    display: flex; justify-content: center;
    margin: -6px 0 2px;
  }
  .hns-load-earlier button {
    font-size: 11px; color: var(--hns-muted, #888);
    background: transparent;
    border: 1px solid var(--hns-border, rgba(128,128,128,.25));
    border-radius: 999px;
    padding: 4px 14px;
    cursor: pointer;
  }
  .hns-load-earlier button:hover { color: var(--hns-accent, #4176e6); border-color: color-mix(in srgb, var(--hns-accent, #4176e6) 45%, transparent); }
  .hns-scroll-bottom {
    position: sticky; bottom: 8px; align-self: flex-end;
    width: 30px; height: 30px; border-radius: 50%;
    display: flex; align-items: center; justify-content: center;
    font-size: 14px; color: var(--hns-muted, #888);
    background: color-mix(in srgb, var(--hns-card, #fff) 90%, transparent);
    border: 1px solid var(--hns-border, rgba(128,128,128,.25));
    box-shadow: 0 2px 8px rgba(0, 0, 0, .12);
    cursor: pointer;
    z-index: 5;
  }
  .hns-scroll-bottom:hover { color: var(--hns-accent, #4176e6); }
  /* ─── 视图切换条（对话 | 轨迹；内容区顶部，与消息流左缘对齐，
       宽度随内容自适应，避免按钮前后大块空白） ─── */
  .hns-view-switch {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    align-self: flex-start;
    margin: 4px 0 2px 24px;
    padding: 6px 6px 2px;
    flex: none;
  }
  .hns-view-switch button {
    border: 0;
    background: transparent;
    color: var(--hns-muted);
    font-size: 12px;
    font-weight: 500;
    padding: 4px 14px;
    border-radius: 999px;
    cursor: pointer;
    transition: color .12s, background .12s;
  }
  .hns-view-switch button:hover { color: var(--hns-text); background: rgba(128, 128, 128, .08); }
  .hns-view-switch button.on {
    background: var(--hns-surface, #fff);
    color: var(--hns-accent);
    box-shadow: 0 1px 3px rgba(0, 0, 0, .14);
    font-weight: 600;
  }
  /* ─── 轨迹视图容器 ─── */
  .hns-trajectory-wrap {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .hns-traj-loading {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--hns-muted);
    font-size: 12.5px;
  }
  .hns-traj-loading.hns-traj-error {
    color: var(--hns-red);
    padding: 0 24px;
    text-align: center;
    word-break: break-all;
  }
  /* ─── 产物文件行（DSH ProducedFiles 迁移：回合尾 chips） ─── */
  .hns-turn-files {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .hns-turn-files-label {
    font-size: 10.5px;
    font-weight: 600;
    color: var(--hns-muted);
    letter-spacing: .05em;
  }
  .hns-file-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11.5px;
    color: var(--hns-text);
    background: rgba(128, 128, 128, .08);
    border: 1px solid var(--hns-border-light, rgba(128, 128, 128, .25));
    border-radius: 999px;
    padding: 2px 10px;
    cursor: pointer;
    max-width: 240px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hns-file-chip:hover { border-color: var(--hns-accent, #4176e6); color: var(--hns-accent, #4176e6); }
  .hns-hero { text-align: center; color: var(--hns-muted); padding: 0 40px; }
  .hns-hero-logo {
    font-size: 40px;
    line-height: 1;
    margin-bottom: 10px;
    filter: drop-shadow(0 4px 14px color-mix(in srgb, var(--hns-accent) 35%, transparent));
  }
  .hns-hero h2 { font-size: 19px; font-weight: 800; color: var(--hns-text); margin-bottom: 8px; letter-spacing: .01em; }
  .hns-hero-badge {
    display: inline-block;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: .08em;
    color: var(--hns-accent);
    border: 1px solid color-mix(in srgb, var(--hns-accent) 45%, transparent);
    border-radius: 999px;
    padding: 1px 9px;
    margin-bottom: 10px;
  }
  .hns-hero p { font-size: 12.5px; line-height: 1.75; }
  /* ─── 空态座位（DSH WorkspacePicker + AgentPresetSeat：chip 选择） ─── */
  .hns-hero-seats { display: flex; gap: 8px; margin-top: 14px; flex-wrap: wrap; justify-content: center; }
  .hns-hero-chip {
    display: inline-flex; align-items: center; gap: 6px;
    font-size: 12px;
    border: 1px solid var(--hns-border, rgba(128, 128, 128, .3));
    border-radius: 999px;
    padding: 5px 14px;
    background: transparent;
    color: var(--hns-muted, #888);
    cursor: pointer;
    transition: color .12s, border-color .12s;
  }
  .hns-hero-chip:hover { color: var(--hns-accent, #4176e6); border-color: color-mix(in srgb, var(--hns-accent, #4176e6) 45%, transparent); }
  /* ─── 回合失败节点（DSH TurnErrorItem 迁移） ─── */
  .hns-turn-error {
    display: flex; align-items: center; gap: 8px;
    align-self: center; max-width: 640px; width: 100%;
    font-size: 12px; color: var(--hns-red);
    background: color-mix(in srgb, #dc2626 8%, transparent);
    border: 1px solid color-mix(in srgb, #dc2626 30%, transparent);
    border-left: 3px solid #dc2626;
    border-radius: 10px; padding: 8px 12px;
  }
  .hns-turn-error-ico { flex: none; font-size: 13px; }
  .hns-turn-error-text { flex: 1; min-width: 0; word-break: break-all; }
  .hns-turn-retry {
    flex: none;
    font-size: 11px; font-weight: 600;
    color: var(--hns-accent, #4176e6);
    background: transparent;
    border: 1px solid color-mix(in srgb, var(--hns-accent, #4176e6) 40%, transparent);
    border-radius: 6px;
    padding: 3px 10px;
    cursor: pointer;
  }
  .hns-turn-retry:hover:not(:disabled) { background: color-mix(in srgb, var(--hns-accent, #4176e6) 12%, transparent); }
  .hns-turn-retry:disabled { opacity: .5; cursor: default; }
  .hns-msg { display: flex; position: relative; }
  .hns-msg-user { justify-content: flex-end; }
  .hns-cmd-bubble {
    max-width: 100%;
    font-family: ui-monospace, Consolas, monospace;
    font-size: 12px;
    color: var(--hns-accent, #4176e6);
    background: color-mix(in srgb, var(--hns-accent, #4176e6) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--hns-accent, #4176e6) 30%, transparent);
    border-radius: 14px 4px 14px 14px;
    padding: 8px 12px;
    word-break: break-all;
  }
  .hns-msg-bot { justify-content: flex-start; }
  .hns-bot-col { display: flex; flex-direction: column; gap: 7px; max-width: min(720px, 88%); min-width: 0; }
  .hns-bubble {
    max-width: 100%;
    background: color-mix(in srgb, var(--hns-card) 95%, transparent);
    border: 1px solid var(--hns-border);
    border-radius: 4px 14px 14px 14px; padding: 10px 14px;
    font-size: 13px; line-height: 1.65; color: var(--hns-text);
    box-shadow: 0 10px 26px -18px rgba(0, 0, 0, .3);
  }
  .hns-msg-user .hns-bubble {
    background: linear-gradient(135deg, color-mix(in srgb, var(--hns-accent) 13%, var(--hns-card)), color-mix(in srgb, var(--hns-accent) 8%, var(--hns-card)));
    border-color: color-mix(in srgb, var(--hns-accent) 26%, var(--hns-border));
    border-radius: 14px 4px 14px 14px;
  }
  .hns-stream-hint { align-self: flex-start; font-size: 11px; color: var(--hns-muted); }
  /* ─── 输入区芯片（DSH PlanChip / PermissionSelect 迁移） ─── */
  .hns-plan-chip {
    flex: none;
    display: inline-flex; align-items: center;
    font-size: 10.5px; font-weight: 700; letter-spacing: .04em;
    color: #fff;
    background: linear-gradient(135deg, #b08800, #d4a72c);
    border: 0; border-radius: 999px;
    padding: 3px 9px;
    cursor: pointer;
  }
  .hns-plan-chip:hover { filter: brightness(1.08); }
  .hns-perm-chip {
    flex: none;
    font-size: 11px;
    color: var(--hns-text);
    background: var(--hns-surface);
    border: 1px solid var(--hns-border);
    border-radius: 8px;
    padding: 4px 6px;
    cursor: pointer;
    max-width: 118px;
  }
  .hns-perm-chip:hover { border-color: color-mix(in srgb, var(--hns-accent) 40%, var(--hns-border)); }
  /* ─── 斜杠命令菜单（DSH ui-input-trigger 迁移） ─── */
  .hns-slash-menu {
    position: absolute;
    left: 12px;
    bottom: calc(100% + 8px);
    width: min(360px, 90%);
    max-height: 300px;
    overflow-y: auto;
    background: var(--hns-surface);
    border: 1px solid var(--hns-border);
    border-radius: 10px;
    box-shadow: 0 14px 36px -14px rgba(0, 0, 0, .5);
    padding: 6px;
    z-index: 25;
  }
  .hns-slash-head {
    font-size: 10.5px; font-weight: 700; letter-spacing: .05em;
    color: var(--hns-muted);
    padding: 4px 8px 6px;
  }
  .hns-slash-menu button {
    display: flex; align-items: center; gap: 8px;
    width: 100%;
    background: transparent; border: 0;
    border-radius: 7px;
    padding: 7px 8px;
    cursor: pointer;
    text-align: left;
  }
  .hns-slash-menu button:hover,
  .hns-slash-menu button.on { background: color-mix(in srgb, var(--hns-accent) 12%, transparent); }
  .hns-slash-name {
    flex: none;
    font-family: ui-monospace, Consolas, monospace;
    font-size: 12px; font-weight: 700;
    color: var(--hns-accent);
  }
  .hns-slash-desc {
    flex: 1; min-width: 0;
    font-size: 11px; color: var(--hns-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  /* ─── 排队坞（DSH QueueDock 迁移） ─── */
  .hns-queue {
    align-self: center;
    width: calc(100% - 32px); max-width: 760px;
    background: color-mix(in srgb, var(--hns-card) 96%, transparent);
    border: 1px solid var(--hns-border);
    border-radius: 12px;
    margin: 0 16px 2px;
    padding: 8px 12px;
    display: flex; flex-direction: column; gap: 4px;
  }
  .hns-queue-head {
    display: flex; align-items: center; gap: 6px;
    font-size: 11px; font-weight: 700; color: var(--hns-text);
  }
  .hns-queue-head > :global(svg) { color: var(--hns-accent); }
  .hns-queue-hint { margin-left: auto; font-size: 10px; font-weight: 400; color: var(--hns-muted); }
  .hns-queue-row {
    display: flex; align-items: center; gap: 8px;
    font-size: 11.5px; color: var(--hns-text);
    padding: 3px 4px;
    border-radius: 6px;
  }
  .hns-queue-row:hover { background: color-mix(in srgb, var(--hns-card-2) 50%, transparent); }
  .hns-queue-text {
    flex: 1; min-width: 0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  /* ─── 图片灯箱（DSH ImageLightbox 迁移） ─── */
  .hns-lightbox {
    position: fixed; inset: 0; z-index: 100;
    background: rgba(0, 0, 0, .82);
    display: flex; align-items: center; justify-content: center;
    flex-direction: column; gap: 10px;
    cursor: zoom-out;
  }
  .hns-lightbox img {
    max-width: 88vw; max-height: 82vh;
    border-radius: 10px;
    box-shadow: 0 24px 80px -20px rgba(0, 0, 0, .8);
    object-fit: contain;
  }
  .hns-lightbox-name { color: #ddd; font-size: 12px; }
  .hns-lightbox-close {
    position: absolute; top: 16px; right: 20px;
    width: 34px; height: 34px;
    background: rgba(255, 255, 255, .12);
    border: 1px solid rgba(255, 255, 255, .25);
    color: #fff; border-radius: 50%;
    font-size: 14px; cursor: pointer;
  }
  .hns-lightbox-close:hover { background: rgba(255, 255, 255, .22); }
  .hns-attachment-img {
    font-size: 11.5px; color: var(--hns-text);
    background: rgba(128, 128, 128, .08);
    border: 1px solid var(--hns-border-light, rgba(128, 128, 128, .25));
    border-radius: 999px;
    padding: 2px 10px;
    cursor: zoom-in;
  }
  .hns-attachment-img:hover { border-color: var(--hns-accent, #4176e6); color: var(--hns-accent, #4176e6); }
  /* ─── 上下文环形仪表（DSH ContextMeter 迁移） ─── */
  .hns-meter-wrap { position: relative; flex: none; display: inline-flex; align-items: center; }
  .hns-meter {
    display: inline-flex; align-items: center; justify-content: center;
    background: transparent; border: 0;
    padding: 0; cursor: pointer; border-radius: 50%;
  }
  .hns-meter:hover { filter: brightness(1.1); }
  .hns-meter-panel {
    position: absolute; right: 0; bottom: calc(100% + 8px);
    width: 220px;
    background: var(--hns-surface);
    border: 1px solid var(--hns-border);
    border-radius: 10px;
    box-shadow: 0 12px 32px -12px rgba(0, 0, 0, .45);
    padding: 10px 12px;
    z-index: 20;
  }
  .hns-meter-head {
    display: flex; align-items: baseline; justify-content: space-between;
    font-size: 12px; font-weight: 700; color: var(--hns-text);
    margin-bottom: 8px;
  }
  .hns-meter-sub { font-size: 10.5px; color: var(--hns-muted); font-weight: 400; font-variant-numeric: tabular-nums; }
  .hns-meter-rows { display: flex; flex-direction: column; gap: 5px; }
  .hns-meter-row {
    display: flex; align-items: center; gap: 6px;
    font-size: 11px; color: var(--hns-muted);
  }
  .hns-meter-row b { margin-left: auto; color: var(--hns-text); font-variant-numeric: tabular-nums; }
  .hns-meter-dot { width: 8px; height: 8px; border-radius: 50%; flex: none; }
  .hns-meter-dot.sys { background: var(--hns-accent); }
  .hns-meter-dot.tools { background: #b08800; }
  .hns-meter-dot.msgs { background: #2ea043; }
  /* ─── 推理 Think 行（DSH ReasoningRow 迁移） ─── */
  .hns-think {
    border: 1px solid var(--hns-border-light);
    border-radius: 8px;
    background: color-mix(in srgb, var(--hns-card-2) 40%, transparent);
    overflow: hidden;
    max-width: 720px;
  }
  .hns-think-head {
    display: flex; align-items: center; gap: 6px;
    width: 100%;
    background: transparent; border: 0;
    color: var(--hns-muted); font-size: 11px;
    padding: 5px 10px;
    cursor: pointer;
  }
  .hns-think-head:hover { color: var(--hns-text); }
  .hns-think-icon { font-size: 12px; }
  .hns-think-label { font-weight: 700; letter-spacing: .02em; }
  .hns-think-running { font-size: 10px; color: var(--hns-accent); }
  .hns-think-chevron { margin-left: auto; }
  .hns-think-body {
    padding: 6px 10px 8px 28px;
    font-size: 11.5px; line-height: 1.7;
    color: var(--hns-muted);
    white-space: pre-wrap; word-break: break-all;
    max-height: 280px; overflow: auto;
    border-top: 1px solid var(--hns-border-light);
  }
  .hns-caret {
    display: inline-block; width: 7px; height: 15px; margin-left: 2px; vertical-align: -2px;
    background: var(--hns-accent); border-radius: 1px;
    animation: hns-caret-blink 0.9s steps(2) infinite;
  }
  @keyframes hns-caret-blink { 0%, 100% { opacity: 1; } 50% { opacity: 0; } }
  /* ─── 工具执行时间线（消息内：先工具 → 后回复） ─── */
  .hns-tool-timeline {
    position: relative; display: flex; flex-direction: column; gap: 2px;
    padding: 8px 10px 8px 32px; min-width: 0;
    background: color-mix(in srgb, var(--hns-surface) 80%, transparent);
    border: 1px solid var(--hns-border-light);
    border-radius: 12px;
  }
  /* 时间线竖线：从首个节点连接到回复气泡 */
  .hns-tool-timeline::before {
    content: ""; position: absolute; left: 15px; top: 22px; bottom: -14px;
    width: 2px; border-radius: 2px;
    background: linear-gradient(var(--hns-border), color-mix(in srgb, var(--hns-accent) 40%, var(--hns-border)));
  }
  .hns-tool-timeline .hns-tool-step {
    position: relative; display: flex; flex-direction: column; min-width: 0;
    font-size: 11.5px; color: var(--hns-text);
    border-radius: 8px;
  }
  .hns-tool-timeline .hns-tool-step.open {
    background: color-mix(in srgb, var(--hns-card-2) 60%, transparent);
  }
  /* 步骤节点（状态圆点：完成绿 / 失败红 / 执行中脉冲） */
  .hns-tool-node {
    position: absolute; left: -32px; top: 7px; z-index: 1;
    width: 22px; height: 22px; border-radius: 50%;
    display: inline-flex; align-items: center; justify-content: center;
    background: var(--hns-card);
    border: 1px solid var(--hns-border);
    color: var(--hns-muted);
    box-shadow: 0 2px 6px -2px rgba(0, 0, 0, .3);
  }
  .hns-tool-step.ok .hns-tool-node {
    color: var(--hns-green);
    border-color: color-mix(in srgb, var(--hns-green) 50%, transparent);
    background: color-mix(in srgb, var(--hns-green) 10%, var(--hns-card));
  }
  .hns-tool-step.err .hns-tool-node {
    color: var(--hns-red);
    border-color: color-mix(in srgb, var(--hns-red) 50%, transparent);
    background: color-mix(in srgb, var(--hns-red) 10%, var(--hns-card));
  }
  .hns-tool-node-dot {
    width: 8px; height: 8px; border-radius: 50%;
    background: var(--hns-accent);
    animation: hns-caret-blink 1s steps(2) infinite;
  }
  .hns-tool-timeline .hns-tool-head {
    display: flex; align-items: center; gap: 6px; min-width: 0; width: 100%;
    background: none; border: none; padding: 4px 6px; border-radius: 7px;
    font-size: 11.5px; color: var(--hns-text); cursor: pointer; text-align: left;
  }
  .hns-tool-timeline .hns-tool-head:hover {
    background: color-mix(in srgb, var(--hns-card-2) 70%, transparent);
  }
  .hns-tool-timeline .hns-tool-chevron { margin-left: auto; }
  .hns-tool-timeline .hns-tool-name { flex: none; font-weight: 700; color: var(--hns-text); font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 11.5px; }
  .hns-tool-args {
    flex: none; color: var(--hns-muted); font-size: 10.5px;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 40%;
  }
  .hns-tool-status {
    flex: none; display: inline-flex; align-items: center; gap: 3px;
    color: var(--hns-muted); font-size: 11px;
  }
  .hns-tool-step.ok .hns-tool-status { color: var(--hns-green); }
  .hns-tool-step.err .hns-tool-status { color: var(--hns-red); }
  .hns-tool-running { color: var(--hns-accent); animation: hns-caret-blink 1s steps(2) infinite; }
  .hns-tool-dur { flex: none; color: var(--hns-muted); font-size: 10.5px; font-variant-numeric: tabular-nums; }
  .hns-tool-timeline .hns-tool-detail { display: flex; flex-direction: column; gap: 6px; padding: 2px 6px 6px; min-width: 0; }
  .hns-tool-detail-actions {
    display: flex; align-items: center; justify-content: flex-end;
    padding: 2px 2px 0;
  }
  .hns-tool-field { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .hns-tool-field-head {
    display: flex; align-items: center; justify-content: space-between; gap: 8px;
    font-size: 10.5px; color: var(--hns-muted);
  }
  .hns-tool-metrics {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
    font-size: 11px;
    color: var(--hns-muted, #888);
    font-variant-numeric: tabular-nums;
  }
  .hns-tool-pre {
    margin: 0; padding: 7px 9px; min-width: 0; max-height: 200px; overflow: auto;
    background: color-mix(in srgb, var(--hns-card) 88%, transparent);
    border: 1px solid var(--hns-border); border-radius: 8px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 10.5px; line-height: 1.55; color: var(--hns-text);
    white-space: pre-wrap; word-break: break-all;
  }
  .hns-tool-copy {
    flex: none; display: inline-flex; align-items: center; gap: 3px;
    background: none; border: none; padding: 1px 6px; border-radius: 5px;
    font-size: 10.5px; color: var(--hns-muted); cursor: pointer;
  }
  .hns-tool-copy:hover { color: var(--hns-text); background: var(--hns-surface); }
  /* ─── 审批卡 / 问题卡 ─── */
  .hns-approvals {
    display: flex; flex-direction: column; gap: 7px;
    margin: 10px 16px 0; max-width: 760px; align-self: center; width: calc(100% - 32px);
  }
  .hns-approval {
    display: flex; flex-direction: column; gap: 5px;
    font-size: 12px; color: var(--hns-text);
    background: var(--hns-amber-soft);
    border: 1px solid color-mix(in srgb, var(--hns-amber) 42%, transparent);
    border-left: 3px solid var(--hns-amber);
    border-radius: 10px; padding: 8px 11px;
  }
  .hns-approval-head { display: flex; align-items: center; gap: 8px; min-width: 0; flex-wrap: wrap; }
  .hns-approval-head > :global(svg) { flex: none; color: var(--hns-amber); }
  .hns-approval-text { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 600; }
  .hns-approval-args {
    flex: none; max-width: 45%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    background: color-mix(in srgb, var(--hns-card) 85%, transparent);
    border: 1px solid var(--hns-border); border-radius: 6px; padding: 1px 7px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 10.5px; color: var(--hns-muted);
  }
  .hns-approval-actions { flex: none; display: inline-flex; align-items: center; gap: 6px; margin-left: auto; }
  .hns-approve, .hns-reject {
    flex: none; border-radius: 7px; border: none; padding: 4px 11px;
    font-size: 12px; font-weight: 600; cursor: pointer; transition: transform .1s, opacity .12s;
  }
  .hns-approve { background: linear-gradient(135deg, var(--hns-accent), color-mix(in srgb, var(--hns-accent) 72%, #0ea5e9)); color: #fff; }
  .hns-approve:hover { opacity: .92; transform: translateY(-1px); }
  .hns-reject { background: transparent; color: var(--hns-red); border: 1px solid color-mix(in srgb, var(--hns-red) 45%, transparent); }
  .hns-reject:hover { background: color-mix(in srgb, var(--hns-red) 8%, transparent); }
  .hns-question-input {
    font-size: 12px;
    border: 1px solid var(--hns-border, rgba(128, 128, 128, .3));
    border-radius: 6px;
    padding: 4px 8px;
    background: transparent;
    color: var(--hns-text, inherit);
    min-width: 180px;
  }
  /* ─── 提问卡多选 / 翻页（DSH QuestionFlow） ─── */
  .hns-q-checks {
    display: inline-flex;
    flex-direction: column;
    gap: 3px;
    align-items: flex-start;
  }
  .hns-q-check {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    color: var(--hns-text, inherit);
    cursor: pointer;
  }
  .hns-q-check input { accent-color: var(--hns-accent, #4176e6); }
  .hns-q-nav {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 6px;
  }
  .hns-q-nav-btn {
    font-size: 11.5px;
    border: 1px solid var(--hns-border, rgba(128, 128, 128, .3));
    border-radius: 6px;
    padding: 3px 10px;
    background: transparent;
    color: var(--hns-muted, #888);
    cursor: pointer;
  }
  .hns-q-nav-btn:hover:not(:disabled) { color: var(--hns-accent, #4176e6); border-color: color-mix(in srgb, var(--hns-accent, #4176e6) 45%, transparent); }
  .hns-q-nav-btn:disabled { opacity: .45; cursor: default; }
  .hns-q-progress { font-size: 11px; color: var(--hns-muted, #888); font-variant-numeric: tabular-nums; }
  /* ─── 计划待审（DSH PlanReviewPanel 迁移） ─── */
  .hns-plan-review {
    border: 1px solid color-mix(in srgb, #b08800 45%, transparent);
    background: color-mix(in srgb, #b08800 6%, transparent);
    border-radius: 10px;
    padding: 10px 14px;
    max-width: 640px;
    align-self: center;
    width: 100%;
  }
  .hns-plan-review-head {
    display: flex;
    align-items: center;
    font-weight: 700;
    font-size: 12.5px;
    color: #b08800;
    margin-bottom: 6px;
  }
  .hns-plan-review-body {
    font-size: 12px;
    line-height: 1.6;
    color: var(--hns-text, inherit);
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 260px;
    overflow: auto;
    margin-bottom: 8px;
  }
  .hns-plan-review-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }
  .hns-plan-review-actions button {
    font-size: 12px;
    font-weight: 600;
    border: 1px solid var(--hns-border, rgba(128, 128, 128, .3));
    border-radius: 7px;
    padding: 5px 14px;
    background: transparent;
    color: var(--hns-text, inherit);
    cursor: pointer;
  }
  .hns-plan-review-actions button:hover { border-color: var(--hns-accent, #4176e6); color: var(--hns-accent, #4176e6); }
  .hns-plan-review-actions button.primary {
    background: var(--hns-accent, #4176e6);
    border-color: var(--hns-accent, #4176e6);
    color: #fff;
  }
  .hns-plan-review-actions button.primary:hover { opacity: .9; }
  /* ─── 工作流运行面板（DSH WorkflowRunPanel 迁移） ─── */
  .hns-workflow-run {
    border: 1px solid var(--hns-border-light, rgba(128, 128, 128, .22));
    border-radius: 10px;
    padding: 8px 12px;
    background: color-mix(in srgb, var(--hns-card, #fff) 90%, transparent);
    font-size: 12px;
  }
  .hns-wf-head {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .hns-wf-title { font-weight: 600; color: var(--hns-text, inherit); }
  .hns-wf-dots { display: inline-flex; gap: 3px; align-items: center; }
  .hns-wf-dot {
    width: 7px; height: 7px; border-radius: 50%;
    background: var(--hns-border-light, rgba(128, 128, 128, .3));
  }
  .hns-wf-dot.done { background: #2ea043; }
  .hns-wf-status { font-size: 11px; color: var(--hns-muted, #888); flex: 1; min-width: 0; }
  .hns-wf-phase {
    margin-top: 8px;
    border-top: 1px solid var(--hns-border-light, rgba(128, 128, 128, .14));
    padding-top: 6px;
  }
  .hns-wf-phase-head {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    color: var(--hns-muted, #888);
    margin-bottom: 4px;
  }
  .hns-wf-phase-status { color: #2ea043; font-weight: 600; }
  .hns-wf-output {
    margin: 0;
    max-height: 240px;
    overflow: auto;
    font-size: 11px;
    font-family: ui-monospace, Consolas, monospace;
    white-space: pre-wrap;
    word-break: break-all;
    color: var(--hns-text, inherit);
  }
  /* ─── 输入区（悬浮输入条；所有控件相对 textarea 垂直居中） ─── */
  .hns-input {
    position: relative;
    display: flex; align-items: center; gap: 8px;
    margin: 10px 16px 14px; max-width: 760px; align-self: center; width: calc(100% - 32px);
    background: color-mix(in srgb, var(--hns-card) 96%, transparent);
    border: 1px solid var(--hns-border);
    border-radius: 16px; padding: 10px 12px;
    box-shadow: 0 16px 40px -20px rgba(0, 0, 0, .4);
    backdrop-filter: blur(10px);
    transition: border-color .15s, box-shadow .15s;
  }
  .hns-input:focus-within {
    border-color: color-mix(in srgb, var(--hns-accent) 50%, var(--hns-border));
    box-shadow: 0 16px 40px -18px rgba(0, 0, 0, .45), 0 0 0 3px var(--hns-accent-soft);
  }
  .hns-input textarea {
    flex: 1; resize: none; min-height: 22px; max-height: 160px;
    background: transparent; color: var(--hns-text);
    border: none; outline: none; font-family: inherit;
    font-size: 13px; line-height: 1.55; padding: 4px 2px;
  }
  .hns-send {
    flex: none; width: 32px; height: 32px; border-radius: 50%;
    display: inline-flex; align-items: center; justify-content: center;
    background: linear-gradient(135deg, var(--hns-accent), color-mix(in srgb, var(--hns-accent) 72%, #0ea5e9));
    color: #fff; border: none; cursor: pointer;
    box-shadow: 0 6px 14px -6px color-mix(in srgb, var(--hns-accent) 60%, transparent);
    transition: transform .1s, opacity .12s;
  }
  .hns-send:hover:not(:disabled) { transform: translateY(-1px); }
  .hns-send:disabled {
    background: var(--hns-surface); color: var(--hns-muted); cursor: not-allowed;
    box-shadow: none;
  }
  .hns-stop {
    flex: none; height: 32px; border-radius: 10px; padding: 0 12px;
    display: inline-flex; align-items: center; gap: 6px;
    background: color-mix(in srgb, var(--hns-red) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--hns-red) 45%, transparent);
    color: var(--hns-red); cursor: pointer; font-size: 12px; font-weight: 600;
    transition: background .12s, transform .1s;
  }
  .hns-stop:hover { background: color-mix(in srgb, var(--hns-red) 22%, transparent); transform: translateY(-1px); }
  .hns-attach-btn {
    flex: none; width: 28px; height: 28px; border-radius: 8px;
    display: inline-flex; align-items: center; justify-content: center;
    background: none; border: 1px solid var(--hns-border);
    color: var(--hns-muted); cursor: pointer; transition: color .12s, border-color .12s;
  }
  .hns-attach-btn:hover { color: var(--hns-text); border-color: color-mix(in srgb, var(--hns-accent) 40%, var(--hns-border)); }
  .hns-attachments {
    display: flex; gap: 6px; flex-wrap: wrap;
    margin: 0 16px 0; max-width: 760px; align-self: center; width: calc(100% - 32px);
  }
  .hns-attachment {
    font-size: 11.5px; color: var(--hns-text);
    background: var(--hns-accent-soft); border: 1px solid color-mix(in srgb, var(--hns-accent) 26%, var(--hns-border));
    border-radius: 999px; padding: 3px 10px;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 320px;
  }
  /* ─── 反馈 / 分叉按钮 ─── */
  .hns-feedback { display: flex; align-items: center; gap: 2px; opacity: 0.65; }
  .hns-msg-bot:hover .hns-feedback { opacity: 1; }
  /* ─── 反馈补充说明（DSH MessageFeedbackActions 备注） ─── */
  .hns-feedback-note {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 6px 10px;
    border: 1px solid var(--hns-border-light, rgba(128, 128, 128, .22));
    border-radius: 8px;
    background: color-mix(in srgb, var(--hns-card, #fff) 80%, transparent);
    max-width: 420px;
  }
  .hns-feedback-note-input {
    font-size: 11.5px;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--hns-text, inherit);
    resize: vertical;
    min-height: 44px;
    font-family: inherit;
  }
  .hns-feedback-note-actions {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
  }
  .hns-fork-btn {
    display: inline-flex; align-items: center; gap: 3px;
    background: none; border: none; border-radius: 6px; padding: 3px 6px;
    font-size: 11px; color: var(--hns-muted); cursor: pointer; transition: color .12s, background .12s;
  }
  .hns-fork-btn:hover { color: var(--hns-accent); background: var(--hns-surface); }
  .hns-msg-user .hns-fork-btn {
    opacity: 0; margin-top: 2px; align-self: flex-start;
  }
  .hns-msg-user:hover .hns-fork-btn { opacity: 1; }
  .hns-copy-btn { opacity: 0; align-self: flex-start; margin-top: 2px; }
  .hns-msg-user:hover .hns-copy-btn { opacity: 0.65; }
  .hns-msg-user:hover .hns-copy-btn:hover { opacity: 1; }
  /* ─── 会话元信息行（压缩 / 角色注入；参考 DSH compaction / context-injection 节点） ─── */
  .hns-meta {
    align-self: center;
    display: flex; align-items: center; gap: 8px; flex-wrap: wrap;
    max-width: 680px; width: fit-content;
    margin: 6px auto;
    padding: 5px 12px;
    border-radius: 999px;
    font-size: 11.5px;
    background: color-mix(in srgb, var(--hns-accent) 7%, transparent);
    border: 1px solid color-mix(in srgb, var(--hns-accent) 22%, transparent);
    color: var(--hns-muted);
  }
  .hns-meta.compaction {
    background: color-mix(in srgb, var(--hns-amber) 8%, transparent);
    border-color: color-mix(in srgb, var(--hns-amber) 26%, transparent);
  }
  .hns-meta-title { display: inline-flex; align-items: center; gap: 6px; font-weight: 600; color: var(--hns-text); }
  .hns-meta-toggle {
    background: none; border: none; padding: 0 2px;
    font-size: 11px; color: var(--hns-accent); cursor: pointer;
  }
  .hns-meta-toggle:hover { text-decoration: underline; }
  .hns-meta-detail {
    flex-basis: 100%;
    font-size: 11.5px; line-height: 1.6;
    color: var(--hns-muted);
    background: color-mix(in srgb, var(--hns-card) 70%, transparent);
    border-radius: 8px; padding: 6px 10px;
    max-height: 140px; overflow-y: auto;
    white-space: pre-wrap; word-break: break-word;
  }
  /* ─── MCP / 配置束导入导出 ─── */
  .hns-port {
    margin-top: 14px; padding-top: 12px;
    border-top: 1px dashed var(--hns-border);
    display: flex; flex-direction: column; gap: 8px;
  }
  .hns-port-head { font-size: 12.5px; font-weight: 700; color: var(--hns-text); }
  /* ─── 语音 / PTY ─── */
  .hns-session-act.speaking { color: var(--hns-accent); }
  .hns-voice-status {
    flex: none; font-size: 11px; color: var(--hns-accent);
    max-width: 160px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .hns-attach-btn.rec { color: #e11d48; border-color: #e11d48; }
  .hns-pty-badge {
    flex: none; font-size: 10px; color: #16a34a;
    background: color-mix(in srgb, #16a34a 12%, transparent);
    border-radius: 999px; padding: 1px 7px;
    border: 1px solid color-mix(in srgb, #16a34a 35%, transparent);
  }
  /* ─── 窄屏适配（头部右侧渐进收敛：先隐藏次要下拉，再压缩标题） ─── */
  @media (max-width: 1180px) {
    .hns-bar-right select.hns-bar-compact { display: none; }
    .hns-bar-title { max-width: 180px; }
  }
  @media (max-width: 960px) {
    .hns-bar-title { max-width: 120px; }
    :global(.hns-model-seat-btn) { max-width: 170px; }
  }
  @media (max-width: 800px) {
    .hns-bar { gap: 6px; padding: 8px 10px; }
    .hns-bar-title { display: none; }
    :global(.hns-model-seat-provider) { display: none; }
    .hns-view-switch { margin: 4px 0 2px 18px; }
  }
</style>
