<script lang="ts">
  /**
   * 消息发送与 CDN 模块 — 严格遵循《消息发送与 CDN — 业务逻辑规范》
   *
   * §3 接收人解析规则：从 `id（展示名）` 提取纯 id
   * §4 与全局会话及本地持久化的同步
   * §5 消息类型与面板（单选切换，保留已填内容）
   * §6 各发送操作的输入与正文映射（显式 toWxid）
   * §7 CDN 下载（无 toWxid）
   * §9 日志与错误语义
   * §C.1 防连点
   */
  import { apiPost, isTokenInvalidPayload } from '../services/api';
  import { consoleState, lookupContactDisplayName } from '../stores/console.svelte';
  import { onMount } from 'svelte';

  // ═══════════════════════════════════════════════════════════
  // 状态
  // ═══════════════════════════════════════════════════════════
  type MsgType = 'text' | 'image' | 'file' | 'link' | 'video' | 'miniapp';

  let msgType = $state<MsgType>('text');
  let toWxidInput = $state('');  // §2 展示串：可能含 `id（展示名）`
  let logs = $state<string[]>([]);
  let isSending = $state(false); // §C.1 防连点

  // ─── 文本 ───
  let textContent = $state('这是来自 ST 控制台的测试消息');
  let textAts = $state('');

  // ─── 图片 ───
  let imgUrl = $state('');

  // ─── 文件 ───
  let fileUrl = $state('');
  let fileName = $state('');

  // ─── 链接 ───
  let linkTitle = $state('');
  let linkDesc = $state('');
  let linkUrl = $state('');
  let linkThumb = $state('');

  // ─── 视频 ───
  let videoUrl = $state('');
  let videoThumb = $state('');
  let videoDurationRaw = $state('10');

  // ─── 小程序 ───
  let miniAppId = $state('');
  let miniUser = $state('');
  let miniTitle = $state('');
  let miniCover = $state('');
  let miniPath = $state('');
  let miniDisplay = $state('');

  // ─── CDN ───
  let cdnAes = $state('');
  let cdnFileId = $state('');
  let cdnType = $state('');
  let cdnSize = $state('');
  let cdnSuffix = $state('');

  // ═══════════════════════════════════════════════════════════
  // §9 日志
  // ═══════════════════════════════════════════════════════════
  function addLog(msg: string) {
    const time = new Date().toLocaleTimeString('zh-CN', { hour12: false });
    logs = [`[${time}] ${msg}`, ...logs].slice(0, 500);
  }

  // ═══════════════════════════════════════════════════════════
  // §3 接收人解析规则（必须实现）
  //
  // 1. 取输入字符串首尾去空白
  // 2. 若为空串 → 无接收人
  // 3. 按第一次出现的分隔截取左侧子串：分隔为 `（` 或 `(`
  // 4. 对左侧子串再次首尾去空白 → 真实接收人标识
  // ═══════════════════════════════════════════════════════════
  function extractToWxid(input: string): string {
    const raw = String(input || '').trim();
    if (!raw) return '';
    // §3 按第一次出现的全角左括号或半角左括号截取左侧
    const idxParenFull = raw.indexOf('（');
    const idxParenHalf = raw.indexOf('(');
    let cutIdx = -1;
    if (idxParenFull >= 0 && idxParenHalf >= 0) {
      cutIdx = Math.min(idxParenFull, idxParenHalf);
    } else if (idxParenFull >= 0) {
      cutIdx = idxParenFull;
    } else if (idxParenHalf >= 0) {
      cutIdx = idxParenHalf;
    }
    const candidate = cutIdx >= 0 ? raw.slice(0, cutIdx) : raw;
    return candidate.trim();
  }

  // ═══════════════════════════════════════════════════════════
  // §4 与全局会话及本地持久化的同步
  //
  // 同步到输入框：
  //   若锁定标识非空：
  //     若有非空展示名 且 展示名与标识归一比较不同 → `id（展示名）`
  //     否则 → 仅 id
  // ═══════════════════════════════════════════════════════════
  function syncTargetFromSession() {
    const id = (consoleState.currentTargetWxid || '').trim();
    if (!id) return;

    // §4 计算展示名
    let displayName = (consoleState.currentTargetDisplayName || '').trim();
    if (!displayName || displayName === id) {
      // 尝试从通讯录缓存反查
      const looked = lookupContactDisplayName(id);
      if (looked && looked !== id) displayName = looked;
    }

    // §4 赋值规则
    if (displayName && displayName !== id) {
      toWxidInput = `${id}（${displayName}）`;
    } else {
      toWxidInput = id;
    }

    addLog(`已同步目标: ${toWxidInput}`);
  }

  // §4 同步按钮反馈 Toast
  function handleSync() {
    syncTargetFromSession();
    const id = extractToWxid(toWxidInput);
    const displayName = (consoleState.currentTargetDisplayName || '').trim();
    if (id && displayName && displayName !== id) {
      addLog(`✅ 已同步: ${displayName} (${id})`);
    } else if (id) {
      addLog(`✅ 已同步: ${id}`);
    } else {
      addLog('⚠️ 无锁定目标');
    }
  }

  // ═══════════════════════════════════════════════════════════
  // §6 视频时长解析（规范要求）
  // 空 → 默认 10；否则取整，下限 1；解析失败 → 回退 10
  // ═══════════════════════════════════════════════════════════
  function parseVideoDuration(raw: string): number {
    const trimmed = String(raw || '').trim();
    if (!trimmed) return 10;
    const num = parseInt(trimmed, 10);
    if (isNaN(num)) return 10;
    return Math.max(1, num);
  }

  // ═══════════════════════════════════════════════════════════
  // §6 发送操作（公共前置：解析接收人；显式 toWxid）
  // §C.1 防连点：isSending 期间 disabled
  // ═══════════════════════════════════════════════════════════
  async function sendRequest(path: string, body: Record<string, unknown>, label: string) {
    // §6 公共前置：解析接收人
    const toWxid = extractToWxid(toWxidInput);
    if (!toWxid) {
      addLog('❌ 请先填写接收人（或从通讯录锁定目标）');
      return;
    }

    // §6 显式包含 toWxid（不得依赖合并规则）
    const payload = { toWxid, ...body };

    isSending = true;
    const t0 = Date.now();
    try {
      const res = await apiPost(path, payload, consoleState);
      const duration = Date.now() - t0;

      // §4.5 令牌失效检测
      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        addLog('❌ TOKEN 已失效');
        return;
      }

      const ret = res.data?.ret;
      const msg = res.data?.msg || '';
      if (ret === 200) {
        addLog(`✅ ${label} 发送成功 (${duration}ms) ret=${ret}`);
      } else {
        // §9 业务层 ret !== 200：写日志
        addLog(`⚠️ ${label} 业务异常 (${duration}ms) ret=${ret} msg=${msg}`);
      }
    } catch (e) {
      // §9 传输层失败：Toast
      const msg = (e as Error).message || '未知错误';
      addLog(`❌ ${label} 发送失败: ${msg}`);
    } finally {
      isSending = false;
    }
  }

  // §6 文本
  async function sendText() {
    if (!textContent.trim()) { addLog('❌ 消息内容不能为空'); return; }
    const body: Record<string, unknown> = { content: textContent };
    // §6 若群 @ 列表非空，增加 ats
    if (textAts.trim()) body.ats = textAts;
    await sendRequest('/message/postText', body, '文本');
  }

  // §6 图片
  async function sendImage() {
    if (!imgUrl.trim()) { addLog('❌ 图片 URL 不能为空'); return; }
    await sendRequest('/message/postImage', { imgUrl }, '图片');
  }

  // §6 文件
  async function sendFile() {
    if (!fileUrl.trim()) { addLog('❌ 文件 URL 不能为空'); return; }
    if (!fileName.trim()) { addLog('❌ 文件名不能为空'); return; }
    await sendRequest('/message/postFile', { fileUrl, fileName }, '文件');
  }

  // §6 链接（字段名须为 linkUrl，不得误用 url）
  async function sendLink() {
    if (!linkTitle.trim()) { addLog('❌ 链接标题不能为空'); return; }
    if (!linkUrl.trim()) { addLog('❌ 链接地址不能为空'); return; }
    await sendRequest('/message/postLink', {
      title: linkTitle,
      desc: linkDesc,
      linkUrl,  // §E 禁止项：不得命名为 url
      thumbUrl: linkThumb,
    }, '链接');
  }

  // §6 视频
  async function sendVideo() {
    if (!videoUrl.trim()) { addLog('❌ 视频 URL 不能为空'); return; }
    if (!videoThumb.trim()) { addLog('❌ 缩略图 URL 不能为空'); return; }
    await sendRequest('/message/postVideo', {
      videoUrl,
      thumbUrl: videoThumb,
      videoDuration: parseVideoDuration(videoDurationRaw),
    }, '视频');
  }

  // §6 小程序（全部非空校验）
  async function sendMiniApp() {
    const fields = { miniAppId, userName: miniUser, title: miniTitle, coverImgUrl: miniCover, pagePath: miniPath, displayName: miniDisplay };
    for (const [k, v] of Object.entries(fields)) {
      if (!String(v || '').trim()) { addLog(`❌ 小程序字段 ${k} 不能为空`); return; }
    }
    await sendRequest('/message/postMiniApp', fields, '小程序');
  }

  // ═══════════════════════════════════════════════════════════
  // §7 CDN 下载（无 toWxid）
  // ═══════════════════════════════════════════════════════════
  async function downloadCdn() {
    // §7 校验
    if (!cdnAes.trim()) { addLog('❌ aesKey 不能为空'); return; }
    if (!cdnFileId.trim()) { addLog('❌ fileId 不能为空'); return; }
    if (!cdnType.trim()) { addLog('❌ type 不能为空'); return; }
    if (!cdnSize.trim()) { addLog('❌ totalSize 不能为空'); return; }
    if (!cdnSuffix.trim()) { addLog('❌ suffix 不能为空'); return; }

    isSending = true;
    const t0 = Date.now();
    try {
      // §7 不得包含 toWxid
      const res = await apiPost('/message/downloadCdn', {
        aesKey: cdnAes,
        fileId: cdnFileId,
        type: cdnType,
        totalSize: cdnSize,
        suffix: cdnSuffix,
      }, consoleState);
      const duration = Date.now() - t0;

      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        addLog('❌ TOKEN 已失效');
        return;
      }

      const ret = res.data?.ret;
      if (ret === 200) {
        addLog(`✅ CDN 下载成功 (${duration}ms)`);
      } else {
        addLog(`⚠️ CDN 业务异常 (${duration}ms) ret=${ret} msg=${res.data?.msg}`);
      }
    } catch (e) {
      addLog(`❌ CDN 下载失败: ${(e as Error).message}`);
    } finally {
      isSending = false;
    }
  }

  function clearLogs() { logs = []; }

  // ═══════════════════════════════════════════════════════════
  // §A.1 / §10 模块初始化：同步接收人
  // ═══════════════════════════════════════════════════════════
  onMount(() => {
    syncTargetFromSession();
  });
</script>

<div class="wa-mod">
  <div class="wa-mod-split">
    <!-- ═══ 左侧：表单 ═══ -->
    <div class="wa-mod-left">
      <!-- §3 接收人输入 + §4 同步按钮 -->
      <div class="wa-card">
        <h3 class="wa-card-title">消息发送</h3>
        <div class="wa-form-grid">
          <label class="wa-field">
            <span class="wa-label">接收人 toWxid</span>
            <div class="wa-input-row">
              <input type="text" bind:value={toWxidInput} placeholder="wxid_xxx 或 wxid_xxx（备注名）" />
              <button class="wa-btn wa-btn-primary" onclick={handleSync}>同步目标</button>
            </div>
            <span class="wa-field-hint">支持粘贴 `id（展示名）` 格式，自动解析纯 id</span>
          </label>
        </div>
      </div>

      <!-- §5 消息类型选择器 -->
      <div class="wa-card">
        <div class="wa-type-selector">
          <button class="wa-type-btn" class:active={msgType === 'text'} onclick={() => msgType = 'text'}>文本</button>
          <button class="wa-type-btn" class:active={msgType === 'image'} onclick={() => msgType = 'image'}>图片</button>
          <button class="wa-type-btn" class:active={msgType === 'file'} onclick={() => msgType = 'file'}>文件</button>
          <button class="wa-type-btn" class:active={msgType === 'link'} onclick={() => msgType = 'link'}>链接</button>
          <button class="wa-type-btn" class:active={msgType === 'video'} onclick={() => msgType = 'video'}>视频</button>
          <button class="wa-type-btn" class:active={msgType === 'miniapp'} onclick={() => msgType = 'miniapp'}>小程序</button>
        </div>
      </div>

      <!-- §5 各类型表单面板（切换时保留已填内容） -->
      {#if msgType === 'text'}
        <div class="wa-card">
          <h3 class="wa-card-title">文本 — /message/postText</h3>
          <div class="wa-form-grid">
            <label class="wa-field">
              <span class="wa-label">content *</span>
              <textarea bind:value={textContent} rows="4"></textarea>
            </label>
            <label class="wa-field">
              <span class="wa-label">ats（可选，群 @ 列表）</span>
              <input type="text" bind:value={textAts} placeholder="wxid1,wxid2 或 notify@all" />
            </label>
          </div>
          <div class="wa-actions">
            <button class="wa-btn wa-btn-primary" onclick={sendText} disabled={isSending}>
              {isSending ? '发送中...' : '发送文本'}
            </button>
          </div>
        </div>

      {:else if msgType === 'image'}
        <div class="wa-card">
          <h3 class="wa-card-title">图片 — /message/postImage</h3>
          <div class="wa-form-grid">
            <label class="wa-field">
              <span class="wa-label">imgUrl *</span>
              <input type="text" bind:value={imgUrl} placeholder="图片直链 URL" />
            </label>
          </div>
          <div class="wa-actions">
            <button class="wa-btn wa-btn-primary" onclick={sendImage} disabled={isSending}>
              {isSending ? '发送中...' : '发送图片'}
            </button>
          </div>
        </div>

      {:else if msgType === 'file'}
        <div class="wa-card">
          <h3 class="wa-card-title">文件 — /message/postFile</h3>
          <div class="wa-form-grid">
            <label class="wa-field">
              <span class="wa-label">fileUrl *</span>
              <input type="text" bind:value={fileUrl} placeholder="文件直链 URL" />
            </label>
            <label class="wa-field">
              <span class="wa-label">fileName *</span>
              <input type="text" bind:value={fileName} placeholder="显示文件名，如 report.xlsx" />
            </label>
          </div>
          <div class="wa-actions">
            <button class="wa-btn wa-btn-primary" onclick={sendFile} disabled={isSending}>
              {isSending ? '发送中...' : '发送文件'}
            </button>
          </div>
        </div>

      {:else if msgType === 'link'}
        <div class="wa-card">
          <h3 class="wa-card-title">链接 — /message/postLink</h3>
          <p class="wa-hint">字段名须为 <code>linkUrl</code>，不得误用 <code>url</code></p>
          <div class="wa-form-grid">
            <label class="wa-field">
              <span class="wa-label">title *</span>
              <input type="text" bind:value={linkTitle} placeholder="链接标题" />
            </label>
            <label class="wa-field">
              <span class="wa-label">desc</span>
              <input type="text" bind:value={linkDesc} placeholder="摘要/描述" />
            </label>
            <label class="wa-field">
              <span class="wa-label">linkUrl *</span>
              <input type="text" bind:value={linkUrl} placeholder="https://..." />
            </label>
            <label class="wa-field">
              <span class="wa-label">thumbUrl</span>
              <input type="text" bind:value={linkThumb} placeholder="缩略图 URL" />
            </label>
          </div>
          <div class="wa-actions">
            <button class="wa-btn wa-btn-primary" onclick={sendLink} disabled={isSending}>
              {isSending ? '发送中...' : '发送链接'}
            </button>
          </div>
        </div>

      {:else if msgType === 'video'}
        <div class="wa-card">
          <h3 class="wa-card-title">视频 — /message/postVideo</h3>
          <div class="wa-form-grid">
            <label class="wa-field">
              <span class="wa-label">videoUrl *</span>
              <input type="text" bind:value={videoUrl} placeholder="视频直链" />
            </label>
            <label class="wa-field">
              <span class="wa-label">thumbUrl *</span>
              <input type="text" bind:value={videoThumb} placeholder="封面图直链" />
            </label>
            <label class="wa-field">
              <span class="wa-label">videoDuration（秒）</span>
              <input type="number" bind:value={videoDurationRaw} placeholder="默认 10，最小 1" min="1" step="1" />
            </label>
          </div>
          <div class="wa-actions">
            <button class="wa-btn wa-btn-primary" onclick={sendVideo} disabled={isSending}>
              {isSending ? '发送中...' : '发送视频'}
            </button>
          </div>
        </div>

      {:else if msgType === 'miniapp'}
        <div class="wa-card">
          <h3 class="wa-card-title">小程序 — /message/postMiniApp</h3>
          <p class="wa-hint">全部字段必填</p>
          <div class="wa-form-grid">
            <label class="wa-field">
              <span class="wa-label">miniAppId *</span>
              <input type="text" bind:value={miniAppId} placeholder="wx1f9ea355b47256dd" />
            </label>
            <label class="wa-field">
              <span class="wa-label">userName *</span>
              <input type="text" bind:value={miniUser} placeholder="gh_xxx@app" />
            </label>
            <label class="wa-field">
              <span class="wa-label">title *</span>
              <input type="text" bind:value={miniTitle} placeholder="卡片标题" />
            </label>
            <label class="wa-field">
              <span class="wa-label">coverImgUrl *</span>
              <input type="text" bind:value={miniCover} placeholder="封面图 URL" />
            </label>
            <label class="wa-field">
              <span class="wa-label">pagePath *</span>
              <input type="text" bind:value={miniPath} placeholder="pages/index/index.html" />
            </label>
            <label class="wa-field">
              <span class="wa-label">displayName *</span>
              <input type="text" bind:value={miniDisplay} placeholder="小程序展示名" />
            </label>
          </div>
          <div class="wa-actions">
            <button class="wa-btn wa-btn-primary" onclick={sendMiniApp} disabled={isSending}>
              {isSending ? '发送中...' : '发送小程序'}
            </button>
          </div>
        </div>
      {/if}

      <!-- §7 CDN 下载（无 toWxid） -->
      <details class="wa-card">
        <summary class="wa-summary">CDN 下载 — /message/downloadCdn</summary>
        <p class="wa-hint" style="margin-top:8px">无接收人；设备标识通过合并规则注入。</p>
        <div class="wa-form-grid" style="margin-top:8px">
          <label class="wa-field">
            <span class="wa-label">aesKey *</span>
            <input type="text" bind:value={cdnAes} />
          </label>
          <label class="wa-field">
            <span class="wa-label">fileId *</span>
            <input type="text" bind:value={cdnFileId} />
          </label>
          <label class="wa-field">
            <span class="wa-label">type *</span>
            <input type="text" bind:value={cdnType} placeholder="1高清 2常规 3缩略 4视频 5文件" />
          </label>
          <label class="wa-field">
            <span class="wa-label">totalSize *</span>
            <input type="text" bind:value={cdnSize} placeholder="文件大小（字符串）" />
          </label>
          <label class="wa-field">
            <span class="wa-label">suffix *</span>
            <input type="text" bind:value={cdnSuffix} placeholder="如 doc, json, mp4" />
          </label>
        </div>
        <div class="wa-actions">
          <button class="wa-btn wa-btn-primary" onclick={downloadCdn} disabled={isSending}>
            {isSending ? '下载中...' : '下载 CDN'}
          </button>
        </div>
      </details>
    </div>

    <!-- ═══ 右侧：日志 ═══ -->
    <div class="wa-mod-right">
      <div class="wa-card wa-card-fill">
        <div class="wa-card-head">
          <h3 class="wa-card-title">全链路日志</h3>
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
  .wa-hint code { font-family: var(--font-mono); background: var(--muted); padding: 1px 4px; border-radius: 3px; font-size: 11.5px; }
  .wa-form-grid { display: flex; flex-direction: column; gap: 10px; }
  .wa-field { display: flex; flex-direction: column; gap: 4px; }
  .wa-label { font-size: 12px; font-weight: 600; }
  .wa-field-hint { font-size: 11.5px; color: var(--muted-foreground); margin-top: 2px; }
  .wa-field input, .wa-field textarea { padding: 6px 10px; border: 1px solid var(--border); border-radius: 6px; background: var(--card); font-size: 13px; color: var(--foreground); }
  .wa-input-row { display: flex; gap: 8px; }
  .wa-input-row input { flex: 1; }
  .wa-actions { display: flex; gap: 8px; margin-top: 12px; flex-wrap: wrap; }
  .wa-btn { padding: 6px 14px; border: 1px solid var(--border); border-radius: 6px; background: var(--card); font-size: 13px; cursor: pointer; color: var(--foreground); transition: background 0.15s; }
  .wa-btn:hover { background: var(--muted); }
  .wa-btn-primary { background: var(--primary); color: var(--primary-foreground); border-color: var(--primary); }
  .wa-btn-primary:hover { opacity: 0.9; }
  .wa-btn-sm { padding: 3px 8px; font-size: 11.5px; }
  .wa-btn:disabled { opacity: 0.4; cursor: default; pointer-events: none; }
  .wa-summary { cursor: pointer; font-weight: 600; font-size: 14px; }

  /* §5 消息类型选择器 */
  .wa-type-selector { display: flex; gap: 2px; flex-wrap: wrap; }
  .wa-type-btn {
    padding: 6px 12px; border: 1px solid var(--border); border-radius: 6px;
    background: var(--card); font-size: 13px; cursor: pointer; color: var(--foreground);
    transition: all 0.15s;
  }
  .wa-type-btn:hover { background: var(--muted); }
  .wa-type-btn.active {
    background: var(--primary); color: var(--primary-foreground); border-color: var(--primary);
    font-weight: 600;
  }

  .wa-log-body { flex: 1; min-height: 0; overflow-y: auto; background: #1e1e1e; border-radius: 8px; padding: 10px; font-family: var(--font-mono); font-size: 12px; color: #a6e22e; }
  .wa-log-line { padding: 2px 0; white-space: pre-wrap; word-break: break-all; }
  .wa-log-empty { color: #888; }
</style>
