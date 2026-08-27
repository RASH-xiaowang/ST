<!-- 设置弹窗（仪表台：固定尺寸两栏框架 · 导航栏 + 页签内容） -->
<script lang="ts">
  import { dbApi } from '../db/services/ipc';
  import { errText } from '../format';
  import { toast } from 'svelte-sonner';
  import { Button } from './ui/button';
  import Modal from './Modal.svelte';
  import PreferencesPanel from './PreferencesPanel.svelte';

  export type SettingsTab = 'general' | 'server' | 'log' | 'personalize' | 'database';

  let {
    open,
    tab,
    onTabChange,
    onClose,
    statusText,
    statusCls,
    serverPort,
    events,
  }: {
    open: boolean;
    tab: SettingsTab;
    onTabChange: (t: SettingsTab) => void;
    onClose: () => void;
    statusText: string;
    statusCls: string;
    serverPort: string | number;
    events: { time: string; event: string; detail: string }[];
  } = $props();

  // ── 数据库配置（参数配置，非数据操作）──
  const configLabels: Record<string, string> = {
    retention_days: '事件保留天数', max_event_rows: '最大事件行数',
    auto_vacuum: '自动清理模式 (0/1/2)', page_size: '页大小 (字节)',
  };
  let dbInfo = $state<{ path: string; size_bytes: number; event_count: number; task_count: number; agent_log_count: number } | null>(null);
  let dbConfigItems = $state<{ key: string; value: string }[]>([]);
  let retentionDays = $state(90);
  let cleanupResult = $state<{ deleted_events: number; deleted_agent: number; days: number } | null>(null);

  async function refreshDbInfo() {
    try { dbInfo = (await dbApi.getDbInfo()) as typeof dbInfo; } catch (e) { toast.error(`加载数据库信息失败：${errText(e)}`); }
  }
  async function loadDbConfig() {
    try {
      dbConfigItems = await dbApi.getDbConfig();
      const rd = dbConfigItems.find((i) => i.key === 'retention_days');
      if (rd) retentionDays = parseInt(rd.value) || 90;
    } catch (e) { toast.error(`加载数据库配置失败：${errText(e)}`); }
  }
  async function saveConfig(key: string, value: string) {
    try {
      await dbApi.setDbConfig(key, value);
      await loadDbConfig();
    } catch (e) { toast.error(`保存配置失败：${errText(e)}`); }
  }
  async function triggerCleanup() {
    try { cleanupResult = await dbApi.cleanupOldData(); } catch (e) { toast.error(`清理失败：${errText(e)}`); }
  }

  $effect(() => {
    if (tab === 'database') {
      refreshDbInfo();
      loadDbConfig();
    }
  });

  const NAV: { key: SettingsTab; label: string; desc: string; icon: string }[] = [
    { key: 'general', label: '常规', desc: '端口与版本', icon: '<circle cx="8" cy="8" r="2.5" stroke="currentColor" stroke-width="1.3"/><path d="M8 1v2M8 13v2M1 8h2M13 8h2M2.5 2.5l1.5 1.5M12 12l1.5 1.5M2.5 13.5l1.5-1.5M12 4l1.5-1.5" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>' },
    { key: 'personalize', label: '个性化', desc: '主题·字体·透明度', icon: '<circle cx="8" cy="8" r="5" stroke="currentColor" stroke-width="1.3"/><path d="M8 3v2M8 11v2M3 8h2M11 8h2" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/><path d="M5 5l1.5 1.5M9.5 9.5L11 11M5 11l1.5-1.5M9.5 6.5L11 5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>' },
    { key: 'server', label: '服务器', desc: '监听地址与状态', icon: '<rect x="1.5" y="1.5" width="13" height="5" rx="1.5" stroke="currentColor" stroke-width="1.3"/><rect x="1.5" y="9.5" width="13" height="5" rx="1.5" stroke="currentColor" stroke-width="1.3"/><circle cx="4" cy="4" r="1" fill="currentColor"/><circle cx="4" cy="12" r="1" fill="currentColor"/>' },
    { key: 'log', label: 'Agent 日志', desc: '接入与断开事件', icon: '<path d="M2 2h12a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H2a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1z" stroke="currentColor" stroke-width="1.3"/><line x1="4.5" y1="5" x2="11.5" y2="5" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/><line x1="4.5" y1="8" x2="11.5" y2="8" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/><line x1="4.5" y1="11" x2="8.5" y2="11" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>' },
    { key: 'database', label: '数据库', desc: '路径·参数·保留', icon: '<ellipse cx="8" cy="3.5" rx="6" ry="2" stroke="currentColor" stroke-width="1.3"/><path d="M2 3.5v9c0 1.1 2.7 2 6 2s6-.9 6-2v-9" stroke="currentColor" stroke-width="1.3"/><line x1="2" y1="8" x2="14" y2="8" stroke="currentColor" stroke-width="1.3"/>' },
  ];
</script>

{#if open}
  <Modal open={open} onClose={onClose} overlayClass="st-overlay" frameClass="st-frame" overlayRole="presentation" labelledBy="st-title">
      <header class="st-hd">
        <div class="st-hd-brand" aria-hidden="true">
          <span class="st-hd-led"></span>
          <span class="st-hd-mark">ST</span>
        </div>
        <div class="st-hd-titles">
          <h2 id="st-title" class="st-title">设置</h2>
          <span class="st-subtitle">应用偏好 · 服务 · 数据</span>
        </div>
        <span class="st-hd-spacer"></span>
        <button class="st-close" onclick={onClose} aria-label="关闭" title="关闭 (Esc)">
          <svg viewBox="0 0 16 16" width="14" height="14" fill="none" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg>
        </button>
      </header>

      <div class="st-body">
        <nav class="st-nav" aria-label="设置分类">
          <span class="st-nav-caption">设置</span>
          {#each NAV as n (n.key)}
            <button class="st-nav-item" class:active={tab === n.key} onclick={() => onTabChange(n.key)} role="tab" aria-selected={tab === n.key}>
              <svg class="st-nav-ico" viewBox="0 0 16 16" width="16" height="16" fill="none" aria-hidden="true">{@html n.icon}</svg>
              <span class="st-nav-label">{n.label}</span>
              {#if n.key === 'log' && events.length > 0}<span class="st-nav-badge">{events.length}</span>{/if}
              <span class="st-nav-desc">{n.desc}</span>
            </button>
          {/each}
          <div class="st-nav-foot">
            <span class="st-nav-foot-dot" class:on={statusCls === 'tag-success'}></span>
            <span>{statusText}</span>
          </div>
        </nav>

        <section class="st-content">
          <!-- 常规 -->
          <div class="st-page" class:panel-hidden={tab !== 'general'}>
            <header class="st-page-hd">
              <h3 class="st-page-title">常规</h3>
              <p class="st-page-desc">服务端口与应用信息</p>
            </header>
            <div class="st-card">
              <div class="st-card-hd">
                <span class="st-card-title">服务</span>
                <span class="st-lamp" class:ok={statusCls === 'tag-success'}></span>
                <span class="st-card-meta">{statusText}</span>
              </div>
              <div class="st-field">
                <label for="ws-port">WebSocket 监听端口</label>
                <input id="ws-port" class="st-input st-mono" type="number" value={serverPort} disabled />
                <span class="st-hint">端口在启动时确定，修改需重启应用</span>
              </div>
            </div>
            <div class="st-card">
              <div class="st-card-hd"><span class="st-card-title">关于</span></div>
              <dl class="st-dl">
                <div><dt>应用名称</dt><dd>ST 控制台</dd></div>
                <div><dt>版本</dt><dd class="st-mono">v1.0 专业版</dd></div>
                <div><dt>运行状态</dt><dd><span class="st-tag {statusCls}">{statusText}</span></dd></div>
              </dl>
            </div>
          </div>

          <!-- 个性化 -->
          <div class="st-page" class:panel-hidden={tab !== 'personalize'}>
            <header class="st-page-hd">
              <h3 class="st-page-title">个性化</h3>
              <p class="st-page-desc">主题 · 字体 · 透明度，实时生效</p>
            </header>
            <PreferencesPanel />
          </div>

          <!-- 服务器 -->
          <div class="st-page" class:panel-hidden={tab !== 'server'}>
            <header class="st-page-hd">
              <h3 class="st-page-title">服务器信息</h3>
              <p class="st-page-desc">监听地址与运行状态</p>
            </header>
            <div class="st-stat-grid">
              <div class="st-stat">
                <span class="st-stat-label">监听地址</span>
                <span class="st-stat-value st-mono">127.0.0.1:{serverPort}</span>
              </div>
              <div class="st-stat">
                <span class="st-stat-label">应用名称</span>
                <span class="st-stat-value">ST 控制台</span>
              </div>
              <div class="st-stat">
                <span class="st-stat-label">版本</span>
                <span class="st-stat-value st-mono">v1.0</span>
              </div>
              <div class="st-stat">
                <span class="st-stat-label">运行状态</span>
                <span class="st-stat-value"><span class="st-lamp" class:ok={statusCls === 'tag-success'}></span>{statusText}</span>
              </div>
            </div>
          </div>

          <!-- 日志 -->
          <div class="st-page" class:panel-hidden={tab !== 'log'}>
            <header class="st-page-hd">
              <h3 class="st-page-title">Agent 日志</h3>
              <p class="st-page-desc">接入与断开事件流</p>
              <span class="st-page-count">{events.length} 条</span>
            </header>
            {#if events.length === 0}
              <div class="st-empty"><span class="st-empty-dot"></span>暂无事件</div>
            {:else}
              <div class="st-log">
                {#each events as e}
                  <div class="st-log-row" class:in={e.event === 'agent_connected'} class:out={e.event === 'agent_disconnected'}>
                    <span class="st-log-time st-mono">{e.time}</span>
                    <span class="st-log-chip">{e.event === 'agent_connected' ? '接入' : e.event === 'agent_disconnected' ? '断开' : e.event}</span>
                    <span class="st-log-txt">{e.detail}</span>
                  </div>
                {/each}
              </div>
            {/if}
          </div>

          <!-- 数据库 -->
          <div class="st-page" class:panel-hidden={tab !== 'database'}>
            <header class="st-page-hd">
              <h3 class="st-page-title">数据库</h3>
              <p class="st-page-desc">连接 · 参数 · 数据保留</p>
            </header>
            <div class="st-stat-grid st-stat-grid-2">
              <div class="st-stat">
                <span class="st-stat-label">数据库引擎</span>
                <span class="st-stat-value st-mono">SQLite</span>
              </div>
              <div class="st-stat">
                <span class="st-stat-label">当前大小</span>
                <span class="st-stat-value st-mono">{dbInfo ? (dbInfo.size_bytes / 1024).toFixed(1) + ' KB' : '—'}</span>
              </div>
              <div class="st-stat">
                <span class="st-stat-label">事件记录</span>
                <span class="st-stat-value st-mono">{dbInfo?.event_count ?? '—'}</span>
              </div>
              <div class="st-stat">
                <span class="st-stat-label">Agent 日志</span>
                <span class="st-stat-value st-mono">{dbInfo?.agent_log_count ?? '—'}</span>
              </div>
            </div>
            <div class="st-card">
              <div class="st-card-hd"><span class="st-card-title">数据库路径</span></div>
              <div class="st-path st-mono">{dbInfo?.path ?? '加载中…'}</div>
            </div>
            <div class="st-card">
              <div class="st-card-hd"><span class="st-card-title">性能参数</span></div>
              <div class="st-field-grid">
                {#each dbConfigItems as item}
                  <div class="st-field">
                    <label for="cfg-{item.key}">{configLabels[item.key] ?? item.key}</label>
                    <input id="cfg-{item.key}" class="st-input st-mono" type="text" value={item.value} oninput={(e) => saveConfig(item.key, (e.currentTarget as HTMLInputElement).value)} />
                  </div>
                {/each}
              </div>
            </div>
            <div class="st-card">
              <div class="st-card-hd"><span class="st-card-title">数据保留</span></div>
              <div class="st-field st-field-inline">
                <label for="retention-days">事件保留天数</label>
                <input id="retention-days" class="st-input st-input-sm st-mono" type="number" bind:value={retentionDays} onchange={() => saveConfig('retention_days', String(retentionDays))} />
                <span class="st-hint">超过此天数的数据将被自动清理</span>
              </div>
              <div class="st-actions">
                <Button size="sm" variant="outline" onclick={triggerCleanup}>立即清理旧数据</Button>
                {#if cleanupResult !== null}
                  <span class="st-ok">已清理 {cleanupResult.deleted_events} 条事件、{cleanupResult.deleted_agent} 条日志</span>
                {/if}
              </div>
            </div>
          </div>

        </section>
      </div>
  </Modal>
{/if}

<style>
  .st-hd {
    flex: none;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
    background: color-mix(in oklab, var(--popover) 94%, var(--brand) 3%);
  }
  .st-hd-brand { position: relative; display: grid; place-items: center; width: 30px; height: 30px; border-radius: 8px; background: var(--primary); color: var(--primary-foreground); font-size: 11px; font-weight: 800; letter-spacing: 0.04em; flex: none; }
  .st-hd-led { position: absolute; top: -2px; right: -2px; width: 8px; height: 8px; border-radius: 50%; background: var(--brand); border: 2px solid var(--popover); box-shadow: 0 0 6px color-mix(in oklab, var(--brand) 65%, transparent); }
  .st-hd-titles { display: flex; flex-direction: column; gap: 1px; }
  .st-title { font-size: 15px; font-weight: 700; letter-spacing: 0.02em; color: var(--foreground); margin: 0; line-height: 1.2; }
  .st-subtitle { font-size: 11px; letter-spacing: 0.1em; color: var(--muted-foreground); }
  .st-hd-spacer { flex: 1; }
  .st-close { width: 30px; height: 30px; display: inline-flex; align-items: center; justify-content: center; border: none; border-radius: 8px; background: transparent; color: var(--muted-foreground); cursor: pointer; transition: background 0.14s, color 0.14s; }
  .st-close:hover { background: color-mix(in oklab, var(--brand) 12%, transparent); color: var(--foreground); }

  .st-body { flex: 1; min-height: 0; display: flex; }
  .panel-hidden { display: none !important; }

  /* ── 导航栏 ── */
  .st-nav {
    flex: none;
    width: 208px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 14px 10px 12px;
    border-right: 1px solid var(--border);
    background: color-mix(in oklab, var(--background) 96%, var(--brand) 2%);
    overflow-y: auto;
  }
  .st-nav-caption { font-size: 11px; font-weight: 600; letter-spacing: 0.16em; color: var(--muted-foreground); padding: 0 10px 8px; }
  .st-nav-item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    padding: 8px 10px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 13px;
    cursor: pointer;
    text-align: left;
    transition: background 0.12s, color 0.12s;
  }
  .st-nav-item:hover { background: var(--muted); color: var(--foreground); }
  .st-nav-item.active { background: color-mix(in oklab, var(--brand) 12%, transparent); color: var(--foreground); font-weight: 600; }
  .st-nav-item.active::before { content: ''; position: absolute; left: 0; top: 18%; bottom: 18%; width: 3.5px; border-radius: 0 4px 4px 0; background: var(--primary); }
  .st-nav-ico { flex: none; color: currentColor; opacity: 0.9; }
  .st-nav-label { flex: none; }
  .st-nav-desc { margin-left: auto; font-size: 10.5px; color: var(--muted-foreground); opacity: 0.75; white-space: nowrap; }
  .st-nav-badge { margin-left: auto; min-width: 18px; height: 18px; padding: 0 5px; border-radius: 9px; background: var(--primary); color: var(--primary-foreground); font-size: 10.5px; font-weight: 700; display: grid; place-items: center; }
  .st-nav-foot { margin-top: auto; display: flex; align-items: center; gap: 7px; padding: 10px 10px 2px; font-size: 11.5px; color: var(--muted-foreground); border-top: 1px solid var(--border); }
  .st-nav-foot-dot { width: 7px; height: 7px; border-radius: 50%; background: #8a93a5; }
  .st-nav-foot-dot.on { background: var(--app-success, #16a34a); box-shadow: 0 0 6px var(--app-success, #16a34a); }

  /* ── 内容区 ── */
  .st-content { flex: 1; min-width: 0; overflow-y: auto; padding: 16px 18px 20px; display: flex; flex-direction: column; gap: 12px; }
  .st-page { display: flex; flex-direction: column; gap: 12px; }
  .st-page-hd { display: flex; align-items: baseline; gap: 10px; flex-wrap: wrap; }
  .st-page-title { font-size: 16px; font-weight: 700; letter-spacing: 0.01em; color: var(--foreground); margin: 0; }
  .st-page-desc { font-size: 12px; color: var(--muted-foreground); margin: 0; }
  .st-page-count { margin-left: auto; font-size: 11.5px; color: var(--muted-foreground); font-variant-numeric: tabular-nums; }

  .st-card { background: var(--card); border: 1px solid var(--border); border-radius: var(--radius-lg); padding: 14px 16px; display: flex; flex-direction: column; gap: 10px; transition: border-color 0.15s; }
  .st-card:hover { border-color: color-mix(in oklab, var(--primary) 32%, var(--border)); }
  .st-card-hd { display: flex; align-items: center; gap: 8px; }
  .st-card-title { font-size: 13px; font-weight: 600; letter-spacing: 0.04em; color: var(--foreground); }
  .st-card-meta { margin-left: auto; font-size: 11.5px; color: var(--muted-foreground); }
  .st-lamp { width: 8px; height: 8px; border-radius: 50%; background: #8a93a5; flex: none; }
  .st-lamp.ok { background: var(--app-success, #16a34a); box-shadow: 0 0 6px var(--app-success, #16a34a); }

  .st-stat-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 10px; }
  .st-stat-grid-2 { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .st-stat { background: var(--card); border: 1px solid var(--border); border-radius: var(--radius-lg); padding: 12px 14px; display: flex; flex-direction: column; gap: 6px; min-width: 0; }
  .st-stat-label { font-size: 11px; font-weight: 600; letter-spacing: 0.12em; text-transform: uppercase; color: var(--muted-foreground); }
  .st-stat-value { font-size: 15px; font-weight: 700; color: var(--foreground); font-variant-numeric: tabular-nums; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; display: inline-flex; align-items: center; gap: 7px; }

  .st-field { display: flex; flex-direction: column; gap: 5px; max-width: 460px; }
  .st-field label { font-size: 12px; font-weight: 500; color: var(--muted-foreground); }
  .st-field-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }
  .st-field-inline { flex-direction: row; align-items: center; gap: 10px; flex-wrap: wrap; }
  .st-input { width: 100%; border-radius: var(--radius-md); border: 1px solid var(--input); background: color-mix(in oklab, var(--card) 78%, var(--background)); color: var(--foreground); padding: 8px 11px; font-size: 13px; outline: none; transition: border-color 0.15s, box-shadow 0.15s; }
  .st-input:focus { border-color: var(--ring); box-shadow: 0 0 0 3px color-mix(in oklab, var(--ring) 25%, transparent); }
  .st-input:disabled { opacity: 0.65; }
  .st-input-sm { width: 140px; }
  .st-mono { font-family: var(--font-mono); font-size: 12px; }
  .st-hint { font-size: 11.5px; color: var(--muted-foreground); }
  .st-path { font-size: 12px; color: var(--foreground); background: color-mix(in oklab, var(--card) 70%, var(--background)); border: 1px dashed var(--border); border-radius: var(--radius-md); padding: 8px 11px; word-break: break-all; }
  .st-actions { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .st-ok { font-size: 12px; color: var(--app-success, #16a34a); }

  .st-dl { display: flex; flex-direction: column; margin: 0; }
  .st-dl > div { display: flex; justify-content: space-between; gap: 16px; padding: 7px 0; border-bottom: 1px dashed var(--border); }
  .st-dl > div:last-child { border-bottom: none; }
  .st-dl dt { color: var(--muted-foreground); font-size: 12px; }
  .st-dl dd { font-size: 13px; color: var(--foreground); text-align: right; word-break: break-all; margin: 0; }

  .st-tag { display: inline-flex; align-items: center; height: 22px; padding: 0 9px; border-radius: 999px; font-size: 11.5px; font-weight: 600; background: var(--muted); color: var(--muted-foreground); }
  .st-tag.tag-success { background: color-mix(in oklab, var(--app-success, #22c55e) 16%, transparent); color: var(--app-success, #16a34a); }
  .st-tag.tag-warn { background: color-mix(in oklab, #f59e0b 16%, transparent); color: #b45309; }
  .st-tag.tag-danger { background: color-mix(in oklab, #ef4444 16%, transparent); color: #b91c1c; }

  .st-empty { display: flex; align-items: center; justify-content: center; gap: 8px; min-height: 180px; color: var(--muted-foreground); font-size: 13px; border: 1px dashed var(--border); border-radius: var(--radius-lg); }
  .st-empty-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--border); }

  .st-log { display: flex; flex-direction: column; gap: 2px; }
  .st-log-row { display: flex; align-items: center; gap: 10px; padding: 7px 10px; border-radius: 8px; font-size: 12px; background: var(--card); border: 1px solid var(--border); }
  .st-log-row:hover { background: var(--muted); }
  .st-log-row.in { border-left: 2px solid var(--app-success, #22c55e); }
  .st-log-row.out { border-left: 2px solid #f59e0b; }
  .st-log-time { flex: none; font-size: 11.5px; color: var(--muted-foreground); }
  .st-log-chip { flex: none; padding: 1px 8px; border-radius: 999px; background: var(--muted); color: var(--muted-foreground); font-size: 11px; font-weight: 600; }
  .st-log-txt { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--foreground); }
</style>
