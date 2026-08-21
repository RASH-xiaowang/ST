<script lang="ts">
  import { onMount } from 'svelte';
  import type { AiRole } from './roleTypes';
  import { roleApi } from '../communication/roleApi';

  /** 规范化来自后端的角色：将 Option<String> 的 null 转为空串，避免输入框显示 "null" */
  function norm(r: AiRole): AiRole {
    const c = JSON.parse(JSON.stringify(r)) as AiRole;
    c.preferred_provider_name = c.preferred_provider_name ?? '';
    c.preferred_model = c.preferred_model ?? '';
    c.behavior_constraints = c.behavior_constraints ?? [];
    c.capabilities = c.capabilities ?? [];
    return c;
  }

  let roles: AiRole[] = $state([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let search = $state('');
  let toast = $state('');

  // 编辑态
  let editing: AiRole | null = $state(null);
  let editId: string | null = $state(null);
  let newConstraint = $state('');
  let newCapability = $state('');

  const emojiPresets = ['🤖', '🧠', '💡', '📊', '✍️', '🔍', '🛡️', '🎯', '⚖️', '🚀', '👩‍💻', '🧑‍🏫'];
  const langOptions = ['跟随用户', '中文', 'English', '日本語', '한국어'];

  let filteredRoles = $derived(
    (search.trim()
      ? roles.filter(
          (r) =>
            r.name.toLowerCase().includes(search.trim().toLowerCase()) ||
            (r.description || '').toLowerCase().includes(search.trim().toLowerCase())
        )
      : roles)
  );

  let promptPreview = $derived(editing ? composeSystemPrompt(editing) : '');

  let constraintsOpen = $state(true);
  let samplingOpen = $state(false);
  let advancedOpen = $state(false);

  onMount(loadRoles);

  function composeSystemPrompt(role: AiRole): string {
    const sections: string[] = [];
    const prompt = (role.system_prompt || '').trim();
    if (prompt) sections.push(prompt);

    const c = (role.behavior_constraints || []).map((s) => s.trim()).filter((s) => !!s);
    if (c.length) {
      sections.push('【行为约束】\n' + c.map((x) => `- ${x}`).join('\n'));
    }

    const k = (role.knowledge_context || '').trim();
    if (k) sections.push('【背景知识】\n' + k);

    const lang = (role.response_language || '').trim();
    if (lang && lang !== '跟随用户') {
      sections.push(`【回复语言】请使用 ${lang} 回复。`);
    }

    return sections.join('\n\n');
  }

  function showToast(msg: string) {
    toast = msg;
    setTimeout(() => (toast = ''), 2200);
  }

  async function loadRoles() {
    loading = true;
    error = null;
    try {
      const res = await roleApi.list();
      roles = res ?? [];
    } catch (e: any) {
      error = e?.message || '加载失败';
    } finally {
      loading = false;
    }
  }

  function blankRole(): AiRole {
    return {
      id: '',
      name: '',
      emoji: '🤖',
      description: '',
      enabled: true,
      system_prompt: '',
      preferred_provider_name: '',
      preferred_model: '',
      temperature: 0.7,
      max_tokens: 2048,
      top_p: 1,
      presence_penalty: 0,
      frequency_penalty: 0,
      behavior_constraints: [],
      capabilities: [],
      response_language: '跟随用户',
      knowledge_context: '',
      created_at: '',
      updated_at: ''
    };
  }

  function startNew() {
    editing = blankRole();
    editId = null;
  }

  function startEdit(r: AiRole) {
    editing = norm(r);
    editId = r.id;
  }

  function cancelEdit() {
    editing = null;
    editId = null;
    newConstraint = '';
    newCapability = '';
  }

  async function save() {
    if (!editing) return;
    if (!editing.name.trim()) {
      showToast('请填写角色名称');
      return;
    }
    try {
      if (editId) {
        await roleApi.update(editing);
        showToast('已保存');
      } else {
        await roleApi.create(editing);
        showToast('已创建');
      }
      await loadRoles();
      // 保存后停留在同一条，便于继续编辑
      const saved = editId
        ? roles.find((r) => r.id === editId)
        : roles.find((r) => r.name === editing!.name);
      if (saved) {
        editing = norm(saved);
        editId = saved.id;
      } else {
        cancelEdit();
      }
    } catch (e: any) {
      showToast('保存失败：' + (e?.message || ''));
    }
  }

  async function remove(r: AiRole) {
    if (!confirm(`确定删除角色「${r.name}」？`)) return;
    try {
      await roleApi.remove(r.id);
      showToast('已删除');
      if (editId === r.id) cancelEdit();
      await loadRoles();
    } catch (e: any) {
      showToast('删除失败：' + (e?.message || ''));
    }
  }

  function addConstraint() {
    if (!editing) return;
    const v = newConstraint.trim();
    if (!v) return;
    editing.behavior_constraints = [...(editing.behavior_constraints || []), v];
    newConstraint = '';
  }

  function removeConstraint(i: number) {
    if (!editing) return;
    editing.behavior_constraints = (editing.behavior_constraints || []).filter((_, idx) => idx !== i);
  }

  function addCapability() {
    if (!editing) return;
    const v = newCapability.trim();
    if (!v) return;
    editing.capabilities = [...(editing.capabilities || []), v];
    newCapability = '';
  }

  function removeCapability(i: number) {
    if (!editing) return;
    editing.capabilities = (editing.capabilities || []).filter((_, idx) => idx !== i);
  }

  async function copyPrompt() {
    if (!promptPreview) return;
    try {
      await navigator.clipboard.writeText(promptPreview);
      showToast('已复制系统提示词');
    } catch {
      showToast('复制失败');
    }
  }
</script>

<div class="rm">
  <!-- 页头 -->
  <header class="rm-head">
    <div class="rm-head-info">
      <div class="rm-head-logo">🎭</div>
      <div>
        <h1 class="rm-title">AI 角色定位</h1>
        <p class="rm-subtitle">对标大模型 system prompt，定义可复用的 AI 角色，供「全局调用」检索与调度</p>
      </div>
    </div>
    <div class="rm-head-actions">
      <div class="rm-search">
        <span class="rm-search-ico">🔍</span>
        <input placeholder="搜索角色名称或描述…" bind:value={search} />
      </div>
      <span class="rm-count">{filteredRoles.length}<i>/{roles.length}</i></span>
      <button class="btn btn-ghost" onclick={loadRoles} title="刷新">⟳</button>
      <button class="btn btn-primary" onclick={startNew}>＋ 新增角色</button>
    </div>
  </header>

  {#if error}
    <div class="rm-error">⚠️ {error}</div>
  {/if}

  <div class="rm-body">
    <!-- 角色列表 -->
    <aside class="rm-list">
      {#if loading && roles.length === 0}
        <div class="rm-list-empty">加载中…</div>
      {:else if filteredRoles.length === 0}
        <div class="rm-list-empty">
          <div class="rm-list-empty-ico">🗂️</div>
          <p>{search.trim() ? '没有匹配的角色' : '还没有 AI 角色'}</p>
          {#if !search.trim()}
            <button class="btn btn-primary btn-sm" onclick={startNew}>创建第一个角色</button>
          {/if}
        </div>
      {:else}
        {#each filteredRoles as r (r.id)}
          <button
            class="rm-card"
            class:active={editing && editId === r.id}
            onclick={() => startEdit(r)}
          >
            <div class="rm-card-avatar">{r.emoji || '🤖'}</div>
            <div class="rm-card-body">
              <div class="rm-card-top">
                <span class="rm-card-name">{r.name || '未命名角色'}</span>
                <span class="rm-pill" class:on={r.enabled}>{r.enabled ? '已启用' : '已停用'}</span>
              </div>
              <p class="rm-card-desc">{r.description || '暂无描述'}</p>
              {#if r.capabilities && r.capabilities.length}
                <div class="rm-card-tags">
                  {#each r.capabilities.slice(0, 3) as c}
                    <span class="rm-tag">{c}</span>
                  {/each}
                  {#if r.capabilities.length > 3}
                    <span class="rm-tag more">+{r.capabilities.length - 3}</span>
                  {/if}
                </div>
              {/if}
            </div>
          </button>
        {/each}
      {/if}
    </aside>

    <!-- 编辑器 -->
    <section class="rm-main">
      {#if !editing}
        <div class="rm-placeholder">
          <div class="rm-placeholder-ico">🎭</div>
          <h2>设计你的 AI 角色</h2>
          <p>在左侧选择角色进行编辑，或点击右上角「＋ 新增角色」开始创建一个全新的 AI 角色定位。</p>
          <ul class="rm-placeholder-tips">
            <li><span>📝</span> 编写系统提示词与行为约束，精准定义角色性格与边界</li>
            <li><span>🏷️</span> 打上能力标签，方便在「全局调用」中检索复用</li>
            <li><span>🎚️</span> 预设采样参数与偏好模型，统一调度时保持稳定的风格</li>
          </ul>
        </div>
      {:else}
        <!-- 编辑器顶部：实时角色卡预览 + 操作 -->
        <div class="rm-editor-head">
          <div class="rm-preview">
            <div class="rm-preview-avatar">{editing.emoji || '🤖'}</div>
            <div class="rm-preview-meta">
              <div class="rm-preview-name">{editing.name || '未命名角色'}</div>
              <div class="rm-preview-desc">{editing.description || '在右侧填写描述…'}</div>
              <div class="rm-preview-tags">
                {#each editing.capabilities as c}
                  <span class="rm-tag">{c}</span>
                {/each}
              </div>
            </div>
          </div>
          <div class="rm-editor-ops">
            <label class="rm-switch" title="启用后可在全局调用中被检索">
              <input type="checkbox" bind:checked={editing.enabled} />
              <span class="rm-switch-track"><span class="rm-switch-thumb"></span></span>
              <span class="rm-switch-label">{editing.enabled ? '已启用' : '已停用'}</span>
            </label>
            <button class="btn btn-ghost btn-sm" onclick={() => editing && remove(editing)}>删除</button>
            <button class="btn btn-default btn-sm" onclick={cancelEdit}>取消</button>
            <button class="btn btn-primary btn-sm" onclick={save}>保存角色</button>
          </div>
        </div>

        <div class="rm-form">
          <!-- 左列：身份 / 提示词 / 语言 -->
          <div class="rm-col">
            <!-- ① 基础信息 -->
            <section class="rm-sec">
              <div class="rm-sec-title"><span>①</span> 基础信息</div>
              <div class="rm-identity">
                <div class="rm-emoji-current">{editing.emoji || '🤖'}</div>
                <div class="rm-identity-main">
                  <div class="rm-field">
                    <label>
                      <span class="rm-cap">角色名称</span>
                      <input class="rm-input" placeholder="如：严谨的数据分析师" bind:value={editing.name} />
                    </label>
                  </div>
                  <div class="rm-field">
                    <label>
                      <span class="rm-cap">一句话描述</span>
                      <input class="rm-input" placeholder="简短说明这个角色的定位与用途" bind:value={editing.description} />
                    </label>
                  </div>
                </div>
              </div>
              <div class="rm-emoji-list">
                {#each emojiPresets as e}
                  <button class="rm-emoji-opt" class:sel={editing.emoji === e} onclick={() => { if (editing) editing.emoji = e; }}>{e}</button>
                {/each}
              </div>
            </section>

            <!-- ② 系统提示词 -->
            <section class="rm-sec">
              <div class="rm-sec-title"><span>②</span> 系统提示词（核心）</div>
              <div class="rm-field">
                <textarea
                  class="rm-textarea"
                  rows="8"
                  placeholder="定义角色的身份、目标、语气与专业知识。例如：你是一名资深数据分析师，擅长用通俗语言解释复杂指标，回答时先给结论再给依据…"
                  bind:value={editing.system_prompt}
                ></textarea>
              </div>
            </section>

            <!-- ④ 语言与背景知识 -->
            <section class="rm-sec">
              <div class="rm-sec-title"><span>④</span> 语言与背景知识</div>
              <div class="rm-grid">
                <div class="rm-field">
                  <label>
                    <span class="rm-cap">回复语言</span>
                    <select class="rm-input" bind:value={editing.response_language}>
                      {#each langOptions as l}<option value={l}>{l}</option>{/each}
                    </select>
                  </label>
                </div>
                <div class="rm-field grow"></div>
              </div>
              <div class="rm-field">
                <label>
                  <span class="rm-cap">背景知识 / 长期上下文</span>
                  <textarea class="rm-textarea" rows="3" placeholder="可写入该角色需要始终掌握的领域知识、术语表或业务背景（可选）" bind:value={editing.knowledge_context}></textarea>
                </label>
              </div>
            </section>
          </div>

          <!-- 右列：约束 / 采样 / 路由 -->
          <div class="rm-col">
            <!-- ③ 行为约束 & 能力标签 -->
            <details class="rm-sec" open={constraintsOpen} ontoggle={(e) => (constraintsOpen = (e.target as HTMLDetailsElement).open)}>
              <summary class="rm-sec-title"><span>③</span> 行为约束 & 能力标签</summary>
              <div class="rm-grid">
                <div class="rm-field">
                  <span class="rm-cap">行为约束</span>
                  <div class="rm-chips">
                    {#each editing.behavior_constraints as c, i}
                      <span class="rm-chip">{c}<button onclick={() => removeConstraint(i)}>×</button></span>
                    {/each}
                  </div>
                  <div class="rm-addrow">
                    <input class="rm-input" placeholder="如：不编造数据来源" bind:value={newConstraint} onkeydown={(e) => e.key === 'Enter' && addConstraint()} />
                    <button class="btn btn-default btn-sm" onclick={addConstraint}>添加</button>
                  </div>
                </div>
                <div class="rm-field">
                  <span class="rm-cap">能力标签</span>
                  <div class="rm-chips">
                    {#each editing.capabilities as c, i}
                      <span class="rm-chip alt">{c}<button onclick={() => removeCapability(i)}>×</button></span>
                    {/each}
                  </div>
                  <div class="rm-addrow">
                    <input class="rm-input" placeholder="如：数据可视化" bind:value={newCapability} onkeydown={(e) => e.key === 'Enter' && addCapability()} />
                    <button class="btn btn-default btn-sm" onclick={addCapability}>添加</button>
                  </div>
                </div>
              </div>
            </details>

            <!-- ⑤ 采样参数 -->
            <details class="rm-sec" open={samplingOpen} ontoggle={(e) => (samplingOpen = (e.target as HTMLDetailsElement).open)}>
              <summary class="rm-sec-title"><span>⑤</span> 采样参数预设</summary>
              <div class="rm-sliders">
                <div class="rm-slider">
                  <div class="rm-slider-head"><span class="rm-cap">Temperature</span><span class="rm-slider-val">{editing.temperature?.toFixed(2)}</span></div>
                  <input type="range" min="0" max="1" step="0.05" bind:value={editing.temperature} />
                </div>
                <div class="rm-slider">
                  <div class="rm-slider-head"><span class="rm-cap">Top P</span><span class="rm-slider-val">{editing.top_p?.toFixed(2)}</span></div>
                  <input type="range" min="0" max="1" step="0.05" bind:value={editing.top_p} />
                </div>
                <div class="rm-slider">
                  <div class="rm-slider-head"><span class="rm-cap">Presence Penalty</span><span class="rm-slider-val">{editing.presence_penalty?.toFixed(2)}</span></div>
                  <input type="range" min="-2" max="2" step="0.1" bind:value={editing.presence_penalty} />
                </div>
                <div class="rm-slider">
                  <div class="rm-slider-head"><span class="rm-cap">Frequency Penalty</span><span class="rm-slider-val">{editing.frequency_penalty?.toFixed(2)}</span></div>
                  <input type="range" min="-2" max="2" step="0.1" bind:value={editing.frequency_penalty} />
                </div>
                <div class="rm-slider">
                  <div class="rm-slider-head"><span class="rm-cap">Max Tokens</span><span class="rm-slider-val">{editing.max_tokens}</span></div>
                  <input type="range" min="256" max="8192" step="256" bind:value={editing.max_tokens} />
                </div>
              </div>
            </details>

            <!-- ⑥ 偏好路由 -->
            <details class="rm-sec" open={advancedOpen} ontoggle={(e) => (advancedOpen = (e.target as HTMLDetailsElement).open)}>
              <summary class="rm-sec-title"><span>⑥</span> 偏好路由（可选）</summary>
              <div class="rm-grid">
                <div class="rm-field">
                  <label>
                    <span class="rm-cap">偏好提供方</span>
                    <input class="rm-input" placeholder="如：OpenAI（留空则使用全局默认）" bind:value={editing.preferred_provider_name} />
                  </label>
                </div>
                <div class="rm-field">
                  <label>
                    <span class="rm-cap">偏好模型</span>
                    <input class="rm-input" placeholder="如：gpt-4o（留空则使用默认）" bind:value={editing.preferred_model} />
                  </label>
                </div>
              </div>
            </details>
          </div>

          <!-- 系统提示词预览（通栏） -->
          <section class="rm-sec rm-span">
            <div class="rm-sec-title">
              <span>🧩</span> 系统提示词预览
              <button class="btn btn-ghost btn-sm rm-copy" onclick={copyPrompt}>复制</button>
            </div>
            <pre class="rm-preview-box">{promptPreview || '（填写系统提示词 / 约束 / 背景知识后，将在此合成为最终 system prompt）'}</pre>
          </section>
        </div>
      {/if}
    </section>
  </div>

  {#if toast}
    <div class="rm-toast">{toast}</div>
  {/if}
</div>

<style>
  .rm {
    /* 仅使用主题背景衍生色：所有强调/高亮通过 background + text 混合过渡实现 */
    --bg: #f1f5f9;
    --surface: #ffffff;
    --border: #e2e8f0;
    --text: #0f172a;
    --muted: #64748b;
    --muted-2: #94a3b8;
    /* 背景色与文字色按比例混合得到强调色，平滑过渡且无色相偏移 */
    --em: color-mix(in srgb, var(--bg) 72%, var(--text));
    --em-strong: color-mix(in srgb, var(--bg) 55%, var(--text));
    --em-soft: color-mix(in srgb, var(--bg) 92%, var(--text));

    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    gap: 14px;
    color: var(--text);
  }

  /* ── 按钮 ── */
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    border: 1px solid transparent;
    border-radius: 8px;
    padding: 8px 14px;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s ease;
    white-space: nowrap;
    font-family: inherit;
  }
  .btn-sm { padding: 6px 10px; font-size: 12px; border-radius: 7px; }
  .btn-primary {
    background: linear-gradient(135deg, var(--em-strong), color-mix(in srgb, var(--em-strong) 65%, var(--surface)));
    color: #fff;
    box-shadow: 0 2px 6px color-mix(in srgb, var(--em-strong) 25%, transparent);
  }
  .btn-primary:hover { filter: brightness(1.05); transform: translateY(-1px); box-shadow: 0 4px 12px color-mix(in srgb, var(--em-strong) 32%, transparent); }
  .btn-default { background: var(--surface); border-color: var(--border); color: var(--text); }
  .btn-default:hover { border-color: var(--em); color: var(--em); background: var(--em-soft); }
  .btn-ghost { background: transparent; color: var(--muted); border-color: var(--border); }
  .btn-ghost:hover { color: var(--em); border-color: var(--em); background: var(--em-soft); }

  /* ── 头部 ── */
  .rm-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 16px 20px;
    box-shadow: 0 1px 3px rgba(15, 23, 42, 0.04);
  }
  .rm-head-info { display: flex; align-items: center; gap: 14px; }
  .rm-head-logo {
    width: 44px; height: 44px;
    display: grid; place-items: center;
    font-size: 22px;
    border-radius: 12px;
    background: linear-gradient(135deg, var(--em-strong), color-mix(in srgb, var(--em-strong) 65%, var(--surface)));
    box-shadow: 0 4px 10px color-mix(in srgb, var(--em) 30%, transparent);
  }
  .rm-title { margin: 0; font-size: 18px; font-weight: 700; letter-spacing: -0.01em; }
  .rm-subtitle { margin: 2px 0 0; font-size: 12.5px; color: var(--muted); }
  .rm-head-actions { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .rm-search {
    display: flex; align-items: center; gap: 6px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 9px;
    padding: 0 10px;
    height: 36px;
  }
  .rm-search:focus-within { border-color: var(--em); background: var(--surface); box-shadow: 0 0 0 3px color-mix(in srgb, var(--em) 12%, transparent); }
  .rm-search-ico { font-size: 12px; opacity: 0.6; }
  .rm-search input { border: none; background: transparent; outline: none; font-size: 13px; width: 190px; color: var(--text); }
  .rm-count { font-size: 13px; font-weight: 700; color: var(--em); }
  .rm-count i { font-style: normal; font-weight: 500; color: var(--muted-2); font-size: 12px; }

  .rm-error {
    background: var(--em-soft); color: var(--em-strong); border: 1px solid color-mix(in srgb, var(--em) 30%, var(--border));
    border-radius: 10px; padding: 10px 14px; font-size: 13px;
  }

  /* ── 主体双列 ── */
  .rm-body {
    flex: 1; min-height: 0;
    display: grid;
    grid-template-columns: 320px 1fr;
    gap: 14px;
  }

  /* ── 列表 ── */
  .rm-list {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 10px;
    overflow-y: auto;
    display: flex; flex-direction: column; gap: 8px;
  }
  .rm-list-empty {
    margin: auto; text-align: center; color: var(--muted);
    display: flex; flex-direction: column; align-items: center; gap: 12px; padding: 30px 10px;
  }
  .rm-list-empty-ico { font-size: 36px; opacity: 0.7; }
  .rm-list-empty p { margin: 0; font-size: 13px; }

  .rm-card {
    text-align: left;
    display: flex; gap: 12px; align-items: flex-start;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 12px;
    cursor: pointer;
    transition: all 0.15s ease;
    font-family: inherit;
    width: 100%;
  }
  .rm-card:hover { border-color: var(--em); box-shadow: 0 3px 10px rgba(15, 23, 42, 0.06); transform: translateY(-1px); }
  .rm-card.active {
    border-color: var(--em);
    background: var(--em-soft);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--em) 18%, transparent);
  }
  .rm-card-avatar {
    width: 40px; height: 40px; flex-shrink: 0;
    display: grid; place-items: center;
    font-size: 20px; border-radius: 10px;
    background: var(--bg);
  }
  .rm-card.active .rm-card-avatar { background: var(--surface); box-shadow: 0 2px 6px color-mix(in srgb, var(--em) 20%, transparent); }
  .rm-card-body { flex: 1; min-width: 0; }
  .rm-card-top { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .rm-card-name { font-size: 14px; font-weight: 700; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rm-card-desc {
    margin: 4px 0 0; font-size: 12px; color: var(--muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .rm-card-tags { display: flex; flex-wrap: wrap; gap: 5px; margin-top: 8px; }
  .rm-pill {
    font-size: 11px; font-weight: 600; padding: 2px 8px; border-radius: 999px;
    background: var(--bg); color: var(--muted-2); flex-shrink: 0;
  }
  .rm-pill.on { background: var(--em-soft); color: var(--em); }

  .rm-tag {
    font-size: 11px; padding: 2px 8px; border-radius: 6px;
    background: var(--em-soft); color: var(--em); font-weight: 600;
  }
  .rm-tag.more { background: var(--bg); color: var(--muted-2); }

  /* ── 编辑器 ── */
  .rm-main {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 14px;
    overflow-y: auto;
    display: flex; flex-direction: column;
    min-width: 0;
  }
  .rm-editor-head {
    position: sticky; top: 0; z-index: 5;
    display: flex; align-items: center; justify-content: space-between; gap: 12px;
    padding: 14px 18px;
    border-bottom: 1px solid var(--border);
    background: rgba(255, 255, 255, 0.92);
    backdrop-filter: blur(6px);
    flex-wrap: wrap;
  }
  .rm-preview { display: flex; gap: 12px; align-items: center; min-width: 0; }
  .rm-preview-avatar {
    width: 46px; height: 46px; flex-shrink: 0;
    display: grid; place-items: center; font-size: 24px;
    border-radius: 12px; background: var(--em-soft);
    box-shadow: 0 2px 8px color-mix(in srgb, var(--em) 20%, transparent);
  }
  .rm-preview-meta { min-width: 0; }
  .rm-preview-name { font-size: 15px; font-weight: 700; }
  .rm-preview-desc { font-size: 12px; color: var(--muted); margin-top: 2px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 360px; }
  .rm-preview-tags { display: flex; flex-wrap: wrap; gap: 5px; margin-top: 6px; }
  .rm-editor-ops { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }

  /* 开关 */
  .rm-switch { display: inline-flex; align-items: center; gap: 8px; cursor: pointer; user-select: none; }
  .rm-switch input { display: none; }
  .rm-switch-track {
    width: 40px; height: 22px; border-radius: 999px; background: #cbd5e1;
    position: relative; transition: background 0.18s ease;
  }
  .rm-switch-thumb {
    position: absolute; top: 2px; left: 2px;
    width: 18px; height: 18px; border-radius: 50%; background: #fff;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2); transition: transform 0.18s ease;
  }
  .rm-switch input:checked + .rm-switch-track { background: var(--em-strong); }
  .rm-switch input:checked + .rm-switch-track .rm-switch-thumb { transform: translateX(18px); }
  .rm-switch-label { font-size: 12px; color: var(--muted); font-weight: 600; }

  .rm-form {
    padding: 22px 24px;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 18px 20px;
    align-items: start;
  }
  .rm-col { display: flex; flex-direction: column; gap: 18px; min-width: 0; }
  .rm-span { grid-column: 1 / -1; }
  .rm-identity { display: flex; gap: 14px; align-items: flex-start; margin-bottom: 14px; }
  .rm-identity-main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 10px; }

  .rm-sec {
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 14px 16px;
    background: var(--surface);
  }
  .rm-sec[open] { background: var(--surface); }
  .rm-sec-title {
    display: flex; align-items: center; gap: 8px;
    font-size: 14px; font-weight: 700; color: var(--text);
    margin-bottom: 12px;
  }
  .rm-sec-title span {
    display: inline-grid; place-items: center;
    width: 22px; height: 22px; border-radius: 7px;
    background: var(--em-soft); color: var(--em);
    font-size: 12px; font-weight: 700;
  }
  details.rm-sec > summary { list-style: none; cursor: pointer; margin-bottom: 0; }
  details.rm-sec > summary::-webkit-details-marker { display: none; }
  details.rm-sec[open] > summary { margin-bottom: 12px; }
  details.rm-sec > summary::after {
    content: '▸'; margin-left: auto; color: var(--muted-2); transition: transform 0.15s ease; font-size: 12px;
  }
  details.rm-sec[open] > summary::after { transform: rotate(90deg); }

  .rm-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
  .rm-field { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
  .rm-field.grow { grid-column: 1 / -1; }
  .rm-field label { display: flex; flex-direction: column; gap: 6px; }
  .rm-cap { font-size: 12px; font-weight: 600; color: var(--muted); }

  .rm-input, .rm-textarea {
    width: 100%; box-sizing: border-box;
    border: 1px solid var(--border); border-radius: 9px;
    padding: 9px 11px; font-size: 13px; color: var(--text);
    background: var(--surface); outline: none; font-family: inherit;
    transition: border-color 0.15s ease, box-shadow 0.15s ease;
  }
  .rm-input:focus, .rm-textarea:focus { border-color: var(--em); box-shadow: 0 0 0 3px color-mix(in srgb, var(--em) 12%, transparent); }
  .rm-textarea { resize: vertical; line-height: 1.55; }

  /* 图标选择 */
  .rm-emoji-current {
    width: 52px; height: 52px; flex-shrink: 0;
    display: grid; place-items: center; font-size: 28px;
    border-radius: 12px; background: var(--em-soft); border: 1px solid var(--border);
  }
  .rm-emoji-list { display: flex; flex-wrap: wrap; gap: 6px; }
  .rm-emoji-opt {
    width: 34px; height: 34px; border-radius: 9px; border: 1px solid var(--border);
    background: var(--surface); cursor: pointer; font-size: 18px; transition: all 0.12s ease;
  }
  .rm-emoji-opt:hover { transform: scale(1.08); border-color: var(--em); }
  .rm-emoji-opt.sel { border-color: var(--em); box-shadow: 0 0 0 2px color-mix(in srgb, var(--em) 20%, transparent); background: var(--em-soft); }

  /* chips */
  .rm-chips { display: flex; flex-wrap: wrap; gap: 6px; min-height: 30px; align-content: flex-start; }
  .rm-chip {
    display: inline-flex; align-items: center; gap: 5px;
    background: color-mix(in srgb, var(--bg) 85%, var(--text));
    color: var(--text);
    border: 1px solid color-mix(in srgb, var(--bg) 65%, var(--text)); border-radius: 7px;
    padding: 4px 6px 4px 9px; font-size: 12px; font-weight: 600;
  }
  .rm-chip.alt { background: var(--em-soft); color: var(--em); border-color: color-mix(in srgb, var(--em) 30%, transparent); }
  .rm-chip button {
    border: none; background: rgba(0, 0, 0, 0.06); color: inherit;
    width: 16px; height: 16px; border-radius: 50%; cursor: pointer;
    font-size: 12px; line-height: 1; display: grid; place-items: center;
  }
  .rm-chip button:hover { background: rgba(0, 0, 0, 0.14); }
  .rm-addrow { display: flex; gap: 6px; margin-top: 8px; }
  .rm-addrow .rm-input { flex: 1; }

  /* 滑块 */
  .rm-sliders { display: grid; grid-template-columns: 1fr 1fr; gap: 14px 20px; }
  .rm-col .rm-sliders { grid-template-columns: 1fr; }
  .rm-slider-head { display: flex; align-items: center; justify-content: space-between; margin-bottom: 4px; }
  .rm-slider-head .rm-cap { font-size: 12.5px; font-weight: 600; color: var(--muted); }
  .rm-slider-val {
    font-size: 12px; font-weight: 700; color: var(--em);
    background: var(--em-soft); padding: 1px 8px; border-radius: 6px;
  }
  .rm-slider input[type='range'] {
    width: 100%; accent-color: var(--em); cursor: pointer; height: 4px;
  }

  /* 提示词预览 */
  .rm-copy { margin-left: auto; }
  .rm-preview-box {
    margin: 0; background: var(--bg); border: 1px solid var(--border);
    border-radius: 10px; padding: 14px; font-size: 12.5px; line-height: 1.6;
    color: var(--text); white-space: pre-wrap; word-break: break-word;
    max-height: 240px; overflow-y: auto; font-family: 'SFMono-Regular', Consolas, monospace;
  }

  /* 占位空态 */
  .rm-placeholder {
    margin: auto; text-align: center; max-width: 440px; padding: 40px 20px;
    display: flex; flex-direction: column; align-items: center; gap: 10px;
  }
  .rm-placeholder-ico {
    width: 72px; height: 72px; display: grid; place-items: center; font-size: 36px;
    border-radius: 20px; background: linear-gradient(135deg, var(--em-strong), color-mix(in srgb, var(--em-strong) 65%, var(--surface)));
    box-shadow: 0 8px 20px color-mix(in srgb, var(--em) 30%, transparent); margin-bottom: 6px;
  }
  .rm-placeholder h2 { margin: 0; font-size: 18px; }
  .rm-placeholder p { margin: 0; font-size: 13px; color: var(--muted); line-height: 1.6; }
  .rm-placeholder-tips { list-style: none; padding: 0; margin: 14px 0 0; display: flex; flex-direction: column; gap: 10px; width: 100%; }
  .rm-placeholder-tips li {
    display: flex; align-items: center; gap: 10px; text-align: left;
    background: var(--bg); border: 1px solid var(--border); border-radius: 10px;
    padding: 10px 12px; font-size: 12.5px; color: var(--muted);
  }
  .rm-placeholder-tips li span { font-size: 16px; }

  /* 提示 toast */
  .rm-toast {
    position: fixed; bottom: 28px; left: 50%; transform: translateX(-50%);
    background: var(--text); color: #fff; padding: 10px 18px; border-radius: 10px;
    font-size: 13px; font-weight: 600; box-shadow: 0 8px 24px rgba(15, 23, 42, 0.25);
    z-index: 100; animation: rm-pop 0.2s ease;
  }
  @keyframes rm-pop { from { opacity: 0; transform: translate(-50%, 8px); } to { opacity: 1; transform: translate(-50%, 0); } }

  /* 滚动条美化 */
  .rm-list::-webkit-scrollbar, .rm-main::-webkit-scrollbar, .rm-preview-box::-webkit-scrollbar { width: 8px; }
  .rm-list::-webkit-scrollbar-thumb, .rm-main::-webkit-scrollbar-thumb, .rm-preview-box::-webkit-scrollbar-thumb {
    background: #cbd5e1; border-radius: 8px;
  }
  .rm-list::-webkit-scrollbar-thumb:hover, .rm-main::-webkit-scrollbar-thumb:hover { background: #94a3b8; }

  /* 响应式 */
  @media (max-width: 920px) {
    .rm-body { grid-template-columns: 1fr; }
    .rm-list { max-height: 240px; }
    .rm-form { grid-template-columns: 1fr; }
    .rm-grid, .rm-sliders { grid-template-columns: 1fr; }
    .rm-preview-desc { max-width: 200px; }
  }
</style>
