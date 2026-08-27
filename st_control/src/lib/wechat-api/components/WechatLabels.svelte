<script lang="ts">
  /**
   * 标签管理模块 — 严格遵循《标签管理 — 业务逻辑规范》
   *
   * §4 列表获取与展示（含复制 ID、删除确认）
   * §5 新增标签（静默刷新列表）
   * §6 修改成员列表（全量 labelIds 字符串）
   * §7 与锁定目标及路由联动（预填 + 辅助展示）
   * §8 清空日志
   */
  import { apiPost, isTokenInvalidPayload } from '../services/api';
  import { consoleState, lookupContactDisplayName } from '../stores/console.svelte';
  import { onMount } from 'svelte';

  // ═══════════════════════════════════════════════════════════
  // §D 参考常量
  // ═══════════════════════════════════════════════════════════
  const PREVIEW_MAX_WXIDS = 3;  // §D 辅助展示最多预览 3 个

  // ═══════════════════════════════════════════════════════════
  // 类型
  // ═══════════════════════════════════════════════════════════
  interface LabelRow {
    labelId: number;
    labelName: string;
    [key: string]: unknown;
  }

  // ═══════════════════════════════════════════════════════════
  // 状态
  // ═══════════════════════════════════════════════════════════
  let labels = $state<LabelRow[]>([]);
  let logs = $state<string[]>([]);

  // §5 新增标签
  let addName = $state('');
  let isAdding = $state(false);

  // §6 修改成员
  let modifyWxids = $state('');
  let modifyLabelIds = $state('');
  let isModifying = $state(false);

  // 加载状态
  let isFetchingList = $state(false);
  let isDeletingId = $state<number | null>(null);

  // ═══════════════════════════════════════════════════════════
  // §8 日志
  // ═══════════════════════════════════════════════════════════
  function addLog(msg: string) {
    const time = new Date().toLocaleTimeString('zh-CN', { hour12: false });
    logs = [`[${time}] ${msg}`, ...logs].slice(0, 500);
  }

  // ═══════════════════════════════════════════════════════════
  // §C.4 HTML 转义
  // ═══════════════════════════════════════════════════════════
  function escapeHtml(s: string): string {
    return String(s || '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;');
  }

  // ═══════════════════════════════════════════════════════════
  // §4.1 拉取标签列表
  // ═══════════════════════════════════════════════════════════
  async function fetchLabels() {
    if (isFetchingList) return;
    isFetchingList = true;

    try {
      const res = await apiPost('/label/list', {}, consoleState);

      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        addLog('❌ TOKEN 已失效');
        return;
      }

      if (res.data?.ret !== 200) {
        addLog(`⚠️ 获取标签失败: ${res.data?.msg || '未知错误'}`);
        // §4.1 失败不覆盖已有列表
        return;
      }

      const data = res.data?.data as Record<string, unknown> | undefined;
      const list = data?.labelList ?? data?.list;

      if (!Array.isArray(list)) {
        addLog('⚠️ 标签数据格式异常（非数组）');
        return;
      }

      // §4.1 保存为当前标签列表
      labels = list.map(item => {
        const row = item as Record<string, unknown>;
        return {
          labelId: Number(row.labelId || row.id || 0),
          labelName: String(row.labelName || row.name || ''),
          ...row,
        } as LabelRow;
      }).filter(l => l.labelId > 0);

      addLog(`✅ 获取标签 ${labels.length} 个`);
    } catch (e) {
      addLog(`❌ 获取标签失败: ${(e as Error).message}`);
    } finally {
      isFetchingList = false;
    }
  }

  // ═══════════════════════════════════════════════════════════
  // §5 新增标签
  // ═══════════════════════════════════════════════════════════
  async function addLabel() {
    const name = addName.trim();
    if (!name) {
      addLog('⚠️ 标签名称不能为空');
      return;
    }

    if (isAdding) return;
    isAdding = true;

    try {
      const res = await apiPost('/label/add', { labelName: name }, consoleState);

      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        addLog('❌ TOKEN 已失效');
        return;
      }

      if (res.data?.ret !== 200) {
        addLog(`⚠️ 添加失败: ${res.data?.msg}`);
        return;
      }

      addLog(`✅ 标签「${name}」已添加`);
      addName = '';

      // §5 静默刷新列表
      await fetchLabels();
    } catch (e) {
      addLog(`❌ 添加失败: ${(e as Error).message}`);
    } finally {
      isAdding = false;
    }
  }

  // ═══════════════════════════════════════════════════════════
  // §4.3 删除标签（二次确认 + 静默刷新）
  // ═══════════════════════════════════════════════════════════
  async function deleteLabel(labelId: number, labelName: string) {
    // §4.3 二次确认弹窗
    const displayName = labelName || '无名称';
    const confirmed = confirm(`确定删除标签？\n\n名称: ${displayName}\nID: ${labelId}`);
    if (!confirmed) return;

    if (isDeletingId !== null) return;
    isDeletingId = labelId;

    try {
      // §E 删除单条：labelIds 传字符串，非数组
      const res = await apiPost('/label/delete', { labelIds: String(labelId) }, consoleState);

      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        addLog('❌ TOKEN 已失效');
        return;
      }

      if (res.data?.ret !== 200) {
        addLog(`⚠️ 删除失败: ${res.data?.msg}`);
        return;
      }

      addLog(`✅ 标签「${displayName}」(${labelId}) 已删除`);

      // §4.3 静默刷新列表
      await fetchLabels();
    } catch (e) {
      addLog(`❌ 删除失败: ${(e as Error).message}`);
    } finally {
      isDeletingId = null;
    }
  }

  // ═══════════════════════════════════════════════════════════
  // §4.3 复制 ID（Clipboard API + 降级）
  // ═══════════════════════════════════════════════════════════
  async function copyLabelId(labelId: number) {
    const text = String(labelId);
    try {
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(text);
        addLog(`✅ 已复制 labelId: ${text}`);
        return;
      }
    } catch {
      // Clipboard API 失败，走降级
    }

    // §4.3 降级：临时 textarea + execCommand
    try {
      const ta = document.createElement('textarea');
      ta.value = text;
      ta.style.cssText = 'position:fixed;left:-9999px;top:-9999px';
      document.body.appendChild(ta);
      ta.select();
      document.execCommand('copy');
      document.body.removeChild(ta);
      addLog(`✅ 已复制 labelId: ${text}`);
    } catch {
      addLog('❌ 复制失败');
    }
  }

  // ═══════════════════════════════════════════════════════════
  // §6 修改成员列表
  // ═══════════════════════════════════════════════════════════
  /**
   * §6 wxIds 解析：按英文逗号、中文逗号、空白字符分割，trim，去空
   */
  function parseWxIdsInput(raw: string): string[] {
    return String(raw || '')
      .split(/[,，\s]+/)
      .map(s => s.trim())
      .filter(Boolean);
  }

  async function modifyMembers() {
    // §6 解析 wxIds
    const wxIds = parseWxIdsInput(modifyWxids);
    if (!wxIds.length) {
      addLog('⚠️ 至少填写一个 wxid');
      return;
    }

    // §6 labelIds 非空校验
    const labelIds = modifyLabelIds.trim();
    if (!labelIds) {
      addLog('⚠️ labelIds 不能为空（全量标签 id 列表）');
      return;
    }

    if (isModifying) return;
    isModifying = true;

    try {
      // §6 wxIds 为数组，labelIds 为字符串（全量）
      const res = await apiPost('/label/modifyMemberList', { wxIds, labelIds }, consoleState);

      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        addLog('❌ TOKEN 已失效');
        return;
      }

      if (res.data?.ret !== 200) {
        addLog(`⚠️ 修改失败: ${res.data?.msg}`);
        return;
      }

      addLog(`✅ 修改已提交: ${wxIds.length} 个对象 → labelIds=${labelIds}`);
    } catch (e) {
      addLog(`❌ 修改失败: ${(e as Error).message}`);
    } finally {
      isModifying = false;
    }
  }

  // ═══════════════════════════════════════════════════════════
  // §7 与锁定目标联动
  // ═══════════════════════════════════════════════════════════
  /** §7.1 预填 wxid 文本域 */
  function prefillFromSession() {
    if (modifyWxids.trim()) return; // 已有内容不覆盖
    const locked = (consoleState.currentTargetWxid || '').trim();
    if (locked) modifyWxids = locked;
  }

  /** §7.2 辅助展示行 */
  const helperDisplay = $derived.by(() => {
    const wxids = parseWxIdsInput(modifyWxids);
    const locked = (consoleState.currentTargetWxid || '').trim();

    if (!wxids.length) {
      if (locked) {
        const name = lookupContactDisplayName(locked) || consoleState.currentTargetDisplayName;
        return name && name !== locked ? `当前锁定: ${name}（${locked}）` : `当前锁定: ${locked}`;
      }
      return '';
    }

    // §7.2 最多预览 3 个
    const preview = wxids.slice(0, PREVIEW_MAX_WXIDS).map(id => {
      const name = lookupContactDisplayName(id);
      return name && name !== id ? `${name}（${id}）` : id;
    });

    let text = preview.join('、');
    if (wxids.length > PREVIEW_MAX_WXIDS) {
      text += ` 等 ${wxids.length} 个对象`;
    }
    return text;
  });

  function clearLogs() { logs = []; }

  // ═══════════════════════════════════════════════════════════
  // §A.1 初始化
  // ═══════════════════════════════════════════════════════════
  onMount(() => {
    prefillFromSession();
    fetchLabels();
  });
</script>

<div class="wa-mod">
  <div class="wa-mod-split">
    <!-- ═══ 左侧：操作区 ═══ -->
    <div class="wa-mod-left">
      <!-- §4.1 获取标签列表 -->
      <div class="wa-card">
        <h3 class="wa-card-title">标签管理</h3>
        <p class="wa-hint">4 个接口：list / add / delete / modifyMemberList</p>
        <div class="wa-actions">
          <button class="wa-btn wa-btn-primary" onclick={fetchLabels} disabled={isFetchingList}>
            {isFetchingList ? '获取中...' : '获取标签列表'}
          </button>
        </div>
      </div>

      <!-- §5 新增标签 -->
      <div class="wa-card">
        <h3 class="wa-card-title">新增标签</h3>
        <div class="wa-form-grid">
          <label class="wa-field">
            <span class="wa-label">labelName *</span>
            <div class="wa-input-row">
              <input type="text" bind:value={addName} placeholder="新标签名称" />
              <button class="wa-btn wa-btn-primary" onclick={addLabel} disabled={isAdding}>
                {isAdding ? '添加中...' : '添加'}
              </button>
            </div>
          </label>
        </div>
      </div>

      <!-- §6 修改成员列表 -->
      <div class="wa-card">
        <h3 class="wa-card-title">修改好友标签</h3>
        <p class="wa-hint">wxIds 支持逗号/空格分隔；labelIds 为<strong>全量</strong>标签 id（英文逗号分隔）</p>
        <div class="wa-form-grid">
          <label class="wa-field">
            <span class="wa-label">wxIds *</span>
            <textarea bind:value={modifyWxids} rows="3" placeholder="wxid_1,wxid_2 或每行一个"></textarea>
          </label>
          <label class="wa-field">
            <span class="wa-label">labelIds（全量）*</span>
            <input type="text" bind:value={modifyLabelIds} placeholder="1,2,3" />
          </label>
        </div>

        <!-- §7.2 辅助展示 -->
        {#if helperDisplay}
          <p class="wa-helper">{helperDisplay}</p>
        {/if}

        <div class="wa-actions">
          <button class="wa-btn wa-btn-primary" onclick={modifyMembers} disabled={isModifying}>
            {isModifying ? '提交中...' : '提交修改'}
          </button>
        </div>
      </div>
    </div>

    <!-- ═══ 右侧：标签列表 + 日志 ═══ -->
    <div class="wa-mod-right">
      <!-- §4.2 标签卡片列表 -->
      <div class="wa-card wa-card-fill">
        <h3 class="wa-card-title">标签列表</h3>
        <div class="wa-label-list">
          {#each labels as lb (lb.labelId)}
            <div class="wa-label-item">
              <div class="wa-label-info">
                <span class="wa-label-name">{escapeHtml(lb.labelName || '无名称')}</span>
                <span class="wa-label-id">#{lb.labelId}</span>
              </div>
              <div class="wa-label-actions">
                <button class="wa-btn wa-btn-sm" onclick={() => copyLabelId(lb.labelId)}>复制 ID</button>
                <button class="wa-btn wa-btn-sm" onclick={() => deleteLabel(lb.labelId, lb.labelName)} disabled={isDeletingId === lb.labelId}>
                  {isDeletingId === lb.labelId ? '删除中...' : '删除'}
                </button>
              </div>
            </div>
          {:else}
            <p class="wa-empty-hint">请先获取标签列表</p>
          {/each}
        </div>
      </div>

      <!-- §8 日志 -->
      <div class="wa-card wa-card-fill">
        <div class="wa-card-head">
          <h3 class="wa-card-title">日志</h3>
          <button class="wa-btn wa-btn-sm" onclick={clearLogs}>清空</button>
        </div>
        <div class="wa-log-body">
          {#each logs as log}
            <div class="wa-log-line">{log}</div>
          {:else}
            <div class="wa-log-empty">暂无日志</div>
          {/each}
        </div>
      </div>
    </div>
  </div>
</div>

<style>
  .wa-mod { height: 100%; display: flex; flex-direction: column; }
  .wa-mod-split { flex: 1; min-height: 0; display: flex; gap: 16px; }
  .wa-mod-left, .wa-mod-right { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 12px; overflow-y: auto; }
  .wa-card { background: var(--card); border: 1px solid var(--border); border-radius: var(--radius-lg); padding: 16px; }
  .wa-card-fill { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .wa-card-head { display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px; }
  .wa-card-title { font-size: 14px; font-weight: 600; margin: 0 0 12px; }
  .wa-card-head .wa-card-title { margin: 0; }
  .wa-hint { font-size: 12px; color: var(--muted-foreground); margin: 0 0 12px; line-height: 1.5; }
  .wa-hint strong { color: var(--warning, #d97706); }
  .wa-form-grid { display: flex; flex-direction: column; gap: 10px; }
  .wa-field { display: flex; flex-direction: column; gap: 4px; }
  .wa-label { font-size: 12px; font-weight: 600; }
  .wa-field input, .wa-field textarea { padding: 6px 10px; border: 1px solid var(--border); border-radius: 6px; background: var(--card); font-size: 13px; color: var(--foreground); }
  .wa-input-row { display: flex; gap: 8px; }
  .wa-input-row input { flex: 1; }
  .wa-helper { font-size: 12px; color: var(--primary); margin: 8px 0 0; }
  .wa-actions { display: flex; gap: 8px; margin-top: 12px; flex-wrap: wrap; }
  .wa-btn { padding: 6px 14px; border: 1px solid var(--border); border-radius: 6px; background: var(--card); font-size: 13px; cursor: pointer; color: var(--foreground); }
  .wa-btn:hover { background: var(--muted); }
  .wa-btn-primary { background: var(--primary); color: var(--primary-foreground); border-color: var(--primary); }
  .wa-btn-sm { padding: 3px 8px; font-size: 11.5px; }
  .wa-btn:disabled { opacity: 0.4; cursor: default; pointer-events: none; }
  .wa-label-list { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 4px; }
  .wa-label-item { display: flex; align-items: center; justify-content: space-between; padding: 10px 12px; border: 1px solid var(--border); border-radius: 8px; }
  .wa-label-info { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .wa-label-name { font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .wa-label-id { font-size: 12px; color: var(--muted-foreground); font-family: var(--font-mono); flex-shrink: 0; }
  .wa-label-actions { display: flex; gap: 4px; flex-shrink: 0; }
  .wa-empty-hint { color: var(--muted-foreground); font-size: 13px; text-align: center; padding: 24px 0; }
  .wa-log-body { flex: 1; min-height: 0; overflow-y: auto; background: #1e1e1e; border-radius: 8px; padding: 10px; font-family: var(--font-mono); font-size: 12px; color: #a6e22e; }
  .wa-log-line { padding: 2px 0; white-space: pre-wrap; word-break: break-all; }
  .wa-log-empty { color: #888; }
</style>
