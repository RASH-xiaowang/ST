<script lang="ts">
  import { kbApi } from './services/ipc';
  import type { AnalyticsSetting, ModelInfo } from './kbTypes';
  import KbIcon from './KbIcon.svelte';
  import { kbChunkCfg, loadKbChunkCfg, saveKbChunkCfg } from './kbChunkStore.svelte';
  import { Root as SelectRoot } from '../components/ui/select';
  import {
  SelectContent,
  SelectItem,
  SelectTrigger,
} from '../components/ui/select';
  import { Button } from '../components/ui/button';
  import { Badge } from '../components/ui/badge';
  import { Switch } from '../components/ui/switch';
  import KbUserManagement from './KbUserManagement.svelte';
  import KbAuditLog from './KbAuditLog.svelte';

  interface Props {
    models: ModelInfo[];
    setModel: (p: string, m: string) => void;
    notify: (msg: string, type?: 'success' | 'error' | 'warn') => void;
    isAdmin?: boolean;
  }
  let { models, setModel, notify, isAdmin = false }: Props = $props();

  type ModelRole = 'inference' | 'parsing' | 'embedding' | 'rerank' | 'multimodal';
  let modelSettings = $state<Record<ModelRole, { providerId: string; model: string }>>({
    inference: { providerId: '', model: '' },
    parsing: { providerId: '', model: '' },
    embedding: { providerId: '', model: '' },
    rerank: { providerId: '', model: '' },
    multimodal: { providerId: '', model: '' },
  });
  // 推理角色：显式「对话/chat」或未标记类型的模型均可选（未标记的对话模型如
  // deepseek-v4-pro 应可自由选择）；显式嵌入/重排序/生图/视频/语音不展示。
  const NON_CHAT_TYPES = ['embedding', '嵌入', 'rerank', '重排序', 'image', '生图', 'video', '视频', 'speech', '语音', 'audio', '音频'];
  const isChatUsable = (m: ModelInfo): boolean => {
    const t = (m.modelType ?? '').trim().toLowerCase();
    return !t || !NON_CHAT_TYPES.includes(t);
  };
  const MODEL_ROLES: { role: ModelRole; label: string; desc: string; filter: (m: ModelInfo) => boolean }[] = [
    { role: 'inference', label: '推理模型', desc: 'LLM/VLM 对话推理接口，支持多轮对话和流式输出', filter: isChatUsable },
    { role: 'parsing', label: '文档解析', desc: '解析 PDF、Word、图片等多格式文档，提取结构化内容', filter: (m) => m.modelType === '对话' || m.modelType === 'chat' || m.modelType === '视觉' },
    { role: 'embedding', label: 'Embeddings', desc: '将文本转换为高维向量，用于语义搜索与相似度计算', filter: (m) => m.modelType === '嵌入' || m.modelType === 'embedding' },
    { role: 'rerank', label: 'Rerank', desc: '对检索结果智能重排序，提升搜索精准度', filter: (m) => m.modelType === '重排序' || m.modelType === 'rerank' },
    { role: 'multimodal', label: '多模态分析', desc: '图片/音视频文件智能分析，自动生成摘要与 OCR 文字', filter: (m) => {
      // 显式标记为多模态的模型，或视觉类模型
      const t = (m.modelType ?? '').trim().toLowerCase();
      return t === '视觉' || t === 'vision' || t === '多模态' || t === 'multimodal' || t === '对话' || t === 'chat';
    }},
  ];
  const roleUsable = $derived.by(() => {
    const out: Record<ModelRole, ModelInfo[]> = { inference: [], parsing: [], embedding: [], rerank: [], multimodal: [] };
    for (const r of MODEL_ROLES) {
      const marked = models.filter(r.filter);
      // 推理角色允许未标记类型的模型（对话模型可能未打标）；嵌入/解析/重排序
      // 必须命中类型标记，避免把对话模型当嵌入模型选择（向量化接口不支持）。
      out[r.role] = r.role === 'inference' || r.role === 'multimodal'
        ? (marked.length > 0 ? marked : models)
        : marked;
    }
    return out;
  });

  async function loadModelSettings() {
    try {
    const s = await kbApi.getModelSettings();
      if (s?.inference) modelSettings.inference = s.inference;
      if (s?.parsing) modelSettings.parsing = s.parsing;
      if (s?.embedding) modelSettings.embedding = s.embedding;
      if (s?.rerank) modelSettings.rerank = s.rerank;
      // multimodal 可能未配置，需要检查是否有 providerId
      const mm = s?.multimodal as Record<string, unknown> | undefined;
      if (mm?.providerId) {
        modelSettings.multimodal = mm as unknown as { providerId: string; model: string };
      }
      if (s?.embedding?.providerId) setModel(s.embedding.providerId, s.embedding.model);
    } catch { /* 未配置模型时忽略 */ }
  }
  async function saveModelSetting(role: ModelRole, providerId: string, model: string) {
    modelSettings[role] = { providerId, model };
    if (role === 'embedding') setModel(providerId, model);
    try {
    await kbApi.setModelSettings(role, providerId, model);
      notify('模型设置已保存：' + MODEL_ROLES.find((r) => r.role === role)?.label);
    } catch (e: unknown) { notify('保存模型设置失败：' + e, 'error'); }
  }

  // ─── 指标配置（显示名 / 可见性） ───
  let analyticsSettings = $state<AnalyticsSetting[]>([]);
  async function loadAnalyticsSettings() {
    try {
    analyticsSettings = await kbApi.getAnalyticsSettings();
    } catch {
      analyticsSettings = [];
    }
  }
  async function saveAnalyticsSetting(s: AnalyticsSetting) {
    try {
    await kbApi.setAnalyticsSettings({ key: s.key, label: s.label, visible: s.visible });
      notify('指标配置已保存');
    } catch (e: unknown) {
      notify('保存指标配置失败：' + e, 'error');
    }
  }

  // ─── 标签页 ───
  type SettingsTab = 'models' | 'chunk' | 'rag' | 'analytics' | 'users' | 'audit';
  let activeTab = $state<SettingsTab>('models');
  const TABS: { id: SettingsTab; label: string; icon: string; adminOnly?: boolean }[] = [
    { id: 'models', label: '模型配置', icon: 'settings' },
    { id: 'chunk', label: '分块设置', icon: 'sliders' },
    { id: 'rag', label: 'RAG 提示词', icon: 'sparkle' },
    { id: 'analytics', label: '指标配置', icon: 'chart' },
    { id: 'users', label: '用户管理', icon: 'users', adminOnly: true },
    { id: 'audit', label: '审计日志', icon: 'list', adminOnly: true },
  ];
  const visibleTabs = $derived(TABS.filter((t) => !t.adminOnly || isAdmin));

  loadModelSettings();
  loadKbChunkCfg();
  loadAnalyticsSettings();

  // ─── 模型连通性测试 ───
  let testResults = $state<Record<string, { ok: boolean; msg: string; latency: number }>>({});
  async function testModel(role: string, providerId: string, model: string) {
    const key = `${role}:${providerId}:${model}`;
    testResults[key] = { ok: false, msg: '测试中…', latency: 0 };
    try {
      const res = await kbApi.testModel(providerId, model, role);
      testResults[key] = {
        ok: true,
        msg: res.note || `连接成功（${res.latencyMs}ms）`,
        latency: res.latencyMs,
      };
    } catch (e: unknown) {
      let msg = String(e);
      // 截断过长的错误信息（如含完整 API 响应）
      if (msg.length > 120) msg = msg.slice(0, 117) + '…';
      testResults[key] = { ok: false, msg: '❌ ' + msg, latency: 0 };
    }
  }

  // ─── RAG 系统提示词 ───
  let ragPrompt = $state('');
  let ragPromptBusy = $state(false);
  const RAG_DEFAULT_PROMPT = '你是企业知识库助手。请严格基于以下【知识上下文】回答用户问题，若上下文无法回答请如实说明，不要编造。回答中可适当引用来源文档。';
  // ragPrompt 必须在 loadRagPrompt() 之前声明：`ragPrompt = await ...` 会先求值左侧引用，
  // 若调用先于声明执行会触发暂时性死区错误（Cannot access 'ragPrompt' before initialization）。
  loadRagPrompt();
  async function loadRagPrompt() {
    try { ragPrompt = await kbApi.getRagSystemPrompt(); } catch { ragPrompt = RAG_DEFAULT_PROMPT; }
  }
  async function saveRagPrompt() {
    ragPromptBusy = true;
    try {
      await kbApi.setRagSystemPrompt(ragPrompt);
      notify('RAG 系统提示词已保存');
    } catch (e: unknown) { notify('保存失败：' + e, 'error'); }
    finally { ragPromptBusy = false; }
  }
  function resetRagPrompt() { ragPrompt = RAG_DEFAULT_PROMPT; }
</script>

<div class="kb-settings-root">
  <!-- 标签页导航 -->
  <nav class="kb-settings-tabs">
    {#each visibleTabs as t}
      <button class="kb-settings-tab" class:active={activeTab === t.id} onclick={() => activeTab = t.id}>
        <KbIcon name={t.icon} size={14} />{t.label}
      </button>
    {/each}
  </nav>

  <!-- 标签页内容 -->
  <div class="kb-settings-content">

  {#if activeTab === 'chunk'}
  <!-- 分块设置 -->
  <div class="kb-card">
    <div class="kb-card-hd">
      <div style="display:flex;align-items:center;gap:8px"><KbIcon name="sliders" size={15} color="var(--kb-accent-bright)" />分块设置</div>
      <Badge variant="default" class="text-[10px] font-normal"><KbIcon name="check" size={11} />修改后自动保存</Badge>
    </div>
    <div class="kb-card-bd" style="display:flex;gap:14px;flex-wrap:wrap;align-items:flex-end">
      <label class="kb-label">
        <span class="kb-label-line">分块策略
          <span class="kb-tip-icon">
            ⓘ
            <span class="kb-tip-card">
              <span class="kb-tip-title">分块策略</span>
              <span class="kb-tip-body">
                <span class="kb-tip-row"><b>递归字符</b><span>按段落/句子逐层切分，通用性最强</span></span>
                <span class="kb-tip-row"><b>标题感知</b><span>按 Markdown 标题层级切分，保留章节上下文</span></span>
                <span class="kb-tip-row"><b>父子分块</b><span>父块用于回答上下文，子块用于精准检索</span></span>
              </span>
              <span class="kb-tip-foot">推荐：大多数文档用「递归字符」，结构化文档用「标题感知」</span>
            </span>
          </span>
        </span>
        <SelectRoot type="single" value={kbChunkCfg.strategy} onValueChange={(v) => { kbChunkCfg.strategy = v as typeof kbChunkCfg.strategy; saveKbChunkCfg(); }}>
          <SelectTrigger class="kb-shadcn-trigger h-8 w-36"><span>{{ recursive: '递归字符', title: '标题感知', parent_child: '父子分块' }[kbChunkCfg.strategy]}</span></SelectTrigger>
          <SelectContent>
            <SelectItem value="recursive">递归字符</SelectItem>
            <SelectItem value="title">标题感知</SelectItem>
            <SelectItem value="parent_child">父子分块</SelectItem>
          </SelectContent>
        </SelectRoot>
      </label>
      <label class="kb-label">
        <span class="kb-label-line">分块大小
          <span class="kb-tip-icon">
            ⓘ
            <span class="kb-tip-card">
              <span class="kb-tip-title">分块大小（字符数）</span>
              <span class="kb-tip-body">
                <span class="kb-tip-row"><b>小 400</b><span>适合短文/FAQ，检索精准但上下文少</span></span>
                <span class="kb-tip-row"><b>中 800</b><span>默认值，平衡精度与上下文</span></span>
                <span class="kb-tip-row"><b>大 1200</b><span>适合长文/技术文档，上下文丰富</span></span>
              </span>
              <span class="kb-tip-foot">越大 → 上下文越完整，检索粒度越粗；越小 → 检索越精准，可能丢失上下文</span>
            </span>
          </span>
        </span>
        <SelectRoot type="single" value={String(kbChunkCfg.size)} onValueChange={(v) => { kbChunkCfg.size = Number(v); saveKbChunkCfg(); }}>
          <SelectTrigger class="kb-shadcn-trigger h-8 w-28"><span>{{ 400: '小 400', 800: '中 800', 1200: '大 1200' }[kbChunkCfg.size] ?? kbChunkCfg.size}</span></SelectTrigger>
          <SelectContent>
            <SelectItem value="400">小 400</SelectItem>
            <SelectItem value="800">中 800</SelectItem>
            <SelectItem value="1200">大 1200</SelectItem>
          </SelectContent>
        </SelectRoot>
      </label>
      <label class="kb-label">
        <span class="kb-label-line">重叠
          <span class="kb-tip-icon">
            ⓘ
            <span class="kb-tip-card">
              <span class="kb-tip-title">重叠字符数</span>
              <span class="kb-tip-body">
                <span class="kb-tip-row"><b>无（0）</b><span>无重叠，处理速度最快</span></span>
                <span class="kb-tip-row"><b>128</b><span>轻度重叠，适合结构清晰的文档</span></span>
                <span class="kb-tip-row"><b>256</b><span>较重重叠，适合长段落连续叙述</span></span>
              </span>
              <span class="kb-tip-foot">重叠可避免关键信息被切断在分片边界，提升召回率</span>
            </span>
          </span>
        </span>
        <SelectRoot type="single" value={String(kbChunkCfg.overlap)} onValueChange={(v) => { kbChunkCfg.overlap = Number(v); saveKbChunkCfg(); }}>
          <SelectTrigger class="kb-shadcn-trigger h-8 w-24"><span>{kbChunkCfg.overlap === 0 ? '无' : kbChunkCfg.overlap}</span></SelectTrigger>
          <SelectContent>
            <SelectItem value="0">无</SelectItem>
            <SelectItem value="128">128</SelectItem>
            <SelectItem value="256">256</SelectItem>
          </SelectContent>
        </SelectRoot>
      </label>
      <p style="font-size:12px;color:var(--kb-text-3);margin:0 0 8px">上传 / 重处理 / 上传新版本时使用以上分块参数。</p>
    </div>
  </div>

  {/if}

  {#if activeTab === 'models'}
  <!-- 模型设置 -->
  <div class="kb-card">
    <div class="kb-card-hd"><KbIcon name="settings" size={15} color="var(--kb-accent-bright)" />模型设置</div>
    <div class="kb-card-bd" style="display:flex;flex-direction:column;gap:16px">
      {#each MODEL_ROLES as r}
        {@const usable = roleUsable[r.role]}
        {@const providers = [...new Set(usable.map((m) => m.providerId))]}
        {@const cur = modelSettings[r.role]}
        <div class="kb-model-row">
          <div class="kb-model-label">
            <div class="kb-model-label-title">{r.label}</div>
            <div class="kb-model-label-desc">{r.desc}</div>
          </div>
          <div class="kb-model-controls">
            <SelectRoot type="single" value={cur.providerId} onValueChange={(p) => {
                const list = usable.filter((m) => m.providerId === p);
                const m = list.find((x) => x.isDefault) ?? list[0];
                saveModelSetting(r.role, p, m ? m.model : '');
              }}>
              <SelectTrigger class="kb-shadcn-trigger h-8" style="min-width:120px;max-width:160px">
                <span class="kb-model-trigger-text">{usable.find((m) => m.providerId === cur.providerId)?.providerName ?? '选择提供方…'}</span>
              </SelectTrigger>
              <SelectContent>
              {#each providers as pid}
                <SelectItem value={pid}>{usable.find((m) => m.providerId === pid)?.providerName}</SelectItem>
              {/each}
              </SelectContent>
            </SelectRoot>
            <SelectRoot type="single" value={cur.model} disabled={!cur.providerId} onValueChange={(v) => saveModelSetting(r.role, cur.providerId, v)}>
              <SelectTrigger class="kb-shadcn-trigger h-8" style="min-width:140px;max-width:220px">
                <span class="kb-model-trigger-text">{usable.find((m) => m.providerId === cur.providerId && m.model === cur.model)?.model ?? '选择模型…'}</span>
              </SelectTrigger>
              <SelectContent>
              {#each usable.filter((m) => m.providerId === cur.providerId) as m}
                <SelectItem value={m.model}>{m.model}{m.isDefault ? '（默认）' : ''}{m.modelType ? ' · ' + m.modelType : ''}</SelectItem>
              {/each}
              </SelectContent>
            </SelectRoot>
            {#if cur.providerId && cur.model}
              {@const testKey = `${r.role}:${cur.providerId}:${cur.model}`}
              {@const tr = testResults[testKey]}
              <Button variant="outline" size="sm" onclick={() => testModel(r.role, cur.providerId, cur.model)}
                title="测试模型连通性">
                <KbIcon name={tr?.ok ? 'check' : 'sparkle'} size={12} />
                {tr?.ok ? '✓' : '测试'}
              </Button>
              {#if tr}
                <span class="kb-model-test-msg" style:color={tr.ok ? 'var(--kb-ok)' : 'var(--kb-warn)'}>{tr.msg}</span>
              {/if}
            {/if}
          </div>
        </div>
      {/each}
      {#if roleUsable.embedding.length === 0}
        <div style="display:flex;gap:8px;align-items:flex-start;font-size:12.5px;color:var(--kb-warn);border:1px solid color-mix(in srgb, var(--app-warning) 40%, var(--kb-border));border-radius:8px;padding:8px 10px;line-height:1.6">
          <span style="flex:none;margin-top:1px"><KbIcon name="warn" size={14} /></span>
          <span>未配置任何 Embeddings 模型：上传的文档只能解析与全文检索，无法进行语义向量检索。请先在「大模型管理」中添加支持 Embedding 的模型（如 text-embedding 系列），再回到这里为「Embeddings」角色选择模型，并到文档列表对已上传文档执行「重处理」。</span>
        </div>
      {/if}
      <p style="font-size:11.5px;color:var(--kb-text-3);margin:0;line-height:1.6">
        推理模型用于问答生成与 Wiki 提炼；Embeddings 用于文档向量化；Rerank 用于检索重排序；
        文档解析当前版本仍走内置解析与系统 OCR。
      </p>
      <div style="display:flex;gap:8px;align-items:flex-start;font-size:11.5px;color:var(--kb-warn);border:1px solid color-mix(in srgb, var(--app-warning) 34%, var(--app-bg-color));border-radius:8px;padding:8px 10px;line-height:1.6">
        <span style="flex:none;margin-top:1px"><KbIcon name="warn" size={14} /></span>
        <span>切换 Embeddings 模型后，已入库知识库的向量仍由旧模型生成，新文档会被拦截直至模型统一。若确实要更换，请对新模型下的全部文档执行「重处理」以保持一致。</span>
      </div>
    </div>
  </div>

  {/if}

  {#if activeTab === 'rag'}
  <!-- RAG 系统提示词 -->
  <div class="kb-card">
    <div class="kb-card-hd">
      <div style="display:flex;align-items:center;gap:8px"><KbIcon name="sparkle" size={15} color="var(--kb-accent-bright)" />RAG 系统提示词</div>
      <span class="kb-badge kb-badge-mute" style="font-weight:400">自定义问答角色与行为</span>
    </div>
    <div class="kb-card-bd" style="display:flex;flex-direction:column;gap:10px">
      <textarea class="kb-textarea" rows="5" style="font-size:12.5px;line-height:1.7;resize:vertical" placeholder="自定义 RAG 问答的系统提示词…" bind:value={ragPrompt}></textarea>
      <div style="display:flex;gap:8px;align-items:center">
        <Button onclick={saveRagPrompt} disabled={ragPromptBusy}>{ragPromptBusy ? '保存中…' : '保存'}</Button>
        <Button variant="outline" onclick={resetRagPrompt}>恢复默认</Button>
        <div style="flex:1"></div>
        <span style="font-size:11.5px;color:var(--kb-text-3)">留空恢复默认。提示词会与检索到的知识上下文拼接后发送给 LLM。</span>
      </div>
    </div>
  </div>

  {/if}

  {#if activeTab === 'analytics'}
  <!-- 指标配置 -->
  <div class="kb-card">
    <div class="kb-card-hd"><KbIcon name="chart" size={15} color="var(--kb-accent-bright)" />指标配置</div>
    <div class="kb-card-bd" style="display:flex;flex-direction:column;gap:10px">
      {#each analyticsSettings as s}
        <div style="display:flex;align-items:center;gap:10px">
          <Switch checked={s.visible} onCheckedChange={(c) => { s.visible = c; saveAnalyticsSetting(s); }} title="是否在首页展示" />
          <input class="kb-input" style="width:200px" bind:value={s.label} onchange={() => saveAnalyticsSetting(s)} placeholder="显示名称" />
          <span style="font-size:11.5px;color:var(--kb-text-3)">{s.key}</span>
        </div>
      {/each}
      <p style="font-size:11.5px;color:var(--kb-text-3);margin:0;line-height:1.6">
        控制首页指标卡与趋势图展示哪些指标及显示名称；勾选状态与改名即时保存、全局生效。
      </p>
    </div>
  </div>
  {/if}

  {#if activeTab === 'users'}
    <!-- 用户管理 -->
    <KbUserManagement {notify} hideHeader={true} />
  {/if}

  {#if activeTab === 'audit'}
    <!-- 审计日志 -->
    <KbAuditLog hideHeader={true} />
  {/if}

  </div><!-- /kb-settings-content -->
</div>

<style>
  /* ── 设置页标签布局 ── */
  .kb-settings-root {
    display: flex;
    gap: 0;
    width: 100%;
    min-height: 0;
  }
  .kb-settings-tabs {
    flex: none;
    width: 160px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 2px 12px 0 0;
    border-right: 1px solid var(--kb-border-subtle);
  }
  .kb-settings-tab {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 8px 10px;
    font-size: 13px;
    font-family: inherit;
    color: var(--kb-text-2);
    background: transparent;
    border: none;
    border-radius: 8px;
    cursor: pointer;
    transition: background .12s, color .12s;
    text-align: left;
    white-space: nowrap;
  }
  .kb-settings-tab:hover {
    background: var(--kb-hover);
    color: var(--kb-text);
  }
  .kb-settings-tab.active {
    background: var(--kb-hover-strong);
    color: var(--kb-accent-bright);
    font-weight: 600;
  }
  .kb-settings-content {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  @media (max-width: 768px) {
    .kb-settings-root { flex-direction: column; }
    .kb-settings-tabs {
      width: 100%;
      flex-direction: row;
      border-right: none;
      border-bottom: 1px solid var(--kb-border-subtle);
      padding: 0 0 8px;
      overflow-x: auto;
      gap: 4px;
    }
    .kb-settings-tab { padding: 6px 10px; font-size: 12px; }
  }

  /* 模型设置行：左侧标签 + 右侧控件，小屏自动换行 */
  .kb-model-row {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    min-width: 0;
  }
  .kb-model-label {
    width: 140px;
    flex: none;
    min-width: 0;
  }
  .kb-model-label-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--kb-text);
  }
  .kb-model-label-desc {
    font-size: 11.5px;
    color: var(--kb-text-3);
    margin-top: 2px;
    line-height: 1.5;
    word-break: break-all;
  }
  /* 右侧控件区：自动换行，不超出父容器 */
  .kb-model-controls {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
    min-width: 0;
    flex: 1;
    max-width: 100%;
  }
  /* Select 触发器文本截断 */
  :global(.kb-shadcn-trigger) {
    max-width: 100%;
    overflow: hidden;
  }
  :global(.kb-shadcn-trigger span) {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    display: block;
    min-width: 0;
  }
  /* 测试结果文本 */
  .kb-model-test-msg {
    font-size: 11.5px;
    word-break: break-all;
    max-width: 280px;
    line-height: 1.4;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: none;
  }
  /* 小屏适配：标签和控件上下排列 */
  @media (max-width: 640px) {
    .kb-model-row {
      flex-direction: column;
      gap: 6px;
    }
    .kb-model-label {
      width: 100%;
    }
    .kb-model-controls {
      width: 100%;
    }
  }
  /* ─── 提示图标（ⓘ）悬浮卡片 ─── */
  .kb-label-line {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .kb-tip-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    font-size: 13px;
    color: var(--kb-text-3);
    cursor: help;
    position: relative;
    border-radius: 50%;
    transition: color .15s, background .15s;
    flex: none;
  }
  .kb-tip-icon:hover {
    color: var(--kb-accent-bright);
    background: color-mix(in srgb, var(--kb-accent) 12%, transparent);
  }
  /* 弹出卡片：默认隐藏，悬浮显示 */
  .kb-tip-card {
    display: none;
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    width: 320px;
    background: var(--app-bg-color);
    border: 1px solid var(--kb-border-strong);
    border-radius: 12px;
    box-shadow: var(--kb-shadow-lg);
    z-index: 200;
    overflow: hidden;
  }
  .kb-tip-icon:hover .kb-tip-card {
    display: block;
  }
  /* 卡片标题栏 */
  .kb-tip-title {
    display: block;
    padding: 10px 14px 8px;
    font-size: 13px;
    font-weight: 600;
    color: var(--kb-text);
    border-bottom: 1px solid var(--kb-border-subtle);
    background: color-mix(in srgb, var(--kb-accent) 5%, transparent);
  }
  /* 卡片选项列表 */
  .kb-tip-body {
    display: flex;
    flex-direction: column;
    padding: 6px 0;
  }
  .kb-tip-row {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 6px 14px;
    font-size: 12.5px;
    line-height: 1.5;
    transition: background .1s;
  }
  .kb-tip-row:hover {
    background: var(--kb-hover);
  }
  .kb-tip-row b {
    flex: none;
    min-width: 72px;
    color: var(--kb-accent-bright);
    font-weight: 600;
    font-size: 12px;
  }
  .kb-tip-row span {
    color: var(--kb-text-2);
  }
  /* 卡片底部建议 */
  .kb-tip-foot {
    display: block;
    padding: 8px 14px;
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--kb-text-3);
    border-top: 1px solid var(--kb-border-subtle);
    background: color-mix(in srgb, var(--kb-accent) 3%, transparent);
  }
</style>

