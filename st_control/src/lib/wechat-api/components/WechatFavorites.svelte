<script lang="ts">
  /**
   * 收藏夹模块 — 严格遵循《收藏夹 — 业务逻辑规范》
   *
   * §3 客户端内存模型（索引 + 解析缓存 + 同步游标）
   * §4 同步列表与翻页（首屏清空 / 下一页合并）
   * §5 列表合并与排序展示
   * §6 获取详情（单条）
   * §7 XML 摘要解析（DOM + 正则兜底）
   * §8 删除收藏（二次确认）
   * §9 发送给当前目标（跨模块 postText）
   * §10 卡片渲染与安全（HTML 转义）
   * §11 当前发送目标展示
   * §12 日志
   */
  import { apiPost, isTokenInvalidPayload } from '../services/api';
  import { consoleState, lookupContactDisplayName } from '../stores/console.svelte';

  // ═══════════════════════════════════════════════════════════
  // §D 可配置常量
  // ═══════════════════════════════════════════════════════════
  const FORWARD_TEXT_MAX_LEN = 8000;  // §D 转发文本最大长度
  const SNIPPET_MAX_LEN = 160;        // §D snippet 压缩截取长度
  const DELETED_FLAG = 1;             // §D 删除成功本地 flag
  const TIME_THRESHOLD = 1e12;        // §D 时间戳秒/毫秒阈值

  // §7 类型展示字典（可配置）
  const TYPE_LABELS: Record<number, string> = {
    1: '文本', 2: '图片', 3: '视频', 4: '音频', 5: '链接',
    6: '位置', 7: '文件', 8: '名片', 14: '聊天记录', 15: '小程序',
    16: '笔记', 17: '音乐', 18: '收藏标签',
  };

  // ═══════════════════════════════════════════════════════════
  // 类型定义
  // ═══════════════════════════════════════════════════════════
  interface FavorRow {
    favId: number;
    type: number;
    flag: number;
    updateTime: number;
    [key: string]: unknown;
  }

  interface ParseCacheEntry {
    desc: string;
    fromUsr: string;
    snippet: string;
    xmlType: number;  // §7 从 XML 探测的类型
  }

  // ═══════════════════════════════════════════════════════════
  // §3 客户端内存模型
  // ═══════════════════════════════════════════════════════════
  let indexMap = $state<Map<number, FavorRow>>(new Map());
  let parseCache = $state<Map<number, ParseCacheEntry>>(new Map());
  let syncKey = $state('');  // §3 同步游标

  // UI 状态
  let favIdInput = $state('');
  let logs = $state<string[]>([]);
  let isSyncing = $state(false);
  let isGettingContent = $state(false);
  let isDeleting = $state(false);
  let isSending = $state(false);

  // ═══════════════════════════════════════════════════════════
  // §12 日志
  // ═══════════════════════════════════════════════════════════
  function addLog(msg: string) {
    const time = new Date().toLocaleTimeString('zh-CN', { hour12: false });
    logs = [`[${time}] ${msg}`, ...logs].slice(0, 500);
  }

  // ═══════════════════════════════════════════════════════════
  // §10 HTML 转义
  // ═══════════════════════════════════════════════════════════
  function escapeHtml(s: string): string {
    return String(s || '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;');
  }

  // ═══════════════════════════════════════════════════════════
  // §7 XML 摘要解析
  // ═══════════════════════════════════════════════════════════
  function parseXmlSummary(xml: string): ParseCacheEntry {
    if (!xml || !xml.trim()) {
      return { desc: '', fromUsr: '', snippet: '无摘要', xmlType: 0 };
    }

    let desc = '';
    let fromUsr = '';
    let xmlType = 0;

    // §7 步骤 2：尝试 DOM 解析
    try {
      const parser = new DOMParser();
      const doc = parser.parseFromString(xml, 'text/xml');
      const parserError = doc.querySelector('parsererror');
      if (!parserError) {
        const descEl = doc.querySelector('desc');
        if (descEl) desc = descEl.textContent || '';
        // §7 大小写兼容
        const fromEl = doc.querySelector('fromusr') || doc.querySelector('fromUsr') || doc.querySelector('FromUsr');
        if (fromEl) fromUsr = fromEl.textContent || '';
      }
    } catch {
      // DOM 解析失败，走正则兜底
    }

    // §7 步骤 3：正则兜底 desc
    if (!desc) {
      const m = xml.match(/<desc[^>]*>(?:<!\[CDATA\[([\s\S]*?)\]\]>|([\s\S]*?))<\/desc>/i);
      if (m) desc = (m[1] ?? m[2] ?? '').trim();
      // 反转义 XML 实体
      desc = desc.replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"');
    }

    // §7 步骤 4：正则兜底 fromUsr
    if (!fromUsr) {
      const m = xml.match(/<fromusr[^>]*>(?:<!\[CDATA\[([\s\S]*?)\]\]>|([\s\S]*?))<\/fromusr>/i);
      if (m) fromUsr = (m[1] ?? m[2] ?? '').trim();
    }

    // §7 类型探测
    const tm = xml.match(/<favitem[^>]*\btype="(\d+)"/i);
    if (tm) xmlType = parseInt(tm[1], 10) || 0;

    // §7 步骤 5：snippet
    let snippet = desc;
    if (!snippet) {
      const compressed = xml.replace(/\s+/g, ' ').trim();
      snippet = compressed.slice(0, SNIPPET_MAX_LEN) || '无摘要';
    }

    return { desc, fromUsr, snippet, xmlType };
  }

  /** §7 类型展示 */
  function typeLabel(type: number): string {
    return TYPE_LABELS[type] || `类型 ${type}`;
  }

  // ═══════════════════════════════════════════════════════════
  // §10 时间格式化（秒/毫秒启发式）
  // ═══════════════════════════════════════════════════════════
  function formatTime(ts: number): string {
    if (!ts) return '—';
    const ms = ts < TIME_THRESHOLD ? ts * 1000 : ts;
    return new Date(ms).toLocaleString('zh-CN', { hour12: false });
  }

  // ═══════════════════════════════════════════════════════════
  // §5 卡片列表（排序展示）
  // ═══════════════════════════════════════════════════════════
  const sortedItems = $derived(
    Array.from(indexMap.values()).sort((a, b) => (b.updateTime || 0) - (a.updateTime || 0))
  );

  // ═══════════════════════════════════════════════════════════
  // §4 同步列表与翻页
  // ═══════════════════════════════════════════════════════════
  async function syncFavors(isNextPage = false) {
    // §4.2 前置：下一页无游标禁止请求
    if (isNextPage && !syncKey) {
      addLog('⚠️ 无游标，请先执行首屏同步');
      return;
    }

    if (isSyncing) return;
    isSyncing = true;

    try {
      const body = { syncKey: isNextPage ? syncKey : '' };
      const res = await apiPost('/favor/sync', body, consoleState);

      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        addLog('❌ TOKEN 已失效');
        return;
      }

      if (res.data?.ret !== 200) {
        addLog(`⚠️ 同步失败: ${res.data?.msg || '未知错误'}`);
        return;
      }

      const data = res.data?.data as Record<string, unknown> | undefined;
      if (!data) {
        addLog('⚠️ 同步返回数据为空');
        return;
      }

      // §4.1 首屏：清空索引与解析缓存
      if (!isNextPage) {
        indexMap = new Map();
        parseCache = new Map();
      }

      // §5 合并列表
      const list = Array.isArray(data.list) ? data.list : [];
      let added = 0;
      for (const item of list) {
        const row = item as Record<string, unknown>;
        const favId = Number(row.favId);
        if (!favId || isNaN(favId)) continue;
        // §5 合并策略：新字段覆盖旧字段
        const existing = indexMap.get(favId) || { favId, type: 0, flag: 0, updateTime: 0 };
        indexMap.set(favId, { ...existing, ...row, favId });
        added++;
      }

      // §4 更新游标
      const newKey = typeof data.syncKey === 'string' ? data.syncKey : '';
      syncKey = newKey;

      addLog(`✅ 同步 ${added} 条收藏（${isNextPage ? '翻页' : '首屏'}）`);
    } catch (e) {
      addLog(`❌ 同步失败: ${(e as Error).message}`);
    } finally {
      isSyncing = false;
    }
  }

  // ═══════════════════════════════════════════════════════════
  // §6 获取详情（单条）
  // ═══════════════════════════════════════════════════════════
  function parseFavId(input: string): number | null {
    const trimmed = String(input || '').trim();
    const num = parseInt(trimmed, 10);
    return isNaN(num) || num <= 0 ? null : num;
  }

  async function getContent() {
    const favId = parseFavId(favIdInput);
    if (favId === null) {
      addLog('⚠️ 请输入有效的 favId（正整数）');
      return;
    }

    if (isGettingContent) return;
    isGettingContent = true;

    try {
      const res = await apiPost('/favor/getContent', { favId }, consoleState);

      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        addLog('❌ TOKEN 已失效');
        return;
      }

      if (res.data?.ret !== 200) {
        addLog(`⚠️ 获取详情失败: ${res.data?.msg}`);
        return;
      }

      const data = res.data?.data as Record<string, unknown> | undefined;
      if (!data) {
        addLog('⚠️ 详情数据为空');
        return;
      }

      // §6 解析 XML 写入解析缓存
      const xml = String(data.content || '');
      const parsed = parseXmlSummary(xml);
      parseCache.set(favId, parsed);

      // §6 索引补全
      const existing = indexMap.get(favId);
      if (!existing) {
        // 插入最小行
        indexMap.set(favId, {
          favId,
          type: parsed.xmlType || 0,
          flag: Number(data.flag || 0),
          updateTime: Number(data.updateTime || 0),
        });
      } else {
        // 合并更新
        if (parsed.xmlType) existing.type = parsed.xmlType;
        if (data.flag != null) existing.flag = Number(data.flag);
        if (data.updateTime != null) existing.updateTime = Number(data.updateTime);
        indexMap.set(favId, { ...existing });
      }

      addLog(`✅ 获取详情成功: favId=${favId} 类型=${typeLabel(parsed.xmlType || indexMap.get(favId)?.type || 0)}`);
    } catch (e) {
      addLog(`❌ 获取详情失败: ${(e as Error).message}`);
    } finally {
      isGettingContent = false;
    }
  }

  // ═══════════════════════════════════════════════════════════
  // §8 删除收藏（二次确认）
  // ═══════════════════════════════════════════════════════════
  async function deleteFavor() {
    const favId = parseFavId(favIdInput);
    if (favId === null) {
      addLog('⚠️ 请输入有效的 favId');
      return;
    }

    // §8 须二次确认
    const confirmed = confirm(`确定删除收藏？\n\nfavId: ${favId}`);
    if (!confirmed) return;

    if (isDeleting) return;
    isDeleting = true;

    try {
      const res = await apiPost('/favor/delete', { favId }, consoleState);

      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        addLog('❌ TOKEN 已失效');
        return;
      }

      if (res.data?.ret !== 200) {
        addLog(`⚠️ 删除失败: ${res.data?.msg}`);
        return;
      }

      // §8 成功：就地将 flag 置为 1
      const row = indexMap.get(favId);
      if (row) {
        row.flag = DELETED_FLAG;
        indexMap.set(favId, { ...row });
      }

      addLog(`✅ 收藏 ${favId} 已标记删除`);
    } catch (e) {
      addLog(`❌ 删除失败: ${(e as Error).message}`);
    } finally {
      isDeleting = false;
    }
  }

  // ═══════════════════════════════════════════════════════════
  // §9 发送给当前目标（跨模块 postText）
  // ═══════════════════════════════════════════════════════════
  async function sendToTarget() {
    const favId = parseFavId(favIdInput);
    if (favId === null) {
      addLog('⚠️ 请输入有效的 favId');
      return;
    }

    // §9 前置：校验全局目标
    const toWxid = (consoleState.currentTargetWxid || '').trim();
    if (!toWxid) {
      addLog('⚠️ 请先在通讯录等模块锁定聊天目标');
      return;
    }

    if (isSending) return;
    isSending = true;

    try {
      // 步骤 1：getContent
      const res = await apiPost('/favor/getContent', { favId }, consoleState);

      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        addLog('❌ TOKEN 已失效');
        return;
      }

      if (res.data?.ret !== 200) {
        addLog(`⚠️ 获取详情失败: ${res.data?.msg}`);
        return;
      }

      const data = res.data?.data as Record<string, unknown> | undefined;
      const xml = String(data?.content || '');

      // 步骤 2：解析 XML
      const parsed = parseXmlSummary(xml);
      parseCache.set(favId, parsed);

      // 步骤 3：组装文本
      const row = indexMap.get(favId);
      const typeNum = row?.type || parsed.xmlType || 0;
      const targetName = (consoleState.currentTargetDisplayName || '').trim() || toWxid;

      const lines = [
        `📎 收藏转发 — favId: ${favId}`,
        `类型: ${typeLabel(typeNum)}`,
      ];
      if (parsed.fromUsr) lines.push(`来源: ${parsed.fromUsr}`);
      lines.push('');
      lines.push(parsed.desc || parsed.snippet || '无文本描述');

      let content = lines.join('\n');
      // §9 截断至 8000 字符
      if (content.length > FORWARD_TEXT_MAX_LEN) {
        content = content.slice(0, FORWARD_TEXT_MAX_LEN) + '\n...(已截断)';
      }

      // 步骤 5：发送
      const sendRes = await apiPost('/message/postText', { toWxid, content }, consoleState);

      if (sendRes.data?.ret === 200) {
        addLog(`✅ 已发送给 ${targetName}`);
      } else {
        addLog(`⚠️ 发送失败: ${sendRes.data?.msg}`);
      }
    } catch (e) {
      addLog(`❌ 发送失败: ${(e as Error).message}`);
    } finally {
      isSending = false;
    }
  }

  // ═══════════════════════════════════════════════════════════
  // §11 当前发送目标展示
  // ═══════════════════════════════════════════════════════════
  const targetDisplay = $derived.by(() => {
    const wxid = (consoleState.currentTargetWxid || '').trim();
    if (!wxid) return '';
    const name = (consoleState.currentTargetDisplayName || '').trim() || lookupContactDisplayName(wxid);
    return name && name !== wxid ? `${name}（${wxid}）` : wxid;
  });

  function clearLogs() { logs = []; }
</script>

<div class="wa-mod">
  <div class="wa-mod-split">
    <!-- ═══ 左侧：操作区 ═══ -->
    <div class="wa-mod-left">
      <!-- §4 同步 -->
      <div class="wa-card">
        <h3 class="wa-card-title">收藏夹</h3>
        <p class="wa-hint">3 个接口：sync / getContent / delete</p>
        <p class="wa-cursor">当前 syncKey：{syncKey || '（未同步）'}</p>
        <div class="wa-actions">
          <button class="wa-btn wa-btn-primary" onclick={() => syncFavors(false)} disabled={isSyncing}>
            {isSyncing ? '同步中...' : '同步收藏夹'}
          </button>
          <button class="wa-btn" onclick={() => syncFavors(true)} disabled={isSyncing || !syncKey}>
            加载下一页
          </button>
        </div>
      </div>

      <!-- §6 获取详情 + §8 删除 + §9 发送 -->
      <div class="wa-card">
        <h3 class="wa-card-title">按 favId 操作</h3>
        <div class="wa-form-grid">
          <label class="wa-field">
            <span class="wa-label">favId *</span>
            <input type="text" bind:value={favIdInput} placeholder="收藏 ID（正整数）" />
          </label>
        </div>
        <div class="wa-actions">
          <button class="wa-btn wa-btn-primary" onclick={getContent} disabled={isGettingContent}>
            {isGettingContent ? '获取中...' : '获取详情'}
          </button>
          <button class="wa-btn" onclick={deleteFavor} disabled={isDeleting}>
            {isDeleting ? '删除中...' : '删除记录'}
          </button>
          <button class="wa-btn" onclick={sendToTarget} disabled={isSending || !consoleState.currentTargetWxid}>
            {isSending ? '发送中...' : '发送给当前目标'}
          </button>
        </div>
      </div>

      <!-- §11 发送目标提示 -->
      {#if targetDisplay}
        <div class="wa-card">
          <p class="wa-target-hint">📤 当前发送目标: <strong>{targetDisplay}</strong></p>
        </div>
      {/if}
    </div>

    <!-- ═══ 右侧：卡片 + 日志 ═══ -->
    <div class="wa-mod-right">
      <!-- §10 卡片列表 -->
      <div class="wa-card wa-card-fill">
        <h3 class="wa-card-title">收藏预览</h3>
        <div class="wa-favor-list">
          {#each sortedItems as row (row.favId)}
            {@const cache = parseCache.get(row.favId)}
            {@const isDeleted = row.flag === DELETED_FLAG}
            <div class="wa-favor-item" class:deleted={isDeleted}>
              <div class="wa-favor-header">
                <span class="wa-favor-id">#{row.favId}</span>
                <span class="wa-favor-type">{typeLabel(row.type)}</span>
                {#if isDeleted}
                  <span class="wa-deleted-badge">已删除</span>
                {/if}
              </div>
              {#if cache?.fromUsr}
                <p class="wa-favor-from">来源: {escapeHtml(cache.fromUsr)}</p>
              {/if}
              <p class="wa-favor-desc">{escapeHtml(cache?.snippet || '—')}</p>
              <div class="wa-favor-footer">
                <span class="wa-favor-time">{formatTime(row.updateTime)}</span>
                <div class="wa-favor-actions">
                  <button class="wa-btn wa-btn-sm" onclick={() => { favIdInput = String(row.favId); getContent(); }}>详情</button>
                  <button class="wa-btn wa-btn-sm" onclick={() => { favIdInput = String(row.favId); sendToTarget(); }} disabled={!consoleState.currentTargetWxid}>发送</button>
                </div>
              </div>
            </div>
          {:else}
            <p class="wa-empty-hint">暂无数据，请先同步收藏夹</p>
          {/each}
        </div>
      </div>

      <!-- §12 日志 -->
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
  .wa-hint { font-size: 12px; color: var(--muted-foreground); margin: 0 0 8px; }
  .wa-cursor { font-size: 11.5px; font-family: var(--font-mono); color: var(--muted-foreground); margin: 0 0 12px; }
  .wa-form-grid { display: flex; flex-direction: column; gap: 10px; }
  .wa-field { display: flex; flex-direction: column; gap: 4px; }
  .wa-label { font-size: 12px; font-weight: 600; }
  .wa-field input { padding: 6px 10px; border: 1px solid var(--border); border-radius: 6px; background: var(--card); font-size: 13px; color: var(--foreground); }
  .wa-actions { display: flex; gap: 8px; margin-top: 12px; flex-wrap: wrap; }
  .wa-btn { padding: 6px 14px; border: 1px solid var(--border); border-radius: 6px; background: var(--card); font-size: 13px; cursor: pointer; color: var(--foreground); }
  .wa-btn:hover { background: var(--muted); }
  .wa-btn-primary { background: var(--primary); color: var(--primary-foreground); border-color: var(--primary); }
  .wa-btn-sm { padding: 3px 8px; font-size: 11.5px; }
  .wa-btn:disabled { opacity: 0.4; cursor: default; pointer-events: none; }
  .wa-target-hint { font-size: 13px; margin: 0; }
  .wa-target-hint strong { color: var(--primary); }
  .wa-favor-list { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 8px; }
  .wa-favor-item { padding: 12px; border: 1px solid var(--border); border-radius: 8px; }
  .wa-favor-item.deleted { opacity: 0.6; }
  .wa-favor-header { display: flex; align-items: center; gap: 8px; margin-bottom: 6px; }
  .wa-favor-id { font-family: var(--font-mono); font-size: 12px; color: var(--muted-foreground); }
  .wa-favor-type { font-size: 12px; padding: 1px 6px; border-radius: 4px; background: var(--muted); }
  .wa-deleted-badge { font-size: 11px; padding: 1px 6px; border-radius: 4px; background: color-mix(in srgb, #dc2626 14%, transparent); color: #b91c1c; }
  .wa-favor-from { font-size: 12px; color: var(--muted-foreground); margin: 0 0 4px; }
  .wa-favor-desc { font-size: 13px; margin: 0 0 8px; white-space: pre-wrap; word-break: break-all; }
  .wa-favor-footer { display: flex; justify-content: space-between; align-items: center; }
  .wa-favor-time { font-size: 11.5px; color: var(--muted-foreground); }
  .wa-favor-actions { display: flex; gap: 4px; }
  .wa-empty-hint { color: var(--muted-foreground); font-size: 13px; text-align: center; padding: 24px 0; }
  .wa-log-body { flex: 1; min-height: 0; overflow-y: auto; background: #1e1e1e; border-radius: 8px; padding: 10px; font-family: var(--font-mono); font-size: 12px; color: #a6e22e; }
  .wa-log-line { padding: 2px 0; white-space: pre-wrap; word-break: break-all; }
  .wa-log-empty { color: #888; }
</style>
