<script lang="ts">
  /**
   * 个人信息管理模块 — 严格遵循《个人信息管理 — 业务逻辑规范》
   *
   * §3 前置条件：非空设备标识
   * §4 资料展示与快照（含冷启动顺序）
   * §5 修改个人资料（sex 仅非空时为 number）
   * §6 我的二维码（Base64 前缀处理）
   * §7 设备记录（HTML 转义）
   * §8 隐私设置
   * §9 更换头像（成功后强提示杀进程）
   * §10 全链路日志
   * §C.1 自动拉取闸门
   */
  import { apiPost, isTokenInvalidPayload } from '../services/api';
  import { consoleState } from '../stores/console.svelte';
  import type { ProfileData } from '../types';
  import { onMount } from 'svelte';

  // ═══════════════════════════════════════════════════════════
  // §D 可配置常量
  // ═══════════════════════════════════════════════════════════
  const SNAPSHOT_STORAGE_KEY = 'wechat_console_profile_snapshot_v1';
  const GATE_SESSION_KEY = 'wechat_console_profile_auto_fetched_appId';
  const QR_BASE64_PREFIX = 'data:image/jpeg;base64,';

  // ═══════════════════════════════════════════════════════════
  // 状态
  // ═══════════════════════════════════════════════════════════
  let profile = $state<ProfileData | null>(null);
  let logs = $state<string[]>([]);

  // 加载状态
  let isFetchingProfile = $state(false);
  let isUpdatingProfile = $state(false);
  let isGettingQr = $state(false);
  let isGettingSafety = $state(false);
  let isSubmittingPrivacy = $state(false);
  let isSubmittingAvatar = $state(false);

  // §6 二维码
  let qrcodeImg = $state('');

  // §7 设备记录
  let safetyList = $state<Array<{ name: string; type: string; lastTime: string; uuid: string }>>([]);

  // §5 表单
  let formNick = $state('');
  let formSex = $state('');
  let formCountry = $state('');
  let formProvince = $state('');
  let formCity = $state('');
  let formSignature = $state('');

  // §8 隐私
  let privacyOption = $state('4');
  let privacyOpen = $state(true);

  // §9 头像
  let formAvatarUrl = $state('');

  // ═══════════════════════════════════════════════════════════
  // §3 前置条件检查
  // ═══════════════════════════════════════════════════════════
  function hasAppId(): boolean {
    return !!(consoleState.appId || '').trim();
  }

  // ═══════════════════════════════════════════════════════════
  // §10 日志（令牌脱敏）
  // ═══════════════════════════════════════════════════════════
  function addLog(msg: string) {
    const time = new Date().toLocaleTimeString('zh-CN', { hour12: false });
    logs = [`[${time}] ${msg}`, ...logs].slice(0, 500);
  }

  function logLocalFailure(path: string, reason: string) {
    addLog(`❌ [本地] ${path} 未请求: ${reason}`);
  }

  // ═══════════════════════════════════════════════════════════
  // §4.2 资料快照持久化
  // ═══════════════════════════════════════════════════════════
  function saveProfileSnapshot(data: ProfileData) {
    const appId = (consoleState.appId || '').trim();
    if (!appId) return;
    try {
      localStorage.setItem(SNAPSHOT_STORAGE_KEY, JSON.stringify({ appId, data }));
    } catch (e) {
      console.warn('[profile-snapshot] 写入失败:', e);
    }
  }

  function loadProfileSnapshot(): ProfileData | null {
    try {
      const raw = localStorage.getItem(SNAPSHOT_STORAGE_KEY);
      if (!raw) return null;
      const o = JSON.parse(raw);
      if (!o || !o.data) return null;
      const currentAppId = (consoleState.appId || '').trim();
      const snapshotAppId = (o.appId || '').trim();
      if (currentAppId && snapshotAppId && currentAppId !== snapshotAppId) return null;
      return o.data as ProfileData;
    } catch {
      return null;
    }
  }

  function clearProfileSnapshot() {
    try { localStorage.removeItem(SNAPSHOT_STORAGE_KEY); } catch {}
  }

  // ═══════════════════════════════════════════════════════════
  // §4.2 渲染资料卡片 + 表单回填
  // ═══════════════════════════════════════════════════════════
  function applyProfile(data: ProfileData) {
    profile = data;
    formNick = data.nickName || '';
    formCountry = data.country || '';
    formProvince = data.province || '';
    formCity = data.city || '';
    formSignature = data.signature || '';
    formSex = data.sex === 1 ? '1' : data.sex === 2 ? '2' : '';
    saveProfileSnapshot(data);
  }

  // ═══════════════════════════════════════════════════════════
  // §4.1 拉取资料
  // ═══════════════════════════════════════════════════════════
  async function fetchProfile() {
    // §3 前置条件
    if (!hasAppId()) {
      logLocalFailure('/personal/getProfile', '缺少设备标识');
      addLog('⚠️ 请先登录获取设备标识');
      return;
    }

    isFetchingProfile = true;
    try {
      const res = await apiPost<ProfileData>('/personal/getProfile', { proxyIp: '' }, consoleState);

      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        addLog('❌ TOKEN 已失效');
        return;
      }

      if (res.data?.ret !== 200) {
        addLog(`⚠️ 获取资料失败: ${res.data?.msg || '未知错误'}`);
        return;
      }

      const data = res.data?.data;
      if (!data || typeof data !== 'object') {
        // §4.2 载荷非法：清除快照
        clearProfileSnapshot();
        profile = null;
        addLog('⚠️ 资料数据为空');
        return;
      }

      applyProfile(data as ProfileData);
      addLog(`✅ 获取资料成功: ${(data as ProfileData).nickName || '—'}`);
    } catch (e) {
      addLog(`❌ 获取资料失败: ${(e as Error).message}`);
    } finally {
      isFetchingProfile = false;
    }
  }

  // ═══════════════════════════════════════════════════════════
  // §5 修改个人资料
  // ═══════════════════════════════════════════════════════════
  async function submitProfile() {
    if (!hasAppId()) {
      logLocalFailure('/personal/updateProfile', '缺少设备标识');
      addLog('⚠️ 请先登录');
      return;
    }

    isUpdatingProfile = true;
    try {
      // §5 正文组装：始终包含这五个字段
      const body: Record<string, unknown> = {
        nickName: formNick.trim(),
        country: formCountry.trim(),
        province: formProvince.trim(),
        city: formCity.trim(),
        signature: formSignature.trim(),
      };
      // §5 sex 仅在非空选择时增加，且为数值
      if (formSex) {
        body.sex = parseInt(formSex, 10);
      }

      const res = await apiPost('/personal/updateProfile', body, consoleState);

      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        addLog('❌ TOKEN 已失效');
        return;
      }

      if (res.data?.ret !== 200) {
        addLog(`⚠️ 修改失败: ${res.data?.msg || '未知错误'}`);
        return;
      }

      addLog('✅ 个人资料已更新，正在刷新...');
      // §5 成功后 await 再次 getProfile
      await fetchProfile();
    } catch (e) {
      addLog(`❌ 修改失败: ${(e as Error).message}`);
    } finally {
      isUpdatingProfile = false;
    }
  }

  // ═══════════════════════════════════════════════════════════
  // §6 我的二维码
  // ═══════════════════════════════════════════════════════════
  async function getQrCode() {
    if (!hasAppId()) {
      logLocalFailure('/personal/getQrCode', '缺少设备标识');
      addLog('⚠️ 请先登录');
      return;
    }

    isGettingQr = true;
    try {
      const res = await apiPost<{ qrCode: string }>('/personal/getQrCode', { proxyIp: '' }, consoleState);

      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        addLog('❌ TOKEN 已失效');
        return;
      }

      if (res.data?.ret !== 200) {
        addLog(`⚠️ 获取二维码失败: ${res.data?.msg}`);
        qrcodeImg = '';
        return;
      }

      const raw = res.data?.data?.qrCode || '';
      if (!raw) {
        qrcodeImg = '';
        addLog('⚠️ 二维码数据为空');
        return;
      }

      // §6 若不以 data: 开头，拼接 Base64 前缀
      qrcodeImg = raw.startsWith('data:') ? raw : QR_BASE64_PREFIX + raw;
      addLog('✅ 获取二维码成功');
    } catch (e) {
      qrcodeImg = '';
      addLog(`❌ 获取二维码失败: ${(e as Error).message}`);
    } finally {
      isGettingQr = false;
    }
  }

  // ═══════════════════════════════════════════════════════════
  // §7 设备记录（安全列表）
  // ═══════════════════════════════════════════════════════════
  function formatDeviceTime(ts: unknown): string {
    const num = Number(ts || 0);
    if (!num) return '—';
    // §7 小于 1e12 按秒乘 1000
    const ms = num < 1e12 ? num * 1000 : num;
    return new Date(ms).toLocaleString('zh-CN', { hour12: false });
  }

  /** §7 HTML 转义（防 XSS） */
  function escapeHtml(s: string): string {
    return String(s || '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;');
  }

  async function getSafetyInfo() {
    if (!hasAppId()) {
      logLocalFailure('/personal/getSafetyInfo', '缺少设备标识');
      addLog('⚠️ 请先登录');
      return;
    }

    isGettingSafety = true;
    try {
      const res = await apiPost<{ list: Array<Record<string, unknown>> }>('/personal/getSafetyInfo', { proxyIp: '' }, consoleState);

      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        addLog('❌ TOKEN 已失效');
        return;
      }

      if (res.data?.ret !== 200) {
        addLog(`⚠️ 获取设备记录失败: ${res.data?.msg}`);
        safetyList = [];
        return;
      }

      const list = res.data?.data?.list;
      if (!Array.isArray(list)) {
        safetyList = [];
        addLog('⚠️ 设备记录数据格式异常');
        return;
      }

      // §7 安全提取：HTML 转义后渲染
      safetyList = list.map(item => ({
        name: escapeHtml(String(item.name || item.deviceName || '')),
        type: escapeHtml(String(item.type || item.deviceType || '')),
        lastTime: formatDeviceTime(item.lastTime || item.lastOperateTime),
        uuid: escapeHtml(String(item.uuid || '')),
      }));
      addLog(`✅ 获取设备记录成功，共 ${safetyList.length} 台`);
    } catch (e) {
      safetyList = [];
      addLog(`❌ 获取设备记录失败: ${(e as Error).message}`);
    } finally {
      isGettingSafety = false;
    }
  }

  // ═══════════════════════════════════════════════════════════
  // §8 隐私设置
  // ═══════════════════════════════════════════════════════════
  async function submitPrivacy() {
    if (!hasAppId()) {
      logLocalFailure('/personal/privacySettings', '缺少设备标识');
      addLog('⚠️ 请先登录');
      return;
    }

    isSubmittingPrivacy = true;
    try {
      const res = await apiPost('/personal/privacySettings', {
        option: parseInt(privacyOption, 10), // §8 容错 parseInt
        open: privacyOpen,
      }, consoleState);

      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        addLog('❌ TOKEN 已失效');
        return;
      }

      if (res.data?.ret !== 200) {
        addLog(`⚠️ 隐私设置失败: ${res.data?.msg}`);
        return;
      }

      addLog(`✅ 隐私设置已更新: option=${privacyOption} open=${privacyOpen}`);
    } catch (e) {
      addLog(`❌ 隐私设置失败: ${(e as Error).message}`);
    } finally {
      isSubmittingPrivacy = false;
    }
  }

  // ═══════════════════════════════════════════════════════════
  // §9 更换头像
  // ═══════════════════════════════════════════════════════════
  async function submitAvatar() {
    const url = formAvatarUrl.trim();
    if (!url) {
      addLog('⚠️ 头像 URL 不能为空');
      return;
    }
    if (!hasAppId()) {
      logLocalFailure('/personal/updateHeadImg', '缺少设备标识');
      addLog('⚠️ 请先登录');
      return;
    }

    isSubmittingAvatar = true;
    try {
      const res = await apiPost('/personal/updateHeadImg', { headImgUrl: url }, consoleState);

      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        addLog('❌ TOKEN 已失效');
        return;
      }

      if (res.data?.ret !== 200) {
        addLog(`⚠️ 头像更换失败: ${res.data?.msg}`);
        return;
      }

      // §9 成功后强提示
      alert('头像更换成功！\n\n请完全退出手机微信进程后重新打开，才能看到新头像。');
      addLog('✅ 头像更换成功（需重启手机微信）');

      // §9 await 再次 getProfile
      await fetchProfile();
    } catch (e) {
      addLog(`❌ 头像更换失败: ${(e as Error).message}`);
    } finally {
      isSubmittingAvatar = false;
    }
  }

  // ═══════════════════════════════════════════════════════════
  // §4.3 冷启动顺序
  // ═══════════════════════════════════════════════════════════
  function coldStart() {
    // 1. 无设备标识 → 结束
    if (!hasAppId()) return;

    // 2. 内存无资料 → 尝试快照预填
    if (!profile) {
      const snapshot = loadProfileSnapshot();
      if (snapshot) {
        applyProfile(snapshot);
        addLog('✅ 已从本地快照恢复资料');
      }
    }

    // 3. 闸门判断
    try {
      const gateAppId = sessionStorage.getItem(GATE_SESSION_KEY);
      if (gateAppId === consoleState.appId) return; // 已自动拉取过
    } catch {}

    // 4. 设置闸门 + 自动拉取
    try { sessionStorage.setItem(GATE_SESSION_KEY, consoleState.appId); } catch {}
    fetchProfile();
  }

  function clearLogs() { logs = []; }

  // ═══════════════════════════════════════════════════════════
  // 初始化（§A.1 微任务触发冷启动）
  // ═══════════════════════════════════════════════════════════
  onMount(() => {
    // §4.3 首次打开用微任务触发
    queueMicrotask(() => coldStart());
  });
</script>

<div class="wa-mod">
  <div class="wa-mod-split">
    <!-- ═══ 左侧：操作区 ═══ -->
    <div class="wa-mod-left">
      <!-- §4.1 获取资料 -->
      <div class="wa-card">
        <h3 class="wa-card-title">个人信息管理</h3>
        <p class="wa-hint">请求经 apiPost 发出，自动带 Token 与 appId。</p>
        <div class="wa-actions">
          <button class="wa-btn wa-btn-primary" onclick={() => fetchProfile()} disabled={isFetchingProfile}>
            {isFetchingProfile ? '获取中...' : '获取个人资料'}
          </button>
        </div>
      </div>

      <!-- §6 我的二维码 -->
      <div class="wa-card">
        <h3 class="wa-card-title">我的二维码</h3>
        <div class="wa-actions">
          <button class="wa-btn" onclick={getQrCode} disabled={isGettingQr}>
            {isGettingQr ? '获取中...' : '获取自己的二维码'}
          </button>
        </div>
        {#if qrcodeImg}
          <div class="wa-qr-wrap"><img src={qrcodeImg} alt="我的二维码" class="wa-my-qr" /></div>
        {/if}
      </div>

      <!-- §7 设备记录 -->
      <div class="wa-card">
        <h3 class="wa-card-title">设备记录</h3>
        <div class="wa-actions">
          <button class="wa-btn" onclick={getSafetyInfo} disabled={isGettingSafety}>
            {isGettingSafety ? '获取中...' : '获取设备记录'}
          </button>
        </div>
        {#if safetyList.length}
          <div class="wa-table-wrap">
            <table class="wa-table">
              <thead><tr><th>设备</th><th>类型</th><th>最后操作</th><th>UUID</th></tr></thead>
              <tbody>
                {#each safetyList as d}
                  <tr><td>{d.name}</td><td>{d.type}</td><td>{d.lastTime}</td><td class="wa-uuid">{d.uuid}</td></tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
      </div>

      <!-- §8 隐私设置 -->
      <div class="wa-card">
        <h3 class="wa-card-title">隐私设置</h3>
        <div class="wa-form-grid">
          <label class="wa-field">
            <span class="wa-label">option</span>
            <select bind:value={privacyOption}>
              <option value="4">4 — 加我为朋友时需要验证</option>
              <option value="7">7 — 向我推荐通讯录朋友</option>
              <option value="8">8 — 添加我的方式：手机号</option>
              <option value="25">25 — 添加我的方式：微信号</option>
              <option value="38">38 — 添加我的方式：群聊</option>
              <option value="39">39 — 添加我的方式：我的二维码</option>
              <option value="40">40 — 添加我的方式：名片</option>
            </select>
          </label>
          <label class="wa-check"><input type="checkbox" bind:checked={privacyOpen} /> 开启该项</label>
        </div>
        <div class="wa-actions">
          <button class="wa-btn wa-btn-primary" onclick={submitPrivacy} disabled={isSubmittingPrivacy}>
            {isSubmittingPrivacy ? '提交中...' : '提交隐私设置'}
          </button>
        </div>
      </div>

      <!-- §5 修改个人资料 -->
      <div class="wa-card">
        <h3 class="wa-card-title">修改个人资料</h3>
        <div class="wa-form-grid">
          <label class="wa-field"><span class="wa-label">昵称 nickName</span><input type="text" bind:value={formNick} /></label>
          <label class="wa-field">
            <span class="wa-label">性别 sex</span>
            <select bind:value={formSex}>
              <option value="">不修改</option>
              <option value="1">男（1）</option>
              <option value="2">女（2）</option>
            </select>
          </label>
          <label class="wa-field"><span class="wa-label">国家 country</span><input type="text" bind:value={formCountry} placeholder="如 CN" /></label>
          <label class="wa-field"><span class="wa-label">省份 province</span><input type="text" bind:value={formProvince} /></label>
          <label class="wa-field"><span class="wa-label">城市 city</span><input type="text" bind:value={formCity} /></label>
          <label class="wa-field"><span class="wa-label">签名 signature</span><textarea bind:value={formSignature} rows="2"></textarea></label>
        </div>
        <div class="wa-actions">
          <button class="wa-btn wa-btn-primary" onclick={submitProfile} disabled={isUpdatingProfile}>
            {isUpdatingProfile ? '提交中...' : '提交修改'}
          </button>
        </div>
      </div>

      <!-- §9 修改头像 -->
      <div class="wa-card">
        <h3 class="wa-card-title">修改头像</h3>
        <p class="wa-hint wa-warn">⚠ 修改头像成功后，请完全退出手机微信进程后重新打开，才能看到新头像。</p>
        <div class="wa-form-grid">
          <label class="wa-field"><span class="wa-label">headImgUrl</span><input type="text" bind:value={formAvatarUrl} placeholder="头像图片 URL" /></label>
        </div>
        <div class="wa-actions">
          <button class="wa-btn" onclick={submitAvatar} disabled={isSubmittingAvatar}>
            {isSubmittingAvatar ? '提交中...' : '提交更换头像'}
          </button>
        </div>
      </div>
    </div>

    <!-- ═══ 右侧：预览 + 日志 ═══ -->
    <div class="wa-mod-right">
      <!-- §4.2 资料卡片 -->
      <div class="wa-card wa-card-fill">
        <h3 class="wa-card-title">当前资料预览</h3>
        {#if profile}
          <div class="wa-profile-card">
            {#if profile.headImgUrl}
              <img src={profile.headImgUrl} alt="头像" class="wa-profile-avatar" />
            {/if}
            <div>
              <p class="wa-profile-nick">{profile.nickName || '—'}</p>
              <p class="wa-profile-sub">wxid: {profile.wxid || '—'}</p>
              {#if profile.alias}<p class="wa-profile-sub">微信号: {profile.alias}</p>{/if}
            </div>
          </div>
          <dl class="wa-dl">
            <dt>地区</dt>
            <dd>{[profile.country, profile.province, profile.city].filter(Boolean).join(' ') || '—'}</dd>
            <dt>签名</dt><dd>{profile.signature || '—'}</dd>
            <dt>手机</dt><dd>{profile.mobile || '—'}</dd>
            <dt>性别</dt><dd>{profile.sex === 1 ? '男' : profile.sex === 2 ? '女' : '—'}</dd>
          </dl>
        {:else}
          <p class="wa-empty-hint">请先点击「获取个人资料」</p>
        {/if}
      </div>

      <!-- §10 全链路日志 -->
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
  .wa-warn { color: var(--warning, #d97706); }
  .wa-form-grid { display: flex; flex-direction: column; gap: 10px; }
  .wa-field { display: flex; flex-direction: column; gap: 4px; }
  .wa-label { font-size: 12px; font-weight: 600; }
  .wa-field input, .wa-field select, .wa-field textarea { padding: 6px 10px; border: 1px solid var(--border); border-radius: 6px; background: var(--card); font-size: 13px; color: var(--foreground); }
  .wa-check { display: flex; align-items: center; gap: 6px; font-size: 13px; cursor: pointer; }
  .wa-actions { display: flex; gap: 8px; margin-top: 12px; flex-wrap: wrap; }
  .wa-btn { padding: 6px 14px; border: 1px solid var(--border); border-radius: 6px; background: var(--card); font-size: 13px; cursor: pointer; color: var(--foreground); transition: background 0.15s; }
  .wa-btn:hover { background: var(--muted); }
  .wa-btn-primary { background: var(--primary); color: var(--primary-foreground); border-color: var(--primary); }
  .wa-btn-primary:hover { opacity: 0.9; }
  .wa-btn-sm { padding: 3px 8px; font-size: 11.5px; }
  .wa-btn:disabled { opacity: 0.4; cursor: default; pointer-events: none; }
  .wa-qr-wrap { margin-top: 12px; text-align: center; }
  .wa-my-qr { max-width: 180px; border-radius: 8px; }
  .wa-table-wrap { margin-top: 8px; overflow-x: auto; }
  .wa-table { width: 100%; border-collapse: collapse; font-size: 12px; }
  .wa-table th, .wa-table td { padding: 6px 8px; border-bottom: 1px solid var(--border); text-align: left; }
  .wa-table th { font-weight: 600; background: var(--muted); }
  .wa-uuid { font-family: var(--font-mono); font-size: 11px; max-width: 120px; overflow: hidden; text-overflow: ellipsis; }
  .wa-profile-card { display: flex; gap: 12px; align-items: center; margin-bottom: 16px; }
  .wa-profile-avatar { width: 56px; height: 56px; border-radius: 50%; object-fit: cover; }
  .wa-profile-nick { font-size: 16px; font-weight: 700; margin: 0; }
  .wa-profile-sub { font-size: 12px; color: var(--muted-foreground); margin: 2px 0 0; font-family: var(--font-mono); }
  .wa-dl { display: grid; grid-template-columns: 60px 1fr; gap: 6px 12px; font-size: 13px; }
  .wa-dl dt { font-weight: 600; color: var(--muted-foreground); }
  .wa-dl dd { margin: 0; word-break: break-all; }
  .wa-empty-hint { color: var(--muted-foreground); font-size: 13px; text-align: center; padding: 24px 0; }
  .wa-log-body { flex: 1; min-height: 0; overflow-y: auto; background: #1e1e1e; border-radius: 8px; padding: 10px; font-family: var(--font-mono); font-size: 12px; color: #a6e22e; }
  .wa-log-line { padding: 2px 0; white-space: pre-wrap; word-break: break-all; }
  .wa-log-empty { color: #888; }
</style>
