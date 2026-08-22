<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { kbUser, refreshKbUser } from './auth.svelte';
  import { onLlmConfigChanged } from '../llm/store.svelte';
  import { kbApi } from './services/ipc';
  import { formatIsoTime } from '../format';
  import type { KbSummary, ModelInfo, QaSessionItem } from './kbTypes';
  import KbDashboard from './KbDashboard.svelte';
  import KbDocs from './KbDocs.svelte';
  import KbChat from './KbChat.svelte';
  import KbSettings from './KbSettings.svelte';
  import KbActivity from './KbActivity.svelte';
  import WikiPanel from './WikiPanel.svelte';
  import KbIcon from './KbIcon.svelte';
  import KbModal from './KbModal.svelte';
  import { loadKbChunkCfg } from './kbChunkStore.svelte';
  import './kbui.css';

  const NAV = [
    { id: 'home', ico: 'dashboard', label: '首页' },
    { id: 'chat', ico: 'chat', label: 'AI问答' },
    { id: 'activity', ico: 'activity', label: '活动' },
    { id: 'settings', ico: 'settings', label: '设置' },
  ] as const;
  // 'kb' / 'docs' / 'wiki' 不是导航项，而是“选择知识库后进入的工作区”内部视图
  type NavId = (typeof NAV)[number]['id'] | 'kb' | 'docs' | 'wiki';

  // ─── 全局状态 ───
let nav = $state<NavId>('home');
  let kbs = $state<KbSummary[]>([]);
  let sessions = $state<QaSessionItem[]>([]);
  // 从侧边栏「历史对话」打开指定会话（ts 用于重复点击同一会话时仍触发）
  let chatTarget = $state<{ id: number; ts: number } | null>(null);
  // 从 AI 问答引用跳转打开指定文档（ts 用于重复触发）
  let pendingDoc = $state<{ id: number; ts: number } | null>(null);
  let docTotal = $state(0);
  let selectedKb = $state<number | null>(null);
  let models = $state<ModelInfo[]>([]);
  let selProvider = $state('');
  let selModel = $state('');

  // ─── Toast 通知 ───
  let toasts = $state<{ id: number; type: 'success' | 'error' | 'warn'; msg: string }[]>([]);
  function notify(msg: string, type: 'success' | 'error' | 'warn' = 'success') {
    const id = Date.now() + Math.random();
    toasts = [...toasts, { id, type, msg }];
    setTimeout(() => { toasts = toasts.filter((t) => t.id !== id); }, 3500);
  }

  // ─── 数据加载 ───
  async function loadKbs() {
    try {
      kbs = await kbApi.list(kbUser.user?.id ?? 1);
    } catch { /* 未登录时忽略 */ }
  }
  async function loadSessions() {
    try { sessions = await kbApi.listSessions(); } catch { sessions = []; }
  }
  async function loadModels() {
    try {
      models = await kbApi.listModels();
    } catch { models = []; /* 未配置模型时忽略 */ }
    try {
      // 用户手动选择仍然有效时保留选择，避免每次刷新把选择重置回默认
      // 只接受「嵌入」类型标记的模型作为向量化模型：对话/其他类型模型没有
      // 嵌入接口，误用会导致全部文档上传后向量化失败（404）。
      const embedMarked = models.filter((m) => m.modelType === '嵌入' || m.modelType === 'embedding');
      // 未配置任何嵌入模型时清空选择：上传/重处理传入 null，后端回退到
      // 「设置 → 模型设置」中的 Embeddings 配置；仍无则跳过向量化
      // （文档正常解析与全文检索），并在界面提示用户配置 Embeddings 模型。
      if (embedMarked.length === 0) { selProvider = ''; selModel = ''; return; }
      const providerOk = embedMarked.some((m) => m.providerId === selProvider);
      const modelOk = embedMarked.some((m) => m.providerId === selProvider && m.model === selModel);
      if (!providerOk || !modelOk) { selProvider = embedMarked[0].providerId; selModel = embedMarked[0].model; }
    } catch { /* 默认模型不可用时忽略（仍保留已加载的模型列表） */ }
  }
  function setModel(p: string, m: string) { selProvider = p; selModel = m; }

  function goNav(id: NavId) {
    nav = id;
  }
  function openChatSession(s: QaSessionItem) {
    chatTarget = { id: s.id, ts: Date.now() };
    nav = 'chat';
  }
  function fmtSessionTime(t: string): string {
    return formatIsoTime(t, { showYear: false, utc: true });
  }

  let kbObserver: IntersectionObserver | null = null;
  let unsubLlmModels: (() => void) | null = null;
  onMount(async () => {
    await refreshKbUser();
    await loadModels();
    await loadKbs();
    await loadSessions();
    await loadKbChunkCfg();
    // 与大模型管理同步：知识库模块重新可见时刷新模型/默认值（含切换应用标签页返回）
    kbObserver = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          loadModels();
          loadKbs();
          loadSessions();
        }
      },
      { threshold: 0.1 },
    );
    const rootEl = document.querySelector('.kb-root') as HTMLElement | null;
    if (rootEl) kbObserver.observe(rootEl);
    // 大模型管理配置变化时实时刷新模型列表（无需人工刷新）
    unsubLlmModels = onLlmConfigChanged(() => {
      loadModels();
    });
  });
  onDestroy(() => {
    kbObserver?.disconnect();
    unsubLlmModels?.();
  });

  // ─── 知识库 CRUD ───
  let createKbOpen = $state(false);
  let newKbName = $state('');
  let newKbDesc = $state('');
  let createKbBusy = $state(false);
  let createKbErr = $state('');
  function openNewKb() {
    newKbName = ''; newKbDesc = ''; createKbErr = ''; createKbBusy = false;
    createKbOpen = true;
  }
  async function doCreateKb() {
    if (createKbBusy) return;
    const name = newKbName.trim();
    if (!name) { createKbErr = '请输入知识库名称'; return; }
    createKbBusy = true; createKbErr = '';
    try {
      await kbApi.create(name, newKbDesc.trim() || null);
      createKbOpen = false;
      await loadKbs();
      notify('知识库已创建：' + name);
    } catch (e: unknown) { createKbErr = '创建失败：' + e; }
    finally { createKbBusy = false; }
  }

  let editKbOpen = $state(false);
  let editKbId = $state<number | null>(null);
  let editKbName = $state('');
  let editKbDesc = $state('');
  let editKbBusy = $state(false);
  let editKbErr = $state('');
  function openEditKb(kb: KbSummary) {
    editKbId = kb.id; editKbName = kb.name; editKbDesc = kb.description ?? '';
    editKbErr = ''; editKbBusy = false; editKbOpen = true;
  }
  async function doEditKb() {
    if (editKbId === null || editKbBusy) return;
    const name = editKbName.trim();
    if (!name) { editKbErr = '请输入知识库名称'; return; }
    editKbBusy = true; editKbErr = '';
    try {
      await kbApi.update(editKbId, name, editKbDesc.trim() || null);
      editKbOpen = false;
      await loadKbs();
      notify('知识库已更新');
    } catch (e: unknown) { editKbErr = '保存失败：' + e; }
    finally { editKbBusy = false; }
  }

  let delKbTarget = $state<KbSummary | null>(null);
  let delKbBusy = $state(false);
  let delKbErr = $state('');
  function openDeleteKb(kb: KbSummary) { delKbTarget = kb; delKbBusy = false; delKbErr = ''; }
  async function doDeleteKb() {
    if (delKbTarget === null || delKbBusy) return;
    delKbBusy = true; delKbErr = '';
    try {
      const wasSelected = selectedKb === delKbTarget.id;
      await kbApi.remove(delKbTarget.id);
      delKbTarget = null;
      if (wasSelected) { selectedKb = null; nav = 'home'; }
      await loadKbs();
      notify('知识库已删除');
    } catch (e: unknown) { delKbErr = '删除失败：' + e; }
    finally { delKbBusy = false; }
  }

  function openKbFromDashboard(id: number) {
    selectedKb = id;
    nav = 'docs';
  }
  const navTitle = $derived(NAV.find((n) => n.id === nav)?.label ?? (nav === 'docs' ? '文档' : nav === 'wiki' ? 'Wiki' : nav === 'kb' ? '知识库' : ''));
  const curKbName = $derived(kbs.find((k) => k.id === selectedKb)?.name ?? '');
  async function togglePin(kb: KbSummary) {
    try {
      await kbApi.setPin(kb.id, !kb.pinned);
      await loadKbs();
      notify(kb.pinned ? '已取消置顶' : '已置顶：' + kb.name);
    } catch (e: unknown) { notify('置顶失败：' + e, 'error'); }
  }
</script>

<div class="kb-root">
  <!-- 左侧导航栏 -->
  <aside class="kb-sidebar">
    <div class="kb-brand" role="button" tabindex="0" title="回到首页"
      onclick={() => nav = 'home'} onkeydown={(e) => e.key === 'Enter' && (nav = 'home')}>
      <span class="kb-brand-ico"><KbIcon name="kb" size={16} weight="bold" /></span>
      <span class="kb-brand-text">
        <span class="kb-brand-name">知识库中心</span>
        <span class="kb-brand-sub">本地知识库 · 语义检索</span>
      </span>
    </div>
    <div class="kb-sidebar-scroll">
      <div class="kb-sidebar-section">主导航</div>
      <button class="kb-sidebar-item" class:active={nav === 'home'} onclick={() => goNav('home')}>
        <KbIcon name="dashboard" size={15} />首页
      </button>
      <button class="kb-sidebar-item" class:active={nav === 'kb'} onclick={() => goNav('kb')} title="管理全部知识库">
        <KbIcon name="kb" size={15} />知识库
      </button>
      <button class="kb-sidebar-item" class:active={nav === 'chat'} onclick={() => goNav('chat')} title="选择知识库进行对话测试">
        <KbIcon name="chat" size={15} />AI问答
      </button>
      <button class="kb-sidebar-item" class:active={nav === 'activity'} onclick={() => goNav('activity')} title="处理任务与检索历史">
        <KbIcon name="activity" size={15} />活动
      </button>
      <div class="kb-sidebar-section kb-sidebar-section-row">
        <span>历史对话</span>
        <button class="kb-sidebar-add" onclick={() => { chatTarget = null; goNav('chat'); }} title="开始新对话"><KbIcon name="plus" size={13} weight="bold" /></button>
      </div>
      {#each sessions as s}
        <button class="kb-sidebar-item" class:active={chatTarget?.id === s.id && nav === 'chat'}
          onclick={() => openChatSession(s)} title={s.title ?? ''}>
          <span class="kb-sidebar-kb-ico"><KbIcon name="chatCircle" size={14} /></span>
          <span class="kb-sidebar-kb-name">{s.title ?? ('会话 #' + s.id)}</span>
          <span class="kb-sidebar-kb-count">{fmtSessionTime(s.updatedAt)}</span>
        </button>
      {/each}
      {#if sessions.length === 0}
        <div class="kb-sidebar-empty">暂无历史对话<br>点击右上角 + 开始新对话</div>
      {/if}
    </div>
    <div class="kb-sidebar-bottom">
      <button class="kb-sidebar-item" class:active={nav === 'settings'} onclick={() => goNav('settings')} title="上传所需的模型与分块设置">
        <KbIcon name="settings" size={15} />设置
      </button>
    </div>
  </aside>

  <!-- 右侧内容区 -->
  <div class="kb-main">
    <header class="kb-contentbar">
      <div class="kb-contentbar-title">
        {#if selectedKb !== null && (nav === 'docs' || nav === 'wiki')}
          <span>知识库</span>
          <span class="kb-contentbar-sep">/</span>
          <span class="kb-contentbar-sub">{curKbName}</span>
        {:else}
          <span>{navTitle}</span>
        {/if}
      </div>
      <div style="flex:1"></div>
      {#if nav === 'home' || nav === 'kb'}
        <button class="kb-btn-md" onclick={openNewKb} title="新建知识库"><KbIcon name="plus" size={14} weight="bold" />新建</button>
      {/if}
      {#if selectedKb !== null && (nav === 'docs' || nav === 'wiki')}
        <div class="kb-seg kb-seg-tabs">
          <button class="kb-seg-item" class:active={nav === 'docs'} onclick={() => goNav('docs')}><KbIcon name="docs" size={14} />文档列表 ({docTotal})</button>
          <button class="kb-seg-item" class:active={nav === 'wiki'} onclick={() => goNav('wiki')}><KbIcon name="wiki" size={14} />Wiki<span class="kb-badge kb-badge-ok" style="font-size:11.5px;padding:0 5px;line-height:16px">NEW</span></button>
        </div>
      {/if}
      <div class="kb-contentbar-user">
        <span class="kb-avatar">{kbUser.user?.username?.charAt(0).toUpperCase() ?? 'A'}</span>
        <div style="min-width:0">
          <div class="kb-sidebar-user">{kbUser.user?.username ?? '本机'}{kbUser.user?.isAdmin ? ' · 管理员' : ''}</div>
          <div class="kb-sidebar-sub">单机部署 · 无需登录</div>
        </div>
      </div>
    </header>
    <main class="kb-content kb-scroll" style="display:flex;flex-direction:column;gap:14px">
      {#if nav === 'home'}
        <KbDashboard {kbs} {selectedKb} refreshKbs={loadKbs} onOpenKb={openKbFromDashboard}
          onNewKb={openNewKb} onEditKb={openEditKb} onDeleteKb={openDeleteKb} onTogglePin={togglePin} {notify} />
      {:else if nav === 'kb'}
        <KbDashboard {kbs} {selectedKb} refreshKbs={loadKbs} onOpenKb={openKbFromDashboard}
          onNewKb={openNewKb} onEditKb={openEditKb} onDeleteKb={openDeleteKb} onTogglePin={togglePin} {notify} mode="kbs" />
      {:else if nav === 'docs'}
        {#if selectedKb === null}
          <div class="kb-card" style="display:flex;flex-direction:column;gap:16px;padding:18px">
            <div class="kb-empty" style="padding:30px 18px">
              <span class="kb-empty-ico"><KbIcon name="folderOpen" size={22} /></span>
              <span>请先选择一个知识库开始管理文档</span>
              <span class="kb-empty-sub">可在左侧导航栏选择，或从下方列表打开</span>
            </div>
            <div style="display:grid;grid-template-columns:repeat(auto-fill,minmax(240px,1fr));gap:10px">
              {#each kbs as kb}
                <button class="kb-card" style="padding:12px 14px;text-align:left;cursor:pointer;display:flex;align-items:center;gap:10px;border:1px solid var(--kb-border);transition:border-color .12s"
                  onmouseover={(e) => (e.currentTarget as HTMLElement).style.borderColor = 'var(--kb-accent)'}
                  onmouseout={(e) => (e.currentTarget as HTMLElement).style.borderColor = 'var(--kb-border)'}
                  onfocus={(e) => (e.currentTarget as HTMLElement).style.borderColor = 'var(--kb-accent)'}
                  onblur={(e) => (e.currentTarget as HTMLElement).style.borderColor = 'var(--kb-border)'}
                  onclick={() => openKbFromDashboard(kb.id)}>
                  <span style="width:34px;height:34px;border-radius:9px;background:var(--kb-hover-strong);color:var(--kb-accent-bright);display:inline-flex;align-items:center;justify-content:center;font-weight:700;font-size:14px;flex:none">{kb.name.charAt(0).toUpperCase()}</span>
                  <span style="flex:1;min-width:0">
                    <span style="display:block;font-size:13px;font-weight:600;color:var(--kb-text);overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{kb.name}</span>
                    <span style="font-size:11.5px;color:var(--kb-text-3)">{kb.docCount} 文档</span>
                  </span>
                  <KbIcon name="arrowRight" size={14} color="var(--kb-text-3)" />
                </button>
              {/each}
            </div>
            {#if kbs.length === 0}
              <div style="font-size:12px;color:var(--kb-text-3);text-align:center">还没有知识库，请到「首页」或左侧导航栏点击「+」新建</div>
            {/if}
          </div>
        {:else}
          <KbDocs {selectedKb} {notify} refreshKbs={loadKbs} {selProvider} {selModel} onTotalDocs={(n) => docTotal = n} openDocId={pendingDoc} />
        {/if}
      {:else if nav === 'chat'}
        <KbChat {selectedKb} {kbs} {notify} {models} openSession={chatTarget}
          onSessionsChanged={loadSessions}
          onOpenDoc={(id, kbId) => { if (kbId != null) selectedKb = kbId; pendingDoc = { id, ts: Date.now() }; nav = 'docs'; }} />
      {:else if nav === 'wiki'}
        <WikiPanel kbId={selectedKb} />
      {:else if nav === 'activity'}
        <KbActivity {selectedKb} {notify} />
      {:else if nav === 'settings'}
        <KbSettings {models} {setModel} {notify} />
      {/if}
    </main>
  </div>

  <!-- 新建知识库 -->
  {#if createKbOpen}
    <KbModal open={createKbOpen} onClose={() => { if (!createKbBusy) createKbOpen = false; }} ariaLabel="关闭新建知识库弹窗">
      <div class="kb-modal">
        <div class="kb-modal-hd"><KbIcon name="plus" size={16} color="var(--kb-accent-bright)" />新建知识库</div>
        <div class="kb-modal-bd">
          <div style="display:flex;flex-direction:column;gap:12px">
            <label class="kb-label">知识库名称
              <input class="kb-input" placeholder="例如：产品手册、技术文档…" maxlength="50" bind:value={newKbName}
                onkeydown={(e) => e.key === 'Enter' && doCreateKb()} />
            </label>
            <label class="kb-label">描述（可选）
              <textarea class="kb-textarea" rows="3" maxlength="200" placeholder="简单介绍用途、范围…" bind:value={newKbDesc}></textarea>
            </label>
            {#if createKbErr}<div class="kb-msg err">{createKbErr}</div>{/if}
          </div>
        </div>
        <div class="kb-modal-ft">
          <button class="kb-btn-md" onclick={() => createKbOpen = false} disabled={createKbBusy}>取消</button>
          <button class="kb-btn" onclick={doCreateKb} disabled={createKbBusy}>{createKbBusy ? '创建中…' : '创建'}</button>
        </div>
      </div>
    </KbModal>
  {/if}

  <!-- 编辑知识库 -->
  {#if editKbOpen}
    <KbModal open={editKbOpen} onClose={() => { if (!editKbBusy) editKbOpen = false; }} ariaLabel="关闭编辑知识库弹窗">
      <div class="kb-modal">
        <div class="kb-modal-hd"><KbIcon name="edit" size={16} color="var(--kb-accent-bright)" />编辑知识库</div>
        <div class="kb-modal-bd">
          <div style="display:flex;flex-direction:column;gap:12px">
            <label class="kb-label">名称
              <input class="kb-input" maxlength="50" bind:value={editKbName} onkeydown={(e) => e.key === 'Enter' && doEditKb()} />
            </label>
            <label class="kb-label">描述（可选）
              <textarea class="kb-textarea" rows="3" maxlength="500" bind:value={editKbDesc}></textarea>
            </label>
            {#if editKbErr}<div class="kb-msg err">{editKbErr}</div>{/if}
          </div>
        </div>
        <div class="kb-modal-ft">
          <button class="kb-btn-md" onclick={() => editKbOpen = false} disabled={editKbBusy}>取消</button>
          <button class="kb-btn" onclick={doEditKb} disabled={editKbBusy}>{editKbBusy ? '保存中…' : '保存'}</button>
        </div>
      </div>
    </KbModal>
  {/if}

  <!-- 删除知识库 -->
  {#if delKbTarget}
    <KbModal open={delKbTarget !== null} onClose={() => { if (!delKbBusy) delKbTarget = null; }} ariaLabel="关闭删除知识库弹窗">
      <div class="kb-modal">
        <div class="kb-modal-hd"><KbIcon name="trash" size={16} color="var(--app-danger)" />删除知识库</div>
        <div class="kb-modal-bd">
          <p style="font-size:13px;line-height:1.7">
            确定删除知识库 <b>{delKbTarget.name}</b>（共 {delKbTarget.docCount} 个文档）？
            删除后目录、文档、分片与问答记录将被永久清除，且无法恢复。
          </p>
          {#if delKbErr}<div class="kb-msg err">{delKbErr}</div>{/if}
        </div>
        <div class="kb-modal-ft">
          <button class="kb-btn-md" onclick={() => delKbTarget = null} disabled={delKbBusy}>取消</button>
          <button class="kb-btn kb-btn-danger" onclick={doDeleteKb} disabled={delKbBusy}>{delKbBusy ? '删除中…' : '确认删除'}</button>
        </div>
      </div>
    </KbModal>
  {/if}

  <!-- Toast -->
  <div class="kb-toasts">
    {#each toasts as t}
      <div class="kb-toast {t.type}">{t.msg}</div>
    {/each}
  </div>
</div>
