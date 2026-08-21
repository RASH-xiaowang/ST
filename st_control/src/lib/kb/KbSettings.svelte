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
  import { Checkbox } from '../components/ui/checkbox';

  interface Props {
    models: ModelInfo[];
    setModel: (p: string, m: string) => void;
    notify: (msg: string, type?: 'success' | 'error' | 'warn') => void;
  }
  let { models, setModel, notify }: Props = $props();

  type ModelRole = 'inference' | 'parsing' | 'embedding' | 'rerank';
  let modelSettings = $state<Record<ModelRole, { providerId: string; model: string }>>({
    inference: { providerId: '', model: '' },
    parsing: { providerId: '', model: '' },
    embedding: { providerId: '', model: '' },
    rerank: { providerId: '', model: '' },
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
  ];
  const roleUsable = $derived.by(() => {
    const out: Record<ModelRole, ModelInfo[]> = { inference: [], parsing: [], embedding: [], rerank: [] };
    for (const r of MODEL_ROLES) {
      const marked = models.filter(r.filter);
      out[r.role] = marked.length > 0 ? marked : models;
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

  loadModelSettings();
  loadKbChunkCfg();
  loadAnalyticsSettings();
</script>

<div style="display:flex;flex-direction:column;gap:16px;width:100%">
  <!-- 页面头 -->
  <div style="display:flex;flex-direction:column;gap:4px;padding:2px 2px 0">
    <div style="font-size:16px;font-weight:700;color:var(--kb-text)">设置</div>
    <div style="font-size:12.5px;color:var(--kb-text-3)">配置文档上传、解析与问答所需的分块参数和模型，修改后自动保存、全局生效。</div>
  </div>

  <!-- 分块设置 -->
  <div class="kb-card">
    <div class="kb-card-hd">
      <div style="display:flex;align-items:center;gap:8px"><KbIcon name="sliders" size={15} color="var(--kb-accent-bright)" />分块设置</div>
      <span class="kb-badge kb-badge-ok" style="font-weight:400"><KbIcon name="check" size={11} />修改后自动保存</span>
    </div>
    <div class="kb-card-bd" style="display:flex;gap:14px;flex-wrap:wrap;align-items:flex-end">
      <label class="kb-label">分块策略
        <SelectRoot type="single" value={kbChunkCfg.strategy} onValueChange={(v) => { kbChunkCfg.strategy = v as typeof kbChunkCfg.strategy; saveKbChunkCfg(); }}>
          <SelectTrigger class="kb-shadcn-trigger h-8 w-36"><span>{{ recursive: '递归字符', title: '标题感知', parent_child: '父子分块' }[kbChunkCfg.strategy]}</span></SelectTrigger>
          <SelectContent>
            <SelectItem value="recursive">递归字符</SelectItem>
            <SelectItem value="title">标题感知</SelectItem>
            <SelectItem value="parent_child">父子分块</SelectItem>
          </SelectContent>
        </SelectRoot>
      </label>
      <label class="kb-label">分块大小
        <SelectRoot type="single" value={String(kbChunkCfg.size)} onValueChange={(v) => { kbChunkCfg.size = Number(v); saveKbChunkCfg(); }}>
          <SelectTrigger class="kb-shadcn-trigger h-8 w-28"><span>{{ 400: '小 400', 800: '中 800', 1200: '大 1200' }[kbChunkCfg.size] ?? kbChunkCfg.size}</span></SelectTrigger>
          <SelectContent>
            <SelectItem value="400">小 400</SelectItem>
            <SelectItem value="800">中 800</SelectItem>
            <SelectItem value="1200">大 1200</SelectItem>
          </SelectContent>
        </SelectRoot>
      </label>
      <label class="kb-label">重叠
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

  <!-- 模型设置 -->
  <div class="kb-card">
    <div class="kb-card-hd"><KbIcon name="settings" size={15} color="var(--kb-accent-bright)" />模型设置</div>
    <div class="kb-card-bd" style="display:flex;flex-direction:column;gap:16px">
      {#each MODEL_ROLES as r}
        {@const usable = roleUsable[r.role]}
        {@const providers = [...new Set(usable.map((m) => m.providerId))]}
        {@const cur = modelSettings[r.role]}
        <div style="display:flex;gap:12px;align-items:center;flex-wrap:wrap">
          <div style="width:150px;flex:none">
            <div style="font-size:13px;font-weight:600;color:var(--kb-text)">{r.label}</div>
            <div style="font-size:11.5px;color:var(--kb-text-3);margin-top:2px;line-height:1.5">{r.desc}</div>
          </div>
          <SelectRoot type="single" value={cur.providerId} onValueChange={(p) => {
              const list = usable.filter((m) => m.providerId === p);
              const m = list.find((x) => x.isDefault) ?? list[0];
              saveModelSetting(r.role, p, m ? m.model : '');
            }}>
            <SelectTrigger class="kb-shadcn-trigger h-8 w-40">
              <span>{usable.find((m) => m.providerId === cur.providerId)?.providerName ?? '选择提供方…'}</span>
            </SelectTrigger>
            <SelectContent>
            {#each providers as pid}
              <SelectItem value={pid}>{usable.find((m) => m.providerId === pid)?.providerName}</SelectItem>
            {/each}
            </SelectContent>
          </SelectRoot>
          <SelectRoot type="single" value={cur.model} disabled={!cur.providerId} onValueChange={(v) => saveModelSetting(r.role, cur.providerId, v)}>
            <SelectTrigger class="kb-shadcn-trigger h-8 w-56">
              <span>{usable.find((m) => m.providerId === cur.providerId && m.model === cur.model)?.model ?? '选择模型…'}</span>
            </SelectTrigger>
            <SelectContent>
            {#each usable.filter((m) => m.providerId === cur.providerId) as m}
              <SelectItem value={m.model}>{m.model}{m.isDefault ? '（默认）' : ''}{m.modelType ? ' · ' + m.modelType : ''}</SelectItem>
            {/each}
            </SelectContent>
          </SelectRoot>
        </div>
      {/each}
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

  <!-- 指标配置 -->
  <div class="kb-card">
    <div class="kb-card-hd"><KbIcon name="chart" size={15} color="var(--kb-accent-bright)" />指标配置</div>
    <div class="kb-card-bd" style="display:flex;flex-direction:column;gap:10px">
      {#each analyticsSettings as s}
        <div style="display:flex;align-items:center;gap:10px">
          <Checkbox checked={s.visible} onCheckedChange={(c) => { s.visible = !!c; saveAnalyticsSetting(s); }} title="是否在首页展示" />
          <input class="kb-input" style="width:200px" bind:value={s.label} onchange={() => saveAnalyticsSetting(s)} placeholder="显示名称" />
          <span style="font-size:11.5px;color:var(--kb-text-3)">{s.key}</span>
        </div>
      {/each}
      <p style="font-size:11.5px;color:var(--kb-text-3);margin:0;line-height:1.6">
        控制首页指标卡与趋势图展示哪些指标及显示名称；勾选状态与改名即时保存、全局生效。
      </p>
    </div>
  </div>
</div>

