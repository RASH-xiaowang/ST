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
  import KbLogin from './KbLogin.svelte';
  import KbMembers from './KbMembers.svelte';
  import KbFaq from './KbFaq.svelte';
  import KbAcl from './KbAcl.svelte';
  import KbHelp from './KbHelp.svelte';
  import KbErrorBoundary from './KbErrorBoundary.svelte';
  import { loadKbChunkCfg } from './kbChunkStore.svelte';
  import { confirmState, confirmOk, confirmCancel } from './KbConfirm.svelte';
  import { Button } from '../components/ui/button';
  import { Badge } from '../components/ui/badge';
  import { Empty, EmptyTitle, EmptyDescription } from '../components/ui/empty';
  import { ScrollArea } from '../components/ui/scroll-area';
  import { TooltipProvider, Tooltip, TooltipContent, TooltipTrigger } from '../components/ui/tooltip';
  import './kbui.css';

  const NAV = [
    { id: 'home', ico: 'dashboard', label: '首页' },
    { id: 'chat', ico: 'chat', label: 'AI问答' },
    { id: 'kb', ico: 'kb', label: '知识库' },
  ] as const;
  // 'docs' / 'wiki' 不是主导航项，而是选择知识库后的工作区视图
  type NavId = (typeof NAV)[number]['id'] | 'activity' | 'settings' | 'docs' | 'wiki';

  // ─── 全局状态 ───
let nav = $state<NavId>('home');
  let sidebarOpen = $state(false);
  let kbs = $state<KbSummary[]>([]);
  let sessions = $state<QaSessionItem[]>([]);
  // 从侧边栏「历史对话」打开指定会话（ts 用于重复点击同一会话时仍触发）
  let chatTarget = $state<{ id: number; ts: number } | null>(null);
  // 从 AI 问答引用跳转打开指定文档（ts 用于重复触发）
  let pendingDoc = $state<{ id: number; ts: number } | null>(null);
  // 顶部全局搜索回车后传入文档列表的关键词（ts 用于重复触发）
  let kbSearchInit = $state<{ query: string; ts: number } | null>(null);
  let docTotal = $state(0);
  let selectedKb = $state<number | null>(null);
  let models = $state<ModelInfo[]>([]);
  let selProvider = $state('');
  let selModel = $state('');
  let membersOpen = $state(false); // 成员管理弹窗
  let faqOpen = $state(false); // FAQ 管理弹窗
  let aclOpen = $state(false); // ACL 权限弹窗
  let helpOpen = $state(false); // 帮助文档弹窗

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
  const navTitle = $derived(NAV.find((n) => n.id === nav)?.label ?? (nav === 'docs' ? '文档' : nav === 'wiki' ? 'Wiki' : ''));
  const curKbName = $derived(kbs.find((k) => k.id === selectedKb)?.name ?? '');
  async function togglePin(kb: KbSummary) {
    try {
      await kbApi.setPin(kb.id, !kb.pinned);
      await loadKbs();
      notify(kb.pinned ? '已取消置顶' : '已置顶：' + kb.name);
    } catch (e: unknown) { notify('置顶失败：' + e, 'error'); }
  }

  // ─── 导出知识库 ───
  let exportBusy = $state(false);
  async function doExportKb(kb: KbSummary) {
    if (exportBusy) return;
    exportBusy = true;
    try {
      notify('正在导出「' + kb.name + '」…');
      const res = await kbApi.exportKb(kb.id);
      const bin = Uint8Array.from(atob(res.dataBase64), (c) => c.charCodeAt(0));
      const blob = new Blob([bin], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url; a.download = res.fileName; a.click();
      URL.revokeObjectURL(url);
      notify('导出完成：' + res.fileName);
    } catch (e: unknown) { notify('导出失败：' + e, 'error'); }
    finally { exportBusy = false; }
  }

  // ─── 导入知识库 ───
  let importBusy = $state(false);
  async function onImportPick(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file || importBusy) return;
    importBusy = true;
    try {
      notify('正在导入「' + file.name + '」…');
      const buf = await file.arrayBuffer();
      const bytes = new Uint8Array(buf);
      const CHUNK = 32766; // 必须是 3 的倍数，保证 base64 无 padding
      let b64 = '';
      for (let i = 0; i < bytes.length; i += CHUNK) {
        const slice = bytes.subarray(i, Math.min(i + CHUNK, bytes.length));
        b64 += btoa(Array.from(slice, (b) => String.fromCharCode(b)).join(''));
      }
      const res = await kbApi.importKb(b64);
      await loadKbs();
      notify(`导入完成：「${res.name}」（${res.documents} 文档，${res.wikiPages} Wiki 页）`);
    } catch (err: unknown) { notify('导入失败：' + err, 'error'); }
    finally { importBusy = false; }
  }
</script>

<TooltipProvider>
<div class="kb-root">
  <!-- 移动端汉堡菜单按钮 -->
  <button class="kb-mobile-menu-btn" onclick={() => sidebarOpen = !sidebarOpen}
    aria-label={sidebarOpen ? '关闭导航' : '打开导航'}>
    <KbIcon name={sidebarOpen ? 'close' : 'menu'} size={20} />
  </button>

  <!-- 移动端遮罩 -->
  {#if sidebarOpen}
    <div class="kb-sidebar-overlay" onclick={() => sidebarOpen = false} role="presentation"></div>
  {/if}

  <!-- 左侧导航栏 -->
  <aside class="kb-sidebar" class:open={sidebarOpen}>
    <div class="kb-brand" role="button" tabindex={0} title="回到首页"
      onclick={() => { nav = 'home'; sidebarOpen = false; }}
      onkeydown={(e) => e.key === 'Enter' && (nav = 'home')}>
      <span class="kb-brand-ico"><KbIcon name="kb" size={16} weight="bold" /></span>
      <span class="kb-brand-text">
        <span class="kb-brand-name">知识库中心</span>
        <span class="kb-brand-sub">本地知识库 · 语义检索</span>
      </span>
    </div>
    <ScrollArea class="flex-1 min-h-0">
    <div class="kb-sidebar-scroll">
      <div class="kb-sidebar-section">主导航</div>
      {#each NAV as item}
        <Tooltip>
          <TooltipTrigger>
            {#snippet child({ props })}
              <button {...props} class="kb-sidebar-item" class:active={nav === item.id}
                onclick={() => { goNav(item.id); sidebarOpen = false; }}>
                <KbIcon name={item.ico} size={15} /><span class="kb-sidebar-label">{item.label}</span>
              </button>
            {/snippet}
          </TooltipTrigger>
          <TooltipContent side="right" class="kb-sidebar-tooltip">{item.label}</TooltipContent>
        </Tooltip>
      {/each}

      <div class="kb-sidebar-section kb-sidebar-section-row">
        <span>历史对话</span>
        <button class="kb-sidebar-add" onclick={() => { chatTarget = null; goNav('chat'); sidebarOpen = false; }} title="开始新对话">
          <KbIcon name="plus" size={13} weight="bold" />
        </button>
      </div>
      {#each sessions as s}
        <button class="kb-sidebar-item" class:active={chatTarget?.id === s.id && nav === 'chat'}
          onclick={() => { openChatSession(s); sidebarOpen = false; }} title={s.title ?? ''}>
          <span class="kb-sidebar-kb-ico"><KbIcon name="chatCircle" size={14} /></span>
          <span class="kb-sidebar-kb-name">{s.title ?? ('会话 #' + s.id)}</span>
          <span class="kb-sidebar-kb-count">{fmtSessionTime(s.updatedAt)}</span>
        </button>
      {/each}
      {#if sessions.length === 0}
        <div class="kb-sidebar-empty">暂无历史对话</div>
      {/if}
    </div>
    </ScrollArea>
    <div class="kb-sidebar-bottom">
      <Tooltip>
        <TooltipTrigger>
          {#snippet child({ props })}
            <button {...props} class="kb-sidebar-item" class:active={nav === 'activity'}
              onclick={() => { goNav('activity'); sidebarOpen = false; }}>
              <KbIcon name="activity" size={15} /><span class="kb-sidebar-label">活动</span>
            </button>
          {/snippet}
        </TooltipTrigger>
        <TooltipContent side="right" class="kb-sidebar-tooltip">处理任务与检索历史</TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger>
          {#snippet child({ props })}
            <button {...props} class="kb-sidebar-item" onclick={() => { helpOpen = true; sidebarOpen = false; }}>
              <KbIcon name="info" size={15} /><span class="kb-sidebar-label">帮助</span>
            </button>
          {/snippet}
        </TooltipTrigger>
        <TooltipContent side="right" class="kb-sidebar-tooltip">使用帮助</TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger>
          {#snippet child({ props })}
            <button {...props} class="kb-sidebar-item" class:active={nav === 'settings'}
              onclick={() => { goNav('settings'); sidebarOpen = false; }}>
              <KbIcon name="settings" size={15} /><span class="kb-sidebar-label">设置</span>
            </button>
          {/snippet}
        </TooltipTrigger>
        <TooltipContent side="right" class="kb-sidebar-tooltip">模型与分块设置</TooltipContent>
      </Tooltip>
    </div>
  </aside>

  <!-- 右侧内容区 -->
  <div class="kb-main">
    <header class="kb-contentbar">
      <!-- 面包屑导航 -->
      <div class="kb-breadcrumb">
        {#if selectedKb !== null && (nav === 'docs' || nav === 'wiki')}
          <button class="kb-breadcrumb-link" onclick={() => goNav('home')}>知识库</button>
          <span class="kb-breadcrumb-sep">/</span>
          <span class="kb-breadcrumb-current">{curKbName}</span>
        {:else}
          <span class="kb-breadcrumb-current">{navTitle}</span>
        {/if}
      </div>

      <div style="flex:1"></div>

      {#if selectedKb !== null && (nav === 'docs' || nav === 'wiki')}
        <div class="kb-tabs-bar">
          <button class="kb-tab-item" class:active={nav === 'docs'} onclick={() => goNav('docs')}>
            <KbIcon name="docs" size={14} />文档 ({docTotal})
          </button>
          <button class="kb-tab-item" class:active={nav === 'wiki'} onclick={() => goNav('wiki')}>
            <KbIcon name="wiki" size={14} />Wiki
            <Badge variant="default" class="text-[10px] px-1 py-0 ml-1">NEW</Badge>
          </button>
          <button class="kb-tab-item" onclick={() => membersOpen = true}>
            <KbIcon name="users" size={14} />成员
          </button>
          <button class="kb-tab-item" onclick={() => faqOpen = true}>
            <KbIcon name="list" size={14} />FAQ
          </button>
          <button class="kb-tab-item" onclick={() => aclOpen = true}>
            <KbIcon name="shield" size={14} />权限
          </button>
        </div>
      {/if}

      <!-- 用户头像 -->
      <div class="kb-user-area">
        <div class="kb-avatar">{kbUser.user?.username?.charAt(0).toUpperCase() ?? 'A'}</div>
        <div class="kb-user-info">
          <div class="kb-user-name">{kbUser.user?.username ?? '本机'}{kbUser.user?.isAdmin ? ' · 管理员' : ''}</div>
          <div class="kb-user-sub">单机部署</div>
        </div>
        <KbLogin {notify} onLoginSuccess={() => { loadKbs(); loadSessions(); }} />
      </div>
    </header>

    <main class="kb-content kb-scroll">
      <KbErrorBoundary>
      {#if nav === 'home'}
        <KbDashboard {kbs} {selectedKb} refreshKbs={loadKbs} onOpenKb={openKbFromDashboard}
          onEditKb={openEditKb} onDeleteKb={openDeleteKb} onTogglePin={togglePin} onExportKb={doExportKb} {notify} isAdmin={kbUser.user?.isAdmin ?? false} />
      {:else if nav === 'kb'}
        <!-- 知识库管理面板：新建/编辑/删除/置顶/导出 -->
        <KbDashboard {kbs} {selectedKb} refreshKbs={loadKbs} onOpenKb={openKbFromDashboard}
          onNewKb={openNewKb} onImportKb={onImportPick} onEditKb={openEditKb} onDeleteKb={openDeleteKb} onTogglePin={togglePin} onExportKb={doExportKb} {notify} mode="kbs" isAdmin={kbUser.user?.isAdmin ?? false} />
      {:else if nav === 'docs'}
        {#if selectedKb === null}
          <div class="kb-select-kb-prompt">
            <Empty>
              <KbIcon name="folderOpen" size={32} color="var(--kb-text-3)" />
              <EmptyTitle>请先选择一个知识库</EmptyTitle>
              <EmptyDescription>从下方列表打开知识库，开始管理文档</EmptyDescription>
            </Empty>
            <div class="kb-select-kb-grid">
              {#each kbs as kb}
                <Button variant="outline" class="justify-start h-auto py-3" onclick={() => openKbFromDashboard(kb.id)}>
                  <div class="kb-kb-monogram" style="width:28px;height:28px;font-size:12px;border-radius:6px">{kb.name.charAt(0).toUpperCase()}</div>
                  <div class="text-left">
                    <div class="text-sm font-semibold">{kb.name}</div>
                    <div class="text-xs text-muted-foreground">{kb.docCount} 文档</div>
                  </div>
                </Button>
              {/each}
            </div>
            {#if kbs.length === 0}
              <p class="text-xs text-muted-foreground text-center mt-2">还没有知识库，请到「知识库」面板新建</p>
            {/if}
          </div>
        {:else}
          <KbDocs {selectedKb} {notify} refreshKbs={loadKbs} {selProvider} {selModel} onTotalDocs={(n) => docTotal = n} openDocId={pendingDoc} searchInit={kbSearchInit} />
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
        <KbSettings {models} {setModel} {notify} isAdmin={kbUser.user?.isAdmin ?? false} />
      {/if}
      </KbErrorBoundary>
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
          <Button variant="outline" onclick={() => createKbOpen = false} disabled={createKbBusy}>取消</Button>
          <Button onclick={doCreateKb} disabled={createKbBusy}>{createKbBusy ? '创建中…' : '创建'}</Button>
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
          <Button variant="outline" onclick={() => editKbOpen = false} disabled={editKbBusy}>取消</Button>
          <Button onclick={doEditKb} disabled={editKbBusy}>{editKbBusy ? '保存中…' : '保存'}</Button>
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
          <Button variant="outline" onclick={() => delKbTarget = null} disabled={delKbBusy}>取消</Button>
          <Button variant="destructive" onclick={doDeleteKb} disabled={delKbBusy}>{delKbBusy ? '删除中…' : '确认删除'}</Button>
        </div>
      </div>
    </KbModal>
  {/if}

  <!-- 成员管理弹窗 -->
  {#if membersOpen && selectedKb !== null}
    <KbModal open={membersOpen} onClose={() => membersOpen = false} ariaLabel="关闭成员管理弹窗">
      <div class="kb-modal" style="min-width:480px">
        <div class="kb-modal-hd"><KbIcon name="users" size={16} color="var(--kb-accent-bright)" />成员管理</div>
        <div class="kb-modal-bd">
          <KbMembers kbId={selectedKb} isAdmin={kbUser.user?.isAdmin ?? false} {notify} />
        </div>
        <div class="kb-modal-ft">
          <Button onclick={() => membersOpen = false}>关闭</Button>
        </div>
      </div>
    </KbModal>
  {/if}

  <!-- FAQ 管理弹窗 -->
  {#if faqOpen && selectedKb !== null}
    <KbModal open={faqOpen} onClose={() => faqOpen = false} ariaLabel="关闭 FAQ 管理弹窗">
      <div class="kb-modal" style="min-width:560px">
        <div class="kb-modal-hd"><KbIcon name="list" size={16} color="var(--kb-accent-bright)" />FAQ 问答对</div>
        <div class="kb-modal-bd">
          <KbFaq kbId={selectedKb} isAdmin={kbUser.user?.isAdmin ?? false} {notify} />
        </div>
        <div class="kb-modal-ft">
          <Button onclick={() => faqOpen = false}>关闭</Button>
        </div>
      </div>
    </KbModal>
  {/if}

  <!-- ACL 权限弹窗 -->
  {#if aclOpen && selectedKb !== null}
    <KbModal open={aclOpen} onClose={() => aclOpen = false} ariaLabel="关闭 ACL 权限弹窗">
      <div class="kb-modal" style="min-width:500px">
        <div class="kb-modal-hd"><KbIcon name="shield" size={16} color="var(--kb-accent-bright)" />ACL 权限管理</div>
        <div class="kb-modal-bd">
          <KbAcl kbId={selectedKb} {notify} />
        </div>
        <div class="kb-modal-ft">
          <Button onclick={() => aclOpen = false}>关闭</Button>
        </div>
      </div>
    </KbModal>
  {/if}

  <!-- 帮助文档弹窗 -->
  <KbHelp open={helpOpen} onClose={() => helpOpen = false} />

  <!-- 全局确认弹窗（替代原生 confirm） -->
  {#if confirmState.open}
    <KbModal open={confirmState.open} onClose={confirmCancel} ariaLabel="关闭确认弹窗">
      <div class="kb-modal">
        <div class="kb-modal-hd">
          <KbIcon name={confirmState.danger ? 'trash' : 'warn'} size={16}
            color={confirmState.danger ? 'var(--app-danger)' : 'var(--kb-accent-bright)'} />
          {confirmState.title}
        </div>
        <div class="kb-modal-bd">
          <p style="font-size:13px;line-height:1.7;white-space:pre-wrap">{confirmState.message}</p>
        </div>
        <div class="kb-modal-ft">
          <Button variant="outline" onclick={confirmCancel}>{confirmState.cancelText}</Button>
          <Button variant={confirmState.danger ? 'destructive' : 'default'} onclick={confirmOk}>{confirmState.confirmText}</Button>
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
</TooltipProvider>
