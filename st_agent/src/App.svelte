<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import {
    connectionState,
    messageHistory,
    start,
    stop,
    send,
    getTaskPath,
    setTaskPath,
    getHostname,
    setHostname,
    type TaskPathInfo,
    type ProtocolMessage,
    type ConnectionState,
  } from './lib/communication';
  import RoleManager from './lib/components/RoleManager.svelte';

  // ---------- 通知 ----------
  let notifications = $state<Array<{ id: number; title: string; message: string; type: 'success' | 'warn' | 'info' }>>([]);

  function notify(title: string, message: string, type: 'success' | 'warn' | 'info') {
    const id = Date.now() + Math.random();
    notifications = [...notifications, { id, title, message, type }];
    setTimeout(() => { notifications = notifications.filter(n => n.id !== id); }, 4000);
  }

  // ---------- 端口 ----------
  let agentPort = $state(window.location.port || 'built');
  let hostname = $state('');

  // ---------- 通知 ----------

  // ---------- 任务路径 ----------
  let taskPathInfo = $state<TaskPathInfo | null>(null);
  let editingPath = $state(false);
  let newPath = $state('');
  let pathBusy = $state(false);

  async function loadPathInfo() {
    try { taskPathInfo = await getTaskPath(); } catch {}
  }

  function startEdit() {
    newPath = taskPathInfo?.path || '';
    editingPath = true;
  }

  function cancelEdit() { editingPath = false; }

  async function confirmEdit() {
    if (!newPath.trim()) return;
    pathBusy = true;
    try {
      const result = await setTaskPath(newPath.trim());
      taskPathInfo = result;
      editingPath = false;
      notify('路径已更新', `已迁移至: ${result.path}`, 'success');
    } catch (err) {
      notify('路径修改失败', String(err), 'error');
    } finally {
      pathBusy = false;
    }
  }

  // ---------- 打开文件夹 ----------
  async function openFolder() {
    const path = taskPathInfo?.path;
    if (!path) return;
    try {
      await invoke('ipc_open_folder', { path });
    } catch (err) {
      notify('打开失败', String(err), 'error');
    }
  }

  // ---------- 生命周期 ----------
  onMount(() => {
    // 禁止 Ctrl+滚轮 / Ctrl+加减号 缩放界面
    const preventZoom = (e: WheelEvent) => { if (e.ctrlKey) e.preventDefault(); };
    const preventKeyZoom = (e: KeyboardEvent) => { if (e.ctrlKey && ['=', '-', '0'].includes(e.key)) e.preventDefault(); };
    document.addEventListener('wheel', preventZoom, { passive: false });
    document.addEventListener('keydown', preventKeyZoom);

    window.addEventListener('st-notification', ((e: CustomEvent) => {
      const { title, message } = e.detail;
      notify(title, message, 'info');
    }) as EventListener);

    // 任务保存后实时刷新路径信息和状态统计
    window.addEventListener('st-task-refresh', ((e: CustomEvent) => {
      taskPathInfo = e.detail;
      loadTaskStatuses();
    }) as EventListener);

    // 先加载主机名，再启动连接，确保首次心跳就携带正确名称
    getHostname().then(n => {
      hostname = n;
      setHostname(n);
    }).catch(() => {}).finally(() => {
      start();
    });
    loadPathInfo();
    loadTaskStatuses();

    const unsub = connectionState.subscribe(state => {
      if (state === 'connected') {
        notify('连接成功', '已接入 ST Control 服务器', 'success');
        // 重连后刷新任务信息（断线期间可能收到任务）
        loadPathInfo();
        loadTaskStatuses();
        const req: ProtocolMessage = {
          type: 'command', id: crypto.randomUUID(), timestamp: Date.now(),
          source: 'st_agent', target: 'st_control', method: 'system.info',
        };
        send(req);
      } else if (state === 'reconnecting') {
        notify('重连中', '正在重新连接 Control 服务器', 'warn');
      }
    });
    return () => unsub();
  });

  onDestroy(() => { stop(); });

  // ---------- 定时刷新任务状态 ----------
  let pollTimer: ReturnType<typeof setInterval> | undefined;
  $effect(() => {
    // activeTab 变化时重启定时器：仅 info 面板需要轮询
    if (activeTab === 'info') {
      pollTimer = setInterval(loadTaskStatuses, 3000);
    } else {
      if (pollTimer !== undefined) {
        clearInterval(pollTimer);
        pollTimer = undefined;
      }
    }
    return () => {
      if (pollTimer !== undefined) {
        clearInterval(pollTimer);
        pollTimer = undefined;
      }
    };
  });

  // ---------- 面板切换 ----------
  let activeTab = $state<'info' | 'sys' | 'log' | 'roles'>('info');

  // ---------- 任务执行统计（实时从磁盘读取） ----------
  interface TaskStatusSummary { total: number; completed: number; failed: number; running: number; pending: number; running_file_name?: string; running_file_path?: string; }
  let taskStatusSummary = $state<TaskStatusSummary | null>(null);

  let taskStats = $derived({
    total: taskStatusSummary?.total ?? 0,
    completed: taskStatusSummary?.completed ?? 0,
    failed: taskStatusSummary?.failed ?? 0,
    running: taskStatusSummary?.running ?? 0,
    pending: taskStatusSummary?.pending ?? 0,
  });

  async function loadTaskStatuses() {
    try {
      taskStatusSummary = await invoke<TaskStatusSummary>('ipc_get_task_statuses');
      // 从汇总中同步更新当前运行文件信息
      if (taskStatusSummary.running_file_name && taskStatusSummary.running_file_path) {
        currentFile = { name: taskStatusSummary.running_file_name, path: taskStatusSummary.running_file_path };
      } else {
        currentFile = null;
      }
    } catch {}
  }

  /** 当前运行文件信息 */
  let currentFile = $state<{ name: string; path: string } | null>(null);

  // ---------- SVG 环形图计算 ----------
  const CHART_RADIUS = 46;
  const CHART_CIRCUMFERENCE = 2 * Math.PI * CHART_RADIUS;

  type ChartSeg = {
    key: string; value: number; color: string; label: string;
    dasharray: string; offset: number;
  };
  let chartSegments = $derived.by<ChartSeg[]>(() => {
    const { total, completed, failed, running, pending } = taskStats;
    const defs: { key: string; value: number; color: string; label: string }[] = [
      { key: 'running',   value: running,   color: '#3b82f6', label: '执行中' },
      { key: 'completed', value: completed, color: '#22c55e', label: '已完成' },
      { key: 'failed',    value: failed,    color: '#ef4444', label: '失败' },
      { key: 'pending',   value: pending,   color: '#f59e0b', label: '待执行' },
    ];
    if (total === 0) {
      return defs.map(d => ({ ...d, dasharray: '0 999', offset: 0 }));
    }
    let acc = 0;
    return defs.map(d => {
      const len = (d.value / total) * CHART_CIRCUMFERENCE;
      const off = -acc;
      acc += len;
      return {
        ...d, dasharray: `${Math.max(len, 0.5)} ${CHART_CIRCUMFERENCE}`, offset: off,
      };
    });
  });

  // ---------- 状态文件列表弹窗 ----------
  interface TaskFileEntry {
    filename: string; filepath: string; method: string;
    task_id: string; status: string; received_at: string;
  }

  let modalVisible = $state(false);
  let modalStatusKey = $state('');
  let modalStatusLabel = $state('');
  let modalStatusColor = $state('');
  let fileList = $state<TaskFileEntry[]>([]);
  let loadingFiles = $state(false);

  /** 选中的文件详情 */
  let selectedFileContent = $state<string | null>(null);
  let selectedFileName = $state('');
  let loadingContent = $state(false);

  const STATUS_MAP: Record<string, { label: string; color: string }> = {
    running:   { label: '执行中', color: '#3b82f6' },
    completed: { label: '已完成', color: '#22c55e' },
    failed:    { label: '失败',   color: '#ef4444' },
    pending:   { label: '待执行', color: '#f59e0b' },
  };

  async function openFileList(key: string) {
    modalStatusKey = key;
    const info = STATUS_MAP[key];
    modalStatusLabel = info?.label ?? key;
    modalStatusColor = info?.color ?? '#94a3b8';
    modalVisible = true;
    fileList = [];
    selectedFileContent = null;
    selectedFileName = '';
    loadingFiles = true;

    try {
      const list = await invoke<TaskFileEntry[]>('ipc_get_task_files_by_status', { status: key });
      fileList = list;
    } catch {
      fileList = [];
    } finally {
      loadingFiles = false;
    }
  }

  function closeModal() {
    modalVisible = false;
    fileList = [];
    selectedFileContent = null;
    selectedFileName = '';
  }

  async function showFileDetail(entry: TaskFileEntry) {
    if (loadingContent) return;
    selectedFileName = entry.filename;
    selectedFileContent = null;
    loadingContent = true;
    try {
      const content = await invoke<string>('ipc_get_task_file_content', { filePath: entry.filepath });
      selectedFileContent = content;
    } catch {
      selectedFileContent = '// 读取失败';
    } finally {
      loadingContent = false;
    }
  }

  function backToList() {
    selectedFileContent = null;
    selectedFileName = '';
  }

  // ---------- 视图模型 ----------
  let connState = $derived($connectionState);
  let msgList = $derived($messageHistory);

  let stateText  = $derived.by(() => {
    const m: Record<ConnectionState, string> = { connected:'已连接', connecting:'连接中', disconnected:'未连接', reconnecting:'重连中', error:'异常' };
    return m[connState] || connState;
  });
  let stateCls = $derived.by(() => {
    const m: Record<ConnectionState, string> = { connected:'tag-success', connecting:'tag-warn', disconnected:'tag-default', reconnecting:'tag-warn', error:'tag-danger' };
    return m[connState] || 'tag-default';
  });
  let stateHint = $derived.by(() => {
    if (connState === 'connected')   return '与 Control 服务器通信正常';
    if (connState === 'reconnecting')  return '正在重连，请稍候';
    if (connState === 'connecting')    return '正在建立连接...';
    if (connState === 'error')         return '连接异常，请确认 Control 已启动';
    return '等待连接';
  });
</script>

<div class="layout">
  <!-- ===== 通知容器 ===== -->
  {#if notifications.length}
    <div class="toast-container">
      {#each notifications as n}
        <div class="toast toast-{n.type}" role="alert" onclick={() => notifications = notifications.filter(x => x.id !== n.id)}>
          <span class="toast-icon">{n.type === 'success' ? '✓' : n.type === 'warn' ? '!' : 'ℹ'}</span>
          <div class="toast-body"><div class="toast-title">{n.title}</div><div class="toast-desc">{n.message}</div></div>
        </div>
      {/each}
    </div>
  {/if}

  <!-- ===== 顶部导航 ===== -->
  <header class="navbar">
    <div class="navbar-left">
      <h1 class="navbar-title">ST Agent</h1>
      <span class="badge badge-outline">v1.0.0</span>
    </div>
    <div class="navbar-right">
      <span class="port-badge mono">:{agentPort}</span>
      <span class="tag {stateCls}">{stateText}</span>
    </div>
  </header>

  <!-- ===== 主体 ===== -->
  <div class="main">
    <!-- Tab 导航 -->
    <div class="tab-bar">
      <button class="tab" class:active={activeTab === 'info'} onclick={() => activeTab = 'info'} role="tab" aria-selected={activeTab === 'info'}>
        信息面板
      </button>
      <button class="tab" class:active={activeTab === 'sys'} onclick={() => activeTab = 'sys'} role="tab" aria-selected={activeTab === 'sys'}>
        系统信息
      </button>
      <button class="tab" class:active={activeTab === 'log'} onclick={() => activeTab = 'log'} role="tab" aria-selected={activeTab === 'log'}>
        通信记录
        {#if msgList.length > 0}<span class="tab-badge">{msgList.length}</span>{/if}
      </button>
      <button class="tab" class:active={activeTab === 'roles'} onclick={() => activeTab = 'roles'} role="tab" aria-selected={activeTab === 'roles'}>
        🎭 AI 角色定位
      </button>
    </div>

    {#if activeTab === 'info'}
      <!-- ===== 信息面板：Dashboard 双列布局 ===== -->

      <!-- 顶部：紧凑连接状态条 -->
      <div class="card info-topbar">
        <div class="info-topbar-body">
          <div class="info-topbar-icon" class:s-icon-ok={connState === 'connected'} class:s-icon-err={connState !== 'connected'}>
            {connState === 'connected' ? '✓' : connState === 'connecting' || connState === 'reconnecting' ? '⟳' : '—'}
          </div>
          <div class="info-topbar-info">
            <span class="info-topbar-title">{stateText}</span>
            <span class="info-topbar-desc">{stateHint}</span>
          </div>
          <span class="info-topbar-target mono">127.0.0.1:9786</span>
          <span class="tag {stateCls}">{stateText}</span>
        </div>
      </div>

      <!-- 双列主体 -->
      <div class="info-grid">
        <!-- 左列：任务存储路径 -->
        <div class="card info-left">
          <div class="card-hd"><h2 class="card-title">任务存储路径</h2></div>

          {#if editingPath}
            <div class="path-edit">
              <input type="text" bind:value={newPath} class="path-input" disabled={pathBusy} />
              <div class="form-actions">
                <button class="btn btn-sm btn-primary" onclick={confirmEdit} disabled={pathBusy || !newPath.trim()}>
                  {pathBusy ? '迁移中...' : '确认修改'}
                </button>
                <button class="btn btn-sm btn-default" onclick={cancelEdit} disabled={pathBusy}>取消</button>
              </div>
              {#if pathBusy}
                <div class="path-hint migrating">正在迁移数据文件，请勿中断...</div>
              {/if}
            </div>
          {:else if taskPathInfo}
            <div class="path-box">
              <div class="path-box-icon">📁</div>
              <div class="path-box-body">
                <div class="path-box-path mono">{taskPathInfo.path}</div>
                <div class="path-box-label">存储路径</div>
              </div>
              <div class="path-box-actions">
                <button class="btn-icon" onclick={openFolder} title="打开文件夹">📂</button>
                <button class="btn-icon" onclick={startEdit} title="修改路径">✎</button>
              </div>
            </div>
            <div class="path-stats">
              <span class="path-stat" class:stat-ok={taskPathInfo.exists} class:stat-err={!taskPathInfo.exists}>
                ● {taskPathInfo.exists ? '正常' : '不可用'}
              </span>
              <span class="path-stat-divider"></span>
              <span class="path-stat">
                <span class="path-stat-num">{taskPathInfo.item_count}</span> 个文件
              </span>
            </div>
          {:else}
            <div class="path-box">
              <div class="path-box-icon">📁</div>
              <div class="path-box-body">
                <div class="path-box-path mono muted">加载中...</div>
                <div class="path-box-label">存储路径</div>
              </div>
            </div>
          {/if}

          <!-- 底部：当前运行文件 -->
          <div class="info-running">
            <div class="info-running-hd">
              <span class="running-icon">{currentFile ? '▶' : '⏸'}</span>
              <span class="running-lbl-inline">当前运行文件</span>
              {#if currentFile}
                <span class="running-badge running">运行中</span>
              {:else}
                <span class="running-badge idle">空闲</span>
              {/if}
            </div>
            {#if currentFile}
              <div class="running-path mono">{currentFile.path.split(/[/\\]/).pop()}</div>
            {:else}
              <div class="running-empty">暂无运行中的文件</div>
            {/if}
          </div>
        </div>

        <!-- 右列：任务执行概览图 -->
        <div class="card card-grow info-right">
          <div class="card-hd">
            <h2 class="card-title">任务执行概览</h2>
            <div class="help-tip">
              <span class="help-icon">?</span>
              <div class="help-popup">
                <div class="help-popup-title">统计说明</div>
                <ul class="help-popup-list">
                  <li>执行中 — JSON 文件中 <code>status</code> 为 <code>"running"</code> 的任务数</li>
                  <li>已完成 — JSON 文件中 <code>status</code> 为 <code>"completed"</code> 的任务数</li>
                  <li>失败 — JSON 文件中 <code>status</code> 为 <code>"failed"</code> 的任务数</li>
                  <li>待执行 — 其余所有任务（含 <code>"pending"</code>、无 <code>status</code> 字段或读取异常）</li>
                </ul>
                <div class="help-popup-foot">数据来源：实时扫描存储目录下所有 <code>.json</code> 文件</div>
              </div>
            </div>
          </div>
          <div class="chart-body">
            <div class="chart-donut-wrap">
              <svg viewBox="0 0 120 120" class="chart-svg">
                <circle cx="60" cy="60" r={CHART_RADIUS} fill="none" stroke="#e2e8f0" stroke-width="10" />
                {#each chartSegments as seg}
                  <circle cx="60" cy="60" r={CHART_RADIUS} fill="none"
                    stroke={seg.color}
                    stroke-width="10" stroke-linecap="round"
                    stroke-dasharray={seg.dasharray}
                    stroke-dashoffset={seg.offset}
                    transform="rotate(-90 60 60)"
                    class="chart-segment"
                    title="{seg.label}: {seg.value}"
                  />
                {/each}
                <text x="60" y="52" text-anchor="middle" class="chart-total-num">{taskStats.total}</text>
                <text x="60" y="70" text-anchor="middle" class="chart-total-lbl">总任务</text>
              </svg>
            </div>
            <div class="chart-legend">
              {#each chartSegments as seg}
                <button class="legend-item" onclick={() => openFileList(seg.key)}>
                  <span class="legend-dot" style="background:{seg.color}"></span>
                  <div class="legend-body">
                    <span class="legend-label">{seg.label}</span>
                    <span class="legend-value">{seg.value}</span>
                  </div>
                </button>
              {/each}
            </div>
          </div>
        </div>
      </div>

    {:else if activeTab === 'sys'}
      <!-- 系统信息面板 -->
      <div class="card card-grow">
        <h2 class="card-title" style="margin-bottom:16px">系统信息</h2>
        <dl class="dl sys-detail">
          <div><dt>应用名称</dt><dd>ST Agent</dd></div>
          <div><dt>版本</dt><dd>1.0.0</dd></div>
          <div><dt>连接状态</dt><dd class={stateCls}>{stateText}</dd></div>
          <div><dt>本地端口</dt><dd class="mono">{agentPort}</dd></div>
          <div><dt>控制端地址</dt><dd class="mono">127.0.0.1:9786</dd></div>
          <div><dt>Agent 名称</dt><dd class="mono">{hostname || '-'}</dd></div>
        </dl>
      </div>
    {:else if activeTab === 'roles'}
      <!-- AI 角色定位面板 -->
      <RoleManager />
    {:else}
      <!-- 通信记录面板（独占剩余空间） -->
      <div class="card card-grow">
        <div class="card-hd"><h2 class="card-title">通信记录</h2><span class="badge badge-num">{msgList.length}</span></div>
        <div class="card-bd">
          {#if msgList.length === 0}
            <div class="empty"><p>暂无通信记录</p></div>
          {:else}
            <div class="log-list">
              {#each msgList as m}
                <div class="log-row log-{m.type}">
                  <span class="log-time mono muted">{new Date(m.timestamp).toLocaleTimeString()}</span>
                  <span class="log-tag">{m.type}</span>
                  <span class="log-dir">{m.source === 'st_agent' ? '→' : '←'}</span>
                  <span class="log-method mono">{m.method || '-'}</span>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      </div>
    {/if}
  </div>
</div>

<!-- ===== 状态文件列表弹窗 ===== -->
{#if modalVisible}
  <div class="modal-overlay" onclick={closeModal} role="dialog" aria-modal="true">
    <div class="modal-panel" onclick={(e) => e.stopPropagation()}>
      <!-- 头部 -->
      <div class="modal-hd">
        {#if selectedFileContent}
          <button class="modal-back" onclick={backToList} title="返回列表">← 返回</button>
          <span class="modal-hd-title" style="color:{modalStatusColor}">{modalStatusLabel} — {selectedFileName}</span>
        {:else}
          <span class="modal-hd-title" style="color:{modalStatusColor}">▎{modalStatusLabel} 文件列表</span>
        {/if}
        <button class="modal-close" onclick={closeModal}>✕</button>
      </div>

      <!-- 文件列表视图 -->
      {#if !selectedFileContent}
        <div class="modal-body">
          {#if loadingFiles}
            <div class="modal-loading">加载中...</div>
          {:else if fileList.length === 0}
            <div class="modal-empty">暂无此状态的任务文件</div>
          {:else}
            <div class="file-list">
              {#each fileList as entry}
                <button class="file-row" onclick={() => showFileDetail(entry)}>
                  <span class="file-row-icon" style="background:{modalStatusColor}80"></span>
                  <div class="file-row-body">
                    <span class="file-row-name mono">{entry.filename}</span>
                    <span class="file-row-meta">
                      <span class="file-row-method">{entry.method}</span>
                      {#if entry.received_at}
                        <span class="file-row-time">· {new Date(entry.received_at).toLocaleString()}</span>
                      {/if}
                    </span>
                  </div>
                  <span class="file-row-arrow">→</span>
                </button>
              {/each}
            </div>
          {/if}
        </div>
      {/if}

      <!-- 文件详情视图 -->
      {#if selectedFileContent !== null}
        <div class="modal-body detail-body">
          {#if loadingContent}
            <div class="modal-loading">读取中...</div>
          {:else}
            <pre class="file-detail-pre"><code>{selectedFileContent}</code></pre>
          {/if}
        </div>
      {/if}
    </div>
  </div>
{/if}

<!-- ============================================================ -->
<!--   CSS 设计系统                                           -->
<!-- ============================================================ -->
<style>
  /* ============================================================
     设计系统令牌
     ============================================================ */
  :global(*) { margin:0; padding:0; box-sizing:border-box; }

  :global(body) {
    /* 字体 */
    font-family: -apple-system, "PingFang SC", "Microsoft YaHei", "Helvetica Neue", sans-serif;
    font-size: 14px; line-height: 1.5715;
    /* 色彩 */
    background: #f1f5f9; color: #0f172a;
    /* 布局 */
    height: 100vh; overflow: hidden;
    -webkit-font-smoothing: antialiased;
    -webkit-text-size-adjust: none;
    touch-action: manipulation;
  }

  /* ---------- 布局 ---------- */
  .layout { height:100vh; display:flex; flex-direction:column; }

  /* ============================================================
     导航栏
     ============================================================ */
  .navbar {
    display:flex; align-items:center; justify-content:space-between;
    height:56px; padding:0 24px;
    background: linear-gradient(135deg, #ffffff 0%, #fafbfc 100%);
    border-bottom:1px solid #e2e8f0;
    flex-shrink:0;
    box-shadow: 0 1px 3px rgba(0,0,0,.04);
  }
  .navbar-left { display:flex; align-items:center; gap:12px; }
  .navbar-title {
    font-size:18px; font-weight:700; color:#0f172a;
    letter-spacing:-0.3px;
  }
  .navbar-left .badge-outline {
    background: linear-gradient(135deg, #6366f1, #8b5cf6);
    color:#fff; border:none; padding:2px 12px;
    font-size:11px; font-weight:600;
    border-radius:10px; line-height:20px;
  }
  .navbar-right { display:flex; align-items:center; gap:10px; }

  /* ---------- 端口徽章 ---------- */
  .port-badge {
    font-size:12px; padding:3px 12px;
    background:#f1f5f9; color:#64748b;
    border:1px solid #e2e8f0; border-radius:6px;
  }

  /* ---------- 状态标签 ---------- */
  .tag {
    display:inline-flex; align-items:center; gap:5px;
    padding:3px 14px; border-radius:20px;
    font-size:12px; font-weight:600; line-height:22px;
    transition: all .2s;
  }
  .tag::before {
    content:''; display:inline-block;
    width:7px; height:7px; border-radius:50%; flex-shrink:0;
  }
  .tag-success { background:#f0fdf4; color:#16a34a; }
  .tag-success::before { background:#22c55e; box-shadow: 0 0 4px rgba(34,197,94,.4); }
  .tag-warn    { background:#fffbeb; color:#d97706; }
  .tag-warn::before { background:#f59e0b; box-shadow: 0 0 4px rgba(245,158,11,.4); }
  .tag-danger  { background:#fef2f2; color:#dc2626; }
  .tag-danger::before { background:#ef4444; box-shadow: 0 0 4px rgba(239,68,68,.4); }
  .tag-default { background:#f8fafc; color:#94a3b8; }
  .tag-default::before { background:#cbd5e1; }

  /* ============================================================
     主体区域
     ============================================================ */
  .main {
    flex:1; padding:20px 24px; overflow:hidden;
    display:flex; flex-direction:column; gap:16px;
  }

  /* ============================================================
     Tab 导航栏
     ============================================================ */
  .tab-bar {
    display:flex; gap:4px; flex-shrink:0;
    background:#ffffff; padding:4px;
    border-radius:10px; border:1px solid #e2e8f0;
    box-shadow: 0 1px 2px rgba(0,0,0,.03);
  }
  .tab {
    position:relative; display:inline-flex; align-items:center; gap:6px;
    padding:8px 18px; font-size:13px; font-weight:500;
    color:#64748b; background:none; border:none;
    border-radius:7px; cursor:pointer;
    transition: all .2s ease;
    white-space:nowrap;
  }
  .tab:hover { color:#334155; background:#f8fafc; }
  .tab:active { transform:scale(.97); }
  .tab.active {
    color:#ffffff; background: linear-gradient(135deg, #6366f1, #818cf8);
    box-shadow: 0 2px 8px rgba(99,102,241,.3);
  }
  .tab-badge {
    display:inline-flex; align-items:center; justify-content:center;
    min-width:20px; height:20px; padding:0 7px;
    border-radius:10px; font-size:11px; font-weight:700;
    background:#eef2ff; color:#6366f1; line-height:1;
    transition: all .2s;
  }
  .tab.active .tab-badge {
    background:rgba(255,255,255,.25); color:#fff;
  }

  /* ============================================================
     卡片
     ============================================================ */
  .card {
    background:#ffffff; border-radius:12px;
    border:1px solid #e2e8f0; padding:20px 24px;
    box-shadow: 0 1px 3px rgba(0,0,0,.04);
    transition: box-shadow .2s;
  }
  .card:hover { box-shadow: 0 2px 8px rgba(0,0,0,.06); }
  .card-grow { flex:1; display:flex; flex-direction:column; }
  .card-grow .card-bd { flex:1; overflow-y:auto; }
  .card-hd { display:flex; align-items:center; justify-content:space-between; margin-bottom:12px; }

  /* ---------- ? 帮助提示 ---------- */
  .help-tip {
    position:relative; display:inline-flex; align-items:center;
    flex-shrink:0;
  }
  .help-icon {
    display:inline-flex; align-items:center; justify-content:center;
    width:20px; height:20px; border-radius:50%;
    background:#e2e8f0; color:#64748b;
    font-size:12px; font-weight:700; line-height:1;
    cursor:help; transition:all .15s;
    user-select:none;
  }
  .help-icon:hover {
    background:#6366f1; color:#fff;
    box-shadow:0 2px 8px rgba(99,102,241,.3);
  }
  .help-popup {
    position:absolute; top:100%; right:0; z-index:100;
    margin-top:8px; min-width:280px; max-width:320px;
    padding:14px 16px;
    background:#fff; border:1px solid #e2e8f0;
    border-radius:10px;
    box-shadow:0 8px 24px rgba(0,0,0,.12);
    opacity:0; visibility:hidden;
    transform:translateY(-4px);
    transition:all .2s ease;
    pointer-events:none;
  }
  .help-tip:hover .help-popup {
    opacity:1; visibility:visible;
    transform:translateY(0);
  }
  .help-popup-title {
    font-size:13px; font-weight:700; color:#0f172a;
    margin-bottom:10px;
  }
  .help-popup-list {
    list-style:none; padding:0; margin:0 0 10px;
    display:flex; flex-direction:column; gap:6px;
  }
  .help-popup-list li {
    position:relative; padding-left:14px;
    font-size:12px; line-height:1.6; color:#475569;
  }
  .help-popup-list li::before {
    content:'•'; position:absolute; left:2px;
    color:#94a3b8; font-weight:700;
  }
  .help-popup-list li code {
    font-family:"SFMono-Regular",Consolas,monospace;
    font-size:11px; padding:1px 5px;
    background:#f1f5f9; border-radius:4px; color:#6366f1;
  }
  .help-popup-foot {
    padding-top:8px; border-top:1px solid #f1f5f9;
    font-size:11px; color:#94a3b8; line-height:1.5;
  }
  .help-popup-foot code {
    font-family:"SFMono-Regular",Consolas,monospace;
    font-size:10px; padding:1px 4px;
    background:#f1f5f9; border-radius:3px; color:#64748b;
  }
  .card-title {
    font-size:14px; font-weight:600; color:#0f172a;
    letter-spacing:0.2px;
  }

  /* ---------- 数字徽章 ---------- */
  .badge { display:inline-block; padding:1px 10px; border-radius:3px; font-size:12px; line-height:22px; }
  .badge-num {
    background:#f1f5f9; color:#64748b;
    min-width:24px; text-align:center; border-radius:10px;
    font-weight:600;
  }

  /* ============================================================
     信息面板 — Dashboard 顶部状态条
     ============================================================ */
  .info-topbar { padding:12px 20px; flex-shrink:0; }
  .info-topbar-body {
    display:flex; align-items:center; gap:14px;
  }
  .info-topbar-icon {
    width:36px; height:36px; border-radius:10px;
    display:flex; align-items:center; justify-content:center;
    font-size:16px; font-weight:700; color:#fff; flex-shrink:0;
    background:#cbd5e1; transition:all .3s;
    box-shadow:0 2px 6px rgba(0,0,0,.06);
  }
  .info-topbar-icon.s-icon-ok {
    background:linear-gradient(135deg,#22c55e,#16a34a);
    box-shadow:0 2px 8px rgba(34,197,94,.25);
  }
  .info-topbar-icon.s-icon-err { background:linear-gradient(135deg,#cbd5e1,#94a3b8); }
  .info-topbar-info { flex:1; display:flex; flex-direction:column; gap:1px; min-width:0; }
  .info-topbar-title { font-size:14px; font-weight:700; color:#0f172a; }
  .info-topbar-desc { font-size:12px; color:#64748b; }
  .info-topbar-target {
    font-size:11px; color:#94a3b8; flex-shrink:0;
    background:#f8fafc; padding:3px 10px; border-radius:6px; border:1px solid #f1f5f9;
  }

  /* ============================================================
     信息面板 — Dashboard 双列网格
     ============================================================ */
  .info-grid {
    flex:1; display:grid;
    grid-template-columns:280px 1fr;
    gap:16px; min-height:0;
  }
  .info-left {
    display:flex; flex-direction:column; gap:0;
    min-height:0;
  }
  .info-right {
    display:flex; flex-direction:column;
    min-height:0;
  }
  .info-right .chart-body {
    flex:1; display:flex; align-items:center; gap:20px;
    min-height:0;
  }
  .info-right .chart-donut-wrap {
    flex-shrink:0; width:120px; height:120px;
  }
  .info-right .chart-svg { width:100%; height:100%; display:block; }
  .info-right .chart-segment {
    transition:stroke-dasharray .4s ease, opacity .2s;
    cursor:pointer;
  }
  .info-right .chart-segment:hover { opacity:.8; stroke-width:12; }
  .info-right .chart-total-num {
    font-size:18px; font-weight:800; fill:#0f172a; font-family:inherit;
  }
  .info-right .chart-total-lbl {
    font-size:10px; fill:#94a3b8; font-family:inherit;
  }
  .info-right .chart-legend {
    flex:1; display:grid;
    grid-template-columns:1fr;
    gap:8px; min-width:0;
  }
  .info-right .chart-legend .legend-item {
    cursor:pointer;
  }

  /* ============================================================
     弹窗（Modal）
     ============================================================ */
  .modal-overlay {
    position:fixed; inset:0; z-index:9998;
    background:rgba(15,23,42,.5);
    display:flex; align-items:center; justify-content:center;
    padding:24px;
    animation:modal-fadein .2s ease;
  }
  @keyframes modal-fadein { from{opacity:0} to{opacity:1} }
  .modal-panel {
    width:100%; max-width:640px; max-height:80vh;
    background:#fff; border-radius:14px;
    display:flex; flex-direction:column;
    box-shadow:0 16px 48px rgba(0,0,0,.18);
    animation:modal-slidein .25s cubic-bezier(.21,1.02,.73,1);
  }
  @keyframes modal-slidein { from{transform:translateY(20px);opacity:0} to{transform:translateY(0);opacity:1} }
  .modal-hd {
    display:flex; align-items:center; gap:8px;
    padding:16px 20px; border-bottom:1px solid #e2e8f0;
    flex-shrink:0;
  }
  .modal-hd-title { font-size:14px; font-weight:700; flex:1; }
  .modal-back {
    background:none; border:none; cursor:pointer;
    font-size:13px; font-weight:600; color:#6366f1;
    padding:4px 8px; border-radius:6px; transition:all .15s;
  }
  .modal-back:hover { background:#eef2ff; }
  .modal-close {
    width:28px; height:28px; border-radius:7px;
    display:flex; align-items:center; justify-content:center;
    background:none; border:none; cursor:pointer;
    font-size:15px; color:#94a3b8; transition:all .15s;
    flex-shrink:0;
  }
  .modal-close:hover { background:#f1f5f9; color:#475569; }
  .modal-body {
    flex:1; overflow-y:auto; padding:12px 0;
    min-height:120px;
  }
  .modal-body.detail-body { padding:0; }
  .modal-loading, .modal-empty {
    display:flex; align-items:center; justify-content:center;
    min-height:120px; font-size:13px; color:#94a3b8;
  }

  /* ---------- 文件列表 ---------- */
  .file-list { display:flex; flex-direction:column; }
  .file-row {
    display:flex; align-items:center; gap:10px;
    padding:10px 20px; width:100%;
    background:none; border:none; cursor:pointer;
    text-align:left; transition:background .1s;
    border-bottom:1px solid #f8fafc;
  }
  .file-row:hover { background:#f8fafc; }
  .file-row:active { background:#f1f5f9; }
  .file-row-icon {
    width:6px; height:6px; border-radius:50%; flex-shrink:0;
  }
  .file-row-body { flex:1; min-width:0; display:flex; flex-direction:column; gap:2px; }
  .file-row-name {
    font-size:12px; font-weight:600; color:#0f172a;
    word-break:break-all; line-height:1.4;
  }
  .file-row-meta {
    display:flex; align-items:center; gap:4px; flex-wrap:wrap;
    font-size:11px; color:#94a3b8;
  }
  .file-row-method {
    padding:1px 6px; border-radius:4px;
    background:#f1f5f9; color:#64748b;
    font-weight:500;
  }
  .file-row-time { color:#94a3b8; }
  .file-row-arrow {
    font-size:13px; color:#cbd5e1; flex-shrink:0;
    transition:transform .2s;
  }
  .file-row:hover .file-row-arrow { color:#6366f1; transform:translateX(2px); }

  /* ---------- 文件详情 ---------- */
  .file-detail-pre {
    margin:0; padding:16px 20px;
    font-family:"SFMono-Regular",Consolas,monospace;
    font-size:12px; line-height:1.7; color:#0f172a;
    white-space:pre-wrap; word-break:break-all;
    background:#fafbfc; min-height:200px;
    max-height:50vh; overflow-y:auto;
  }
  .file-detail-pre code { font-family:inherit; }

  /* ============================================================
     信息面板 — 任务存储路径（左列）
     ============================================================ */
  /* ---------- 编辑模式 ---------- */
  .path-edit {  }
  .path-input {
    width:100%; padding:9px 14px; font-size:13px;
    font-family:"SFMono-Regular", Consolas, monospace;
    border:1px solid #e2e8f0; border-radius:8px;
    color:#0f172a; background:#fafbfc;
    transition: all .2s;
  }
  .path-input:focus {
    outline:none; border-color:#6366f1;
    box-shadow:0 0 0 3px rgba(99,102,241,.12);
    background:#fff;
  }
  .path-input:disabled { opacity:.6; cursor:not-allowed; }
  .path-hint { margin-top:10px; padding:8px 14px; font-size:12px; border-radius:8px; }
  .path-hint.migrating {
    background:#fffbeb; border:1px solid #fde68a; color:#d97706;
    display:flex; align-items:center; gap:8px;
  }
  .path-hint.migrating::before {
    content:'⟳'; display:inline-block;
    animation:spin 1.2s linear infinite; font-size:14px;
  }
  @keyframes spin { to{transform:rotate(360deg)} }

  /* ---------- 路径展示卡片 ---------- */
  .path-box {
    display:flex; align-items:center; gap:10px;
    background:#f8fafc; border-radius:10px;
    border:1px solid #e2e8f0; padding:10px 14px;
    transition:border-color .2s;
  }
  .path-box:hover { border-color:#cbd5e1; }
  .path-box-icon {
    width:28px; height:28px; border-radius:8px;
    display:flex; align-items:center; justify-content:center;
    font-size:13px; background:#eef2ff; color:#6366f1;
    flex-shrink:0;
  }
  .path-box-body { flex:1; min-width:0; }
  .path-box-path {
    font-size:12px; color:#0f172a; font-weight:500;
    overflow:hidden; text-overflow:ellipsis; white-space:nowrap;
    font-family:"SFMono-Regular", Consolas, monospace;
  }
  .path-box-label { font-size:10px; color:#94a3b8; margin-top:2px; }

  /* ---------- 路径操作按钮组 ---------- */
  .path-box-actions {
    display:flex; align-items:center; gap:2px; flex-shrink:0;
  }
  .btn-icon {
    display:inline-flex; align-items:center; justify-content:center;
    width:28px; height:28px; border-radius:7px;
    background:transparent; border:1px solid transparent;
    color:#94a3b8; font-size:13px; cursor:pointer;
    transition:all .15s; flex-shrink:0;
  }
  .btn-icon:hover { background:#f1f5f9; border-color:#e2e8f0; color:#6366f1; }
  .btn-icon:active { transform:scale(.9); }

  /* ---------- 路径状态统计 ---------- */
  .path-stats {
    display:flex; align-items:center; gap:0;
    margin-top:10px;
  }
  .path-stat {
    display:inline-flex; align-items:center; gap:5px;
    font-size:12px; color:#64748b; font-weight:500;
    padding:0 10px 0 0;
  }
  .path-stat:first-child { padding-left:0; }
  .path-stat.stat-ok { color:#16a34a; }
  .path-stat.stat-err { color:#dc2626; }
  .path-stat-num {
    font-size:14px; font-weight:700; color:#0f172a;
  }
  .path-stat-divider {
    display:inline-block; width:1px; height:12px;
    background:#e2e8f0; margin:0 10px; flex-shrink:0;
  }

  /* ============================================================
     信息面板 — 图例统一样式
     ============================================================ */
  .legend-item {
    display:flex; align-items:center; gap:8px;
    padding:8px 10px;
    background:#fafbfc; border-radius:8px;
    border:1px solid #f1f5f9;
    transition: all .15s;
    font:inherit; text-align:left; color:inherit;
    cursor:pointer; width:100%;
  }
  .legend-item:hover { background:#f1f5f9; border-color:#e2e8f0; }
  .legend-dot {
    width:8px; height:8px; border-radius:50%; flex-shrink:0;
    box-shadow: 0 0 4px rgba(0,0,0,.1);
  }
  .legend-body { display:flex; flex-direction:column; gap:1px; min-width:0; }
  .legend-label { font-size:11px; color:#64748b; }
  .legend-value { font-size:15px; font-weight:700; color:#0f172a; line-height:1.2; }

  /* ============================================================
     信息面板 — 左列底部：当前运行文件
     ============================================================ */
  .info-running {
    margin-top:auto; padding-top:12px;
    border-top:1px solid #e2e8f0;
  }
  .info-running-hd {
    display:flex; align-items:center; gap:6px;
  }
  .running-icon {
    width:22px; height:22px; border-radius:6px;
    display:flex; align-items:center; justify-content:center;
    font-size:10px; color:#6366f1;
    background:#eef2ff; flex-shrink:0;
  }
  .running-lbl-inline { font-size:12px; font-weight:600; color:#0f172a; flex:1; }
  .running-path {
    font-size:11px; color:#64748b; margin-top:8px;
    font-family:"SFMono-Regular", Consolas, monospace;
    word-break:break-all; line-height:1.5;
  }
  .running-empty { font-size:11px; color:#94a3b8; margin-top:8px; }

  /* ---------- 运行中 / 空闲 徽章（左右列共用） ---------- */
  .running-badge {
    display:inline-flex; align-items:center; gap:4px;
    padding:1px 10px; border-radius:20px;
    font-size:11px; font-weight:600; line-height:18px;
  }
  .running-badge.running {
    background:#f0fdf4; color:#16a34a;
  }
  .running-badge.running::before {
    content:''; display:inline-block; width:5px; height:5px;
    border-radius:50%; background:#22c55e;
    animation:pulse-dot 1.5s ease-in-out infinite;
  }
  .running-badge.idle { background:#f8fafc; color:#94a3b8; }
  @keyframes pulse-dot {
    0%,100%{opacity:1;transform:scale(1)}
    50%{opacity:.5;transform:scale(.8)}
  }

  /* ============================================================
     系统信息面板
     ============================================================ */
  .sys-detail {
    display:grid; grid-template-columns:1fr 1fr 1fr;
    gap:24px 20px; padding:8px 0;
  }
  .sys-detail > div {
    background:#fafbfc; border-radius:10px;
    padding:16px 18px; border:1px solid #f1f5f9;
    transition: all .2s;
  }
  .sys-detail > div:hover {
    background:#f1f5f9; border-color:#e2e8f0;
    transform:translateY(-1px);
    box-shadow: 0 2px 6px rgba(0,0,0,.04);
  }
  .sys-detail dt {
    font-size:11px; color:#94a3b8; margin-bottom:6px;
    text-transform:uppercase; letter-spacing:0.5px; font-weight:600;
  }
  .sys-detail dd {
    font-size:15px; color:#0f172a; font-weight:600;
    word-break:break-all;
  }

  /* ============================================================
     通信记录面板
     ============================================================ */
  .log-list { margin:-4px -24px; }
  .log-row {
    display:flex; align-items:center; gap:10px;
    padding:10px 24px; font-size:13px;
    border-bottom:1px solid #f1f5f9;
    transition: background .15s;
  }
  .log-row:last-child { border-bottom:none; }
  .log-row:hover { background:#fafbfc; }
  .log-time {
    min-width:68px; flex-shrink:0;
    font-size:12px; color:#94a3b8;
  }
  .log-tag {
    display:inline-flex; align-items:center; justify-content:center;
    min-width:52px; padding:2px 10px; border-radius:6px;
    font-size:11px; font-weight:700; letter-spacing:0.3px;
    background:#f1f5f9; color:#64748b; text-transform:uppercase;
    border:1px solid transparent;
  }
  .log-command  .log-tag {
    background:#eef2ff; color:#6366f1; border-color:#e0e7ff;
  }
  .log-response .log-tag {
    background:#f0fdf4; color:#16a34a; border-color:#dcfce7;
  }
  .log-heartbeat .log-tag {
    background:#f8fafc; color:#94a3b8; border-color:#f1f5f9;
  }
  .log-error    .log-tag {
    background:#fef2f2; color:#ef4444; border-color:#fecaca;
  }
  .log-event    .log-tag {
    background:#fffbeb; color:#d97706; border-color:#fde68a;
  }

  .log-dir {
    color:#cbd5e1; min-width:18px; text-align:center;
    font-size:13px; font-weight:600;
  }
  .log-method {
    color:#0f172a; font-weight:500;
    flex:1; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;
  }

  /* ============================================================
     按钮
     ============================================================ */
  .btn {
    display:inline-flex; align-items:center; justify-content:center; gap:6px;
    padding:7px 20px; font-size:14px; font-weight:500;
    border-radius:8px; border:1px solid transparent;
    cursor:pointer; transition: all .15s ease;
    user-select:none;
  }
  .btn:active { transform:scale(.96); }
  .btn:disabled { opacity:.5; cursor:not-allowed; transform:none !important; }

  .btn-primary {
    background: linear-gradient(135deg, #6366f1, #818cf8);
    color:#fff; border-color:transparent;
    box-shadow: 0 2px 8px rgba(99,102,241,.25);
  }
  .btn-primary:hover {
    background: linear-gradient(135deg, #4f46e5, #6366f1);
    box-shadow: 0 4px 14px rgba(99,102,241,.35);
    transform:translateY(-1px);
  }

  .btn-default {
    background:#fff; color:#475569;
    border-color:#e2e8f0;
  }
  .btn-default:hover {
    border-color:#6366f1; color:#6366f1; background:#f8fafc;
  }

  .btn-sm { padding:5px 14px; font-size:12px; border-radius:6px; }

  /* ============================================================
     定义列表（通用）
     ============================================================ */
  .dl { display:grid; grid-template-columns:1fr 1fr 1fr; gap:12px; }
  .dl dt { font-size:12px; color:#94a3b8; margin-bottom:2px; }
  .dl dd { font-size:14px; color:#0f172a; font-weight:500; }

  /* ============================================================
     空状态
     ============================================================ */
  .empty { text-align:center; padding:48px 16px; color:#94a3b8; }
  .empty p { font-size:13px; line-height:1.8; }

  /* ============================================================
     通用工具类
     ============================================================ */
  .mono { font-family:"SFMono-Regular", Consolas, monospace; }
  .muted { color:#94a3b8; }

  /* ============================================================
     通知（Toast）
     ============================================================ */
  .toast-container {
    position:fixed; top:66px; right:24px; z-index:9999;
    display:flex; flex-direction:column; gap:8px; max-width:360px;
  }
  .toast {
    display:flex; align-items:flex-start; gap:12px;
    padding:14px 18px; border-radius:12px;
    background:#ffffff; border:1px solid #e2e8f0;
    box-shadow: 0 8px 24px rgba(0,0,0,.1);
    cursor:pointer; animation:toast-in .3s cubic-bezier(.21,1.02,.73,1);
  }
  @keyframes toast-in {
    from{transform:translateX(120%);opacity:0}
    to{transform:translateX(0);opacity:1}
  }
  @keyframes toast-out {
    from{transform:translateX(0);opacity:1}
    to{transform:translateX(120%);opacity:0}
  }
  .toast-success { border-left:4px solid #22c55e; }
  .toast-warn    { border-left:4px solid #f59e0b; }
  .toast-info    { border-left:4px solid #6366f1; }
  .toast-icon {
    font-size:16px; font-weight:700; line-height:22px; flex-shrink:0;
    width:24px; height:24px; border-radius:50%;
    display:flex; align-items:center; justify-content:center;
  }
  .toast-success .toast-icon { background:#f0fdf4; color:#22c55e; }
  .toast-warn    .toast-icon { background:#fffbeb; color:#f59e0b; }
  .toast-info    .toast-icon { background:#eef2ff; color:#6366f1; }
  .toast-title { font-size:13px; font-weight:600; color:#0f172a; }
  .toast-desc  { font-size:12px; color:#64748b; margin-top:2px; }

  /* ============================================================
     表单操作区
     ============================================================ */
  .form-actions { display:flex; gap:8px; margin-top:12px; }

  /* ============================================================
     响应式设计
     ============================================================ */
  @media (max-width:640px) {
    .navbar { padding:0 16px; }
    .main { padding:16px; gap:12px; }
    .card { padding:14px 16px; border-radius:10px; }

    .tab-bar { overflow-x:auto; }
    .tab { padding:7px 14px; font-size:12px; flex-shrink:0; }

    .sys-detail { grid-template-columns:1fr 1fr; gap:12px; }
    .sys-detail > div { padding:12px 14px; }

    .info-grid { grid-template-columns:1fr; }
    .info-topbar-target { display:none; }
    .info-right .chart-legend { grid-template-columns:1fr 1fr; }

    .log-row { padding:8px 18px; flex-wrap:wrap; gap:6px; }

    .toast-container { right:12px; max-width:calc(100vw - 24px); }

    /* 弹窗响应式 */
    .modal-overlay { padding:12px; align-items:flex-end; }
    .modal-panel { max-width:100%; max-height:85vh; border-radius:14px 14px 0 0; }
    .modal-hd { padding:14px 16px; }
    .file-row { padding:10px 16px; }
    .file-detail-pre { padding:12px 16px; font-size:11px; }
  }

  @media (max-width:480px) {
    .sys-detail { grid-template-columns:1fr; }
    .dl { grid-template-columns:1fr; }
    .status-row { gap:14px; }
    .navbar-title { font-size:16px; }
    .log-time { min-width:56px; }
    .log-tag { min-width:44px; padding:1px 8px; }
  }

  /* ============================================================
     焦点 / 键盘辅助
     ============================================================ */
  :global(:focus-visible) { outline:2px solid #6366f1; outline-offset:2px; border-radius:4px; }
  :global([tabindex]:focus) { outline:2px solid #6366f1; outline-offset:2px; }

  /* ============================================================
     滚动条
     ============================================================ */
  :global(::-webkit-scrollbar) { width:5px; }
  :global(::-webkit-scrollbar-track) { background:transparent; }
  :global(::-webkit-scrollbar-thumb) { background:#cbd5e1; border-radius:3px; }
  :global(::-webkit-scrollbar-thumb:hover) { background:#94a3b8; }
</style>
