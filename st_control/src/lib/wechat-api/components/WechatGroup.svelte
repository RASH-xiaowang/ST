<script lang="ts">
  /**
   * 群管理模块 — 严格遵循《群管理 — 业务逻辑规范》
   *
   * 线 A：创建群聊并落库群 ID
   * 线 B：扫码进群 / 同意邀请链接进群
   * 线 C：成员与资料维护
   * 线 D：群公告
   * 线 E：群二维码与通讯录标记
   * 线 F：管理员与会话属性
   * 线 G：进群申请审批
   *
   * 核心约束：
   * - 每次 POST 之间强制随机休眠 2～5 秒
   * - 结构化日志：[ISO8601] [动作] [path] [ret] [摘要]
   * - 令牌脱敏
   * - 二次确认危险操作
   */
  import { apiPost, isTokenInvalidPayload } from '../services/api';
  import { consoleState } from '../stores/console.svelte';

  // ═══════════════════════════════════════════════════════════
  // §D 可配置常量
  // ═══════════════════════════════════════════════════════════
  const DELAY_MIN_MS = 2000;
  const DELAY_MAX_MS = 5000;
  const QR_BACKOFF_BASE_MS = 2000;
  const QR_BACKOFF_MAX_RETRIES = 5;

  // ═══════════════════════════════════════════════════════════
  // 状态
  // ═══════════════════════════════════════════════════════════
  type TabId = 'create' | 'join' | 'member' | 'announcement' | 'qr' | 'admin' | 'approval';

  let activeTab = $state<TabId>('member');
  let logs = $state<string[]>([]);
  let isBusy = $state(false);

  // 群 ID（核心上下文）
  let chatroomId = $state('');

  // 线 A
  let createWxids = $state('');
  let createdRoomId = $state('');

  // 线 B
  let qrUrl = $state('');
  let agreeUrl = $state('');

  // 线 C
  let memberList = $state<Array<{ wxid: string; nickName: string; displayName: string }>>([]);
  let memberDetailWxids = $state('');
  let inviteWxids = $state('');
  let inviteReason = $state('');
  let removeWxids = $state('');
  let roomName = $state('');
  let roomRemark = $state('');
  let selfNick = $state('');
  let addMemberWxid = $state('');
  let addMemberContent = $state('');

  // 线 D
  let announcement = $state('');
  let announcementEditor = $state('');
  let announcementTime = $state('');
  let newAnnouncement = $state('');

  // 线 E
  let qrBase64 = $state('');
  let qrTips = $state('');
  let contractOperType = $state('3');

  // 线 F
  let adminOperType = $state('1');
  let adminWxids = $state('');
  let pinTop = $state(true);
  let silenceMode = $state(false);

  // 线 G
  let approveMsgId = $state('');
  let approveMsgContent = $state('');

  // ═══════════════════════════════════════════════════════════
  // 随机延迟（§C.3 核心约束）
  // ═══════════════════════════════════════════════════════════
  function randomDelay(): Promise<void> {
    const ms = DELAY_MIN_MS + Math.random() * (DELAY_MAX_MS - DELAY_MIN_MS);
    return new Promise(r => setTimeout(r, Math.round(ms)));
  }

  // ═══════════════════════════════════════════════════════════
  // 结构化日志（§C.4）
  // ═══════════════════════════════════════════════════════════
  function addLog(action: string, path: string, ret: number | string, summary: string) {
    const time = new Date().toISOString();
    const line = `[${time}] [${action}] [${path}] [ret=${ret}] [${summary}]`;
    logs = [line, ...logs].slice(0, 500);
  }

  /** 统一请求封装：含日志 + 随机延迟 */
  async function groupPost(action: string, path: string, body: Record<string, unknown>): Promise<{ ok: boolean; data: Record<string, unknown> | null }> {
    try {
      const res = await apiPost(path, body, consoleState);

      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        addLog(action, path, 'ERR', 'TOKEN 已失效');
        return { ok: false, data: null };
      }

      const ret = res.data?.ret;
      const msg = res.data?.msg || '';
      const data = res.data?.data as Record<string, unknown> | undefined;

      if (ret === 200) {
        addLog(action, path, ret, '成功');
      } else {
        addLog(action, path, ret, msg || '业务异常');
      }

      // §C.3 强制随机延迟
      await randomDelay();

      return { ok: ret === 200, data: data || null };
    } catch (e) {
      const msg = (e as Error).message || '未知错误';
      addLog(action, path, 'ERR', msg);
      await randomDelay();
      return { ok: false, data: null };
    }
  }

  // ═══════════════════════════════════════════════════════════
  // wxIds 解析工具
  // ═══════════════════════════════════════════════════════════
  function parseWxIds(raw: string): string[] {
    return String(raw || '').split(/[,，\s]+/).map(s => s.trim()).filter(Boolean);
  }

  function parseWxIdsComma(raw: string): string {
    return parseWxIds(raw).join(',');
  }

  // ═══════════════════════════════════════════════════════════
  // 线 A：创建群聊并落库群 ID
  // ═══════════════════════════════════════════════════════════
  async function createChatroom() {
    const wxids = parseWxIds(createWxids);
    if (wxids.length < 2) {
      addLog('创建群聊', '/group/createChatroom', 'WARN', '至少需要 2 个好友 wxid');
      return;
    }

    isBusy = true;
    const result = await groupPost('创建群聊', '/group/createChatroom', { wxids });
    if (result.ok && result.data) {
      createdRoomId = String(result.data.chatroomId || '');
      chatroomId = createdRoomId;
      addLog('创建群聊', '/group/createChatroom', 200, `群ID: ${createdRoomId}`);
    }
    isBusy = false;
  }

  async function getChatroomInfo() {
    if (!chatroomId) { addLog('拉取群信息', '/group/getChatroomInfo', 'WARN', 'chatroomId 为空'); return; }
    isBusy = true;
    const result = await groupPost('拉取群信息', '/group/getChatroomInfo', { chatroomId });
    if (result.ok && result.data) {
      const info = result.data;
      roomName = String(info.chatroomName || '');
      addLog('拉取群信息', '/group/getChatroomInfo', 200, `群名: ${roomName}`);
    }
    isBusy = false;
  }

  // ═══════════════════════════════════════════════════════════
  // 线 B：扫码进群 / 同意邀请
  // ═══════════════════════════════════════════════════════════
  async function joinByQr() {
    if (!qrUrl.trim()) { addLog('扫码进群', '/group/joinRoomUsingQRCode', 'WARN', 'qrUrl 为空'); return; }
    isBusy = true;
    const result = await groupPost('扫码进群', '/group/joinRoomUsingQRCode', { qrUrl: qrUrl.trim() });
    if (result.ok && result.data) {
      chatroomId = String(result.data.chatroomId || chatroomId);
      addLog('扫码进群', '/group/joinRoomUsingQRCode', 200, `群ID: ${chatroomId}`);
    }
    isBusy = false;
  }

  async function agreeJoin() {
    if (!agreeUrl.trim()) { addLog('同意邀请', '/group/agreeJoinRoom', 'WARN', 'url 为空'); return; }
    isBusy = true;
    const result = await groupPost('同意邀请', '/group/agreeJoinRoom', { url: agreeUrl.trim() });
    if (result.ok && result.data) {
      chatroomId = String(result.data.chatroomId || chatroomId);
    }
    isBusy = false;
  }

  // ═══════════════════════════════════════════════════════════
  // 线 C：成员与资料维护
  // ═══════════════════════════════════════════════════════════
  async function getMemberList() {
    if (!chatroomId) { addLog('获取成员', '/group/getChatroomMemberList', 'WARN', 'chatroomId 为空'); return; }
    isBusy = true;
    const result = await groupPost('获取成员', '/group/getChatroomMemberList', { chatroomId });
    if (result.ok && result.data) {
      const list = result.data.memberList;
      if (Array.isArray(list)) {
        memberList = list.map(m => ({
          wxid: String((m as Record<string, unknown>).wxid || (m as Record<string, unknown>).userName || ''),
          nickName: String((m as Record<string, unknown>).nickName || ''),
          displayName: String((m as Record<string, unknown>).displayName || (m as Record<string, unknown>).nickName || ''),
        }));
        addLog('获取成员', '/group/getChatroomMemberList', 200, `共 ${memberList.length} 人`);
      }
    }
    isBusy = false;
  }

  async function getMemberDetail() {
    const wxids = parseWxIds(memberDetailWxids);
    if (!chatroomId || !wxids.length) { addLog('成员详情', '/group/getChatroomMemberDetail', 'WARN', '参数不完整'); return; }
    isBusy = true;
    await groupPost('成员详情', '/group/getChatroomMemberDetail', { chatroomId, memberWxids: wxids });
    isBusy = false;
  }

  async function inviteMember() {
    const wxids = parseWxIdsComma(inviteWxids);
    if (!chatroomId || !wxids) { addLog('邀请成员', '/group/inviteMember', 'WARN', '参数不完整'); return; }
    isBusy = true;
    await groupPost('邀请成员', '/group/inviteMember', { chatroomId, wxids, reason: inviteReason || '' });
    isBusy = false;
  }

  async function removeMember() {
    const wxids = parseWxIdsComma(removeWxids);
    if (!chatroomId || !wxids) { addLog('踢人', '/group/removeMember', 'WARN', '参数不完整'); return; }
    if (!confirm(`确定将 ${wxids} 移出群聊？`)) return;
    isBusy = true;
    await groupPost('踢人', '/group/removeMember', { chatroomId, wxids });
    isBusy = false;
  }

  async function modifyName() {
    if (!chatroomId || !roomName.trim()) { addLog('修改群名', '/group/modifyChatroomName', 'WARN', '参数不完整'); return; }
    isBusy = true;
    await groupPost('修改群名', '/group/modifyChatroomName', { chatroomId, chatroomName: roomName.trim() });
    addLog('修改群名', '—', 'INFO', '手机端可能缓存未刷新，请等待或重启微信');
    isBusy = false;
  }

  async function modifyRemark() {
    if (!chatroomId || !roomRemark.trim()) { addLog('修改群备注', '/group/modifyChatroomRemark', 'WARN', '参数不完整'); return; }
    isBusy = true;
    await groupPost('修改群备注', '/group/modifyChatroomRemark', { chatroomId, chatroomRemark: roomRemark.trim() });
    isBusy = false;
  }

  async function modifySelfNick() {
    if (!chatroomId || !selfNick.trim()) { addLog('修改群昵称', '/group/modifyChatroomNickNameForSelf', 'WARN', '参数不完整'); return; }
    isBusy = true;
    await groupPost('修改群昵称', '/group/modifyChatroomNickNameForSelf', { chatroomId, nickName: selfNick.trim() });
    isBusy = false;
  }

  async function addGroupMemberAsFriend() {
    if (!chatroomId || !addMemberWxid.trim() || !addMemberContent.trim()) {
      addLog('加群友', '/group/addGroupMemberAsFriend', 'WARN', '参数不完整（wxid 和 content 必填）');
      return;
    }
    isBusy = true;
    const result = await groupPost('加群友', '/group/addGroupMemberAsFriend', {
      chatroomId, memberWxid: addMemberWxid.trim(), content: addMemberContent.trim(),
    });
    if (result.ok && result.data) {
      addLog('加群友', '/group/addGroupMemberAsFriend', 200, `v3: ${String(result.data.v3 || '').slice(0, 20)}...`);
    }
    isBusy = false;
  }

  async function quitChatroom() {
    if (!chatroomId) return;
    if (!confirm(`确定退出群聊 ${chatroomId}？`)) return;
    isBusy = true;
    const result = await groupPost('退出群聊', '/group/quitChatroom', { chatroomId });
    if (result.ok) {
      addLog('退出群聊', '/group/quitChatroom', 200, '已退出');
      chatroomId = '';
      memberList = [];
    }
    isBusy = false;
  }

  // ═══════════════════════════════════════════════════════════
  // 线 D：群公告
  // ═══════════════════════════════════════════════════════════
  async function getAnnouncement() {
    if (!chatroomId) { addLog('读取公告', '/group/getChatroomAnnouncement', 'WARN', 'chatroomId 为空'); return; }
    isBusy = true;
    const result = await groupPost('读取公告', '/group/getChatroomAnnouncement', { chatroomId });
    if (result.ok && result.data) {
      announcement = String(result.data.announcement || '');
      announcementEditor = String(result.data.announcementEditor || '');
      announcementTime = String(result.data.publishTime || '');
    }
    isBusy = false;
  }

  async function setAnnouncement() {
    if (!chatroomId || !newAnnouncement.trim()) { addLog('发布公告', '/group/setChatroomAnnouncement', 'WARN', '参数不完整'); return; }
    isBusy = true;
    await groupPost('发布公告', '/group/setChatroomAnnouncement', { chatroomId, content: newAnnouncement.trim() });
    isBusy = false;
  }

  // ═══════════════════════════════════════════════════════════
  // 线 E：群二维码与通讯录标记
  // ═══════════════════════════════════════════════════════════
  let qrRetryCount = $state(0);

  async function getQrCode() {
    if (!chatroomId) { addLog('获取群二维码', '/group/getChatroomQrCode', 'WARN', 'chatroomId 为空'); return; }

    isBusy = true;
    // §C.3 指数退避 + 随机抖动
    for (let attempt = 0; attempt <= QR_BACKOFF_MAX_RETRIES; attempt++) {
      const result = await groupPost('获取群二维码', '/group/getChatroomQrCode', { chatroomId });

      if (result.ok && result.data) {
        qrBase64 = String(result.data.qrBase64 || result.data.qrCode || '');
        qrTips = String(result.data.qrTips || '');
        if (!qrBase64.startsWith('data:')) {
          qrBase64 = 'data:image/jpeg;base64,' + qrBase64;
        }
        qrRetryCount = 0;
        isBusy = false;
        return;
      }

      // 失败退避
      qrRetryCount = attempt + 1;
      if (attempt < QR_BACKOFF_MAX_RETRIES) {
        const backoff = QR_BACKOFF_BASE_MS * Math.pow(2, attempt) + Math.random() * 1000;
        addLog('获取群二维码', '/group/getChatroomQrCode', 'BACKOFF', `第${attempt + 1}次失败，退避 ${Math.round(backoff)}ms`);
        await new Promise(r => setTimeout(r, backoff));
      }
    }

    addLog('获取群二维码', '/group/getChatroomQrCode', 'FAIL', `已重试 ${QR_BACKOFF_MAX_RETRIES} 次，放弃`);
    isBusy = false;
  }

  async function saveContract() {
    if (!chatroomId) return;
    isBusy = true;
    await groupPost('通讯录标记', '/group/saveContractList', {
      chatroomId, operType: parseInt(contractOperType, 10),
    });
    isBusy = false;
  }

  // ═══════════════════════════════════════════════════════════
  // 线 F：管理员与会话属性
  // ═══════════════════════════════════════════════════════════
  async function doAdminOperate() {
    const wxids = parseWxIds(adminWxids);
    const oper = parseInt(adminOperType, 10);
    if (!chatroomId || !wxids.length) { addLog('管理员操作', '/group/adminOperate', 'WARN', '参数不完整'); return; }
    // §C 前置拦截：转让仅允许 1 个 wxid
    if (oper === 3 && wxids.length !== 1) {
      addLog('管理员操作', '/group/adminOperate', 'WARN', '转让群主仅允许 1 个 wxid');
      return;
    }
    isBusy = true;
    await groupPost('管理员操作', '/group/adminOperate', { chatroomId, operType: oper, wxids });
    isBusy = false;
  }

  async function doPinChat() {
    if (!chatroomId) return;
    isBusy = true;
    await groupPost('聊天置顶', '/group/pinChat', { chatroomId, top: pinTop });
    isBusy = false;
  }

  async function doSetSilence() {
    if (!chatroomId) return;
    isBusy = true;
    await groupPost('消息免打扰', '/group/setMsgSilence', { chatroomId, silence: silenceMode });
    isBusy = false;
  }

  // ═══════════════════════════════════════════════════════════
  // 线 G：进群申请审批
  // ═══════════════════════════════════════════════════════════
  async function approveAccess() {
    if (!chatroomId || !approveMsgId.trim() || !approveMsgContent.trim()) {
      addLog('审批进群', '/group/roomAccessApplyCheckApprove', 'WARN', '参数不完整');
      return;
    }
    isBusy = true;
    await groupPost('审批进群', '/group/roomAccessApplyCheckApprove', {
      chatroomId, newMsgId: approveMsgId.trim(), msgContent: approveMsgContent.trim(),
    });
    isBusy = false;
  }

  // ═══════════════════════════════════════════════════════════
  // 工具
  // ═══════════════════════════════════════════════════════════
  function clearLogs() { logs = []; }
</script>

<div class="wa-mod">
  <!-- 顶部：群 ID 输入 -->
  <div class="wa-topbar">
    <label class="wa-topbar-field">
      <span class="wa-topbar-label">chatroomId:</span>
      <input type="text" bind:value={chatroomId} placeholder="群 ID（如 xxx@chatroom）" class="wa-topbar-input" />
    </label>
    <span class="wa-topbar-hint">所有操作基于此群 ID</span>
  </div>

  <div class="wa-mod-split">
    <!-- ═══ 左侧：Tab + 操作 ═══ -->
    <div class="wa-mod-left">
      <div class="wa-tabs">
        <button class="wa-tab" class:active={activeTab === 'create'} onclick={() => activeTab = 'create'}>创建群</button>
        <button class="wa-tab" class:active={activeTab === 'join'} onclick={() => activeTab = 'join'}>进群</button>
        <button class="wa-tab" class:active={activeTab === 'member'} onclick={() => activeTab = 'member'}>成员</button>
        <button class="wa-tab" class:active={activeTab === 'announcement'} onclick={() => activeTab = 'announcement'}>公告</button>
        <button class="wa-tab" class:active={activeTab === 'qr'} onclick={() => activeTab = 'qr'}>二维码</button>
        <button class="wa-tab" class:active={activeTab === 'admin'} onclick={() => activeTab = 'admin'}>管理</button>
        <button class="wa-tab" class:active={activeTab === 'approval'} onclick={() => activeTab = 'approval'}>审批</button>
      </div>

      {#if activeTab === 'create'}
        <!-- 线 A -->
        <div class="wa-card">
          <h3 class="wa-card-title">创建群聊（线 A）</h3>
          <p class="wa-hint">至少 2 个好友 wxid；企微好友须用文档要求的 username 形式</p>
          <div class="wa-form-grid">
            <label class="wa-field">
              <span class="wa-label">wxids *</span>
              <textarea bind:value={createWxids} rows="3" placeholder="wxid_1,wxid_2,..."></textarea>
            </label>
          </div>
          <div class="wa-actions">
            <button class="wa-btn wa-btn-primary" onclick={createChatroom} disabled={isBusy}>创建群聊</button>
            <button class="wa-btn" onclick={getChatroomInfo} disabled={isBusy || !chatroomId}>拉取群信息</button>
          </div>
          {#if createdRoomId}
            <p class="wa-success">✅ 已创建: {createdRoomId}</p>
          {/if}
        </div>

      {:else if activeTab === 'join'}
        <!-- 线 B -->
        <div class="wa-card">
          <h3 class="wa-card-title">扫码进群（线 B）</h3>
          <div class="wa-form-grid">
            <label class="wa-field">
              <span class="wa-label">qrUrl *</span>
              <input type="text" bind:value={qrUrl} placeholder="群二维码解析得到的链接" />
            </label>
          </div>
          <div class="wa-actions">
            <button class="wa-btn wa-btn-primary" onclick={joinByQr} disabled={isBusy}>扫码进群</button>
          </div>
        </div>
        <div class="wa-card">
          <h3 class="wa-card-title">同意邀请进群</h3>
          <div class="wa-form-grid">
            <label class="wa-field">
              <span class="wa-label">邀请 URL *</span>
              <input type="text" bind:value={agreeUrl} placeholder="回调中的完整 HTTPS" />
            </label>
          </div>
          <div class="wa-actions">
            <button class="wa-btn" onclick={agreeJoin} disabled={isBusy}>同意邀请</button>
          </div>
        </div>

      {:else if activeTab === 'member'}
        <!-- 线 C -->
        <div class="wa-card">
          <h3 class="wa-card-title">群成员管理（线 C）</h3>
          <div class="wa-actions" style="margin-top:0">
            <button class="wa-btn wa-btn-primary" onclick={getMemberList} disabled={isBusy || !chatroomId}>获取成员列表</button>
            <button class="wa-btn" onclick={getMemberDetail} disabled={isBusy || !chatroomId}>批量详情</button>
          </div>
          {#if memberList.length}
            <p class="wa-info">群成员 ({memberList.length}):</p>
            <div class="wa-member-list">
              {#each memberList.slice(0, 50) as m}
                <span class="wa-member-chip" title={m.wxid}>{m.displayName || m.nickName || m.wxid}</span>
              {/each}
              {#if memberList.length > 50}<span class="wa-member-more">...等 {memberList.length} 人</span>{/if}
            </div>
          {/if}
        </div>

        <div class="wa-card">
          <h3 class="wa-card-title">邀请 / 踢人</h3>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">邀请 wxids</span><input type="text" bind:value={inviteWxids} placeholder="逗号分隔" /></label>
            <label class="wa-field"><span class="wa-label">邀请理由</span><input type="text" bind:value={inviteReason} placeholder="可为空" /></label>
            <div class="wa-actions"><button class="wa-btn" onclick={inviteMember} disabled={isBusy}>邀请成员</button></div>
            <label class="wa-field"><span class="wa-label">踢出 wxids</span><input type="text" bind:value={removeWxids} placeholder="逗号分隔" /></label>
            <div class="wa-actions"><button class="wa-btn" onclick={removeMember} disabled={isBusy}>踢出成员</button></div>
          </div>
        </div>

        <div class="wa-card">
          <h3 class="wa-card-title">修改群名 / 备注 / 群昵称</h3>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">群名称</span>
              <div class="wa-input-row"><input type="text" bind:value={roomName} /><button class="wa-btn" onclick={modifyName} disabled={isBusy}>修改</button></div>
            </label>
            <label class="wa-field"><span class="wa-label">群备注</span>
              <div class="wa-input-row"><input type="text" bind:value={roomRemark} /><button class="wa-btn" onclick={modifyRemark} disabled={isBusy}>修改</button></div>
            </label>
            <label class="wa-field"><span class="wa-label">本人群昵称</span>
              <div class="wa-input-row"><input type="text" bind:value={selfNick} /><button class="wa-btn" onclick={modifySelfNick} disabled={isBusy}>修改</button></div>
            </label>
          </div>
          <p class="wa-hint">⚠ 手机端可能缓存未刷新，需等待或重启微信</p>
        </div>

        <div class="wa-card">
          <h3 class="wa-card-title">添加群成员为好友</h3>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">memberWxid *</span><input type="text" bind:value={addMemberWxid} /></label>
            <label class="wa-field"><span class="wa-label">招呼语 *</span><input type="text" bind:value={addMemberContent} /></label>
          </div>
          <div class="wa-actions">
            <button class="wa-btn" onclick={addGroupMemberAsFriend} disabled={isBusy}>添加好友</button>
            <button class="wa-btn" onclick={quitChatroom} disabled={isBusy || !chatroomId}>退出群聊</button>
          </div>
        </div>

      {:else if activeTab === 'announcement'}
        <!-- 线 D -->
        <div class="wa-card">
          <h3 class="wa-card-title">群公告（线 D）</h3>
          <div class="wa-actions" style="margin-top:0">
            <button class="wa-btn wa-btn-primary" onclick={getAnnouncement} disabled={isBusy || !chatroomId}>读取公告</button>
          </div>
          {#if announcement}
            <div class="wa-announcement-box">
              <p class="wa-announcement-text">{announcement}</p>
              <p class="wa-announcement-meta">编辑者: {announcementEditor || '—'} | 时间: {announcementTime || '—'}</p>
            </div>
          {/if}
        </div>
        <div class="wa-card">
          <h3 class="wa-card-title">发布公告（群主/管理员）</h3>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">公告内容 *</span><textarea bind:value={newAnnouncement} rows="3"></textarea></label>
          </div>
          <div class="wa-actions">
            <button class="wa-btn wa-btn-primary" onclick={setAnnouncement} disabled={isBusy}>发布公告</button>
          </div>
        </div>

      {:else if activeTab === 'qr'}
        <!-- 线 E -->
        <div class="wa-card">
          <h3 class="wa-card-title">群二维码（线 E）</h3>
          <p class="wa-hint">新设备登录后 1～3 天内可能不可用；二维码 7 天有效</p>
          <div class="wa-actions" style="margin-top:0">
            <button class="wa-btn wa-btn-primary" onclick={getQrCode} disabled={isBusy || !chatroomId}>获取群二维码</button>
          </div>
          {#if qrBase64}
            <div class="wa-qr-wrap"><img src={qrBase64} alt="群二维码" class="wa-qr-img" /></div>
            {#if qrTips}<p class="wa-hint">{qrTips}</p>{/if}
          {/if}
          {#if qrRetryCount > 0}
            <p class="wa-warn">已重试 {qrRetryCount} 次</p>
          {/if}
        </div>
        <div class="wa-card">
          <h3 class="wa-card-title">保存到通讯录</h3>
          <div class="wa-form-grid">
            <label class="wa-field">
              <span class="wa-label">操作</span>
              <select bind:value={contractOperType}>
                <option value="3">保存到通讯录</option>
                <option value="2">从通讯录移除</option>
              </select>
            </label>
          </div>
          <div class="wa-actions">
            <button class="wa-btn" onclick={saveContract} disabled={isBusy || !chatroomId}>执行</button>
          </div>
        </div>

      {:else if activeTab === 'admin'}
        <!-- 线 F -->
        <div class="wa-card">
          <h3 class="wa-card-title">管理员操作（线 F）</h3>
          <div class="wa-form-grid">
            <label class="wa-field">
              <span class="wa-label">操作类型</span>
              <select bind:value={adminOperType}>
                <option value="1">添加管理员</option>
                <option value="2">删除管理员</option>
                <option value="3">转让群主（仅1个wxid）</option>
              </select>
            </label>
            <label class="wa-field"><span class="wa-label">wxids</span><input type="text" bind:value={adminWxids} placeholder="逗号分隔" /></label>
          </div>
          <div class="wa-actions">
            <button class="wa-btn wa-btn-primary" onclick={doAdminOperate} disabled={isBusy}>执行</button>
          </div>
        </div>
        <div class="wa-card">
          <h3 class="wa-card-title">会话属性</h3>
          <div class="wa-form-grid">
            <label class="wa-check"><input type="checkbox" bind:checked={pinTop} /> 聊天置顶</label>
            <div class="wa-actions"><button class="wa-btn" onclick={doPinChat} disabled={isBusy || !chatroomId}>设置置顶</button></div>
            <label class="wa-check"><input type="checkbox" bind:checked={silenceMode} /> 消息免打扰</label>
            <div class="wa-actions"><button class="wa-btn" onclick={doSetSilence} disabled={isBusy || !chatroomId}>设置免打扰</button></div>
          </div>
        </div>

      {:else if activeTab === 'approval'}
        <!-- 线 G -->
        <div class="wa-card">
          <h3 class="wa-card-title">审批进群申请（线 G）</h3>
          <p class="wa-hint">msgContent 须为推送/回调中的完整 sysmsg XML 原文</p>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">newMsgId *</span><input type="text" bind:value={approveMsgId} /></label>
            <label class="wa-field"><span class="wa-label">msgContent (XML) *</span><textarea bind:value={approveMsgContent} rows="4"></textarea></label>
          </div>
          <div class="wa-actions">
            <button class="wa-btn wa-btn-primary" onclick={approveAccess} disabled={isBusy}>审批通过</button>
          </div>
        </div>
      {/if}
    </div>

    <!-- ═══ 右侧：日志 ═══ -->
    <div class="wa-mod-right">
      <div class="wa-card wa-card-fill">
        <div class="wa-card-head">
          <h3 class="wa-card-title">结构化日志</h3>
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
  .wa-mod { height: 100%; display: flex; flex-direction: column; gap: 10px; }
  .wa-topbar { display: flex; align-items: center; gap: 12px; padding: 10px 16px; background: var(--card); border: 1px solid var(--border); border-radius: var(--radius-lg); }
  .wa-topbar-field { display: flex; align-items: center; gap: 8px; flex: 1; }
  .wa-topbar-label { font-size: 13px; font-weight: 600; white-space: nowrap; }
  .wa-topbar-input { flex: 1; padding: 6px 10px; border: 1px solid var(--border); border-radius: 6px; font-size: 13px; font-family: var(--font-mono); background: var(--card); color: var(--foreground); }
  .wa-topbar-hint { font-size: 12px; color: var(--muted-foreground); white-space: nowrap; }
  .wa-mod-split { flex: 1; min-height: 0; display: flex; gap: 16px; }
  .wa-mod-left, .wa-mod-right { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 12px; overflow-y: auto; }
  .wa-card { background: var(--card); border: 1px solid var(--border); border-radius: var(--radius-lg); padding: 16px; }
  .wa-card-fill { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .wa-card-head { display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px; }
  .wa-card-title { font-size: 14px; font-weight: 600; margin: 0 0 12px; }
  .wa-card-head .wa-card-title { margin: 0; }
  .wa-hint { font-size: 12px; color: var(--muted-foreground); margin: 0 0 8px; line-height: 1.5; }
  .wa-info { font-size: 12px; color: var(--primary); margin: 8px 0 4px; }
  .wa-warn { font-size: 12px; color: var(--warning, #d97706); margin: 8px 0 0; }
  .wa-success { font-size: 13px; color: var(--success, #16a34a); margin: 8px 0 0; font-weight: 500; }
  .wa-tabs { display: flex; gap: 2px; border-bottom: 1px solid var(--border); flex-wrap: wrap; }
  .wa-tab { padding: 6px 10px; border: none; background: none; font-size: 12px; cursor: pointer; color: var(--muted-foreground); border-bottom: 2px solid transparent; }
  .wa-tab.active { color: var(--primary); border-bottom-color: var(--primary); font-weight: 600; }
  .wa-form-grid { display: flex; flex-direction: column; gap: 10px; }
  .wa-field { display: flex; flex-direction: column; gap: 4px; }
  .wa-label { font-size: 12px; font-weight: 600; }
  .wa-field input, .wa-field select, .wa-field textarea { padding: 6px 10px; border: 1px solid var(--border); border-radius: 6px; background: var(--card); font-size: 13px; color: var(--foreground); }
  .wa-input-row { display: flex; gap: 8px; }
  .wa-input-row input { flex: 1; }
  .wa-check { display: flex; align-items: center; gap: 6px; font-size: 13px; cursor: pointer; }
  .wa-actions { display: flex; gap: 8px; margin-top: 12px; flex-wrap: wrap; }
  .wa-btn { padding: 6px 14px; border: 1px solid var(--border); border-radius: 6px; background: var(--card); font-size: 13px; cursor: pointer; color: var(--foreground); }
  .wa-btn:hover { background: var(--muted); }
  .wa-btn-primary { background: var(--primary); color: var(--primary-foreground); border-color: var(--primary); }
  .wa-btn-sm { padding: 3px 8px; font-size: 11.5px; }
  .wa-btn:disabled { opacity: 0.4; cursor: default; pointer-events: none; }
  .wa-member-list { display: flex; flex-wrap: wrap; gap: 4px; margin-top: 4px; max-height: 120px; overflow-y: auto; }
  .wa-member-chip { font-size: 11.5px; padding: 2px 8px; border-radius: 4px; background: var(--muted); white-space: nowrap; }
  .wa-member-more { font-size: 11.5px; color: var(--muted-foreground); padding: 2px 0; }
  .wa-announcement-box { padding: 10px; border: 1px solid var(--border); border-radius: 8px; margin-top: 8px; }
  .wa-announcement-text { font-size: 13px; margin: 0 0 6px; white-space: pre-wrap; }
  .wa-announcement-meta { font-size: 11.5px; color: var(--muted-foreground); margin: 0; }
  .wa-qr-wrap { margin-top: 10px; text-align: center; }
  .wa-qr-img { max-width: 180px; border-radius: 8px; }
  .wa-log-body { flex: 1; min-height: 0; overflow-y: auto; background: #1e1e1e; border-radius: 8px; padding: 10px; font-family: var(--font-mono); font-size: 11.5px; color: #a6e22e; }
  .wa-log-line { padding: 2px 0; white-space: pre-wrap; word-break: break-all; }
  .wa-log-empty { color: #888; }
</style>
