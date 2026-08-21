<script module lang="ts">
  import { errText } from '../../format';
  // 跨标签页切换时 ModelManagerTab 会被卸载重挂载，实例 state 丢失；
  // 用模块级变量记住上次选择的提供方，避免每次进入都自动跳回默认/首个。
  let rememberedProviderId = "";
</script>

<script lang="ts">
  import { llmApi } from "../services/ipc";
  import type { LlmConfig, ProviderConfig } from "../types";
  import { RippleButton } from "fancy-ui-svelte";
  import { NativeSelect, NativeSelectOption } from "../../components/ui/native-select";

  let { config, onConfigChange }: { config: LlmConfig; onConfigChange: () => Promise<void> } =
    $props();

  let selectedId = $state("");
  let selected = $derived<ProviderConfig | null>(
    config.providers.find((p) => p.id === selectedId) ?? null,
  );
  let discovered = $state<string[]>([]);
  let probing = $state(false);
  let newModel = $state("");
  let error = $state("");
  let success = $state("");

  // 记住用户当前选择，跨标签页重挂载后保留
  $effect(() => {
    if (selectedId) rememberedProviderId = selectedId;
  });

  $effect(() => {
    if (!selectedId && config.providers.length > 0) {
      const inList = (id: string | null | undefined) =>
        !!id && config.providers.some((p) => p.id === id);
      const cand =
        (inList(rememberedProviderId) ? rememberedProviderId : null) ??
        (inList(config.default_provider_id) ? config.default_provider_id : null) ??
        config.providers[0].id;
      selectedId = cand;
    }
  });

  function logError(ctx: string, e: unknown) {
    console.error(`[ModelManagerTab] ${ctx}:`, e);
  }

  async function probe() {
    if (!selectedId) return;
    probing = true;
    error = "";
    success = "";
    discovered = [];
    try {
      const models = await llmApi.listModels(selectedId);
      discovered = models;
      // 自动把新模型加入提供方列表
      let changed = false;
      for (const m of models) {
        if (selected && !selected.models.includes(m)) {
          await llmApi.addModel(selectedId, m);
          changed = true;
        }
      }
      if (changed) await onConfigChange();
      success = `已探测到 ${models.length} 个模型`;
    } catch (e: unknown) {
      error = `探测失败：${errText(e)}`;
      logError("probe", e);
    } finally {
      probing = false;
    }
  }

  async function addManual() {
    if (!selectedId || !newModel.trim()) return;
    try {
      await llmApi.addModel(selectedId, newModel.trim());
      newModel = "";
      success = "已添加模型";
      error = "";
      await onConfigChange();
    } catch (e: unknown) {
      error = `添加失败：${errText(e)}`;
      logError("addManual", e);
    }
  }

  async function remove(m: string) {
    if (!selectedId) return;
    try {
      await llmApi.removeModel(selectedId, m);
      success = "已移除模型";
      await onConfigChange();
    } catch (e: unknown) {
      error = `移除失败：${errText(e)}`;
      logError("remove", e);
    }
  }

  async function setDefault(m: string) {
    if (!selectedId) return;
    try {
      await llmApi.setDefaultModel(selectedId, m);
      success = `已设置默认模型：${m}`;
      await onConfigChange();
    } catch (e: unknown) {
      error = `设置失败：${errText(e)}`;
      logError("setDefault", e);
    }
  }

  // ─── 模型能力元数据（类型 / 标签）编辑 ───
  // 类型与标签均为前端约定的可选项；后端不做强校验，原样持久化字符串。
  const MODEL_TYPES = ["对话", "生图", "视频", "语音", "嵌入", "重排序"];
  const MODEL_TAGS = ["视觉", "MoE", "推理", "Tools", "FIM", "Math", "Coder"];

  let editingMetaFor = $state<string | null>(null);
  let draftType = $state("");
  let draftTags = $state<string[]>([]);

  function openMeta(m: string) {
    const cur = selected?.model_meta?.[m];
    editingMetaFor = m;
    draftType = cur?.model_type ?? "";
    draftTags = [...(cur?.tags ?? [])];
  }
  function cancelMeta() {
    editingMetaFor = null;
  }
  function handleKey(e: KeyboardEvent) {
    if (e.key === "Escape" && editingMetaFor) cancelMeta();
  }
  function toggleDraftTag(t: string) {
    draftTags = draftTags.includes(t) ? draftTags.filter((x) => x !== t) : [...draftTags, t];
  }

  async function saveMeta(m: string) {
    if (!selectedId) return;
    try {
      await llmApi.setModelMeta(selectedId, m, draftType || null, [...draftTags]);
      editingMetaFor = null;
      success = "能力已保存";
      error = "";
      await onConfigChange();
    } catch (e: unknown) {
      error = `保存失败：${errText(e)}`;
      logError("saveMeta", e);
    }
  }

  // ─── 批量选择移除 ───
  let checked = $state<Record<string, boolean>>({});
  const selectedCount = $derived(Object.values(checked).filter(Boolean).length);
  const allSelected = $derived(
    !!selected && selected.models.length > 0 && selected.models.every((m) => checked[m]),
  );

  // 切换提供方时清空选择 / 关闭编辑气泡
  $effect(() => {
    selectedId;
    checked = {};
    editingMetaFor = null;
  });

  function toggle(m: string) {
    checked = { ...checked, [m]: !checked[m] };
  }

  function toggleAll(e: Event) {
    const v = (e.target as HTMLInputElement).checked;
    const next: Record<string, boolean> = {};
    for (const m of selected?.models ?? []) next[m] = v;
    checked = next;
  }

  async function batchRemove() {
    if (!selected) return;
    const toRemove = selected.models.filter((m) => checked[m]);
    if (toRemove.length === 0) return;
    if (!confirm(`确认移除选中的 ${toRemove.length} 个模型？`)) return;
    try {
      await llmApi.removeModels(selectedId, toRemove);
      success = `已批量移除 ${toRemove.length} 个模型`;
      checked = {};
      await onConfigChange();
    } catch (e: unknown) {
      error = `批量移除失败：${errText(e)}`;
      logError("batchRemove", e);
    }
  }
</script>

<svelte:window onkeydown={handleKey} />

<div class="llm-models">
  <div class="llm-test-bar">
    <label class="llm-field">
      <span>选择提供方</span>
      <NativeSelect wrapperClass="min-w-[220px]" bind:value={selectedId}>
        {#each config.providers as p (p.id)}
          <NativeSelectOption value={p.id}>{p.name}</NativeSelectOption>
        {/each}
      </NativeSelect>
    </label>
    <RippleButton onclick={probe} disabled={probing || !selected} rippleColor="#a5f3fc"
      class="h-9 rounded-md border-0 bg-[var(--primary)] px-4 text-sm font-medium text-[var(--primary-foreground)] hover:opacity-90">
      {probing ? "探测中…" : "从接口探测模型"}
    </RippleButton>
  </div>

  {#if config.providers.length === 0}
    <div class="llm-empty">请先在「接入配置」中添加提供方。</div>
  {/if}
  {#if error}<div class="llm-error">{error}</div>{/if}
  {#if success}<div class="llm-success">{success}</div>{/if}

  {#if selected}
    <div class="llm-add-row">
      <input bind:value={newModel} placeholder="手动添加模型 id，如 gpt-4o-mini" />
      <RippleButton onclick={addManual} disabled={!newModel.trim()} rippleColor="#22d3ee"
        class="h-9 rounded-md border border-[var(--border)] bg-[var(--card)] px-4 text-sm font-medium text-[var(--foreground)] hover:bg-[var(--muted)]">添加</RippleButton>
    </div>

    <div class="llm-model-head">
      <div class="llm-section-label">已登记模型（{selected.models.length}）</div>
      {#if selected.models.length > 0}
        <div class="llm-model-actions">
          <label class="llm-checkline">
            <input type="checkbox" checked={allSelected} onchange={toggleAll} /> 全选
          </label>
          <button class="llm-btn llm-btn-sm llm-btn-danger" onclick={batchRemove} disabled={selectedCount === 0}>
            批量移除{selectedCount > 0 ? ` (${selectedCount})` : ""}
          </button>
        </div>
      {/if}
    </div>
    {#if selected.models.length === 0}
      <div class="llm-empty">暂无模型，可点击「从接口探测」或手动添加。</div>
    {:else}
      <div class="llm-model-list">
        {#each selected.models as m (m)}
          {@const meta = selected.model_meta?.[m]}
          <div class="llm-model-item">
            <div class="llm-model-left">
              <label class="llm-checkline">
                <input type="checkbox" checked={!!checked[m]} onchange={() => toggle(m)} />
              </label>
              <span class="llm-model-name">
                {m}
                {#if selected.default_model === m}<span class="llm-badge llm-badge-default">默认</span>{/if}
              </span>
              <span class="llm-model-metas">
                {#if meta?.model_type}<span class="llm-meta-chip llm-meta-type">{meta.model_type}</span>{/if}
                {#each (meta?.tags ?? []) as t (t)}<span class="llm-meta-chip">{t}</span>{/each}
              </span>
            </div>
            <div class="llm-model-actions">
              {#if selected.default_model !== m}
                <button class="llm-btn llm-btn-sm" onclick={() => setDefault(m)}>设为默认</button>
              {/if}
              <button class="llm-btn llm-btn-sm llm-btn-danger" onclick={() => remove(m)}>移除</button>
              <button class="llm-btn llm-btn-sm" onclick={() => openMeta(m)}>能力</button>
            </div>
          </div>
        {/each}
      </div>
    {/if}

    {#if discovered.length > 0}
      <div class="llm-section-label">本次探测结果（已自动并入上方列表）</div>
      <div class="llm-discovered">
        {#each discovered as m (m)}<span class="llm-chip">{m}</span>{/each}
      </div>
    {/if}
  {/if}

  {#if editingMetaFor}
    <div class="llm-meta-mask" onclick={(e) => { if (e.target === e.currentTarget) cancelMeta(); }} role="presentation">
      <div class="llm-meta-modal" onclick={(e) => e.stopPropagation()} role="presentation">
        <div class="llm-meta-modal-title">编辑模型能力 · {editingMetaFor}</div>
        <div class="llm-meta-section-label">类型（单选）</div>
        <div class="llm-meta-chips">
          {#each MODEL_TYPES as t}
            <button type="button" class="llm-meta-chip-btn" class:on={draftType === t} onclick={() => (draftType = draftType === t ? "" : t)}>{t}</button>
          {/each}
        </div>
        <div class="llm-meta-section-label">标签（可多选）</div>
        <div class="llm-meta-chips">
          {#each MODEL_TAGS as t}
            <button type="button" class="llm-meta-chip-btn" class:on={draftTags.includes(t)} onclick={() => toggleDraftTag(t)}>{t}</button>
          {/each}
        </div>
        <div class="llm-meta-pop-actions">
          <button class="llm-btn llm-btn-sm" onclick={cancelMeta}>取消</button>
          <button class="llm-btn llm-btn-sm llm-btn-primary" onclick={() => { if (editingMetaFor) saveMeta(editingMetaFor); }}>保存</button>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .llm-models { display: flex; flex-direction: column; gap: 12px; min-height: 100%; }
  .llm-test-bar { display: flex; align-items: flex-end; gap: 12px; flex-wrap: wrap; }
  .llm-field { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--app-color-muted); }
  .llm-btn {
    display: inline-flex; align-items: center; gap: 5px; white-space: nowrap;
    background: var(--app-color-surface-alt); color: var(--app-color-text);
    border: 1px solid var(--app-color-border); border-radius: 7px; padding: 7px 12px; font-size: 13px; cursor: pointer;
  }
  .llm-btn-primary { background: var(--app-color-accent); color: #fff; border-color: var(--app-color-accent); font-weight: 600; }
  .llm-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .llm-btn-sm { padding: 3px 8px; font-size: 12px; }
  .llm-btn-danger { color: #f87171; border-color: #ef444433; }
  .llm-btn-danger:hover:not(:disabled) { background: #ef44441a; }
  .llm-empty { flex: 1; display: flex; align-items: center; justify-content: center; padding: 24px; min-height: 140px; text-align: center; color: var(--app-color-muted); border: 1px dashed var(--app-color-border); border-radius: 10px; }
  .llm-error { background: #ef44441a; color: #f87171; border: 1px solid #ef444433; padding: 8px 10px; border-radius: 7px; font-size: 13px; }
  .llm-success { background: #22c55e1a; color: #4ade80; border: 1px solid #22c55e33; padding: 8px 10px; border-radius: 7px; font-size: 13px; }
  .llm-add-row { display: flex; gap: 8px; }
  .llm-add-row input {
    flex: 1; background: var(--app-color-surface); color: var(--app-color-text);
    border: 1px solid var(--app-color-border); border-radius: 7px; padding: 7px 9px; font-size: 13px;
  }
  .llm-section-label { margin-top: 4px; font-size: 12px; color: var(--app-color-accent); font-weight: 600; }
  .llm-model-head { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; }
  .llm-checkline { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: var(--app-color-muted); cursor: pointer; user-select: none; }
  .llm-checkline input { width: 15px; height: 15px; accent-color: var(--app-color-accent); cursor: pointer; }
  .llm-model-list { display: flex; flex-direction: column; gap: 6px; }
  .llm-model-item {
    display: flex; align-items: center; justify-content: space-between;
    background: var(--app-color-surface); border: 1px solid var(--app-color-border);
    border-radius: 8px; padding: 8px 10px;
  }
  .llm-model-left { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .llm-model-name { color: var(--app-color-text); font-size: 13px; display: flex; align-items: center; gap: 6px; }
  .llm-model-actions { display: flex; gap: 6px; }
  .llm-badge { font-size: 11.5px; padding: 1px 6px; border-radius: 5px; }
  .llm-badge-default { background: #22c55e1a; color: #4ade80; border: 1px solid #22c55e33; }
  .llm-discovered { display: flex; flex-wrap: wrap; gap: 6px; }
  .llm-chip { font-size: 12px; background: var(--app-color-surface-alt); border: 1px solid var(--app-color-border); color: var(--app-color-text); border-radius: 6px; padding: 3px 8px; }

  .llm-model-metas { display: inline-flex; flex-wrap: wrap; gap: 4px; }
  .llm-meta-chip {
    font-size: 11.5px; padding: 1px 7px; border-radius: 10px;
    background: var(--app-color-surface-alt); border: 1px solid var(--app-color-border);
    color: var(--app-color-muted);
  }
  .llm-meta-chip.llm-meta-type {
    color: var(--app-color-accent);
    border-color: color-mix(in srgb, var(--app-color-accent) 35%, var(--app-color-border));
    background: color-mix(in srgb, var(--app-color-accent) 8%, transparent);
  }
  .llm-meta-mask {
    position: fixed; inset: 0; z-index: 1000;
    background: rgba(0,0,0,0.45);
    display: flex; align-items: center; justify-content: center; padding: 20px;
  }
  .llm-meta-modal {
    background: var(--app-color-card-bg); border: 1px solid var(--app-color-border);
    border-radius: 14px; padding: 18px; width: min(360px, 100%);
    box-shadow: 0 18px 50px rgba(0,0,0,0.35);
    animation: llm-meta-in 0.16s ease-out;
  }
  @keyframes llm-meta-in { from { opacity: 0; transform: translateY(8px) scale(0.98); } to { opacity: 1; transform: none; } }
  .llm-meta-modal-title { font-weight: 600; color: var(--app-color-text); margin-bottom: 10px; font-size: 14px; word-break: break-all; }
  .llm-meta-section-label { font-size: 11.5px; color: var(--app-color-muted); margin: 6px 0 4px; }
  .llm-meta-chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .llm-meta-chip-btn {
    font-size: 12px; padding: 4px 10px; border-radius: 14px; cursor: pointer;
    background: var(--app-color-surface-alt); border: 1px solid var(--app-color-border);
    color: var(--app-color-text); transition: 0.15s;
  }
  .llm-meta-chip-btn:hover { background: var(--app-color-hover-bg); }
  .llm-meta-chip-btn.on {
    background: var(--app-color-accent); color: #fff; border-color: var(--app-color-accent);
  }
  .llm-meta-pop-actions { display: flex; justify-content: flex-end; gap: 6px; margin-top: 10px; }
</style>

