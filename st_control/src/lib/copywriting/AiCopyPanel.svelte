<script lang="ts">
  import { errText } from '../format';
  import { onMount } from "svelte";
  import { copyText } from '../clipboard';
  import { lsGet, lsSet } from '../storage';
  import { llmApi } from "../llm/services/ipc";
  import { llmStore, refreshLlmConfig } from "../llm/store.svelte";
  import type { LlmConfig, ChatRequest, ChatChunk } from "../llm/types";
  import LlmStatsBadge from "../llm/components/LlmStatsBadge.svelte";
  import ModelSelect from "../llm/components/ModelSelect.svelte";
  import PanelHeader from "../components/PanelHeader.svelte";
  import { Button } from "../components/ui/button";
  import { RippleButton } from "fancy-ui-svelte";
  import { Input } from "../components/ui/input";
  import { Textarea } from "../components/ui/textarea";
  import { Label } from "../components/ui/label";
  import { Skeleton } from "../components/ui/skeleton";
  import { NativeSelect, NativeSelectOption } from "../components/ui/native-select";
  import PenLineIcon from "@lucide/svelte/icons/pen-line";
  import RefreshCwIcon from "@lucide/svelte/icons/refresh-cw";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import CheckIcon from "@lucide/svelte/icons/check";
  import Trash2Icon from "@lucide/svelte/icons/trash-2";
  import SparklesIcon from "@lucide/svelte/icons/sparkles";
  import ClockIcon from "@lucide/svelte/icons/clock";
  import XIcon from "@lucide/svelte/icons/x";

  // 内嵌模式（并入「大模型管理」页签）：隐藏面板头部，改为紧凑工具条
  let { embedded = false }: { embedded?: boolean } = $props();

  // ─── 创作场景模板 ───
  interface Scene {
    id: string;
    name: string;
    emoji: string;
    desc: string;
    system: string;
  }

  const SCENES: Scene[] = [
    {
      id: "marketing",
      name: "营销文案",
      emoji: "📣",
      desc: "活动促销、产品推广、广告语",
      system:
        "你是一位资深的营销策划与文案专家。请围绕「{topic}」创作营销文案。\n" +
        "要求：突出卖点与差异化价值，给出明确行动号召；可适当使用数字、对比与场景化表达，但不得夸大失实。\n" +
        "目标受众：{audience}；语气：{tone}；篇幅：{length}；语言：{language}。\n" +
        "附加要求：{extra}",
    },
    {
      id: "moments",
      name: "朋友圈",
      emoji: "💬",
      desc: "朋友圈配文、状态分享",
      system:
        "你是一位擅长社交媒体文案的编辑。请围绕「{topic}」创作一条适合微信朋友圈发布的内容。\n" +
        "要求：开头抓人、口语化、有生活气息，结尾可配互动引导（提问/共鸣）；避免硬广感。\n" +
        "目标受众：{audience}；语气：{tone}；篇幅：{length}；语言：{language}。\n" +
        "附加要求：{extra}",
    },
    {
      id: "xiaohongshu",
      name: "小红书笔记",
      emoji: "📕",
      desc: "种草笔记、经验分享",
      system:
        "你是一位小红书爆款笔记作者。请围绕「{topic}」创作一篇小红书笔记。\n" +
        "要求：标题吸睛（可含 emoji），正文分段清晰、干货感强，结尾带话题标签（#标签）与互动引导。\n" +
        "目标受众：{audience}；语气：{tone}；篇幅：{length}；语言：{language}。\n" +
        "附加要求：{extra}",
    },
    {
      id: "video",
      name: "短视频脚本",
      emoji: "🎬",
      desc: "口播脚本、分镜脚本",
      system:
        "你是一位短视频编导。请围绕「{topic}」创作短视频脚本。\n" +
        "要求：包含「开头 3 秒钩子 → 正文内容 → 结尾引导」结构；标注口播文案与画面建议，时长按内容自然分配。\n" +
        "目标受众：{audience}；语气：{tone}；篇幅：{length}；语言：{language}。\n" +
        "附加要求：{extra}",
    },
    {
      id: "article",
      name: "公众号文章",
      emoji: "📰",
      desc: "公众号推文、观点长文",
      system:
        "你是一位公众号资深作者。请围绕「{topic}」创作一篇公众号文章。\n" +
        "要求：标题 + 摘要 + 引言 + 分层小标题正文 + 结尾升华；观点鲜明、例证充分，可读性强。\n" +
        "目标受众：{audience}；语气：{tone}；篇幅：{length}；语言：{language}。\n" +
        "附加要求：{extra}",
    },
    {
      id: "product",
      name: "产品介绍",
      emoji: "🏷️",
      desc: "产品详情页、卖点提炼",
      system:
        "你是一位产品经理与文案策划。请围绕「{topic}」撰写产品介绍。\n" +
        "要求：先提炼 3-5 个核心卖点，再按「一句话定位 → 场景痛点 → 功能价值 → 信任背书 → 行动号召」展开。\n" +
        "目标受众：{audience}；语气：{tone}；篇幅：{length}；语言：{language}。\n" +
        "附加要求：{extra}",
    },
    {
      id: "report",
      name: "周报 / 日报",
      emoji: "📊",
      desc: "工作总结、进度汇报",
      system:
        "你是一位高效职场助手。请根据「{topic}」生成结构化工作汇报。\n" +
        "要求：按「本周/今日完成 → 数据与结果 → 问题与风险 → 下周/明日计划 → 需协调事项」组织；语言简洁、量化优先。\n" +
        "目标受众：{audience}；语气：{tone}；篇幅：{length}；语言：{language}。\n" +
        "附加要求：{extra}",
    },
    {
      id: "translate",
      name: "翻译润色",
      emoji: "🌐",
      desc: "中英互译、文字润色",
      system:
        "你是一位专业译者与文字编辑。请对下面的内容进行翻译/润色：\n" +
        "要求：忠实原意、表达地道自然；润色时保留原意并提升文采与逻辑；如有专有名词请保持一致性。\n" +
        "目标受众：{audience}；语气：{tone}；篇幅：{length}；语言：{language}。\n" +
        "附加要求：{extra}",
    },
    {
      id: "notice",
      name: "通知 / 公告",
      emoji: "📢",
      desc: "公司通知、活动公告",
      system:
        "你是一位行政与公关文案专员。请围绕「{topic}」撰写正式通知/公告。\n" +
        "要求：要素齐全（对象、事项、时间、地点、要求、联系人）、条理清晰、措辞得体；如信息不全可合理留白标注。\n" +
        "目标受众：{audience}；语气：{tone}；篇幅：{length}；语言：{language}。\n" +
        "附加要求：{extra}",
    },
    {
      id: "reply",
      name: "评论回复",
      emoji: "💬",
      desc: "用户评论、客诉回复",
      system:
        "你是一位客服与社区运营专家。请针对下面的评论/反馈撰写得体回复。\n" +
        "要求：先共情再解决，语气真诚克制；正面评论顺势互动，负面评论不推诿、给出处理路径。\n" +
        "目标受众：{audience}；语气：{tone}；篇幅：{length}；语言：{language}。\n" +
        "附加要求：{extra}",
    },
    {
      id: "speech",
      name: "演讲稿",
      emoji: "🎤",
      desc: "致辞、演讲、发言稿",
      system:
        "你是一位演讲撰稿人。请围绕「{topic}」撰写演讲稿。\n" +
        "要求：开场抓住注意力、正文有故事与论点支撑、结尾金句收束；标注适合停顿的段落，节奏口语化。\n" +
        "目标受众：{audience}；语气：{tone}；篇幅：{length}；语言：{language}。\n" +
        "附加要求：{extra}",
    },
    {
      id: "email",
      name: "邮件",
      emoji: "✉️",
      desc: "商务邮件、跟进邮件",
      system:
        "你是一位商务沟通专家。请根据「{topic}」撰写一封得体的邮件。\n" +
        "要求：包含主题行、称呼、正文（目的→背景→请求/说明→下一步）、结尾与署名；语气专业克制。\n" +
        "目标受众：{audience}；语气：{tone}；篇幅：{length}；语言：{language}。\n" +
        "附加要求：{extra}",
    },
  ];

  const TONES = ["专业正式", "简洁干练", "轻松活泼", "亲切温暖", "幽默有趣", "高端大气"];
  const LENGTHS = ["不限", "简短（100 字内）", "标准（100-300 字）", "较长（300-600 字）", "长篇（600 字以上）"];
  const LANGS = ["中文", "英文", "中英双语"];

  const HISTORY_KEY = "ai_copy_history_v1";

  // ─── 状态 ───
  const config = $derived(llmStore.config);
  const loading = $derived(llmStore.loading);
  const loadError = $derived(llmStore.error);

  let selectedId = $state("");
  let selected = $state<LlmConfig["providers"][number] | null>(null);
  let modelId = $state("");

  let sceneId = $state("marketing");
  const scene = $derived(SCENES.find((s) => s.id === sceneId) ?? SCENES[0]);

  let topic = $state("");
  let audience = $state("");
  let tone = $state("专业正式");
  let length = $state("标准（100-300 字）");
  let lang = $state("中文");
  let extra = $state("");
  let material = $state("");

  let generating = $state(false);
  let output = $state("");
  let error = $state("");
  let lastUsage = $state("");
  let outputEl = $state<HTMLDivElement | null>(null);
  let copied = $state(false);
  let history = $state<Array<{ time: string; scene: string; topic: string; text: string }>>([]);
  let historyOpen = $state(false);

  const canGenerate = $derived(
    !generating && !!selected && !!modelId && topic.trim().length > 0,
  );

  function modelTypeOf(p: typeof selected, m: string): string | null | undefined {
    return p?.model_meta?.[m]?.model_type;
  }
  const isChatModel = $derived(
    !["生图", "视频", "语音", "嵌入", "重排序"].includes(modelTypeOf(selected, modelId) ?? ""),
  );

  // ─── 配置加载 ───
  async function loadConfig() {
    await refreshLlmConfig();
  }

  function loadHistory() {
    try {
      const raw = lsGet(HISTORY_KEY);
      history = raw ? (JSON.parse(raw) as typeof history) : [];
    } catch {
      history = [];
    }
  }

  function saveHistory() {
    lsSet(HISTORY_KEY, JSON.stringify(history.slice(0, 50)));
  }

  onMount(() => {
    loadConfig();
    loadHistory();
  });

  // 默认提供方 / 模型（优先上次聊天）
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

  // ─── 生成 ───
  function buildPrompt(): { system: string; user: string } {
    const s = scene.system
      .replaceAll("{topic}", topic.trim() || "（未指定主题，请围绕输入内容展开）")
      .replaceAll("{audience}", audience.trim() || "（未指定，通用受众）")
      .replaceAll("{tone}", tone)
      .replaceAll("{length}", length)
      .replaceAll("{language}", lang)
      .replaceAll("{extra}", extra.trim() || "（无）");

    const user = [
      topic.trim() ? `主题/需求：${topic.trim()}` : "",
      material.trim() ? `参考素材：\n${material.trim()}` : "",
      "请直接输出成品文案，不要解释创作过程。",
    ]
      .filter(Boolean)
      .join("\n\n");

    return { system: s, user };
  }

  async function generate() {
    if (generating) return;
    if (!selectedId || !modelId) {
      error = "请先选择提供方与模型";
      return;
    }
    if (!isChatModel) {
      error = "当前模型不是对话模型，请在「大模型管理 → 模型管理」中将模型类型标注为对话。";
      return;
    }
    if (!topic.trim()) {
      error = "请填写主题 / 需求";
      return;
    }
    error = "";
    output = "";
    lastUsage = "";
    generating = true;

    const { system, user } = buildPrompt();
    const req: ChatRequest = {
      provider_id: selectedId,
      model: modelId,
      messages: [
        { role: "system", content: system },
        { role: "user", content: user },
      ],
      temperature: 0.75,
    };

    let acc = "";
    try {
      await llmApi.chatStream(req, (chunk: ChatChunk) => {
        if (chunk.type === "delta") {
          acc += chunk.content;
          output = acc;
          if (outputEl) outputEl.scrollTop = outputEl.scrollHeight;
        } else if (chunk.type === "done") {
          acc = chunk.content;
          output = acc;
          lastUsage = `本次消耗 ${chunk.total_tokens} tokens · 估算成本 $${chunk.cost.toFixed(6)} · ${chunk.model}`;
        } else if (chunk.type === "error") {
          error = `调用失败：${chunk.message}`;
        }
      });
    } catch (e: unknown) {
      error = `调用失败：${errText(e)}`;
    } finally {
      generating = false;
    }

    if (output.trim()) {
      history = [
        { time: new Date().toLocaleString("zh-CN", { hour12: false }), scene: scene.name, topic: topic.trim(), text: output },
        ...history,
      ].slice(0, 50);
      saveHistory();
    }
  }

  async function copyOutput() {
    if (!output) return;
    const ok = await copyText(output);
    if (ok) {
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } else {
      error = "复制失败：剪贴板不可用";
    }
  }

  function resetOutput() {
    output = "";
    lastUsage = "";
    error = "";
  }

  function clearHistory() {
    history = [];
    saveHistory();
  }

  function fillFromHistory(item: (typeof history)[number]) {
    sceneId = SCENES.find((s) => s.name === item.scene)?.id ?? sceneId;
    topic = item.topic;
    output = item.text;
    historyOpen = false;
  }

</script>

<div class="cp-root">
  {#snippet headIcon()}
    <PenLineIcon class="size-4.5" />
  {/snippet}
  {#snippet headBadge()}
    <LlmStatsBadge />
  {/snippet}
  {#snippet headActions()}
    <Button size="sm" variant="outline" onclick={() => (historyOpen = !historyOpen)}>
      <ClockIcon class="size-3.5" />
      创作历史{history.length ? `（${history.length}）` : ""}
    </Button>
    <Button size="sm" variant="outline" onclick={loadConfig} disabled={loading}>
      <RefreshCwIcon class="size-3.5 {loading ? 'animate-spin' : ''}" />
      {loading ? "加载中…" : "刷新"}
    </Button>
  {/snippet}
  {#if embedded}
    <!-- 内嵌于大模型面板：不重复渲染面板头部，仅保留操作工具条 -->
    <div class="flex items-center justify-end gap-2 pb-1">
      {@render headActions()}
    </div>
  {:else}
    <PanelHeader title="AI 文案" icon={headIcon} badge={headBadge} actions={headActions} />
  {/if}

  {#if loadError}
    <div class="cp-error">
      <span>{loadError}</span>
      <Button size="sm" variant="outline" onclick={loadConfig}>重试</Button>
    </div>
  {:else if !loading && config.providers.length === 0}
    <div class="cp-empty-providers">
      尚未配置任何模型提供方，请先在「大模型管理 → 接入配置」中添加并启用后再使用文案创作。
      <Button size="sm" variant="outline" onclick={loadConfig}>刷新</Button>
    </div>
  {:else if loading}
    <div class="cp-loading">
      <Skeleton class="cp-skel cp-skel-side" />
      <div class="cp-loading-main">
        <Skeleton class="cp-skel" />
        <Skeleton class="cp-skel" />
        <Skeleton class="cp-skel cp-skel-out" />
      </div>
    </div>
  {:else}
    <div class="cp-body">
      <!-- 左侧：场景选择 -->
      <aside class="cp-side">
        <div class="cp-side-title">创作场景</div>
        <div class="cp-scenes">
          {#each SCENES as s (s.id)}
            <button
              class="cp-scene"
              class:cp-scene-active={s.id === sceneId}
              onclick={() => { sceneId = s.id; error = ""; }}
            >
              <span class="cp-scene-emoji">{s.emoji}</span>
              <span class="cp-scene-body">
                <span class="cp-scene-name">{s.name}</span>
                <span class="cp-scene-desc">{s.desc}</span>
              </span>
            </button>
          {/each}
        </div>
      </aside>

      <!-- 右侧：表单 + 输出 -->
      <main class="cp-main">
        <div class="cp-params">
          <label class="cp-field">
            <span class="cp-field-label">提供方 / 模型</span>
            <ModelSelect
              providerClass="min-w-[128px] max-w-[190px]"
              modelClass="min-w-[128px] max-w-[190px]"
              bind:providerId={selectedId}
              bind:model={modelId}
            />
          </label>
          <label class="cp-field">
            <span class="cp-field-label">语气</span>
            <NativeSelect class="min-w-[128px] max-w-[190px]" bind:value={tone}>
              {#each TONES as t}<NativeSelectOption value={t}>{t}</NativeSelectOption>{/each}
            </NativeSelect>
          </label>
          <label class="cp-field">
            <span class="cp-field-label">篇幅</span>
            <NativeSelect class="min-w-[128px] max-w-[190px]" bind:value={length}>
              {#each LENGTHS as l}<NativeSelectOption value={l}>{l}</NativeSelectOption>{/each}
            </NativeSelect>
          </label>
          <label class="cp-field">
            <span class="cp-field-label">语言</span>
            <NativeSelect class="min-w-[128px] max-w-[190px]" bind:value={lang}>
              {#each LANGS as l}<NativeSelectOption value={l}>{l}</NativeSelectOption>{/each}
            </NativeSelect>
          </label>
        </div>

        {#if !isChatModel && selected && modelId}
          <div class="cp-warn">当前模型非对话类型（{modelTypeOf(selected, modelId)}），请在「大模型管理 → 模型管理」中调整模型类型后使用。 </div>
        {/if}

        <div class="cp-form">
          <div class="cp-field cp-field-block">
            <Label for="cp-topic">主题 / 需求 <span class="cp-req">*</span></Label>
            <Textarea
              id="cp-topic"
              rows={4}
              bind:value={topic}
              placeholder={scene.id === "translate" || scene.id === "reply" ? "粘贴需要翻译 / 润色 / 回复的内容…" : "例如：为春季汽车养护活动写一份朋友圈文案"}
            />
          </div>
          <div class="cp-form-row">
            <div class="cp-field cp-field-block">
              <Label for="cp-audience">目标受众</Label>
              <Input id="cp-audience" bind:value={audience} placeholder="如：25-40 岁车主、公司管理层（可选）" />
            </div>
            <div class="cp-field cp-field-block">
              <Label for="cp-extra">附加要求</Label>
              <Input id="cp-extra" bind:value={extra} placeholder="如：突出限时优惠、含联系方式（可选）" />
            </div>
          </div>
          <div class="cp-field cp-field-block">
            <Label for="cp-material">参考素材（可选）</Label>
            <Textarea
              id="cp-material"
              rows={3}
              bind:value={material}
              placeholder="粘贴产品参数、数据、原稿片段等，供创作参考…"
            />
          </div>
        </div>

        <div class="cp-actions">
          <RippleButton
            onclick={generate}
            disabled={!canGenerate}
            rippleColor="#a5f3fc"
            class="h-9 rounded-md border-0 bg-[var(--primary)] px-4 text-sm font-medium text-[var(--primary-foreground)] hover:opacity-90"
          >
            <SparklesIcon class="size-4" />
            {generating ? "生成中…" : output ? "重新生成" : "生成文案"}
          </RippleButton>
          {#if output}
            <Button variant="outline" onclick={copyOutput}>
              {#if copied}
                <CheckIcon class="size-4" />
              {:else}
                <CopyIcon class="size-4" />
              {/if}
              {copied ? "已复制" : "复制结果"}
            </Button>
            <Button variant="outline" onclick={resetOutput}>清空</Button>
          {/if}
        </div>

        {#if error}<div class="cp-error">{error}</div>{/if}

        <div class="cp-output-wrap">
          <div class="cp-output-hd">
            <span class="text-sm font-semibold">创作结果</span>
            {#if scene}<span class="cp-output-scene">{scene.emoji} {scene.name}</span>{/if}
            {#if lastUsage}<span class="cp-output-usage">{lastUsage}</span>{/if}
          </div>
          <div class="cp-output" bind:this={outputEl}>
            {#if generating && !output}
              <div class="cp-typing">
                <span></span><span></span><span></span>
                <p>正在构思「{scene.name}」…</p>
              </div>
            {:else if output}
              <pre class="cp-output-text">{output}</pre>
            {:else}
              <div class="cp-output-empty">
                <PenLineIcon class="size-8 text-muted-foreground/40" />
                <p>填写主题后点击「生成文案」</p>
                <p class="cp-output-empty-sub">支持 12 类创作场景，可切换语气、篇幅与语言</p>
              </div>
            {/if}
          </div>
        </div>
      </main>
    </div>
  {/if}

  <!-- 创作历史抽屉 -->
  {#if historyOpen}
    <div
      class="cp-overlay"
      onclick={() => (historyOpen = false)}
      onkeydown={(e) => { if (e.key === "Escape") historyOpen = false; }}
      role="presentation"
      tabindex="-1"
    >
      <div
        class="cp-drawer"
        role="dialog"
        aria-modal="true"
        tabindex="-1"
        onclick={(e) => e.stopPropagation()}
        onkeydown={(e) => e.stopPropagation()}
      >
        <div class="cp-drawer-hd">
          <div>
            <h3 style="display:inline-flex;align-items:center;gap:7px"><ClockIcon class="size-4" /> 创作历史</h3>
            <p class="cp-drawer-sub">本地保存最近 50 条生成结果，点击可回填</p>
          </div>
          <div class="flex items-center gap-2">
            <Button size="sm" variant="ghost" onclick={clearHistory} disabled={history.length === 0}>
              <Trash2Icon class="size-3.5" />
              清空
            </Button>
            <button class="cp-close" onclick={() => (historyOpen = false)} aria-label="关闭"><XIcon class="size-4" /></button>
          </div>
        </div>
        <div class="cp-drawer-list">
          {#if history.length === 0}
            <div class="cp-drawer-empty">暂无历史记录</div>
          {:else}
            {#each history as item, i (i)}
              <button class="cp-history-item" onclick={() => fillFromHistory(item)}>
                <span class="cp-history-top">
                  <span class="cp-history-scene">{item.scene}</span>
                  <span class="cp-history-time">{item.time}</span>
                </span>
                <span class="cp-history-topic">{item.topic}</span>
                <span class="cp-history-prev">{item.text.slice(0, 120)}</span>
              </button>
            {/each}
          {/if}
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .cp-root {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }
  .cp-loading {
    display: flex;
    gap: 14px;
    padding: 16px 20px;
    flex: 1;
    min-height: 0;
  }
  .cp-loading-main {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  :global(.cp-skel) {
    height: 56px;
    border-radius: 10px;
  }
  :global(.cp-skel-side) { width: 260px; height: 100%; }
  :global(.cp-skel-out) { flex: 1; }
  .cp-error {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 12px 20px 0;
    padding: 10px 14px;
    border-radius: 10px;
    border: 1px solid color-mix(in oklab, var(--destructive) 45%, transparent);
    background: color-mix(in oklab, var(--destructive) 10%, transparent);
    color: var(--destructive);
    font-size: 12.5px;
  }
  .cp-error > span { flex: 1; }
  .cp-empty-providers {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    flex: 1;
min-height: 0;
margin: 0 20px;
    padding: 18px;
    border-radius: 12px;
    border: 1px solid var(--border);
    background: color-mix(in oklab, var(--card) 60%, black 8%);
    color: var(--muted-foreground);
    font-size: 13px;
  }
  .cp-warn {
    margin: 10px 0 0;
    padding: 8px 12px;
    border-radius: 8px;
    border: 1px solid color-mix(in oklab, #f59e0b 40%, transparent);
    background: color-mix(in oklab, #f59e0b 9%, transparent);
    color: #fbbf24;
    font-size: 12.5px;
  }
  .cp-body {
    display: flex;
    gap: 14px;
    flex: 1;
    min-height: 0;
    padding: 14px 20px 16px;
  }
  .cp-side {
    width: 264px;
    flex: none;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .cp-side-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--muted-foreground);
    letter-spacing: .04em;
    padding: 2px 4px 8px;
  }
  .cp-scenes {
    display: flex;
    flex-direction: column;
    gap: 4px;
    overflow-y: auto;
    min-height: 0;
    padding-right: 4px;
  }
  .cp-scene {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 10px;
    border-radius: 10px;
    border: 1px solid transparent;
    background: transparent;
    text-align: left;
    cursor: pointer;
    transition: background .15s, border-color .15s;
  }
  .cp-scene:hover {
    background: color-mix(in oklab, var(--foreground) 6%, transparent);
  }
  .cp-scene-active {
    background: color-mix(in oklab, var(--primary) 12%, transparent);
    border-color: color-mix(in oklab, var(--primary) 32%, transparent);
  }
  .cp-scene-emoji {
    font-size: 18px;
    flex: none;
  }
  .cp-scene-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .cp-scene-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--foreground);
  }
  .cp-scene-desc {
    font-size: 11.5px;
    color: var(--muted-foreground);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cp-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .cp-params {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 12px;
    border: 1px solid var(--border);
    background: color-mix(in oklab, var(--card) 55%, black 6%);
  }
  .cp-field {
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
  }
  .cp-field-label {
    font-size: 11.5px;
    color: var(--muted-foreground);
  }
  .cp-form {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-top: 14px;
  }
  .cp-field-block {
    flex: 1;
  }
  .cp-form-row {
    display: flex;
    gap: 12px;
  }
  .cp-req {
    color: var(--destructive);
  }
  .cp-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 14px;
  }
  .cp-output-wrap {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 180px;
    margin-top: 14px;
    border-radius: 12px;
    border: 1px solid var(--border);
    background: color-mix(in oklab, var(--card) 55%, black 6%);
    overflow: hidden;
  }
  .cp-output-hd {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  .cp-output-scene {
    padding: 2px 8px;
    border-radius: 6px;
    background: color-mix(in oklab, var(--primary) 14%, transparent);
    color: var(--primary);
    font-size: 11.5px;
  }
  .cp-output-usage {
    margin-left: auto;
    font-size: 11.5px;
    color: var(--muted-foreground);
  }
  .cp-output {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 14px 16px;
  }
  .cp-output-text {
    font-family: var(--font-sans);
    font-size: 13.5px;
    line-height: 1.75;
    color: var(--foreground);
    white-space: pre-wrap;
    word-break: break-word;
    margin: 0;
  }
  .cp-output-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 100%;
    color: var(--muted-foreground);
    font-size: 13px;
  }
  .cp-output-empty-sub {
    font-size: 11.5px;
    color: color-mix(in oklab, var(--muted-foreground) 70%, transparent);
  }
  .cp-typing {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 20px;
    color: var(--muted-foreground);
    font-size: 12.5px;
  }
  .cp-typing p { margin: 0 0 0 8px; }
  .cp-typing span {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--primary);
    /* impeccable-disable-next-line bounce-easing -- 打字指示点动画（有意为之） */
    animation: cp-bounce 1.2s infinite ease-in-out;
  }
  .cp-typing span:nth-child(2) { animation-delay: .15s; }
  .cp-typing span:nth-child(3) { animation-delay: .3s; }
  @keyframes cp-bounce {
    0%, 60%, 100% { transform: translateY(0); opacity: .5; }
    30% { transform: translateY(-5px); opacity: 1; }
  }
  .cp-overlay {
    position: fixed;
    inset: 0;
    z-index: 50;
    background: rgba(0, 0, 0, .45);
    backdrop-filter: blur(3px);
    display: flex;
    justify-content: flex-end;
  }
  .cp-drawer {
    width: min(440px, 90%);
    height: 100%;
    background: var(--card);
    border-left: 1px solid var(--border);
    box-shadow: -18px 0 40px rgba(0, 0, 0, .35);
    display: flex;
    flex-direction: column;
  }
  .cp-drawer-hd {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 10px;
    padding: 16px 18px;
    border-bottom: 1px solid var(--border);
  }
  .cp-drawer-hd h3 { margin: 0; font-size: 16px; }
  .cp-drawer-sub { margin: 4px 0 0; font-size: 12px; color: var(--muted-foreground); }
  .cp-close {
    width: 28px;
    height: 28px;
    border-radius: 7px;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--muted-foreground);
    cursor: pointer;
    font-size: 13px;
  }
  .cp-close:hover {
    background: color-mix(in oklab, var(--foreground) 8%, transparent);
    color: var(--foreground);
  }
  .cp-drawer-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .cp-drawer-empty {
    padding: 48px 0;
    text-align: center;
    color: var(--muted-foreground);
    font-size: 13px;
  }
  .cp-history-item {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 11px 12px;
    border-radius: 10px;
    border: 1px solid var(--border);
    background: color-mix(in oklab, var(--card) 60%, black 8%);
    text-align: left;
    cursor: pointer;
    transition: border-color .15s;
  }
  .cp-history-item:hover {
    border-color: color-mix(in oklab, var(--primary) 45%, transparent);
  }
  .cp-history-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .cp-history-scene {
    font-size: 11.5px;
    padding: 1px 7px;
    border-radius: 5px;
    background: color-mix(in oklab, var(--primary) 14%, transparent);
    color: var(--primary);
  }
  .cp-history-time {
    font-size: 11.5px;
    color: var(--muted-foreground);
  }
  .cp-history-topic {
    font-size: 13px;
    font-weight: 600;
    color: var(--foreground);
  }
  .cp-history-prev {
    font-size: 12px;
    color: var(--muted-foreground);
    display: -webkit-box;
    line-clamp: 2;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
</style>
