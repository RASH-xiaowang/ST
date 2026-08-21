<script lang="ts">
  import { errText } from '../../format';
  import { llmApi } from "../services/ipc";
  import { numOrNull } from "../numOrNull";
  import {
    PROVIDER_TYPE_LABELS,
    type LlmConfig,
    type ProviderConfig,
    type ProviderType,
    type TestResult,
  } from "../types";
  import SparklesIcon from "@lucide/svelte/icons/sparkles";
  import CloudIcon from "@lucide/svelte/icons/cloud";
  import ServerIcon from "@lucide/svelte/icons/server";
  import Settings2Icon from "@lucide/svelte/icons/settings-2";
  import { RippleButton } from "fancy-ui-svelte";
  import { NativeSelect, NativeSelectOption } from "../../components/ui/native-select";
  import type { Component } from 'svelte';

  const PROVIDER_ICONS: Record<ProviderType, Component> = {
    openai: SparklesIcon,
    azure: CloudIcon,
    ollama: ServerIcon,
    custom: Settings2Icon,
  };

  let { config, onConfigChange }: { config: LlmConfig; onConfigChange: () => Promise<void> } =
    $props();

  let error = $state("");
  let success = $state("");
  let saving = $state(false);
  let editing = $state<ProviderConfig | null>(null);
  let showEditor = $state(false);

  // 模型能力区域的折叠状态（按 provider id 记录），默认全部展开
  // 模型能力弹窗：记录当前打开弹窗的 provider id（null 表示关闭）
  let modelDialogProviderId = $state<string | null>(null);
  function openModelDialog(p: ProviderConfig) {
    modelDialogProviderId = p.id;
  }
  function closeModelDialog() {
    modelDialogProviderId = null;
  }
  $effect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeModelDialog();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  // 编辑表单的本地副本字段
  let f_name = $state("");
  let f_type = $state<ProviderType>("openai");
  let f_base = $state("");
  let f_key = $state("");
  let f_org = $state("");
  let f_azure_version = $state("");
  let f_model = $state("");
  let f_enabled = $state(true);
  let f_in_price = $state(0);
  let f_out_price = $state(0);
  let f_token_limit = $state<number | null>(null);
  let f_cost_limit = $state<number | null>(null);
  let f_headers = $state<Array<{ key: string; value: string }>>([]);

  function logError(ctx: string, e: unknown) {
    console.error(`[ProviderConfigTab] ${ctx}:`, e);
  }

  function openCreate() {
    editing = null;
    f_name = "";
    f_type = "openai";
    f_base = "";
    f_key = "";
    f_org = "";
    f_azure_version = "";
    f_model = "";
    f_enabled = true;
    f_in_price = 0;
    f_out_price = 0;
    f_token_limit = null;
    f_cost_limit = null;
    f_headers = [];
    error = "";
    success = "";
    showEditor = true;
  }

  /** 快速填入硅基流动接入参数（语音转写模型 TeleAI/TeleSpeechASR） */
  function quickFillSiliconFlow() {
    openCreate();
    f_name = "硅基流动";
    f_type = "openai";
    f_base = "https://api.siliconflow.cn/v1";
    f_model = "TeleAI/TeleSpeechASR";
  }

  function openEdit(p: ProviderConfig) {
    editing = p;
    f_name = p.name;
    f_type = p.provider_type;
    f_base = p.base_url;
    f_key = p.api_key;
    f_org = p.organization ?? "";
    f_azure_version = p.azure_api_version ?? "";
    f_model = p.default_model;
    f_enabled = p.enabled;
    f_in_price = p.input_price_per_1m;
    f_out_price = p.output_price_per_1m;
    f_token_limit = p.monthly_token_limit;
    f_cost_limit = p.monthly_cost_limit;
    f_headers = Object.entries(p.extra_headers).map(([key, value]) => ({ key, value }));
    error = "";
    success = "";
    showEditor = true;
  }

  function addHeaderRow() {
    f_headers = [...f_headers, { key: "", value: "" }];
  }
  function removeHeaderRow(i: number) {
    f_headers = f_headers.filter((_, idx) => idx !== i);
  }
  async function save() {
    if (!f_name.trim()) {
      error = "请填写配置名称";
      return;
    }
    if (!f_base.trim()) {
      error = "请填写 API Base URL";
      return;
    }
    saving = true;
    error = "";
    success = "";
    try {
      const headers: Record<string, string> = {};
      for (const h of f_headers) {
        if (h.key.trim()) headers[h.key.trim()] = h.value;
      }
      const payload: ProviderConfig = {
        id: editing?.id ?? "",
        name: f_name.trim(),
        provider_type: f_type,
        base_url: f_base.trim(),
        api_key: f_key,
        organization: f_org.trim() || undefined,
        azure_api_version: f_azure_version.trim() || undefined,
        default_model: f_model.trim(),
        models: editing?.models ?? [],
        enabled: f_enabled,
        input_price_per_1m: Number(f_in_price) || 0,
        output_price_per_1m: Number(f_out_price) || 0,
        monthly_token_limit: numOrNull(String(f_token_limit)),
        monthly_cost_limit: numOrNull(String(f_cost_limit)),
        extra_headers: headers,
        created_at: editing?.created_at ?? "",
        updated_at: editing?.updated_at ?? "",
      };
      await llmApi.upsertProvider(payload);
      success = editing ? "配置已更新" : "配置已添加";
      showEditor = false;
      await onConfigChange();
    } catch (e: unknown) {
      error = `保存失败：${errText(e)}`;
      logError("save", e);
    } finally {
      saving = false;
    }
  }

  async function remove(p: ProviderConfig) {
    if (!confirm(`确定删除「${p.name}」？此操作不可恢复。`)) return;
    try {
      await llmApi.deleteProvider(p.id);
      success = "已删除";
      await onConfigChange();
    } catch (e: unknown) {
      error = `删除失败：${errText(e)}`;
      logError("remove", e);
    }
  }

  async function setDefault(p: ProviderConfig) {
    try {
      await llmApi.setDefaultProvider(p.id);
      success = `已将「${p.name}」设为全局默认`;
      await onConfigChange();
    } catch (e: unknown) {
      error = `设置失败：${errText(e)}`;
      logError("setDefault", e);
    }
  }

  /** 点击卡片中的模型 pill 即可快速设为该提供方的默认模型 */
  async function setModelDefault(p: ProviderConfig, m: string) {
    if (p.default_model === m) return;
    try {
      await llmApi.setDefaultModel(p.id, m);
      success = `已设置默认模型：${m}`;
      error = "";
      await onConfigChange();
    } catch (e: unknown) {
      error = `设置默认失败：${errText(e)}`;
      logError("setModelDefault", e);
    }
  }

  const providerTypes = Object.keys(PROVIDER_TYPE_LABELS) as ProviderType[];

  // 连接测试状态：按 provider id 分别保存，避免卡片间互相干扰
  type TestState = { testing: boolean; result: TestResult | null; error: string; showBubble: boolean };
  const EMPTY_TEST: TestState = { testing: false, result: null, error: "", showBubble: false };
  let testStates = $state<Record<string, TestState>>({});

  async function runTest(p: ProviderConfig) {
    testStates[p.id] = { testing: true, result: null, error: "", showBubble: false };
    try {
      const r = await llmApi.testConnection(p.id);
      testStates[p.id] = { testing: false, result: r, error: "", showBubble: true };
    } catch (e: unknown) {
      testStates[p.id] = { testing: false, result: null, error: `测试失败：${errText(e)}`, showBubble: true };
      logError("runTest", e);
    }
  }

  function closeBubble(id: string) {
    const cur = testStates[id];
    if (cur) testStates[id] = { ...cur, showBubble: false };
  }
</script>

<div class="llm-config">
  <div class="llm-toolbar">
    <div class="llm-subtitle">已接入 {config.providers.length} 个大模型提供方</div>
    <RippleButton
      onclick={openCreate}
      rippleColor="#a5f3fc"
      class="h-9 rounded-md border-0 bg-[var(--primary)] px-4 text-sm font-medium text-[var(--primary-foreground)] hover:opacity-90"
    >+ 新增接入</RippleButton>
  </div>

  {#if error}<div class="llm-error">{error}</div>{/if}
  {#if success}<div class="llm-success">{success}</div>{/if}

  {#if config.providers.length === 0}
    <div class="llm-empty">尚未配置任何外部大模型，点击「新增接入」开始。</div>
  {:else}
    <div class="llm-providers">
      {#each config.providers as p (p.id)}
        {@const st = testStates[p.id] ?? EMPTY_TEST}
        {@const isDefaultProv = config.default_provider_id === p.id}
        {@const PIcon = PROVIDER_ICONS[p.provider_type] ?? Settings2Icon}
        <div class="llm-provider-card" class:llm-default={isDefaultProv} class:llm-off={!p.enabled}>
          <!-- 头部：名称 + 状态标签 + 类型 -->
          <div class="llm-c-head">
            <span class="llm-c-logo"><PIcon class="size-4.5" /></span>
            <div class="llm-c-name-wrap">
              <div class="llm-c-name-row">
                <span class="llm-c-name">{p.name}</span>
                <span class="llm-c-tags">
                  {#if isDefaultProv}<span class="llm-tag llm-tag-default">默认</span>{/if}
                  {#if !p.enabled}<span class="llm-tag llm-tag-off">禁用</span>{/if}
                  <span class="llm-tag llm-tag-type">{PROVIDER_TYPE_LABELS[p.provider_type]}</span>
                </span>
              </div>
            </div>
          </div>

          <!-- Meta：URL + 默认模型/数量（2 行紧凑） -->
          <div class="llm-c-meta">
            <div class="llm-c-meta-url" title={p.base_url || ""}>{p.base_url || "未配置 Base URL"}</div>
            <div class="llm-c-meta-summary">
              <span>默认 <span class="llm-c-mono">{p.default_model || "—"}</span></span>
              <span class="llm-c-dot">·</span>
              <span>{p.models.length} 个模型</span>
            </div>
          </div>

          <!-- 模型能力：点击弹出对话框 -->
          {#if p.models.length > 0}
            {@const metas = p.models.filter((m) => p.model_meta?.[m])}
            <div class="llm-c-models">
              <button
                type="button"
                class="llm-c-models-toggle"
                onclick={() => openModelDialog(p)}
                title="查看全部模型能力"
              >
                <span>模型能力</span>
                <span class="llm-c-models-count">{metas.length} / {p.models.length}</span>
                <svg class="llm-c-popout" viewBox="0 0 12 12" width="11" height="11" aria-hidden="true">
                  <path d="M4.5 7.5L8 4M8 4H5M8 4v3" stroke="currentColor" stroke-width="1.4" fill="none" stroke-linecap="round" stroke-linejoin="round" />
                </svg>
              </button>
            </div>
          {/if}

          <!-- 底部：操作按钮（推到卡片最底） -->
          <div class="llm-c-foot">
            <div class="llm-c-foot-actions">
              <button class="llm-c-btn" onclick={() => openEdit(p)}>编辑</button>
              {#if !isDefaultProv}
                <button class="llm-c-btn" onclick={() => setDefault(p)}>设为默认</button>
              {/if}
              <button class="llm-c-btn llm-c-btn-danger" onclick={() => remove(p)}>删除</button>
            </div>
            <div class="llm-test-wrap">
              <button class="llm-c-btn llm-c-btn-test" onclick={() => runTest(p)} disabled={st.testing}>
                {st.testing ? "测试中…" : "测试连接"}
              </button>
              {#if st.showBubble && (st.result || st.error)}
                <div class="llm-test-pop" class:ok={st.result?.ok} class:fail={st.error || (st.result != null && !st.result.ok)}>
                  <button class="llm-test-pop-close" onclick={(e) => { e.stopPropagation(); closeBubble(p.id); }} title="关闭">×</button>
                  {#if st.error && !st.result}
                    <div class="llm-test-pop-head"><span class="llm-test-dot"></span>测试失败</div>
                    <div class="llm-test-err">{st.error}</div>
                  {:else if st.result}
                    <div class="llm-test-pop-head"><span class="llm-test-dot" class:on={st.result.ok}></span>{st.result.ok ? "连接成功" : "连接失败"}</div>
                    <div class="llm-test-pop-row"><span>模型</span>{st.result.model ?? "—"}</div>
                    <div class="llm-test-pop-row"><span>耗时</span>{st.result.latency_ms} ms</div>
                    {#if st.result.error}<div class="llm-test-err">错误：{st.result.error}</div>{/if}
                  {/if}
                </div>
              {/if}
            </div>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

{#if showEditor}
  <div class="llm-modal-mask" onclick={() => (showEditor = false)} role="presentation">
    <div class="llm-modal" onclick={(e) => e.stopPropagation()} role="presentation">
      <div class="llm-modal-title">
        {editing ? "编辑接入配置" : "新增外部大模型接入"}
        {#if !editing}
          <button
            class="llm-sf-quick"
            onclick={quickFillSiliconFlow}
            title="填入硅基流动 Base URL 与 TeleAI/TeleSpeechASR 语音转写模型"
          >
            快速填入：硅基流动
          </button>
        {/if}
      </div>

      <div class="llm-form-grid">
        <label class="llm-field">
          <span>配置名称 *</span>
          <input bind:value={f_name} placeholder="如 OpenAI / DeepSeek / 本地Ollama" />
        </label>
        <label class="llm-field">
          <span>提供方类型</span>
          <NativeSelect wrapperClass="w-full" bind:value={f_type}>
            {#each providerTypes as t}
              <NativeSelectOption value={t}>{PROVIDER_TYPE_LABELS[t]}</NativeSelectOption>
            {/each}
          </NativeSelect>
        </label>
        <label class="llm-field llm-field-wide">
          <span>API Base URL *</span>
          <input bind:value={f_base} placeholder="https://api.openai.com/v1" />
        </label>
        <label class="llm-field llm-field-wide">
          <span>API Key</span>
          <input bind:value={f_key} type="password" placeholder="sk-..." autocomplete="off" />
        </label>
        <label class="llm-field">
          <span>Organization</span>
          <input bind:value={f_org} placeholder="可选" />
        </label>
        {#if f_type === "azure"}
          <label class="llm-field">
            <span>API Version</span>
            <input bind:value={f_azure_version} placeholder="如 2024-02-15-preview" />
          </label>
        {/if}
        <label class="llm-field">
          <span>默认模型</span>
          <input bind:value={f_model} placeholder="gpt-4o / deepseek-chat ..." />
        </label>
        <label class="llm-field llm-field-wide">
          <span class="llm-check">
            <input type="checkbox" bind:checked={f_enabled} /> 启用该提供方
          </span>
        </label>
      </div>

      <div class="llm-section-label">计费单价（USD / 每百万 token）</div>
      <div class="llm-form-grid">
        <label class="llm-field">
          <span>输入价格</span>
          <input type="number" step="0.0001" bind:value={f_in_price} />
        </label>
        <label class="llm-field">
          <span>输出价格</span>
          <input type="number" step="0.0001" bind:value={f_out_price} />
        </label>
      </div>

      <div class="llm-section-label">流量与成本管控（每月配额，留空表示不限制）</div>
      <div class="llm-form-grid">
        <label class="llm-field">
          <span>Token 上限</span>
          <input type="number" step="1" bind:value={f_token_limit} placeholder="如 1000000" />
        </label>
        <label class="llm-field">
          <span>成本上限 USD</span>
          <input type="number" step="0.01" bind:value={f_cost_limit} placeholder="如 20" />
        </label>
      </div>

      <div class="llm-section-label">自定义请求头（可选）</div>
      <div class="llm-headers">
        {#each f_headers as h, i (i)}
          <div class="llm-header-row">
            <input placeholder="Header-Name" bind:value={h.key} />
            <input placeholder="value" bind:value={h.value} />
            <button class="llm-btn llm-btn-danger" onclick={() => removeHeaderRow(i)}>×</button>
          </div>
        {/each}
        <button class="llm-btn" onclick={addHeaderRow}>+ 添加请求头</button>
      </div>

      {#if error}<div class="llm-error">{error}</div>{/if}

      <div class="llm-modal-actions">
        <button class="llm-btn" onclick={() => (showEditor = false)}>取消</button>
        <RippleButton onclick={save} disabled={saving} rippleColor="#a5f3fc"
          class="h-9 rounded-md border-0 bg-[var(--primary)] px-4 text-sm font-medium text-[var(--primary-foreground)] hover:opacity-90">
          {saving ? "保存中…" : "保存"}
        </RippleButton>
      </div>
    </div>
  </div>
{/if}

{#if modelDialogProviderId}
  {@const dp = config.providers.find((x) => x.id === modelDialogProviderId)}
  {#if dp}
    {@const dMetas = dp.models.filter((m) => dp.model_meta?.[m])}
    <div class="llm-modal-mask" onclick={closeModelDialog} role="presentation">
      <div class="llm-modal llm-pill-modal" onclick={(e) => e.stopPropagation()} role="presentation">
        <div class="llm-modal-title llm-pill-modal-title">
          <span>模型能力 · {dp.name}</span>
          <span class="llm-pill-modal-stat">{dMetas.length} / {dp.models.length} 已标注</span>
        </div>

        {#if dMetas.length === 0}
          <div class="llm-c-hint">尚未标注模型能力，可在「模型管理」中设置</div>
        {:else}
          <div class="llm-pill-modal-tip">点击任意模型可设为该提供方的默认模型（绿色为当前默认）</div>
          <div class="llm-c-pills">
            {#each dMetas as m (m)}
              {@const meta = dp.model_meta?.[m]}
              <button
                type="button"
                class="llm-c-pill"
                class:default={dp.default_model === m}
                title={`${m}${meta?.model_type ? ` · ${meta.model_type}` : ""}${(meta?.tags ?? []).length ? ` · ${(meta?.tags ?? []).join(" · ")}` : ""}`}
                onclick={() => dp && setModelDefault(dp, m)}
              >
                {#if meta?.model_type}
                  <span class="llm-c-pill-type">{meta.model_type}</span>
                {/if}
                <span class="llm-c-pill-name">{m}</span>
                {#each (meta?.tags ?? []) as t (t)}
                  <span class="llm-c-pill-tag">{t}</span>
                {/each}
              </button>
            {/each}
          </div>
        {/if}

        <div class="llm-modal-actions">
          <button class="llm-btn" onclick={closeModelDialog}>关闭</button>
        </div>
      </div>
    </div>
  {/if}
{/if}

<style>
  .llm-config { display: flex; flex-direction: column; gap: 12px; min-height: 100%; }
  .llm-toolbar { display: flex; align-items: center; justify-content: space-between; }
  .llm-subtitle { color: var(--app-color-muted); font-size: 13px; }
  .llm-empty { flex: 1; display: flex; align-items: center; justify-content: center; padding: 30px; min-height: 140px; text-align: center; color: var(--app-color-muted); border: 1px dashed var(--app-color-border); border-radius: 10px; }
  /* ============ Provider 卡片整体样式（重构后） ============ */
  .llm-providers {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 12px;
    align-items: start;
  }
  .llm-provider-card {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 12px 10px;
    background: var(--app-color-surface);
    border: 1px solid var(--app-color-border);
    border-radius: 12px;
    position: relative;
    transition: border-color 0.15s;
  }
  .llm-provider-card:hover { border-color: color-mix(in srgb, var(--app-color-accent) 45%, var(--app-color-border)); }
  .llm-provider-card.llm-default { border-top: 2px solid #22c55e; padding-top: 11px; }
  .llm-provider-card.llm-off { opacity: 0.65; }

  /* Header：名称 + 标签内联 */
  .llm-c-head { display: flex; align-items: center; gap: 10px; }
  .llm-c-logo {
    flex: none;
    width: 36px;
    height: 36px;
    border-radius: 9px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: color-mix(in srgb, var(--app-color-accent) 16%, var(--app-color-surface));
    color: var(--app-color-accent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--app-color-accent) 28%, transparent);
  }
  .llm-c-name-wrap { min-width: 0; }
  .llm-c-name-row {
    display: flex; align-items: center; gap: 7px;
    flex-wrap: wrap; min-width: 0;
  }
  .llm-c-name { font-weight: 600; font-size: 14px; color: var(--app-color-text); }
  .llm-c-tags { display: inline-flex; gap: 4px; flex-wrap: wrap; }
  .llm-tag {
    font-size: 11.5px; font-weight: 500;
    padding: 2px 7px; border-radius: 4px;
    white-space: nowrap;
  }
  .llm-tag-default { background: #22c55e1a; color: #4ade80; border: 1px solid #22c55e33; }
  .llm-tag-off { background: #ef44441a; color: #f87171; border: 1px solid #ef444433; }
  .llm-tag-type {
    background: var(--app-color-surface-alt); color: var(--app-color-muted);
    border: 1px solid var(--app-color-border);
  }

  /* Meta：URL + 默认模型 / 模型数（2 行紧凑） */
  .llm-c-meta {
    display: flex; flex-direction: column; gap: 4px;
    font-size: 12px; color: var(--app-color-muted);
  }
  .llm-c-meta-url {
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 11.5px;
    color: var(--app-color-text); opacity: 0.8;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .llm-c-meta-summary {
    display: inline-flex; gap: 6px;
    flex-wrap: wrap; align-items: center;
  }
  .llm-c-mono {
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 11.5px; color: var(--app-color-text); font-weight: 500;
  }
  .llm-c-dot { opacity: 0.4; }

  /* 模型能力（卡片内：点击弹出） */
  .llm-c-models {
    border-top: 1px dashed var(--app-color-border);
    padding-top: 8px;
  }
  .llm-c-models-toggle {
    display: inline-flex; align-items: center; gap: 6px;
    background: transparent; border: none; padding: 2px 0;
    cursor: pointer;
    color: var(--app-color-muted);
    font-size: 12px;
    white-space: nowrap;
    transition: color 0.15s;
  }
  .llm-c-models-toggle:hover { color: var(--app-color-text); }
  .llm-c-popout { color: var(--app-color-muted); opacity: 0.7; }
  .llm-c-models-toggle:hover .llm-c-popout { opacity: 1; }
  .llm-c-models-count {
    font-size: 11.5px;
    font-variant-numeric: tabular-nums;
    background: var(--app-color-surface-alt);
    padding: 1px 6px; border-radius: 8px;
    border: 1px solid var(--app-color-border);
    color: var(--app-color-muted);
  }
  .llm-c-hint {
    font-size: 11.5px; color: var(--app-color-muted); opacity: 0.7;
    margin-top: 6px;
  }

  /* 模型能力弹窗 */
  .llm-pill-modal { width: min(560px, 100%); }
  .llm-pill-modal-title {
    display: flex; align-items: center; justify-content: space-between; gap: 10px;
    margin-bottom: 10px;
  }
  .llm-pill-modal-stat { font-size: 11.5px; font-weight: 400; color: var(--app-color-muted); }
  .llm-pill-modal-tip {
    font-size: 11.5px; color: var(--app-color-muted); opacity: 0.75;
    margin-bottom: 10px;
  }

  .llm-c-pills {
    display: flex; flex-wrap: wrap; gap: 5px;
    margin-top: 8px;
    max-height: 52vh; overflow-y: auto;
    padding: 2px 4px 2px 0;
  }
  /* 单个模型 pill：紧凑 */
  .llm-c-pill {
    display: inline-flex; align-items: center;
    gap: 5px;
    padding: 3px 7px;
    border-radius: 6px;
    background: var(--app-color-surface-alt);
    border: 1px solid var(--app-color-border);
    max-width: 100%; min-width: 0;
    cursor: pointer;
    transition: border-color 0.15s, background 0.15s;
    font-size: 11.5px;
    color: inherit;
  }
  .llm-c-pill:hover { border-color: var(--app-color-muted); }
  .llm-c-pill.default {
    border-color: color-mix(in srgb, #22c55e 50%, var(--app-color-border));
    background: color-mix(in srgb, #22c55e 6%, var(--app-color-surface-alt));
  }

  .llm-c-pill-type {
    font-size: 11.5px; font-weight: 600;
    padding: 1px 6px; border-radius: 8px;
    color: var(--app-color-accent);
    background: color-mix(in srgb, var(--app-color-accent) 14%, transparent);
    white-space: nowrap;
  }
  .llm-c-pill-name {
    color: var(--app-color-text);
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 11.5px; font-weight: 500;
    white-space: nowrap;
    overflow: hidden; text-overflow: ellipsis;
    max-width: 180px;
  }
  .llm-c-pill.default .llm-c-pill-name { color: #4ade80; }
  .llm-c-pill-tag {
    font-size: 10.5px;
    padding: 0 5px; border-radius: 7px;
    color: var(--app-color-muted);
    background: var(--app-color-surface);
    border: 1px solid var(--app-color-border);
    white-space: nowrap;
  }

  /* Footer：操作按钮（紧贴卡片底部） */
  .llm-c-foot {
    display: flex; align-items: center; justify-content: space-between;
    gap: 8px;
    border-top: 1px solid var(--app-color-border);
    padding-top: 10px;
    margin-top: 4px;
  }
  .llm-c-foot-actions { display: flex; gap: 5px; }
  .llm-c-btn {
    background: transparent;
    border: 1px solid transparent;
    font-size: 12px; padding: 4px 9px;
    border-radius: 6px;
    color: var(--app-color-text);
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s, color 0.15s;
  }
  .llm-c-btn:hover:not(:disabled) { background: var(--app-color-surface-alt); }
  .llm-c-btn:disabled { opacity: 0.6; cursor: not-allowed; }
  .llm-c-btn-danger { color: #f87171; }
  .llm-c-btn-danger:hover:not(:disabled) { background: #ef44441a; }
  .llm-c-btn-test {
    border-color: color-mix(in srgb, var(--app-color-accent) 40%, var(--app-color-border));
    color: var(--app-color-accent);
    white-space: nowrap;
  }
  .llm-c-btn-test:hover:not(:disabled) {
    background: var(--app-color-accent); color: #fff; border-color: var(--app-color-accent);
  }

  .llm-test-wrap { position: relative; }
  .llm-test-pop {
    position: absolute; bottom: calc(100% + 10px); right: 0; width: 240px; z-index: 20;
    background: var(--app-color-card-bg); border: 1px solid var(--app-color-border);
    border-radius: 10px; padding: 10px 12px 11px; box-shadow: 0 8px 24px rgba(0,0,0,0.18);
    font-size: 12px; color: var(--app-color-text);
  }
  .llm-test-pop::after {
    content: ""; position: absolute; top: 100%; right: 18px;
    border: 7px solid transparent; border-top-color: var(--app-color-card-bg);
  }
  .llm-test-pop.ok { border-color: #22c55e55; }
  .llm-test-pop.fail { border-color: #ef444455; }
  .llm-test-pop-close {
    position: absolute; top: 3px; right: 6px; border: none; background: transparent;
    color: var(--app-color-muted); font-size: 16px; line-height: 1; cursor: pointer; padding: 2px 5px;
  }
  .llm-test-pop-close:hover { color: var(--app-color-text); }
  .llm-test-pop-head { display: flex; align-items: center; gap: 6px; font-weight: 600; margin-bottom: 6px; padding-right: 16px; }
  .llm-test-pop.ok .llm-test-pop-head { color: #4ade80; }
  .llm-test-pop.fail .llm-test-pop-head { color: #f87171; }
  .llm-test-pop-row { display: flex; gap: 8px; margin-top: 3px; color: var(--app-color-text); }
  .llm-test-pop-row span { color: var(--app-color-muted); min-width: 34px; }
  .llm-test-dot { width: 8px; height: 8px; border-radius: 50%; background: #f87171; flex: none; }
  .llm-test-dot.on { background: #4ade80; }
  .llm-test-err { color: #f87171; word-break: break-all; margin-top: 4px; }

  .llm-btn {
    display: inline-flex; align-items: center; gap: 5px; white-space: nowrap;
    background: var(--app-color-surface-alt); color: var(--app-color-text);
    border: 1px solid var(--app-color-border); border-radius: 7px;
    padding: 6px 12px; font-size: 13px; cursor: pointer; transition: 0.15s;
  }
  .llm-btn:hover:not(:disabled) { background: var(--app-color-hover-bg); }
  .llm-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .llm-btn-danger { color: #f87171; border-color: #ef444433; }
  .llm-btn-danger:hover:not(:disabled) { background: #ef44441a; }

  .llm-error { background: #ef44441a; color: #f87171; border: 1px solid #ef444433; padding: 8px 10px; border-radius: 7px; font-size: 13px; }
  .llm-success { background: #22c55e1a; color: #4ade80; border: 1px solid #22c55e33; padding: 8px 10px; border-radius: 7px; font-size: 13px; }

  .llm-modal-mask {
    position: fixed; inset: 0; background: rgba(0,0,0,0.45);
    display: flex; align-items: center; justify-content: center; z-index: 50; padding: 20px;
  }
  .llm-modal {
    background: var(--app-color-card-bg); border: 1px solid var(--app-color-border);
    border-radius: 12px; padding: 18px; width: min(640px, 100%); max-height: 88vh; overflow-y: auto;
  }
  .llm-modal-title { font-size: 15px; font-weight: 600; color: var(--app-color-text); margin-bottom: 14px; }
  .llm-sf-quick {
    margin-left: 10px; font-size: 11.5px; color: var(--app-color-accent);
    background: color-mix(in srgb, var(--app-color-accent) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--app-color-accent) 35%, transparent);
    border-radius: 6px; padding: 3px 8px; cursor: pointer; vertical-align: middle;
  }
  .llm-sf-quick:hover { background: color-mix(in srgb, var(--app-color-accent) 18%, transparent); }
  .llm-form-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .llm-field { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--app-color-muted); }
  .llm-field-wide { grid-column: 1 / -1; }
  .llm-field input {
    background: var(--app-color-surface); color: var(--app-color-text);
    border: 1px solid var(--app-color-border); border-radius: 7px; padding: 7px 9px; font-size: 13px;
  }
  .llm-check { display: flex; align-items: center; gap: 6px; color: var(--app-color-text); }
  .llm-section-label { margin: 14px 0 8px; font-size: 12px; color: var(--app-color-accent); font-weight: 600; }
  .llm-headers { display: flex; flex-direction: column; gap: 6px; }
  .llm-header-row { display: grid; grid-template-columns: 1fr 1fr auto; gap: 6px; }
  .llm-header-row input {
    background: var(--app-color-surface); color: var(--app-color-text);
    border: 1px solid var(--app-color-border); border-radius: 7px; padding: 6px 8px; font-size: 13px;
  }
  .llm-modal-actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; }
</style>

