<script lang="ts">
  import { errText } from '../../format';
  import { onMount } from 'svelte';
  import { copyText } from '../../clipboard';
  import { llmApi } from '../services/ipc';
  import { composeSystemPrompt, createEmptyRole, normalizeRole } from '../roleUtils';
  import type { AiRole } from '../types';
  import { filterByAnyKeyword } from '../../utils/filter';
  import { createMsg } from '../../services/msg.svelte';
  import { RippleButton, CardSpotlight } from 'fancy-ui-svelte';
  import { NativeSelect, NativeSelectOption } from '../../components/ui/native-select';
  import DramaIcon from "@lucide/svelte/icons/drama";
  import SearchIcon from "@lucide/svelte/icons/search";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import Trash2Icon from "@lucide/svelte/icons/trash-2";
  import XIcon from "@lucide/svelte/icons/x";
  import PlusIcon from "@lucide/svelte/icons/plus";
  import SparklesIcon from "@lucide/svelte/icons/sparkles";
  import AlertTriangleIcon from "@lucide/svelte/icons/alert-triangle";

  let roles: AiRole[] = $state([]);
  let loading = $state(true);
  let search = $state('');
  const toast = createMsg(2000);

  // 编辑态
  let editing: AiRole | null = $state(null);
  let saving = $state(false);
  let newConstraint = $state('');
  let newCap = $state('');
  let formErr = $state('');
  let samplingOpen = $state(false);
  let routeOpen = $state(false);
  let previewOpen = $state(false);

  const emojiPresets = ['🤖', '🧠', '💡', '📊', '✍️', '🔍', '🛡️', '🎯', '⚖️', '🚀', '👩‍💻', '🧑‍🏫'];
  const langOptions = ['跟随用户', '中文', 'English', '日本語', '한국어'];

  let filtered = $derived(
    filterByAnyKeyword(roles, search, (r) => r.name || '', (r) => r.description || ''),
  );

  let promptPreview = $derived(editing ? composeSystemPrompt(editing) : '');

  onMount(loadRoles);

  async function loadRoles() {
    loading = true;
    try {
      roles = (await llmApi.getAiRoles()) ?? [];
    } catch {
      roles = [];
    } finally {
      loading = false;
    }
  }

  function openCreate() {
    editing = createEmptyRole();
    formErr = '';
    newConstraint = '';
    newCap = '';
    previewOpen = false;
    focusName();
  }

  function openEdit(r: AiRole) {
    editing = normalizeRole(r);
    formErr = '';
    newConstraint = '';
    newCap = '';
    previewOpen = false;
    focusName();
  }

  /** 抽屉打开后自动聚焦角色名称（等下一帧渲染完成） */
  function focusName() {
    setTimeout(() => {
      document.getElementById('rp-name')?.focus();
    }, 80);
  }

  function closeEdit() {
    if (saving) return;
    editing = null;
    formErr = '';
  }

  function addConstraint() {
    if (!editing) return;
    const v = newConstraint.trim();
    if (!v) return;
    if (!(editing.behavior_constraints || []).includes(v)) {
      editing.behavior_constraints = [...editing.behavior_constraints, v];
    }
    newConstraint = '';
  }
  function removeConstraint(i: number) {
    if (!editing) return;
    editing.behavior_constraints = editing.behavior_constraints.filter((_, idx) => idx !== i);
  }

  function addCap() {
    if (!editing) return;
    const v = newCap.trim();
    if (!v) return;
    if (!(editing.capabilities || []).includes(v)) {
      editing.capabilities = [...editing.capabilities, v];
    }
    newCap = '';
  }
  function removeCap(i: number) {
    if (!editing) return;
    editing.capabilities = editing.capabilities.filter((_, idx) => idx !== i);
  }

  async function saveEdit() {
    if (!editing) return;
    formErr = '';
    if (!editing.name.trim()) {
      formErr = '请填写角色名称';
      return;
    }
    saving = true;
    try {
      if (!editing.preferred_provider_name?.trim()) editing.preferred_provider_name = null;
      if (!editing.preferred_model?.trim()) editing.preferred_model = null;
      const saved = await llmApi.saveAiRole(editing);
      const idx = roles.findIndex((r) => r.id === saved.id);
      if (idx >= 0) roles[idx] = saved;
      else roles = [saved, ...roles];
      editing = null;
      toast.show('角色已保存');
    } catch (e: unknown) {
      formErr = String(errText(e));
    } finally {
      saving = false;
    }
  }

  async function deleteRole(role: AiRole) {
    if (!confirm(`确认删除角色「${role.name}」？此操作不可撤销。`)) return;
    try {
      await llmApi.deleteAiRole(role.id);
      roles = roles.filter((r) => r.id !== role.id);
      if (editing && editing.id === role.id) editing = null;
      toast.show('已删除');
    } catch (e: unknown) {
      toast.show('删除失败：' + errText(e));
    }
  }

  async function toggleEnabled(r: AiRole) {
    try {
      const next = normalizeRole(r);
      next.enabled = !next.enabled;
      const saved = await llmApi.saveAiRole(next);
      const idx = roles.findIndex((x) => x.id === saved.id);
      if (idx >= 0) roles[idx] = saved;
    } catch (e: unknown) {
      toast.show('更新状态失败：' + errText(e));
    }
  }

  function useRole(r: AiRole) {
    window.dispatchEvent(new CustomEvent('role-selected', { detail: r }));
    toast.show(`已选用「${r.name}」，正在跳转全局调用…`);
  }

  function copyPrompt() {
    if (!promptPreview) return;
    void copyText(promptPreview).then((ok) => toast.show(ok ? '已复制系统提示词' : '复制失败'));
  }
</script>

<div class="rp">
  <!-- 页头 -->
  <header class="rp-head">
    <div class="rp-head-info">
      <div class="rp-head-logo" aria-hidden="true"><DramaIcon class="size-5" /></div>
      <div>
        <h1 class="rp-title">AI 角色定位</h1>
        <p class="rp-subtitle">对标大模型 system prompt，定义可复用的 AI 角色，供「全局调用」检索与调度</p>
      </div>
      <span class="rp-count">
        {#if search.trim()}<b>{filtered.length}</b> / {/if}<b>{roles.length}</b><i> 个角色</i>
      </span>
    </div>
    <div class="rp-head-actions">
      <div class="rp-search">
        <span class="rp-search-ico" aria-hidden="true"><SearchIcon class="size-4" /></span>
        <input placeholder="搜索角色名称或描述…" bind:value={search} />
        {#if search}
          <button class="rp-search-clear" onclick={() => (search = '')} title="清空">×</button>
        {/if}
      </div>
      <button class="rp-btn rp-btn-ghost rp-btn-ico" onclick={loadRoles} title="刷新" aria-label="刷新">
        <svg class="rp-ico" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M21 12a9 9 0 1 1-2.64-6.36" />
          <path d="M21 3v6h-6" />
        </svg>
      </button>
      <RippleButton
        onclick={openCreate}
        rippleColor="#a5f3fc"
        class="h-9 rounded-md border-0 bg-[var(--primary)] px-4 text-sm font-medium text-[var(--primary-foreground)] hover:opacity-90"
      ><PlusIcon class="size-4" /> 新建角色</RippleButton>
    </div>
  </header>

  <!-- 角色卡片网格 -->
  <div class="rp-grid">
    {#if loading && roles.length === 0}
      <div class="rp-state">
        <div class="rp-spinner" aria-hidden="true"></div>
        <p>加载中…</p>
      </div>
    {:else if filtered.length === 0}
      <div class="rp-state">
        <div class="rp-state-orb" aria-hidden="true">{#if search.trim()}<SearchIcon class="size-7" />{:else}<DramaIcon class="size-7" />{/if}</div>
        <h3>{search.trim() ? '没有匹配的角色' : '还没有 AI 角色'}</h3>
        <p>{search.trim() ? '换个关键词，或清空搜索试试' : '为全局 LLM 调用定义第一个可复用的角色吧'}</p>
        {#if !search.trim()}
          <RippleButton
            onclick={openCreate}
            rippleColor="#a5f3fc"
            class="h-9 rounded-md border-0 bg-[var(--primary)] px-4 text-sm font-medium text-[var(--primary-foreground)] hover:opacity-90"
          ><PlusIcon class="size-4" /> 创建第一个角色</RippleButton>
        {/if}
      </div>
    {:else}
      {#each filtered as r (r.id)}
        <CardSpotlight
          class="rp-spot"
          gradientColor="var(--rp-em)"
          gradientOpacity={0.2}
          gradientSize={280}
        >
          <article class="rp-card">
            <div class="rp-card-top">
              <div class="rp-card-avatar">{r.emoji || '🤖'}</div>
              <span class="rp-pill" class:on={r.enabled}>{r.enabled ? '已启用' : '已停用'}</span>
              <button
                class="rp-switch"
                class:on={r.enabled}
                role="switch"
                aria-checked={r.enabled}
                aria-label={r.enabled ? '点击停用' : '点击启用'}
                onclick={() => toggleEnabled(r)}
              ><span class="rp-switch-knob" aria-hidden="true"></span></button>
            </div>

            <div class="rp-card-body">
              <h4 class="rp-card-name">{r.name || '未命名角色'}</h4>
              <p class="rp-card-desc">{r.description || '暂无描述'}</p>
              {#if (r.capabilities || []).length}
                <div class="rp-card-tags">
                  {#each r.capabilities.slice(0, 3) as c}
                    <span class="rp-tag">{c}</span>
                  {/each}
                  {#if r.capabilities.length > 3}
                    <span class="rp-tag rp-tag-more">+{r.capabilities.length - 3}</span>
                  {/if}
                </div>
              {/if}
            </div>

            <div class="rp-card-foot">
              <button class="rp-btn rp-btn-accent-soft rp-btn-sm" onclick={() => useRole(r)}>使用此角色</button>
              <div class="rp-card-ops">
                <button class="rp-icon-btn" onclick={() => openEdit(r)} title="编辑" aria-label="编辑"><PencilIcon class="size-4" /></button>
                <button class="rp-icon-btn rp-icon-danger" onclick={() => deleteRole(r)} title="删除" aria-label="删除"><Trash2Icon class="size-4" /></button>
              </div>
            </div>
          </article>
        </CardSpotlight>
      {/each}
    {/if}
  </div>

  <!-- 编辑器（右侧抽屉） -->
  {#if editing}
    <div class="rp-overlay" onclick={closeEdit} onkeydown={(e) => e.key === 'Escape' && closeEdit()} role="presentation">
      <div class="rp-drawer" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()} role="dialog" aria-modal="true" aria-labelledby="rp-drawer-title" tabindex={-1}>
        <header class="rp-drawer-hd">
          <div class="rp-drawer-icon" aria-hidden="true"><SparklesIcon class="size-5" /></div>
          <div class="rp-drawer-titles">
            <h2 id="rp-drawer-title" class="rp-drawer-title">{editing.id ? '编辑角色' : '新建角色'}</h2>
            <p class="rp-drawer-sub">{editing.id ? '修改会立即持久化到全局角色库' : '创建后可在「全局调用」中检索并使用'}</p>
          </div>
          <button class="rp-icon-btn" onclick={closeEdit} title="关闭" aria-label="关闭" disabled={saving}><XIcon class="size-4" /></button>
        </header>

        <div class="rp-drawer-body">
          {#if formErr}<div class="rp-form-err"><AlertTriangleIcon class="size-4" /> {formErr}</div>{/if}

          <!-- 基本信息 -->
          <section class="rp-sec">
            <h3 class="rp-sec-title"><span>①</span> 基本信息</h3>
            <div class="rp-identity">
              <div class="rp-emoji-current">{editing.emoji || '🤖'}</div>
              <div class="rp-identity-main">
                <div class="rp-field">
                  <label class="rp-cap" for="rp-name">角色名称 <span class="rp-req">*</span></label>
                  <div class="rp-name-row">
                    <input id="rp-name" class="rp-input" type="text" bind:value={editing.name} placeholder="如：资深客服" disabled={saving} />
                    <!-- 启用开关：角色基础状态，随基本信息一起设置 -->
                    <div class="rp-enable-inline" title={editing.enabled ? '创建后即可在全局调用中检索使用' : '创建后默认停用，需手动启用'}>
                      <button
                        class="rp-switch"
                        class:on={editing.enabled}
                        role="switch"
                        aria-checked={editing.enabled}
                        disabled={saving}
                        onclick={() => { if (!saving && editing) editing.enabled = !editing.enabled; }}
                        aria-label={editing.enabled ? '点击停用' : '点击启用'}
                      ><span class="rp-switch-knob" aria-hidden="true"></span></button>
                      <span class="rp-enable-label">{editing.enabled ? '启用' : '停用'}</span>
                    </div>
                  </div>
                </div>
                <div class="rp-field">
                  <label class="rp-cap" for="rp-desc">一句话描述 <span class="rp-hint">可选</span></label>
                  <input id="rp-desc" class="rp-input" type="text" bind:value={editing.description} placeholder="简短说明角色定位与用途" disabled={saving} />
                </div>
              </div>
            </div>
            <div class="rp-field">
              <span class="rp-cap">选择图标</span>
              <div class="rp-emoji-list">
                {#each emojiPresets as e}
                  <button class="rp-emoji-opt" class:sel={editing?.emoji === e} onclick={() => { if (editing) editing.emoji = e; }} type="button" title={e}>{e}</button>
                {/each}
              </div>
            </div>
          </section>

          <!-- 提示词 -->
          <section class="rp-sec">
            <h3 class="rp-sec-title" title="系统提示词（核心）：定义角色身份、行为准则、语气、输出要求。每次调用都会注入。"><span>②</span> 系统提示词（核心）</h3>
            <div class="rp-field">
              <label class="rp-cap" for="rp-sys" title="系统提示词：角色定义、行为准则、语气与输出要求。每次调用都会注入。">系统提示词</label>
              <textarea id="rp-sys" class="rp-input rp-textarea" rows="6"
                bind:value={editing.system_prompt}
                placeholder="角色定义、行为准则、语气、输出要求…"
                disabled={saving}></textarea>
              <div class="rp-field-foot">
                <button class="rp-btn rp-btn-ghost rp-btn-sm" type="button" onclick={() => (previewOpen = !previewOpen)}>
                  {previewOpen ? '收起预览' : '预览合成结果'}
                </button>
              </div>
            </div>
            <div class="rp-field">
              <label class="rp-cap" for="rp-know" title="背景知识：长期资料/行业上下文，每次调用注入。如产品手册、行业术语表等。">背景知识 <span class="rp-hint">可选</span></label>
              <textarea id="rp-know" class="rp-input rp-textarea" rows="3"
                bind:value={editing.knowledge_context}
                placeholder="长期背景、行业资料… 每次调用注入"
                disabled={saving}></textarea>
            </div>
          </section>

          <!-- 行为约束 + 能力标签 -->
          <section class="rp-sec">
            <h3 class="rp-sec-title"><span>③</span> 行为约束 & 能力标签 <span class="rp-hint">可选</span></h3>
            <div class="rp-chips-wrap">
              <div class="rp-field">
                <span class="rp-cap" title="为角色添加行为规则。例如「不回答政治问题」「回答需引用来源」。每条独立，回车或点添加提交。">行为约束</span>
                <div class="rp-chips">
                  {#each editing.behavior_constraints as c, i}
                    <span class="rp-chip rp-chip-warn">{c}<button type="button" onclick={() => removeConstraint(i)} disabled={saving} aria-label="删除">×</button></span>
                  {/each}
                  {#if editing.behavior_constraints.length === 0}
                    <span class="rp-chips-example">示例：回答需引用来源 · 不透露用户隐私</span>
                  {/if}
                </div>
                <div class="rp-addrow">
                  <input class="rp-input" type="text" bind:value={newConstraint} placeholder="输入约束，回车或点添加" onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); addConstraint(); } }} disabled={saving} />
                  <button class="rp-btn rp-btn-default rp-btn-sm" type="button" onclick={addConstraint} disabled={saving}>添加</button>
                </div>
              </div>
              <div class="rp-field">
                <span class="rp-cap" title="标记角色擅长的能力，如「写作 / 翻译 / 数据分析」。便于全局调用时按能力筛选。">能力标签</span>
                <div class="rp-chips">
                  {#each editing.capabilities as c, i}
                    <span class="rp-chip rp-chip-accent">{c}<button type="button" onclick={() => removeCap(i)} disabled={saving} aria-label="删除">×</button></span>
                  {/each}
                  {#if editing.capabilities.length === 0}
                    <span class="rp-chips-example">示例：写作 · 翻译 · 数据分析</span>
                  {/if}
                </div>
                <div class="rp-addrow">
                  <input class="rp-input" type="text" bind:value={newCap} placeholder="输入能力，回车或点添加" onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); addCap(); } }} disabled={saving} />
                  <button class="rp-btn rp-btn-default rp-btn-sm" type="button" onclick={addCap} disabled={saving}>添加</button>
                </div>
              </div>
            </div>
          </section>

          <!-- 采样参数 -->
          <details class="rp-sec" open={samplingOpen} ontoggle={(e) => (samplingOpen = (e.target as HTMLDetailsElement).open)}>
            <summary class="rp-sec-title" title="进阶调参：控制模型生成的随机性、采样范围、重复惩罚与输出上限。一般保持默认即可。"><span>④</span> 采样参数预设
              {#if !samplingOpen}<span class="rp-summary-val">温度 {Number(editing.temperature ?? 0).toFixed(2)} · Top P {Number(editing.top_p ?? 0).toFixed(2)} · Max {editing.max_tokens}</span>{/if}
            </summary>
            <div class="rp-grid-2">
              <div class="rp-slider">
                <div class="rp-slider-hd"><label class="rp-cap" for="rp-temp" title="温度：控制回复随机性。0=稳定可重复；1=最大多样性。默认 0.7 平衡创造与连贯。">温度 <span class="rp-hint">0–1</span></label><span class="rp-slider-val">{Number(editing.temperature ?? 0).toFixed(2)}</span></div>
                <input id="rp-temp" type="range" min="0" max="1" step="0.05" bind:value={editing.temperature} disabled={saving} />
              </div>
              <div class="rp-slider">
                <div class="rp-slider-hd"><label class="rp-cap" for="rp-topp" title="Top P（核采样）：只从累计概率达到 P 的候选词中挑选。1=全词表；0.9=聚焦高频词。">Top P <span class="rp-hint">0–1</span></label><span class="rp-slider-val">{Number(editing.top_p ?? 0).toFixed(2)}</span></div>
                <input id="rp-topp" type="range" min="0" max="1" step="0.05" bind:value={editing.top_p} disabled={saving} />
              </div>
              <div class="rp-slider">
                <div class="rp-slider-hd"><label class="rp-cap" for="rp-pres" title="存在惩罚：已出现过的 token 在后续生成中被惩罚的程度。正数=鼓励模型不重复同一主题。">存在惩罚 <span class="rp-hint">-2–2</span></label><span class="rp-slider-val">{Number(editing.presence_penalty ?? 0).toFixed(2)}</span></div>
                <input id="rp-pres" type="range" min="-2" max="2" step="0.1" bind:value={editing.presence_penalty} disabled={saving} />
              </div>
              <div class="rp-slider">
                <div class="rp-slider-hd"><label class="rp-cap" for="rp-freq" title="频率惩罚：按 token 出现频率惩罚。正数=降低重复词频，适合需要多样化表达的场景。">频率惩罚 <span class="rp-hint">-2–2</span></label><span class="rp-slider-val">{Number(editing.frequency_penalty ?? 0).toFixed(2)}</span></div>
                <input id="rp-freq" type="range" min="-2" max="2" step="0.1" bind:value={editing.frequency_penalty} disabled={saving} />
              </div>
              <div class="rp-slider rp-slider-wide">
                <div class="rp-slider-hd"><label class="rp-cap" for="rp-max" title="Max Tokens：单次回复的最大 token 数。超出将被截断。">Max Tokens</label><span class="rp-slider-val">{editing.max_tokens}</span></div>
                <input id="rp-max" type="range" min="256" max="8192" step="256" bind:value={editing.max_tokens} disabled={saving} />
              </div>
            </div>
          </details>

          <!-- 路由偏好 -->
          <details class="rp-sec" open={routeOpen} ontoggle={(e) => (routeOpen = (e.target as HTMLDetailsElement).open)}>
            <summary class="rp-sec-title" title="路由偏好：指定回复语言、绑定特定 LLM 提供方/模型。留空走全局默认。"><span>⑤</span> 路由偏好 <span class="rp-hint">可选</span>
              {#if !routeOpen}<span class="rp-summary-val">{editing.response_language || '跟随用户'}{editing.preferred_model ? ` · ${editing.preferred_model}` : ' · 全局默认'}</span>{/if}
            </summary>
            <div class="rp-grid-2">
              <div class="rp-field">
                <label class="rp-cap" for="rp-lang" title="回复语言：指定模型输出语言。「跟随用户」=匹配用户输入语言。">回复语言</label>
                <NativeSelect id="rp-lang" wrapperClass="w-full" bind:value={editing.response_language} disabled={saving}>
                  {#each langOptions as opt}
                    <NativeSelectOption value={opt}>{opt}</NativeSelectOption>
                  {/each}
                </NativeSelect>
              </div>
              <div class="rp-field"></div>
              <div class="rp-field">
                <label class="rp-cap" for="rp-prov" title="偏好提供方：固定 LLM 提供商（如 DeepSeek / OpenAI）。留空走全局默认。">偏好提供方</label>
                <input id="rp-prov" class="rp-input" type="text" bind:value={editing.preferred_provider_name} placeholder="留空则使用全局默认" disabled={saving} />
              </div>
              <div class="rp-field">
                <label class="rp-cap" for="rp-model" title="偏好模型：在已选提供方下，固定具体模型名（如 deepseek-chat）。留空走默认。">偏好模型</label>
                <input id="rp-model" class="rp-input" type="text" bind:value={editing.preferred_model} placeholder="留空则使用默认" disabled={saving} />
              </div>
            </div>
          </details>

          <!-- 提示词预览已移至抽屉左侧悬浮弹窗 -->
        </div>

        <footer class="rp-drawer-ft">
          {#if !editing.name.trim()}
            <span class="rp-ft-hint">请先填写角色名称（标注 * 的必填项）</span>
          {/if}
          <button class="rp-btn rp-btn-default" type="button" onclick={closeEdit} disabled={saving}>取消</button>
          <button class="rp-btn rp-btn-primary" type="button" onclick={saveEdit} disabled={saving || !editing.name.trim()}>
            {saving ? '保存中…' : (editing.id ? '保存修改' : '创建角色')}
          </button>
        </footer>
      </div>

      <!-- 系统提示词预览：悬浮在抽屉左侧 -->
      <!-- 系统提示词预览：默认收起，点击「预览合成结果」展开 -->
      {#if previewOpen}
        <div class="rp-preview-pop" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()} role="dialog" tabindex="-1" aria-label="系统提示词预览">
          <div class="rp-pp-hd">
            <div class="rp-pp-title"><span>🧩</span> 系统提示词预览</div>
            <button class="rp-btn rp-btn-ghost rp-btn-sm" type="button" onclick={copyPrompt}>复制</button>
          </div>
          <pre class="rp-pp-box">{promptPreview || '（填写系统提示词 / 约束 / 背景知识后，将在此合成为最终 system prompt）'}</pre>
        </div>
      {/if}
    </div>
  {/if}

  {#if toast.state.text}
    <div class="rp-toast" role="status">{toast.state.text}</div>
  {/if}
</div>

<style>
  /* ── 主题令牌（自动适配 Control 的浅/深主题） ── */
  .rp {
    --rp-bg: var(--app-bg-color, #f0f2f5);
    --rp-surface: var(--app-color-card-bg, #ffffff);
    --rp-surface-alt: var(--app-color-surface-alt, #f8fafc);
    --rp-text: var(--app-color-text, #0f172a);
    --rp-secondary: var(--app-color-secondary, #475569);
    --rp-muted: var(--app-color-muted, #94a3b8);
    --rp-border: var(--app-color-border, #e2e8f0);
    --rp-border-light: var(--app-color-border-light, #f1f5f9);
    --rp-hover: var(--app-color-hover-bg, #f8fafc);
    --rp-input-border: var(--app-color-input-border, #cbd5e1);
    /* 仅使用主题背景衍生色：所有强调/高亮通过 background + text 混合过渡实现 */
    --rp-em: color-mix(in srgb, var(--rp-bg) 72%, var(--rp-text));
    --rp-em-strong: color-mix(in srgb, var(--rp-bg) 55%, var(--rp-text));
    --rp-em-soft: color-mix(in srgb, var(--rp-bg) 92%, var(--rp-text));
    /* 启用/激活态用品牌青色（与首页、全站一致） */
    --rp-accent: var(--brand, #22d3ee);

    display: flex;
    flex-direction: column;
    gap: 14px;
    height: 100%;
    color: var(--rp-text);
    font-family: -apple-system, "PingFang SC", "Microsoft YaHei", "Helvetica Neue", sans-serif;
  }

  /* ── 按钮 ── */
  .rp-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    border: 1px solid transparent;
    border-radius: 8px;
    padding: 9px 16px;
    min-height: 38px;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s ease;
    white-space: nowrap;
    font-family: inherit;
    background: transparent;
    color: inherit;
  }
  .rp-btn-sm { padding: 5px 10px; font-size: 12px; border-radius: 7px; }
  .rp-btn-primary {
    background: linear-gradient(135deg, var(--rp-em), color-mix(in srgb, var(--rp-em) 65%, #fff));
    color: #fff;
    box-shadow: 0 2px 8px color-mix(in srgb, var(--rp-em) 25%, transparent);
  }
  .rp-btn-primary:hover:not(:disabled) { filter: brightness(1.06); transform: translateY(-1px); box-shadow: 0 4px 14px color-mix(in srgb, var(--rp-em) 32%, transparent); }
  .rp-btn-default { background: var(--rp-surface); border-color: var(--rp-border); color: var(--rp-text); }
  .rp-btn-default:hover:not(:disabled) { border-color: var(--rp-em); color: var(--rp-em); }
  .rp-btn-ghost { background: transparent; color: var(--rp-secondary); border-color: var(--rp-border); }
  .rp-btn-ghost:hover:not(:disabled) { color: var(--rp-em); border-color: var(--rp-em); }
  .rp-btn-ico { padding: 8px; }
  .rp-ico { display: block; flex-shrink: 0; }
  .rp-btn-accent-soft {
    background: color-mix(in srgb, var(--rp-accent) 18%, transparent);
    color: color-mix(in srgb, var(--rp-accent) 65%, var(--rp-text));
    border-color: color-mix(in srgb, var(--rp-accent) 45%, transparent);
    font-weight: 700;
    box-shadow: 0 1px 4px color-mix(in srgb, var(--rp-accent) 20%, transparent);
  }
  .rp-btn-accent-soft:hover:not(:disabled) {
    background: color-mix(in srgb, var(--rp-accent) 26%, transparent);
    border-color: var(--rp-accent);
    box-shadow: 0 2px 8px color-mix(in srgb, var(--rp-accent) 32%, transparent);
    transform: translateY(-1px);
  }
  .rp-btn:disabled { opacity: 0.55; cursor: not-allowed; }

  /* ── 页头 ── */
  .rp-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
    background: var(--rp-surface);
    border: 1px solid var(--rp-border);
    border-radius: 14px;
    padding: 16px 20px;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.04);
  }
  .rp-head-info { display: flex; align-items: center; gap: 14px; min-width: 0; }
  .rp-head-logo {
    width: 44px; height: 44px;
    display: grid; place-items: center;
    font-size: 22px;
    border-radius: 12px;
    background: linear-gradient(135deg, var(--rp-em), color-mix(in srgb, var(--rp-em) 55%, #fff));
    color: #fff;
    box-shadow: 0 4px 10px color-mix(in srgb, var(--rp-em) 30%, transparent);
    flex-shrink: 0;
  }
  .rp-title { margin: 0; font-size: 16px; font-weight: 700; letter-spacing: -0.01em; }
  .rp-subtitle { margin: 2px 0 0; font-size: 12px; color: var(--rp-muted); }
  .rp-head-actions { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
  /* 刷新图标按钮与搜索框同高，视觉一致 */
  .rp-head-actions .rp-btn-ico { height: 36px; width: 36px; }

  .rp-search {
    display: flex; align-items: center; gap: 6px;
    background: var(--rp-bg);
    border: 1px solid var(--rp-border);
    border-radius: 9px;
    padding: 0 10px;
    height: 36px;
    min-width: 240px;
  }
  .rp-search:focus-within { border-color: var(--rp-em); background: var(--rp-surface); box-shadow: 0 0 0 3px color-mix(in srgb, var(--rp-em) 14%, transparent); }
  .rp-search-ico { font-size: 12px; opacity: 0.6; }
  .rp-search input { border: none; background: transparent; outline: none; font-size: 13px; flex: 1; min-width: 0; color: var(--rp-text); }
  .rp-search-clear { background: none; border: none; color: var(--rp-muted); font-size: 16px; cursor: pointer; padding: 0 4px; line-height: 1; }
  .rp-search-clear:hover { color: var(--rp-text); }

  /* 角色计数：标题旁的统计徽章，与操作区分离 */
  .rp-count {
    font-size: 12px; font-weight: 600;
    color: var(--rp-em);
    background: color-mix(in srgb, var(--rp-em) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--rp-em) 30%, transparent);
    border-radius: 999px;
    padding: 2px 10px;
    align-self: flex-start;
    white-space: nowrap;
  }
  .rp-count b { color: var(--rp-em); font-weight: 700; }
  .rp-count i { font-style: normal; color: var(--rp-muted); font-size: 12px; }

  /* ── 卡片网格 ── */
  /* 与首页功能卡一致：默认一行 4 张，随分辨率降档 4→3→2→1 */
  .rp-grid {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 14px;
    align-content: start;
  }
  .rp-state {
    grid-column: 1 / -1;
    background: var(--rp-surface);
    border: 1px solid var(--rp-border);
    border-radius: 14px;
    padding: 60px 20px;
    display: flex; flex-direction: column; align-items: center; gap: 10px;
    text-align: center;
    color: var(--rp-muted);
  }
  .rp-state-orb {
    width: 72px; height: 72px;
    display: grid; place-items: center;
    font-size: 36px;
    border-radius: 20px;
    background: linear-gradient(135deg, var(--rp-em), color-mix(in srgb, var(--rp-em) 55%, #fff));
    color: #fff;
    box-shadow: 0 8px 20px color-mix(in srgb, var(--rp-em) 28%, transparent);
    margin-bottom: 6px;
  }
  .rp-state h3 { margin: 0; font-size: 16px; font-weight: 700; color: var(--rp-text); }
  .rp-state p { margin: 0; font-size: 13px; line-height: 1.6; }

  .rp-spinner {
    width: 36px; height: 36px; border-radius: 50%;
    border: 3px solid var(--rp-border-light);
    border-top-color: var(--rp-em);
    animation: rp-spin 0.9s linear infinite;
  }
  @keyframes rp-spin { to { transform: rotate(360deg); } }

  .rp-card {
    background: var(--rp-surface);
    border: 1px solid var(--rp-border);
    border-radius: 14px;
    padding: 18px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    transition: all 0.18s ease;
    min-height: 200px;
    width: 100%;
    height: 100%;
    position: relative;
    overflow: hidden;
    /* 显式主题前景色：CardSpotlight 外壳带 dark:text-white，
       未显式设色的文字（如角色名）会继承错色 */
    color: var(--rp-text);
  }
  /* 悬停扫光（与首页功能卡一致） */
  .rp-card::after {
    content: '';
    position: absolute;
    inset: 0;
    pointer-events: none;
    background: linear-gradient(115deg, transparent 32%, color-mix(in srgb, var(--rp-em) 14%, transparent) 50%, transparent 68%);
    transform: translateX(-110%);
    transition: transform 0.6s ease;
  }
  .rp-card:hover::after {
    transform: translateX(110%);
  }
  /* CardSpotlight 外壳：子组件渲染，作用域规则打不中——全 :global 中和
     其硬编码背景，并让内容容器与卡片撑满单元格 */
  :global(.rp-spot) {
    background: transparent;
    border: none;
    border-radius: 14px;
    padding: 0;
  }
  :global(.rp-spot .relative) {
    width: 100%;
    height: 100%;
    display: flex;
  }
  .rp-card:hover {
    border-color: color-mix(in srgb, var(--rp-em) 50%, var(--rp-border));
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.06), 0 0 0 1px color-mix(in srgb, #22d3ee 12%, transparent), 0 12px 30px -20px rgba(34, 211, 238, 0.35);
    transform: translateY(-1px);
  }

  .rp-card-top {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 48px;
  }
  .rp-card-avatar {
    width: 48px; height: 48px;
    display: grid; place-items: center;
    font-size: 24px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--rp-em) 10%, var(--rp-bg));
    border: 1px solid var(--rp-border-light);
    flex-shrink: 0;
  }
  .rp-pill {
    font-size: 11.5px; font-weight: 600;
    padding: 3px 10px; border-radius: 999px;
    background: color-mix(in srgb, var(--rp-muted) 15%, transparent);
    color: var(--rp-secondary);
    white-space: nowrap;
  }
  .rp-pill.on {
    background: color-mix(in srgb, var(--rp-accent) 16%, transparent);
    color: color-mix(in srgb, var(--rp-accent) 70%, var(--rp-text));
    border: 1px solid color-mix(in srgb, var(--rp-accent) 40%, transparent);
  }

  .rp-card-body { flex: 1; min-width: 0; }
  .rp-card-name {
    margin: 0 0 4px;
    font-size: 14px; font-weight: 700;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .rp-card-desc {
    margin: 0;
    font-size: 12.5px;
    color: var(--rp-muted);
    line-height: 1.55;
    display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .rp-card-tags { display: flex; flex-wrap: wrap; gap: 5px; margin-top: 10px; }
  .rp-tag {
    font-size: 11.5px;
    padding: 2px 8px; border-radius: 6px;
    background: color-mix(in srgb, var(--rp-em) 10%, transparent);
    color: var(--rp-em);
    font-weight: 600;
  }
  .rp-tag-more { background: var(--rp-bg); color: var(--rp-muted); }

  .rp-card-foot {
    display: flex; align-items: center; justify-content: space-between;
    padding-top: 12px;
    border-top: 1px solid var(--rp-border-light);
    gap: 8px;
  }
  .rp-card-ops { display: flex; gap: 8px; }
  .rp-icon-btn {
    width: 32px; height: 32px;
    display: inline-flex; align-items: center; justify-content: center;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 8px;
    cursor: pointer;
    color: var(--rp-muted);
    font-size: 14px;
    transition: all 0.12s ease;
  }
  .rp-icon-btn:hover:not(:disabled) {
    background: var(--rp-hover);
    border-color: var(--rp-border);
    color: var(--rp-em);
  }
  .rp-icon-btn.rp-icon-danger:hover:not(:disabled) {
    color: #f87171;
    border-color: color-mix(in srgb, #ef4444 35%, transparent);
    background: color-mix(in srgb, #ef4444 10%, transparent);
  }
  .rp-icon-btn:disabled { opacity: 0.5; cursor: not-allowed; }

  /* ── 开关 ── */
  .rp-switch {
    width: 40px; height: 22px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--rp-muted) 40%, var(--rp-border));
    border: none; cursor: pointer; padding: 0;
    position: relative;
    transition: background 0.18s ease;
    flex-shrink: 0;
    margin-left: auto;
  }
  .rp-switch-knob {
    position: absolute; top: 2px; left: 2px;
    width: 18px; height: 18px; border-radius: 50%; background: #fff;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.25);
    transition: transform 0.18s ease;
  }
  .rp-switch.on { background: var(--rp-accent); box-shadow: 0 0 10px color-mix(in srgb, var(--rp-accent) 45%, transparent); }
  .rp-switch.on .rp-switch-knob { transform: translateX(18px); }
  .rp-switch:disabled { opacity: 0.55; cursor: not-allowed; }

  /* ── 抽屉 ── */
  .rp-overlay {
    position: fixed; left: 0; right: 0; bottom: 0; top: 33px; z-index: 9998;
    background: rgba(15, 23, 42, 0.45);
    display: flex; justify-content: flex-end;
    animation: rp-fadein 0.2s ease;
  }
  @keyframes rp-fadein { from { opacity: 0; } to { opacity: 1; } }
  .rp-drawer {
    width: min(640px, 100%);
    height: 100%;
    background: var(--rp-bg);
    border-left: 1px solid var(--rp-border);
    display: flex; flex-direction: column;
    box-shadow: -16px 0 40px rgba(0, 0, 0, 0.18);
    animation: rp-slidein 0.28s cubic-bezier(0.21, 1.02, 0.73, 1);
  }
  @keyframes rp-slidein { from { transform: translateX(100%); } to { transform: translateX(0); } }

  .rp-drawer-hd {
    display: flex; align-items: center; gap: 12px;
    padding: 16px 20px;
    background: var(--rp-surface);
    border-bottom: 1px solid var(--rp-border);
    flex-shrink: 0;
  }
  .rp-drawer-icon {
    width: 36px; height: 36px;
    display: grid; place-items: center;
    font-size: 16px;
    border-radius: 10px;
    background: linear-gradient(135deg, var(--rp-em), color-mix(in srgb, var(--rp-em) 60%, #fff));
    color: #fff;
    flex-shrink: 0;
  }
  .rp-drawer-titles { flex: 1; min-width: 0; }
  .rp-drawer-title { margin: 0; font-size: 16px; font-weight: 700; }
  .rp-drawer-sub { margin: 2px 0 0; font-size: 12px; color: var(--rp-muted); }

  .rp-drawer-body {
    flex: 1; overflow-y: auto;
    padding: 22px 24px;
    display: flex; flex-direction: column; gap: 18px;
  }

  .rp-drawer-ft {
    display: flex; justify-content: flex-end; gap: 10px;
    padding: 14px 20px;
    background: var(--rp-surface);
    border-top: 1px solid var(--rp-border);
    flex-shrink: 0;
  }

  /* ── 表单 ── */
  .rp-form-err {
    background: color-mix(in srgb, var(--rp-em) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--rp-em) 35%, transparent);
    color: color-mix(in srgb, var(--rp-em) 60%, var(--rp-text));
    border-radius: 10px; padding: 9px 14px; font-size: 12.5px;
  }

  .rp-sec {
    background: var(--rp-surface);
    border: 1px solid var(--rp-border);
    border-radius: 12px;
    padding: 14px 16px;
  }
  .rp-sec-title {
    display: flex; align-items: center; gap: 8px;
    font-size: 14px; font-weight: 700; color: var(--rp-text);
    margin: 0 0 12px;
  }
  .rp-sec-title span {
    display: inline-grid; place-items: center;
    width: 22px; height: 22px; border-radius: 7px;
    background: color-mix(in srgb, var(--rp-em) 12%, transparent);
    color: var(--rp-em);
    font-size: 12px; font-weight: 700;
  }
  /* 折叠分区的当前值摘要 */
  .rp-summary-val {
    margin-left: 8px;
    font-size: 11.5px;
    font-weight: 500;
    color: var(--rp-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* 名称行：输入框 + 内联启用开关 */
  .rp-name-row {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .rp-name-row .rp-input { flex: 1; min-width: 0; }
  .rp-enable-inline {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }
  .rp-enable-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--rp-secondary);
  }
  /* 页脚置灰提示 */
  .rp-ft-hint {
    margin-right: auto;
    font-size: 12px;
    color: var(--rp-muted);
  }
  /* 标签空态示例 */
  .rp-chips-example {
    font-size: 11.5px;
    color: var(--rp-muted);
    padding: 3px 2px;
    opacity: 0.8;
  }

  /* 可折叠分区（<details>） */
  details.rp-sec > summary.rp-sec-title {
    margin: 0; cursor: pointer; list-style: none; user-select: none;
  }
  details.rp-sec > summary.rp-sec-title::-webkit-details-marker { display: none; }
  details.rp-sec[open] > summary.rp-sec-title { margin: 0 0 12px; }
  details.rp-sec > summary.rp-sec-title::after {
    content: '▸'; margin-left: auto; color: var(--rp-muted);
    font-size: 12px; transition: transform 0.15s ease;
  }
  details.rp-sec[open] > summary.rp-sec-title::after { transform: rotate(90deg); }

  .rp-field { display: flex; flex-direction: column; gap: 6px; min-width: 0; margin-bottom: 10px; }
  .rp-field:last-child { margin-bottom: 0; }
  .rp-cap { font-size: 12px; font-weight: 600; color: var(--rp-secondary); }
  .rp-req { color: var(--rp-em); margin-left: 2px; }
  .rp-hint { font-weight: 400; color: var(--rp-muted); font-size: 11.5px; margin-left: 4px; white-space: nowrap; }

  .rp-input, .rp-textarea {
    width: 100%; box-sizing: border-box;
    border: 1px solid var(--rp-input-border);
    border-radius: 9px;
    padding: 9px 11px;
    font-size: 13px;
    color: var(--rp-text);
    background: var(--rp-surface);
    outline: none;
    font-family: inherit;
    transition: border-color 0.15s ease, box-shadow 0.15s ease;
  }
  .rp-input:focus, .rp-textarea:focus {
    border-color: var(--rp-em);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--rp-em) 14%, transparent);
  }
  .rp-input:disabled, .rp-textarea:disabled { opacity: 0.6; cursor: not-allowed; }
  .rp-textarea { resize: vertical; line-height: 1.55; }
  .rp-identity { display: flex; gap: 14px; align-items: flex-start; margin-bottom: 14px; }
  .rp-identity-main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 12px; }
  .rp-emoji-current {
    width: 52px; height: 52px; flex-shrink: 0;
    display: grid; place-items: center; font-size: 28px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--rp-em) 10%, var(--rp-bg));
    border: 1px solid var(--rp-border);
  }
  .rp-emoji-list { display: flex; flex-wrap: wrap; gap: 6px; }
  .rp-emoji-opt {
    width: 34px; height: 34px; border-radius: 9px;
    border: 1px solid var(--rp-border);
    background: var(--rp-surface);
    cursor: pointer; font-size: 18px;
    transition: all 0.12s ease;
    color: var(--rp-text);
  }
  .rp-emoji-opt:hover { transform: scale(1.08); border-color: var(--rp-em); }
  .rp-emoji-opt.sel {
    border-color: var(--rp-accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--rp-accent) 35%, transparent);
    background: color-mix(in srgb, var(--rp-accent) 14%, transparent);
    transform: scale(1.1);
  }

  .rp-chips-wrap { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
  .rp-chips { display: flex; flex-wrap: wrap; gap: 6px; min-height: 2px; align-content: flex-start; margin-bottom: 0; }
  .rp-chip {
    display: inline-flex; align-items: center; gap: 5px;
    padding: 4px 6px 4px 10px;
    border-radius: 7px;
    font-size: 12px; font-weight: 600;
    border: 1px solid transparent;
  }
  .rp-chip-warn {
    background: color-mix(in srgb, var(--rp-em) 10%, transparent);
    border-color: color-mix(in srgb, var(--rp-em) 25%, transparent);
    color: color-mix(in srgb, var(--rp-em) 65%, var(--rp-text));
  }
  .rp-chip-accent {
    background: color-mix(in srgb, var(--rp-em) 10%, transparent);
    border-color: color-mix(in srgb, var(--rp-em) 22%, transparent);
    color: var(--rp-em);
  }
  .rp-chip button {
    border: none; background: rgba(0, 0, 0, 0.06);
    color: inherit;
    width: 16px; height: 16px; border-radius: 50%;
    cursor: pointer; font-size: 12px; line-height: 1;
    display: grid; place-items: center;
  }
  .rp-chip button:hover { background: rgba(0, 0, 0, 0.14); }
  .rp-chip button:disabled { opacity: 0.5; cursor: not-allowed; }
  .rp-addrow { display: flex; gap: 6px; }
  .rp-addrow .rp-input { flex: 1; }

  .rp-grid-2 { display: grid; grid-template-columns: 1fr 1fr; gap: 12px 14px; }
  .rp-slider { display: flex; flex-direction: column; gap: 4px; }
  .rp-slider-wide { grid-column: 1 / -1; }
  .rp-slider-hd { display: flex; align-items: center; justify-content: space-between; }
  .rp-slider-val {
    font-size: 11.5px; font-weight: 700; color: var(--rp-em);
    background: color-mix(in srgb, var(--rp-em) 10%, transparent);
    padding: 1px 8px; border-radius: 6px;
    font-variant-numeric: tabular-nums;
  }
  .rp-slider input[type='range'] {
    width: 100%; accent-color: var(--rp-em); cursor: pointer; height: 4px;
  }

  /* 字段底部的辅助操作行（如提示词预览按钮） */
  .rp-field-foot {
    display: flex;
    justify-content: flex-end;
    margin-top: 6px;
  }

  /* 悬浮预览弹窗：位于抽屉左侧 */
  .rp-preview-pop {
    position: absolute;
    top: 24px;
    right: calc(min(640px, 100vw) + 24px);
    width: 360px;
    max-width: calc(100vw - min(640px, 100vw) - 48px);
    max-height: calc(100vh - 48px);
    display: flex; flex-direction: column;
    background: var(--rp-surface);
    border: 1px solid var(--rp-border);
    border-radius: 12px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.28);
    overflow: hidden;
    z-index: 1;
    animation: rp-popin 0.28s cubic-bezier(0.21, 1.02, 0.73, 1);
  }
  @keyframes rp-popin {
    from { opacity: 0; transform: translateX(12px); }
    to { opacity: 1; transform: translateX(0); }
  }
  .rp-pp-hd {
    display: flex; align-items: center; justify-content: space-between; gap: 8px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--rp-border);
    background: var(--rp-surface);
  }
  .rp-pp-title {
    display: flex; align-items: center; gap: 8px;
    font-size: 14px; font-weight: 700; color: var(--rp-text);
  }
  .rp-pp-title span {
    display: inline-grid; place-items: center;
    width: 22px; height: 22px; border-radius: 7px;
    background: color-mix(in srgb, var(--rp-em) 12%, transparent);
    color: var(--rp-em); font-size: 12px;
  }
  .rp-pp-box {
    margin: 0;
    padding: 14px;
    font-size: 12.5px; line-height: 1.65;
    color: var(--rp-text);
    white-space: pre-wrap; word-break: break-word;
    overflow-y: auto;
    font-family: "SFMono-Regular", Consolas, monospace;
  }

  /* ── Toast ── */
  .rp-toast {
    position: fixed; bottom: 24px; left: 50%;
    transform: translateX(-50%);
    background: var(--rp-text); color: var(--rp-surface);
    padding: 10px 20px; border-radius: 10px;
    font-size: 13px; font-weight: 600;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.25);
    z-index: 9999;
    animation: rp-pop 0.22s cubic-bezier(0.21, 1.02, 0.73, 1);
  }
  @keyframes rp-pop {
    from { opacity: 0; transform: translate(-50%, 10px); }
    to { opacity: 1; transform: translate(-50%, 0); }
  }

  /* ── 滚动条 ── */
  .rp-drawer-body::-webkit-scrollbar,
  .rp-pp-box::-webkit-scrollbar { width: 8px; }
  .rp-drawer-body::-webkit-scrollbar-thumb,
  .rp-pp-box::-webkit-scrollbar-thumb {
    background: color-mix(in srgb, var(--rp-muted) 40%, transparent);
    border-radius: 8px;
  }
  .rp-drawer-body::-webkit-scrollbar-thumb:hover,
  .rp-pp-box::-webkit-scrollbar-thumb:hover {
    background: color-mix(in srgb, var(--rp-muted) 60%, transparent);
  }

  /* ── 响应式 ── */
  @media (max-width: 1180px) {
    .rp-grid { grid-template-columns: repeat(3, minmax(0, 1fr)); }
  }
  @media (max-width: 880px) {
    .rp-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  }
  @media (max-width: 760px) {
    .rp-grid { grid-template-columns: 1fr; }
    .rp-chips-wrap, .rp-grid-2 { grid-template-columns: 1fr; }
    .rp-slider-wide { grid-column: auto; }
    .rp-preview-pop { display: none; }
  }
</style>
