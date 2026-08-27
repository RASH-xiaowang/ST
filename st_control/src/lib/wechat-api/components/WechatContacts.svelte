<script lang="ts">
  /**
   * 联系人与群模块 — 严格遵循《联系人与群 — 业务逻辑规范》
   *
   * §3 标识归一与三分类规则
   * §4 展示名解析：remark > nickName > userName > id
   * §5 详情补全流水线：N=20, K=3, T=90000ms
   * §6 UI 刷新合并策略（requestAnimationFrame）
   * §7 锁定目标与会话
   * §8 本地通讯录缓存（版本化）
   * §9 单点接口
   * §10 Tab 与客户端过滤
   */
  import { apiPost, assertApiOk, isTokenInvalidPayload } from '../services/api';
  import {
    consoleState, setTargetWxid, saveContactsCache, loadContactsCache, clearContactsCache,
    pickDisplayName, normalizeWxid, isChatroomId, isGhId,
  } from '../stores/console.svelte';
  import type { ContactsListData, DetailContactInfo } from '../types';
  import { onMount, onDestroy } from 'svelte';

  // ═══════════════════════════════════════════════════════════
  // 可配置常量（§D）
  // ═══════════════════════════════════════════════════════════
  const BATCH_SIZE = 20;           // §D 每批最大 id 数 N
  const CONCURRENCY = 3;           // §D 并发工作者数 K
  const BATCH_TIMEOUT_MS = 90000;  // §D 单批超时 T

  // ═══════════════════════════════════════════════════════════
  // 状态
  // ═══════════════════════════════════════════════════════════
  type TabId = 'friends' | 'chatrooms' | 'ghs';

  // §3 归一后三类列表
  let friendsIds = $state<string[]>([]);
  let chatroomIds = $state<string[]>([]);
  let ghIds = $state<string[]>([]);

  // §4 详情映射：userName → DetailContactInfo
  let detailMap = $state<Map<string, DetailContactInfo>>(new Map());

  let activeTab = $state<TabId>('friends');
  let filterText = $state('');
  let logs = $state<string[]>([]);

  // §5 流水线状态
  let isEnriching = $state(false);
  let enrichProgress = $state('');
  let enrichBatchDone = $state(0);
  let enrichBatchTotal = $state(0);
  let isFetching = $state(false);

  // §7 锁定目标（从全局会话读取）
  let lockedWxid = $derived(consoleState.currentTargetWxid);
  let lockedDisplayName = $derived(consoleState.currentTargetDisplayName);

  // 操作表单
  let searchQuery = $state('');
  let searchResult = $state('');
  let addV3 = $state('');
  let addV4 = $state('');
  let addScene = $state('3');
  let addOption = $state('2');
  let addContent = $state('');
  let delWxid = $state('');
  let briefWxid = $state('');
  let detailWxid = $state('');
  let permWxid = $state('');
  let permOnlyChat = $state(true);
  let remarkWxid = $state('');
  let remarkValue = $state('');

  // §5 流水线取消标志
  let pipelineGeneration = $state(0);

  // §6 UI 刷新合并调度
  let rafPending = false;

  // ═══════════════════════════════════════════════════════════
  // 日志（§C.3）
  // ═══════════════════════════════════════════════════════════
  function addLog(msg: string) {
    const time = new Date().toLocaleTimeString('zh-CN', { hour12: false });
    logs = [`[${time}] ${msg}`, ...logs].slice(0, 500);
  }

  // ═══════════════════════════════════════════════════════════
  // §3 标识归一与三分类算法
  // ═══════════════════════════════════════════════════════════
  /**
   * §3 归一算法：
   * 1. 去重（保持顺序）
   * 2. 遍历 friends：群聊归入误分类群，公众号归入误分类号，其余为好友
   * 3. 群聊 = 原始 chatrooms ∪ 误分类群，去重
   * 4. 公众号 = 原始 ghs ∪ 误分类号，去重
   */
  function classifyContacts(raw: ContactsListData): { friends: string[]; chatrooms: string[]; ghs: string[] } {
    // 去重函数（保持顺序）
    function dedup(arr: string[]): string[] {
      const seen = new Set<string>();
      const result: string[] = [];
      for (const id of arr) {
        const n = normalizeWxid(id);
        if (!n) continue;
        const key = n.toLowerCase();
        if (seen.has(key)) continue;
        seen.add(key);
        result.push(n);
      }
      return result;
    }

    const rawFriends = dedup(raw.friends || []);
    const rawChatrooms = dedup(raw.chatrooms || []);
    const rawGhs = dedup(raw.ghs || []);

    // §3.2 遍历 friends，纠正误分类
    const realFriends: string[] = [];
    const misclassifiedChatrooms: string[] = [];
    const misclassifiedGhs: string[] = [];

    for (const id of rawFriends) {
      if (isChatroomId(id)) {
        misclassifiedChatrooms.push(id);
      } else if (isGhId(id)) {
        misclassifiedGhs.push(id);
      } else {
        realFriends.push(id);
      }
    }

    // §3.3 群聊 = 原始 ∪ 误分类，去重
    const allChatrooms = dedup([...rawChatrooms, ...misclassifiedChatrooms]);
    // §3.4 公众号 = 原始 ∪ 误分类，去重
    const allGhs = dedup([...rawGhs, ...misclassifiedGhs]);

    if (misclassifiedChatrooms.length || misclassifiedGhs.length) {
      addLog(`[归一] 纠正误分类: ${misclassifiedChatrooms.length} 个群, ${misclassifiedGhs.length} 个号`);
    }

    return { friends: realFriends, chatrooms: allChatrooms, ghs: allGhs };
  }

  // ═══════════════════════════════════════════════════════════
  // §4 展示名解析
  // ═══════════════════════════════════════════════════════════
  function getDisplayName(id: string): string {
    const n = normalizeWxid(id);
    const row = getDetailRow(n);
    return pickDisplayName(row, n);
  }

  function getDetailRow(id: string): DetailContactInfo | undefined {
    const nl = id.toLowerCase();
    // §4 同时支持原始值与小写键
    for (const [key, val] of detailMap) {
      if (key === id || key.toLowerCase() === nl) return val;
    }
    return undefined;
  }

  // ═══════════════════════════════════════════════════════════
  // §10 Tab 列表与客户端过滤
  // ═══════════════════════════════════════════════════════════
  const currentList = $derived.by(() => {
    // §10 Tab 展示二次过滤：保证分类正确
    let list: string[];
    if (activeTab === 'friends') {
      list = friendsIds.filter(id => !isChatroomId(id) && !isGhId(id));
    } else if (activeTab === 'chatrooms') {
      list = chatroomIds.filter(id => isChatroomId(id));
    } else {
      list = ghIds.filter(id => isGhId(id));
    }

    // §10 客户端过滤：按展示名 + id 子串
    if (filterText.trim()) {
      const q = filterText.toLowerCase();
      list = list.filter(id => {
        const name = getDisplayName(id).toLowerCase();
        return name.includes(q) || id.toLowerCase().includes(q);
      });
    }

    // §7 锁定目标排序置顶
    if (lockedWxid) {
      const lockedLower = lockedWxid.toLowerCase();
      list = [...list].sort((a, b) => {
        const aMatch = a.toLowerCase() === lockedLower ? -1 : 0;
        const bMatch = b.toLowerCase() === lockedLower ? -1 : 0;
        return aMatch - bMatch;
      });
    }

    return list;
  });

  // ═══════════════════════════════════════════════════════════
  // §6 UI 刷新合并策略（requestAnimationFrame）
  // ═══════════════════════════════════════════════════════════
  function scheduleUiRefresh() {
    if (rafPending) return;
    rafPending = true;
    requestAnimationFrame(() => {
      rafPending = false;
      // Svelte 的 $state 已自动触发响应式更新
      // 此处仅用于批量补全时的合并调度语义
    });
  }

  // ═══════════════════════════════════════════════════════════
  // §8 缓存恢复（模块装载 §A.1）
  // ═══════════════════════════════════════════════════════════
  function restoreFromCache() {
    const cache = loadContactsCache();
    if (!cache) return false;

    friendsIds = cache.friendsIds || [];
    chatroomIds = cache.chatroomIds || [];
    ghIds = cache.ghIds || [];

    // 重建详情映射
    const map = new Map<string, DetailContactInfo>();
    for (const row of (cache.details || [])) {
      if (!row) continue;
      const key = String(row.userName || '');
      if (key) map.set(key, row);
    }
    detailMap = map;

    // §7 同步锁定目标展示名
    if (lockedWxid && detailMap.size) {
      const name = getDisplayName(lockedWxid);
      if (name && name !== lockedWxid) {
        setTargetWxid(lockedWxid, name);
      }
    }

    const total = friendsIds.length + chatroomIds.length + ghIds.length;
    if (total > 0) {
      addLog(`✅ 已从本地缓存恢复 ${total} 条通讯录`);
    }
    return total > 0;
  }

  // ═══════════════════════════════════════════════════════════
  // §5 详情补全流水线
  // ═══════════════════════════════════════════════════════════
  /**
   * §5 详情补全流水线：
   * - 将全量 id 按 N=20 切片
   * - §5.6 若存在锁定目标 id，包含该 id 的批次优先
   * - §5.5 并发 K=3 工作者
   * - §5.7 单批超时 T=90000ms
   * - §5.8 每批 finally 触发合并 UI 刷新
   * - §5.9 全部完成后再写缓存
   */
  async function enrichDetails(allIds: string[], generation: number) {
    if (!allIds.length) return;

    const batches: string[][] = [];
    for (let i = 0; i < allIds.length; i += BATCH_SIZE) {
      batches.push(allIds.slice(i, i + BATCH_SIZE));
    }

    // §5.6 包含锁定目标的批次优先
    if (lockedWxid) {
      const lockedLower = lockedWxid.toLowerCase();
      batches.sort((a, b) => {
        const aHas = a.some(id => id.toLowerCase() === lockedLower) ? 0 : 1;
        const bHas = b.some(id => id.toLowerCase() === lockedLower) ? 0 : 1;
        return aHas - bHas;
      });
    }

    enrichBatchTotal = batches.length;
    enrichBatchDone = 0;
    isEnriching = true;
    enrichProgress = `补全详情: 0/${batches.length} 批`;

    // §5.5 并发 K 个工作者
    let batchIndex = 0;
    const workers: Promise<void>[] = [];

    async function worker() {
      while (batchIndex < batches.length) {
        // §C.2 模块卸载时中止
        if (generation !== pipelineGeneration) return;

        const idx = batchIndex++;
        const batch = batches[idx];

        try {
          // §5.7 单批超时竞态
          const timeoutPromise = new Promise<never>((_, reject) =>
            setTimeout(() => reject(new Error('批次超时')), BATCH_TIMEOUT_MS)
          );

          const fetchPromise = apiPost<DetailContactInfo[]>(
            '/contacts/getDetailInfo',
            { wxids: batch },
            consoleState,
          );

          const res = await Promise.race([fetchPromise, timeoutPromise]);

          // §4.5 令牌失效检测
          if (isTokenInvalidPayload((res as { data: unknown }).data)) {
            consoleState.tokenStatus = 'invalid';
            addLog('❌ TOKEN 已失效');
            return;
          }

          const details = assertApiOk(res as Parameters<typeof assertApiOk>[0]);

          // §5.7 业务成功且 data 为数组 → 合并进详情映射
          if (Array.isArray(details)) {
            for (const row of details) {
              if (!row) continue;
              const key = String(row.userName || '');
              if (key) detailMap.set(key, row);
            }
          }

        } catch (e) {
          // §5.7 超时或失败 → 记录日志，跳过该批
          const msg = (e as Error).message || '未知错误';
          addLog(`⚠️ 批次 ${idx + 1} 跳过: ${msg} (wxids: ${batch.slice(0, 3).join(',')}...)`);
        } finally {
          // §5.8 递增已完成批次，触发合并 UI 刷新
          enrichBatchDone++;
          enrichProgress = `补全详情: ${enrichBatchDone}/${batches.length} 批`;
          scheduleUiRefresh();
        }
      }
    }

    // 启动 K 个工作者
    for (let i = 0; i < Math.min(CONCURRENCY, batches.length); i++) {
      workers.push(worker());
    }

    await Promise.all(workers);

    // §5.9 流水线结束
    if (generation === pipelineGeneration) {
      isEnriching = false;
      enrichProgress = `补全完成: ${detailMap.size} 条详情`;

      // §5.9 同步锁定目标展示名
      if (lockedWxid) {
        const name = getDisplayName(lockedWxid);
        if (name && name !== lockedWxid) {
          setTargetWxid(lockedWxid, name);
        }
      }

      // §5.9 写入本地通讯录缓存
      saveContactsCache(friendsIds, chatroomIds, ghIds, Array.from(detailMap.values()));
      addLog(`✅ 详情补全完成，共 ${detailMap.size} 条，已写入缓存`);
    }
  }

  // ═══════════════════════════════════════════════════════════
  // §9 拉取通讯录（§A.2）
  // ═══════════════════════════════════════════════════════════
  async function fetchContacts() {
    if (isFetching || isEnriching) return;
    isFetching = true;

    try {
      addLog('正在拉取通讯录...');
      const res = await apiPost<ContactsListData>('/contacts/fetchContactsList', {}, consoleState);

      // 令牌失效检测
      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        addLog('❌ TOKEN 已失效');
        return;
      }

      const data = assertApiOk(res);

      if (!data) {
        addLog('❌ 拉取失败: 返回数据为空');
        return;
      }

      // §3 执行归一
      const classified = classifyContacts(data);
      friendsIds = classified.friends;
      chatroomIds = classified.chatrooms;
      ghIds = classified.ghs;

      const total = friendsIds.length + chatroomIds.length + ghIds.length;
      addLog(`✅ 通讯录: 好友${friendsIds.length} 群聊${chatroomIds.length} 公众号${ghIds.length}`);

      // §A.2 空列表：清除缓存
      if (total === 0) {
        clearContactsCache();
        detailMap = new Map();
        addLog('通讯录为空，已清除缓存');
        return;
      }

      // §5 立即用空映射渲染（避免白屏），然后写入初始缓存
      saveContactsCache(friendsIds, chatroomIds, ghIds, []);

      // §5 进入详情补全流水线
      const allIds = [...new Set([...friendsIds, ...chatroomIds, ...ghIds])];
      pipelineGeneration++;
      await enrichDetails(allIds, pipelineGeneration);

    } catch (e) {
      const msg = (e as Error).message || '未知错误';
      addLog(`❌ 拉取失败: ${msg}`);
    } finally {
      isFetching = false;
    }
  }

  // ═══════════════════════════════════════════════════════════
  // §7 锁定目标
  // ═══════════════════════════════════════════════════════════
  function selectTarget(id: string) {
    const name = getDisplayName(id);
    setTargetWxid(id, name || id);
    addLog(`已锁定目标: ${name || id}`);
  }

  // ═══════════════════════════════════════════════════════════
  // §9 单点接口
  // ═══════════════════════════════════════════════════════════
  async function searchContacts() {
    const q = searchQuery.trim();
    if (!q) { addLog('⚠️ 搜索内容不能为空'); return; }
    try {
      const res = await apiPost('/contacts/search', { contactsInfo: q }, consoleState);
      if (isTokenInvalidPayload(res.data)) { consoleState.tokenStatus = 'invalid'; addLog('❌ TOKEN 已失效'); return; }
      const data = assertApiOk(res);
      searchResult = JSON.stringify(data, null, 2);
      addLog('✅ 搜索完成');
    } catch (e) { addLog(`❌ ${(e as Error).message}`); }
  }

  async function addContact() {
    if (!addV3.trim() || !addV4.trim()) { addLog('⚠️ v3 和 v4 必填'); return; }
    try {
      await apiPost('/contacts/addContacts', {
        v3: addV3, v4: addV4, scene: Number(addScene), option: Number(addOption), content: addContent,
      }, consoleState);
      addLog('✅ 添加/同意好友请求已发送');
    } catch (e) { addLog(`❌ ${(e as Error).message}`); }
  }

  // §9 删除好友须二次确认
  async function deleteFriend() {
    const target = delWxid.trim() || lockedWxid;
    if (!target) { addLog('⚠️ 请填写 wxid 或锁定目标'); return; }
    const name = getDisplayName(target);
    const confirmed = confirm(`确定删除好友？\n\n目标: ${name}\nwxid: ${target}`);
    if (!confirmed) return;

    try {
      await apiPost('/contacts/deleteFriend', { wxid: target }, consoleState);
      addLog(`✅ 删除好友请求已发送: ${name}`);
    } catch (e) { addLog(`❌ ${(e as Error).message}`); }
  }

  async function getBriefInfo() {
    const target = briefWxid.trim() || lockedWxid;
    if (!target) { addLog('⚠️ 请填写 wxid 或锁定目标'); return; }
    try {
      const res = await apiPost('/contacts/getBriefInfo', { wxids: [target] }, consoleState);
      if (isTokenInvalidPayload(res.data)) { consoleState.tokenStatus = 'invalid'; addLog('❌ TOKEN 已失效'); return; }
      const data = assertApiOk(res);
      addLog(`✅ 简要信息: ${JSON.stringify(data).slice(0, 300)}`);
    } catch (e) { addLog(`❌ ${(e as Error).message}`); }
  }

  async function getDetailInfo() {
    const target = detailWxid.trim() || lockedWxid;
    if (!target) { addLog('⚠️ 请填写 wxid 或锁定目标'); return; }
    try {
      const res = await apiPost<DetailContactInfo[]>('/contacts/getDetailInfo', { wxids: [target] }, consoleState);
      if (isTokenInvalidPayload(res.data)) { consoleState.tokenStatus = 'invalid'; addLog('❌ TOKEN 已失效'); return; }
      const data = assertApiOk(res);
      addLog(`✅ 详细信息: ${JSON.stringify(data).slice(0, 300)}`);
    } catch (e) { addLog(`❌ ${(e as Error).message}`); }
  }

  async function setPermissions() {
    const target = permWxid.trim() || lockedWxid;
    if (!target) { addLog('⚠️ 请填写 wxid 或锁定目标'); return; }
    try {
      await apiPost('/contacts/setFriendPermissions', { wxid: target, onlyChat: permOnlyChat }, consoleState);
      addLog(`✅ 权限已设置: ${getDisplayName(target)} onlyChat=${permOnlyChat}`);
    } catch (e) { addLog(`❌ ${(e as Error).message}`); }
  }

  async function setRemark() {
    const target = remarkWxid.trim() || lockedWxid;
    if (!target) { addLog('⚠️ 请填写 wxid 或锁定目标'); return; }
    if (!remarkValue.trim()) { addLog('⚠️ 备注不能为空'); return; }
    try {
      await apiPost('/contacts/setFriendRemark', { wxid: target, remark: remarkValue }, consoleState);
      addLog(`✅ 备注已设置: ${remarkValue}`);
    } catch (e) { addLog(`❌ ${(e as Error).message}`); }
  }

  function clearLogs() { logs = []; }

  // ═══════════════════════════════════════════════════════════
  // 生命周期
  // ═══════════════════════════════════════════════════════════
  onMount(() => {
    // §A.1 模块装载：尝试恢复缓存
    restoreFromCache();
  });

  // §C.2 模块卸载时中止流水线
  onDestroy(() => {
    pipelineGeneration++;
  });
</script>

<div class="wa-mod">
  <div class="wa-mod-split">
    <!-- ═══ 左侧：操作区 ═══ -->
    <div class="wa-mod-left">
      <div class="wa-card">
        <h3 class="wa-card-title">拉取通讯录</h3>
        <p class="wa-hint">调用 /contacts/fetchContactsList，自动执行归一与详情补全流水线。</p>
        {#if enrichProgress}
          <p class="wa-progress">{enrichProgress} {#if enrichBatchTotal > 0}({enrichBatchDone}/{enrichBatchTotal}){/if}</p>
        {/if}
        <div class="wa-actions">
          <button class="wa-btn wa-btn-primary" onclick={fetchContacts} disabled={isFetching || isEnriching}>
            {isFetching ? '拉取中...' : isEnriching ? '补全中...' : '拉取通讯录'}
          </button>
        </div>
      </div>

      <div class="wa-card">
        <h3 class="wa-card-title">搜索好友</h3>
        <div class="wa-form-grid">
          <label class="wa-field">
            <span class="wa-label">contactsInfo</span>
            <div class="wa-input-row">
              <input type="text" bind:value={searchQuery} placeholder="微信号、手机号等" />
              <button class="wa-btn wa-btn-primary" onclick={searchContacts}>搜索</button>
            </div>
          </label>
        </div>
        {#if searchResult}
          <pre class="wa-inline-result">{searchResult}</pre>
        {/if}
      </div>

      <div class="wa-card">
        <h3 class="wa-card-title">添加/同意好友</h3>
        <div class="wa-form-grid">
          <label class="wa-field"><span class="wa-label">v3 *</span><input type="text" bind:value={addV3} /></label>
          <label class="wa-field"><span class="wa-label">v4 *</span><input type="text" bind:value={addV4} /></label>
          <div class="wa-row-2">
            <label class="wa-field"><span class="wa-label">scene</span><input type="number" bind:value={addScene} /></label>
            <label class="wa-field"><span class="wa-label">option</span><input type="number" bind:value={addOption} /></label>
          </div>
          <label class="wa-field"><span class="wa-label">content</span><input type="text" bind:value={addContent} /></label>
        </div>
        <div class="wa-actions"><button class="wa-btn wa-btn-primary" onclick={addContact}>调用接口</button></div>
      </div>

      <div class="wa-card">
        <h3 class="wa-card-title">删除好友</h3>
        <p class="wa-hint">须二次确认</p>
        <div class="wa-form-grid">
          <label class="wa-field">
            <span class="wa-label">wxid</span>
            <input type="text" bind:value={delWxid} placeholder={lockedWxid || '输入或从列表锁定'} />
          </label>
        </div>
        <div class="wa-actions"><button class="wa-btn" onclick={deleteFriend}>删除好友</button></div>
      </div>

      <div class="wa-card">
        <h3 class="wa-card-title">获取信息</h3>
        <div class="wa-form-grid">
          <label class="wa-field">
            <span class="wa-label">wxid</span>
            <input type="text" bind:value={briefWxid} placeholder={lockedWxid || '输入或从列表锁定'} />
          </label>
        </div>
        <div class="wa-actions">
          <button class="wa-btn" onclick={getBriefInfo}>简要信息</button>
          <button class="wa-btn" onclick={getDetailInfo}>详细信息</button>
        </div>
      </div>

      <div class="wa-card">
        <h3 class="wa-card-title">权限与备注</h3>
        <div class="wa-form-grid">
          <label class="wa-field"><span class="wa-label">wxid</span><input type="text" bind:value={permWxid} placeholder={lockedWxid} /></label>
          <label class="wa-check"><input type="checkbox" bind:checked={permOnlyChat} /> onlyChat</label>
          <div class="wa-actions"><button class="wa-btn" onclick={setPermissions}>设置权限</button></div>
          <label class="wa-field"><span class="wa-label">remark</span><input type="text" bind:value={remarkValue} /></label>
          <div class="wa-actions"><button class="wa-btn" onclick={setRemark}>设置备注</button></div>
        </div>
      </div>
    </div>

    <!-- ═══ 右侧：列表 + 日志 ═══ -->
    <div class="wa-mod-right">
      <div class="wa-card wa-card-fill">
        <div class="wa-card-head">
          <h3 class="wa-card-title">通讯录数据</h3>
        </div>

        <!-- §7 锁定目标横幅 -->
        {#if lockedWxid}
          <div class="wa-target-banner">
            <span class="wa-target-label">当前锁定:</span>
            <span class="wa-target-name">{lockedDisplayName || lockedWxid}</span>
            <span class="wa-target-id">{lockedWxid}</span>
          </div>
        {/if}

        <!-- §10 三 Tab -->
        <div class="wa-tabs">
          <button class="wa-tab" class:active={activeTab === 'friends'} onclick={() => { activeTab = 'friends'; filterText = ''; }}>
            好友 ({friendsIds.length})
          </button>
          <button class="wa-tab" class:active={activeTab === 'chatrooms'} onclick={() => { activeTab = 'chatrooms'; filterText = ''; }}>
            群聊 ({chatroomIds.length})
          </button>
          <button class="wa-tab" class:active={activeTab === 'ghs'} onclick={() => { activeTab = 'ghs'; filterText = ''; }}>
            公众号 ({ghIds.length})
          </button>
        </div>

        <!-- §10 搜索框 -->
        <input type="text" class="wa-filter-input" bind:value={filterText} placeholder="按展示名或 id 搜索..." />

        <!-- 列表 -->
        <div class="wa-contact-list">
          {#each currentList as id (id)}
            {@const name = getDisplayName(id)}
            {@const isLocked = id.toLowerCase() === lockedWxid?.toLowerCase()}
            <button class="wa-contact-item" class:locked={isLocked} onclick={() => selectTarget(id)} title={id}>
              <div class="wa-contact-info">
                <span class="wa-contact-name">{name}</span>
                {#if name !== id}
                  <span class="wa-contact-id">{id}</span>
                {/if}
              </div>
              {#if isLocked}
                <span class="wa-lock-badge">🔒</span>
              {/if}
            </button>
          {:else}
            <p class="wa-empty-hint">
              {#if friendsIds.length + chatroomIds.length + ghIds.length === 0}
                请先拉取通讯录（或等待缓存恢复）
              {:else}
                无匹配结果
              {/if}
            </p>
          {/each}
        </div>
      </div>

      <!-- 全链路日志 -->
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
  .wa-progress { font-size: 12px; color: var(--primary); margin: 0 0 8px; font-family: var(--font-mono); }
  .wa-form-grid { display: flex; flex-direction: column; gap: 10px; }
  .wa-field { display: flex; flex-direction: column; gap: 4px; }
  .wa-label { font-size: 12px; font-weight: 600; }
  .wa-field input { padding: 6px 10px; border: 1px solid var(--border); border-radius: 6px; background: var(--card); font-size: 13px; color: var(--foreground); }
  .wa-input-row { display: flex; gap: 8px; }
  .wa-input-row input { flex: 1; }
  .wa-row-2 { display: flex; gap: 10px; }
  .wa-row-2 .wa-field { flex: 1; }
  .wa-check { display: flex; align-items: center; gap: 6px; font-size: 13px; cursor: pointer; }
  .wa-actions { display: flex; gap: 8px; margin-top: 12px; flex-wrap: wrap; }
  .wa-btn { padding: 6px 14px; border: 1px solid var(--border); border-radius: 6px; background: var(--card); font-size: 13px; cursor: pointer; color: var(--foreground); transition: background 0.15s; }
  .wa-btn:hover { background: var(--muted); }
  .wa-btn-primary { background: var(--primary); color: var(--primary-foreground); border-color: var(--primary); }
  .wa-btn-primary:hover { opacity: 0.9; }
  .wa-btn-sm { padding: 3px 8px; font-size: 11.5px; }
  .wa-btn:disabled { opacity: 0.4; cursor: default; pointer-events: none; }

  /* §7 锁定目标横幅 */
  .wa-target-banner {
    display: flex; align-items: center; gap: 8px; padding: 8px 12px; margin-bottom: 10px;
    background: color-mix(in srgb, var(--primary) 8%, transparent); border: 1px solid color-mix(in srgb, var(--primary) 20%, transparent);
    border-radius: 8px; font-size: 12px;
  }
  .wa-target-label { font-weight: 600; color: var(--primary); }
  .wa-target-name { font-weight: 600; }
  .wa-target-id { font-family: var(--font-mono); color: var(--muted-foreground); }

  /* §10 Tab */
  .wa-tabs { display: flex; gap: 2px; margin-bottom: 10px; border-bottom: 1px solid var(--border); }
  .wa-tab { padding: 6px 12px; border: none; background: none; font-size: 13px; cursor: pointer; color: var(--muted-foreground); border-bottom: 2px solid transparent; transition: all 0.15s; }
  .wa-tab.active { color: var(--primary); border-bottom-color: var(--primary); font-weight: 600; }

  /* §10 搜索框 */
  .wa-filter-input { width: 100%; padding: 6px 10px; border: 1px solid var(--border); border-radius: 6px; font-size: 13px; margin-bottom: 8px; background: var(--card); color: var(--foreground); box-sizing: border-box; }

  /* 列表 */
  .wa-contact-list { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 2px; }
  .wa-contact-item {
    display: flex; justify-content: space-between; align-items: center; padding: 8px 10px;
    border: 1px solid transparent; border-radius: 6px; background: none; cursor: pointer;
    font-size: 13px; color: var(--foreground); text-align: left; transition: all 0.12s;
  }
  .wa-contact-item:hover { background: var(--muted); }
  /* §7 锁定目标高亮 */
  .wa-contact-item.locked {
    background: color-mix(in srgb, var(--primary) 8%, transparent);
    border-color: color-mix(in srgb, var(--primary) 20%, transparent);
  }
  .wa-contact-info { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .wa-contact-name { font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .wa-contact-id { font-size: 11.5px; color: var(--muted-foreground); font-family: var(--font-mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .wa-lock-badge { font-size: 14px; flex-shrink: 0; }
  .wa-empty-hint { color: var(--muted-foreground); font-size: 13px; text-align: center; padding: 24px 0; }

  .wa-inline-result { background: #1e1e1e; color: #a6e22e; padding: 10px; border-radius: 8px; font-size: 12px; font-family: var(--font-mono); overflow-x: auto; max-height: 200px; margin-top: 8px; }

  .wa-log-body { flex: 1; min-height: 0; overflow-y: auto; background: #1e1e1e; border-radius: 8px; padding: 10px; font-family: var(--font-mono); font-size: 12px; color: #a6e22e; }
  .wa-log-line { padding: 2px 0; white-space: pre-wrap; word-break: break-all; }
  .wa-log-empty { color: #888; }
</style>
