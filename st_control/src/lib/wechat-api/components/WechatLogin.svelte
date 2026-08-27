<script lang="ts">
  /**
   * 登录模块 — 严格遵循《微信扫码登录 — 业务逻辑规范》
   *
   * 状态机（§3.1）：
   *   idle → qr_ready → polling → success / cancelled / timeout / fatal_error
   *                               → waiting_verification → polling（重启）
   *
   * 成功判定（§3.2）：
   *   用户信息载体为对象、非数组、非空键集合，且至少一个字段为非空字符串/数值/布尔/非空对象。
   *   不得唯一依赖 status === 2。
   *
   * 客户端取码 TTL（§4.3）：
   *   取码成功后启动本地计时；超时停止轮询并提示重新取码。
   *
   * 致命传输错误（§3.1.8）：
   *   单次检查请求发生网络级失败或不可解析响应 → 停止轮询。
   */
  import { apiPost, assertApiOk, isTokenInvalidPayload } from '../services/api';
  import { consoleState, setLoginInfo, clearLoginInfo, saveLoginSnapshot } from '../stores/console.svelte';
  import type { LoginQrCodeData, CheckLoginData } from '../types';
  import { onDestroy } from 'svelte';

  // ═══════════════════════════════════════════════════════════
  // 可配置常量（§D.4）
  // ═══════════════════════════════════════════════════════════
  const POLL_INTERVAL_MS = 5000;        // 轮询间隔 ≥ 5s（§D.1）
  const QR_TTL_MS = 5 * 60 * 1000;     // 客户端取码 TTL：5 分钟
  const LOGOUT_RETRY_MAX = 3;           // 退出重试次数
  const LOGOUT_RETRY_DELAY_MS = 800;    // 退出重试退避

  // ═══════════════════════════════════════════════════════════
  // 状态类型定义（§3.1）
  // ═══════════════════════════════════════════════════════════
  type LoginPhase =
    | 'idle'                    // §3.1.1 空闲
    | 'qr_ready'               // §3.1.2 已取码、待轮询条件满足
    | 'polling'                // §3.1.3 轮询中
    | 'waiting_verification'   // §3.1.4 等待二次验证
    | 'success'                // §3.1.5 成功
    | 'cancelled'              // §3.1.6 用户主动取消
    | 'timeout'                // §3.1.7 本地取码超时
    | 'fatal_error';           // §3.1.8 致命传输错误

  // ═══════════════════════════════════════════════════════════
  // 响应式状态
  // ═══════════════════════════════════════════════════════════
  let phase = $state<LoginPhase>('idle');
  let qrImgBase64 = $state('');
  let qrUuid = $state('');                 // 取码票据（§2 会话票据）
  let flowAppId = $state('');              // 流程内设备标识（§4.3 解析规则）
  let pollCount = $state(0);
  let pollStatusText = $state('');
  let verificationUrl = $state('');        // 二次验证资源（§4.3 分支）

  // 登录成功后的用户信息
  let loginProfileAvatar = $state('');
  let loginProfileNick = $state('');
  let loginProfileWxid = $state('');

  // 全链路日志（§D.3）
  let logs = $state<string[]>([]);

  // 配置表单
  let configAppId = $state('');
  let configType = $state<'mac' | 'ipad'>('mac');
  let configRegionId = $state('110000');
  let configProxyIp = $state('');

  // ─── 内部调度器引用（§6 单一轮询调度器）───
  let pollTimer: ReturnType<typeof setInterval> | null = null;
  let qrExpireTimer: ReturnType<typeof setTimeout> | null = null;
  let qrCreatedAt = $state(0);            // 取码时间戳，用于 TTL 计算

  // ═══════════════════════════════════════════════════════════
  // 派生状态
  // ═══════════════════════════════════════════════════════════
  const canGetQr = $derived(
    consoleState.tokenStatus === 'valid_locked' &&
    (phase === 'idle' || phase === 'cancelled' || phase === 'timeout' || phase === 'fatal_error')
  );
  const canStopPoll = $derived(phase === 'polling' || phase === 'waiting_verification');
  const isLoginSuccess = $derived(phase === 'success');

  // TTL 剩余秒数（用于 UI 展示）
  let ttlRemaining = $state(0);
  let ttlInterval: ReturnType<typeof setInterval> | null = null;

  // ═══════════════════════════════════════════════════════════
  // 地区列表
  // ═══════════════════════════════════════════════════════════
  const regions = [
    { value: '110000', label: '北京市' }, { value: '120000', label: '天津市' },
    { value: '130000', label: '河北省' }, { value: '140000', label: '山西省' },
    { value: '150000', label: '内蒙古' }, { value: '210000', label: '辽宁省' },
    { value: '220000', label: '吉林省' }, { value: '230000', label: '黑龙江' },
    { value: '310000', label: '上海市' }, { value: '320000', label: '江苏省' },
    { value: '330000', label: '浙江省' }, { value: '340000', label: '安徽省' },
    { value: '350000', label: '福建省' }, { value: '360000', label: '江西省' },
    { value: '370000', label: '山东省' }, { value: '410000', label: '河南省' },
    { value: '420000', label: '湖北省' }, { value: '430000', label: '湖南省' },
    { value: '440000', label: '广东省' }, { value: '450000', label: '广西省' },
    { value: '460000', label: '海南省' }, { value: '500000', label: '重庆市' },
    { value: '510000', label: '四川省' }, { value: '520000', label: '贵州省' },
    { value: '530000', label: '云南省' }, { value: '540000', label: '西藏' },
    { value: '610000', label: '陕西省' }, { value: '620000', label: '甘肃省' },
    { value: '630000', label: '青海省' }, { value: '640000', label: '宁夏' },
    { value: '650000', label: '新疆' },
  ];

  // ═══════════════════════════════════════════════════════════
  // 日志（§D.3 全链路日志，令牌脱敏）
  // ═══════════════════════════════════════════════════════════
  function addLog(msg: string) {
    const time = new Date().toLocaleTimeString('zh-CN', { hour12: false });
    logs = [`[${time}] ${msg}`, ...logs].slice(0, 500);
  }

  // ═══════════════════════════════════════════════════════════
  // 成功判定（§3.2 用户信息载体判定）
  // ═══════════════════════════════════════════════════════════
  /**
   * 判定用户信息载体是否满足成功条件：
   * 为对象、非数组、非空键集合，且至少一个字段为非空字符串、数值、布尔值或非空对象。
   */
  function isValidUserInfo(obj: unknown): boolean {
    if (!obj || typeof obj !== 'object' || Array.isArray(obj)) return false;
    const keys = Object.keys(obj);
    if (keys.length === 0) return false;
    return keys.some((k) => {
      const v = (obj as Record<string, unknown>)[k];
      if (v === null || v === undefined || v === '') return false;
      if (typeof v === 'string') return v.trim().length > 0;
      if (typeof v === 'number') return true;
      if (typeof v === 'boolean') return true;
      if (typeof v === 'object' && !Array.isArray(v)) return Object.keys(v).length > 0;
      return false;
    });
  }

  // ═══════════════════════════════════════════════════════════
  // 状态机转换（单一归约函数，§D.2）
  // ═══════════════════════════════════════════════════════════
  function transition(newPhase: LoginPhase, statusText: string) {
    phase = newPhase;
    pollStatusText = statusText;
    addLog(`[状态] ${statusText}`);
  }

  // ═══════════════════════════════════════════════════════════
  // 取码（§4.2）
  // ═══════════════════════════════════════════════════════════
  async function getQrCode() {
    // §1.2 前置条件校验
    if (!consoleState.token.trim()) {
      addLog('❌ 请先输入并保存 Token');
      return;
    }
    if (consoleState.tokenStatus !== 'valid_locked') {
      addLog('❌ 请先保存并校验 Token');
      return;
    }
    if (!configRegionId) {
      addLog('❌ 请选择地区');
      return;
    }

    // §6 禁止重复提交取码
    if (phase === 'qr_ready' || phase === 'polling') {
      addLog('⚠️ 已在取码或轮询中，请先停止');
      return;
    }

    // 清理旧状态
    stopAllSchedulers();

    transition('idle', '正在获取登录二维码...');
    try {
      const res = await apiPost<LoginQrCodeData>('/login/getLoginQrCode', {
        appId: configAppId,
        proxyIp: configProxyIp,
        regionId: configRegionId,
        type: configType,
      }, consoleState);

      // §4.5 令牌失效检测
      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        transition('fatal_error', 'TOKEN 已失效，请重新申请');
        addLog('❌ TOKEN 不可用或已过期');
        return;
      }

      const data = assertApiOk(res);

      // §4.2 解析二维码数据源（三选一优先级：qrImgBase64 > qrUrl > qrCode）
      qrImgBase64 = data.qrImgBase64 || data.qrCode || '';
      qrUuid = data.uuid || '';
      flowAppId = data.appId || configAppId || consoleState.appId;

      // §4.2 失败：无二维码数据
      if (!qrImgBase64 && !data.qrUrl) {
        transition('fatal_error', '未获取到二维码数据');
        addLog('❌ 二维码数据为空');
        return;
      }

      // §4.2 失败：无取码票据
      if (!qrUuid) {
        transition('fatal_error', '未获取到取码票据(uuid)');
        addLog('❌ uuid 为空');
        return;
      }

      // 回填设备标识
      if (flowAppId) {
        consoleState.appId = flowAppId;
      }

      // §4.2 成功：保存取码票据，启动轮询
      qrCreatedAt = Date.now();
      transition('qr_ready', '二维码已生成，准备轮询');
      addLog(`✅ 二维码已生成 appId=${flowAppId} uuid=${qrUuid}`);

      // §4.3 启动客户端取码 TTL
      startQrTtl();

      // §4.3 启动轮询调度器并立即执行第一次检查
      startPolling();

    } catch (e) {
      const msg = (e as Error).message || '未知错误';
      transition('fatal_error', `取码失败: ${msg}`);
      addLog(`❌ 获取二维码失败: ${msg}`);
    }
  }

  // ═══════════════════════════════════════════════════════════
  // 客户端取码 TTL（§4.3）
  // ═══════════════════════════════════════════════════════════
  function startQrTtl() {
    stopQrTtl();
    ttlRemaining = Math.floor(QR_TTL_MS / 1000);
    ttlInterval = setInterval(() => {
      const elapsed = Date.now() - qrCreatedAt;
      ttlRemaining = Math.max(0, Math.floor((QR_TTL_MS - elapsed) / 1000));
      if (ttlRemaining <= 0) {
        stopQrTtl();
      }
    }, 1000);

    qrExpireTimer = setTimeout(() => {
      if (phase === 'polling' || phase === 'qr_ready' || phase === 'waiting_verification') {
        stopPolling();
        transition('timeout', '二维码已过期，请重新取码');
        addLog('⏰ 客户端取码 TTL 超时，已停止轮询');
      }
    }, QR_TTL_MS);
  }

  function stopQrTtl() {
    if (qrExpireTimer) { clearTimeout(qrExpireTimer); qrExpireTimer = null; }
    if (ttlInterval) { clearInterval(ttlInterval); ttlInterval = null; }
    ttlRemaining = 0;
  }

  // ═══════════════════════════════════════════════════════════
  // 轮询调度器（§4.3, §6 单一轮询调度器）
  // ═══════════════════════════════════════════════════════════
  function startPolling() {
    // §6 同一时刻仅允许一个轮询调度器
    if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }

    pollCount = 0;
    transition('polling', '轮询中...');

    // §4.3 立即执行第一次检查
    executePollOnce();

    // §4.3 按固定间隔重复
    pollTimer = setInterval(() => {
      executePollOnce();
    }, POLL_INTERVAL_MS);
  }

  function stopPolling() {
    if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
    stopQrTtl();
    // 不改变 phase，由调用方决定新状态
  }

  function stopAllSchedulers() {
    stopPolling();
    stopQrTtl();
  }

  // ═══════════════════════════════════════════════════════════
  // 单次轮询检查（§4.3 分支处理）
  // ═══════════════════════════════════════════════════════════
  async function executePollOnce() {
    // §4.3 先判断设备标识与取码票据是否齐全
    const appId = flowAppId || consoleState.appId;
    if (!appId || !qrUuid) {
      // 不齐 → 不增加轮询计数，UI 提示等待
      pollStatusText = '等待设备标识或票据就绪...';
      return;
    }

    // §4.3 先判断客户端取码 TTL
    const elapsed = Date.now() - qrCreatedAt;
    if (elapsed >= QR_TTL_MS) {
      stopPolling();
      transition('timeout', '二维码已过期，请重新取码');
      addLog('⏰ 客户端取码 TTL 超时');
      return;
    }

    pollCount++;
    const autoSliding = configType === 'mac';

    try {
      // §4.3 调用检查接口（显式携带 appId 与 uuid）
      const res = await apiPost<CheckLoginData>('/login/checkLogin', {
        appId,
        uuid: qrUuid,
        autoSliding,
        proxyIp: configProxyIp,
      }, consoleState);

      // §4.5 令牌失效检测
      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        stopPolling();
        transition('fatal_error', 'TOKEN 已失效，请重新申请');
        addLog('❌ TOKEN 不可用或已过期');
        return;
      }

      const body = res.data;
      const data = body?.data;

      // §4.3 业务信封非成功 → 记录告警，继续调度（§3.1.8 区分）
      if (body?.ret !== 200) {
        addLog(`⚠️ 业务异常 ret=${body?.ret} msg=${body?.msg}`);
        pollStatusText = `轮询中... (第${pollCount}次) 业务异常`;
        return;
      }

      if (!data) {
        pollStatusText = `轮询中... (第${pollCount}次)`;
        return;
      }

      // §4.3 用户取消（status === 4 或其他取消语义）
      if (data.status === 4) {
        stopPolling();
        transition('cancelled', '登录已取消');
        addLog('❌ 用户取消登录');
        return;
      }

      // §4.3 二次验证（终端形态为需外部验证类且出现非空验证资源）
      if (data.url && typeof data.url === 'string' && data.url.trim()) {
        stopPolling();
        verificationUrl = data.url;
        transition('waiting_verification', '需要二次验证');
        addLog(`🔒 需要二次验证: ${data.url}`);
        return;
      }

      // §3.2 成功判定：用户信息载体满足条件即判成功
      // 不得唯一依赖 status === 2
      const userInfo = data.loginInfo;
      if (isValidUserInfo(userInfo)) {
        stopPolling();

        const info = userInfo as Record<string, unknown>;
        const wxid = String(info.wxid || '');
        const nick = String(info.nickName || data.nickName || '');
        const avatar = String(data.headImgUrl || '');

        loginProfileAvatar = avatar;
        loginProfileNick = nick;
        loginProfileWxid = wxid;

        transition('success', '登录成功');
        addLog(`✅ 登录成功！昵称=${nick} wxid=${wxid}`);

        // §5 持久化
        setLoginInfo(appId, qrUuid, nick);
        saveLoginSnapshot({ appId, wxid, nickName: nick, headImgUrl: avatar });
        return;
      }

      // §4.3 否则继续轮询；UI 展示进度
      if (data.status === 1) {
        pollStatusText = `已扫码，等待确认... (第${pollCount}次)`;
      } else if (data.status === 0) {
        pollStatusText = `等待扫码... (第${pollCount}次)`;
      } else {
        pollStatusText = `轮询中... (第${pollCount}次) status=${data.status}`;
      }

    } catch (e) {
      // §3.1.8 致命传输错误：网络级失败或不可解析响应 → 停止轮询
      const msg = (e as Error).message || '未知错误';
      stopPolling();
      transition('fatal_error', `轮询异常: ${msg}`);
      addLog(`❌ 致命传输错误: ${msg}`);
    }
  }

  // ═══════════════════════════════════════════════════════════
  // 二次验证后继续（§4.3, §D.2 唯一入口重启轮询）
  // ═══════════════════════════════════════════════════════════
  function continueAfterVerification() {
    if (phase !== 'waiting_verification') return;
    verificationUrl = '';
    // 重启轮询（重置 TTL 起点）
    qrCreatedAt = Date.now();
    startQrTtl();
    startPolling();
    addLog('🔄 二次验证完成，重启轮询');
  }

  // ═══════════════════════════════════════════════════════════
  // 辅助接口（§4.4）
  // ═══════════════════════════════════════════════════════════
  async function checkOnline() {
    try {
      const res = await apiPost<boolean>('/login/checkOnline', { appId: consoleState.appId }, consoleState);
      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        addLog('❌ TOKEN 已失效');
        return;
      }
      const data = assertApiOk(res);
      addLog(`在线状态: ${data ? '✅ 在线' : '❌ 离线'}`);
    } catch (e) {
      addLog(`❌ 检查失败: ${(e as Error).message}`);
    }
  }

  async function reconnect() {
    try {
      addLog('正在重连...');
      const res = await apiPost<boolean>('/login/reconnection', { appId: consoleState.appId }, consoleState);
      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        addLog('❌ TOKEN 已失效');
        return;
      }
      assertApiOk(res);
      addLog('✅ 重连请求已发送');
    } catch (e) {
      addLog(`❌ 重连失败: ${(e as Error).message}`);
    }
  }

  // §4.4 退出登录：带有限次数重试与固定退避
  async function logout() {
    addLog('正在退出...');
    let logoutOk = false;

    for (let i = 0; i < LOGOUT_RETRY_MAX; i++) {
      try {
        const res = await apiPost('/login/logout', {
          appId: consoleState.appId,
          proxyIp: '',
          regionId: '88',
        }, consoleState);
        if (res.data?.ret === 200) { logoutOk = true; break; }
      } catch {
        // 网络异常：继续重试
      }
      if (i < LOGOUT_RETRY_MAX - 1) {
        await new Promise((r) => setTimeout(r, LOGOUT_RETRY_DELAY_MS));
      }
    }

    if (!logoutOk) {
      addLog('⚠️ 退出接口失败，仍执行本地清理');
    } else {
      addLog('✅ 已退出登录');
    }

    // §5 无论退出接口成功与否，均执行本地清理
    resetToIdle();
  }

  // ═══════════════════════════════════════════════════════════
  // 重置与清理（§5）
  // ═══════════════════════════════════════════════════════════
  function resetToIdle() {
    stopAllSchedulers();
    clearLoginInfo();
    qrImgBase64 = '';
    qrUuid = '';
    flowAppId = '';
    verificationUrl = '';
    loginProfileAvatar = '';
    loginProfileNick = '';
    loginProfileWxid = '';
    pollCount = 0;
    transition('idle', '未在轮询');
  }

  function clearLogs() { logs = []; }

  // ═══════════════════════════════════════════════════════════
  // 组件卸载时清理（§D.2）
  // ═══════════════════════════════════════════════════════════
  onDestroy(() => {
    stopAllSchedulers();
  });
</script>

<div class="wa-mod">
  <div class="wa-mod-split">
    <!-- ═══ 左侧：配置 + 日志 ═══ -->
    <div class="wa-mod-left">
      <div class="wa-card">
        <h3 class="wa-card-title">登录配置</h3>
        <p class="wa-hint">填写 appId、代理、地区和设备类型，然后获取二维码。</p>
        <div class="wa-form-grid">
          <label class="wa-field">
            <span class="wa-label">appId</span>
            <input type="text" bind:value={configAppId} placeholder="首次可留空，重连填已有 appId" />
          </label>
          <label class="wa-field">
            <span class="wa-label">type</span>
            <div class="wa-radio-group">
              <label><input type="radio" bind:group={configType} value="mac" /> Mac（自动滑块）</label>
              <label><input type="radio" bind:group={configType} value="ipad" /> iPad（手动滑块）</label>
            </div>
          </label>
          <label class="wa-field">
            <span class="wa-label">regionId *</span>
            <select bind:value={configRegionId}>
              <option value="">请选择地区</option>
              {#each regions as r}
                <option value={r.value}>{r.value} * {r.label}</option>
              {/each}
            </select>
          </label>
          <label class="wa-field">
            <span class="wa-label">proxyIp</span>
            <input type="text" bind:value={configProxyIp} placeholder="可选 socks5://user:pass@host:port" />
          </label>
        </div>
        <div class="wa-actions">
          <button class="wa-btn wa-btn-primary" onclick={getQrCode} disabled={!canGetQr}>
            获取二维码
          </button>
          <button class="wa-btn" onclick={resetToIdle} disabled={!canStopPoll}>
            停止轮询
          </button>
        </div>
      </div>

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

    <!-- ═══ 右侧：二维码 + 状态 + 登录信息 ═══ -->
    <div class="wa-mod-right">
      <div class="wa-card">
        <h3 class="wa-card-title">二维码与轮询状态</h3>
        <div class="wa-qr-zone">
          <div class="wa-qr-frame">
            {#if qrImgBase64}
              <img src={qrImgBase64} alt="登录二维码" class="wa-qr-img" />
            {:else}
              <div class="wa-qr-empty">等待生成二维码...</div>
            {/if}
          </div>
          <div class="wa-qr-meta">
            <p class="wa-qr-hint">请使用微信扫描</p>

            <!-- 状态指示器 -->
            <div class="wa-phase-tag" class:idle={phase === 'idle'} class:polling={phase === 'polling'} class:success={phase === 'success'} class:error={phase === 'fatal_error' || phase === 'cancelled' || phase === 'timeout'} class:verify={phase === 'waiting_verification'}>
              {#if phase === 'idle'}⚪ 空闲
              {:else if phase === 'qr_ready'}🟡 已取码
              {:else if phase === 'polling'}🔵 轮询中
              {:else if phase === 'waiting_verification'}🟠 等待验证
              {:else if phase === 'success'}🟢 成功
              {:else if phase === 'cancelled'}🔴 已取消
              {:else if phase === 'timeout'}⏰ 已超时
              {:else if phase === 'fatal_error'}💀 致命错误
              {/if}
            </div>

            <p class="wa-poll-status">{pollStatusText}</p>

            {#if pollCount > 0}
              <p class="wa-poll-count">轮询次数: {pollCount}</p>
            {/if}

            {#if ttlRemaining > 0 && (phase === 'polling' || phase === 'qr_ready')}
              <p class="wa-ttl">二维码剩余: {ttlRemaining}s</p>
            {/if}
          </div>
        </div>

        <!-- 二次验证 UI -->
        {#if phase === 'waiting_verification' && verificationUrl}
          <div class="wa-verify-box">
            <p class="wa-verify-title">🔒 需要二次验证</p>
            <p class="wa-verify-hint">请使用安盾 APP 扫描或访问以下链接完成验证：</p>
            <a href={verificationUrl} target="_blank" rel="noopener" class="wa-verify-link">{verificationUrl}</a>
            <button class="wa-btn wa-btn-primary" onclick={continueAfterVerification}>
              我已完成操作，继续检查
            </button>
          </div>
        {/if}
      </div>

      <div class="wa-card wa-card-fill">
        <h3 class="wa-card-title">登录信息</h3>
        {#if isLoginSuccess}
          <div class="wa-login-success">
            <div class="wa-login-identity">
              {#if loginProfileAvatar}
                <img src={loginProfileAvatar} alt="头像" class="wa-login-avatar" />
              {/if}
              <div>
                <p class="wa-login-nick">{loginProfileNick}</p>
                <p class="wa-login-wxid">wxid: {loginProfileWxid}</p>
                <p class="wa-login-appid">appId: {flowAppId}</p>
              </div>
            </div>
            <div class="wa-actions">
              <button class="wa-btn" onclick={checkOnline}>检查在线</button>
              <button class="wa-btn" onclick={reconnect}>异常断线重连</button>
              <button class="wa-btn wa-btn-danger" onclick={logout}>退出登录</button>
            </div>
          </div>
        {:else}
          <div class="wa-login-waiting">
            {#if phase === 'fatal_error'}
              <p class="wa-error-text">❌ {pollStatusText}</p>
              <p class="wa-hint">请检查配置后重新取码。</p>
            {:else if phase === 'timeout'}
              <p class="wa-error-text">⏰ 二维码已过期</p>
              <p class="wa-hint">请点击「获取二维码」重新开始。</p>
            {:else if phase === 'cancelled'}
              <p class="wa-error-text">🔴 登录已取消</p>
              <p class="wa-hint">请点击「获取二维码」重新开始。</p>
            {:else}
              <p>等待登录成功...</p>
              <p class="wa-hint">扫码并完成验证后，本栏将显示昵称、wxid、头像与 appId。</p>
            {/if}
          </div>
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .wa-mod { height: 100%; display: flex; flex-direction: column; }
  .wa-mod-split { flex: 1; min-height: 0; display: flex; gap: 16px; }
  .wa-mod-left, .wa-mod-right { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 12px; }
  .wa-card { background: var(--card); border: 1px solid var(--border); border-radius: var(--radius-lg); padding: 16px; }
  .wa-card-fill { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .wa-card-head { display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px; }
  .wa-card-title { font-size: 14px; font-weight: 600; margin: 0 0 12px; }
  .wa-card-head .wa-card-title { margin: 0; }
  .wa-hint { font-size: 12px; color: var(--muted-foreground); margin: 0 0 12px; line-height: 1.5; }
  .wa-form-grid { display: flex; flex-direction: column; gap: 10px; }
  .wa-field { display: flex; flex-direction: column; gap: 4px; }
  .wa-label { font-size: 12px; font-weight: 600; color: var(--foreground); }
  .wa-field input, .wa-field select {
    padding: 6px 10px; border: 1px solid var(--border); border-radius: 6px;
    background: var(--card); font-size: 13px; color: var(--foreground);
  }
  .wa-radio-group { display: flex; gap: 16px; }
  .wa-radio-group label { display: flex; align-items: center; gap: 4px; font-size: 13px; cursor: pointer; }
  .wa-actions { display: flex; gap: 8px; margin-top: 12px; flex-wrap: wrap; }
  .wa-btn {
    padding: 6px 14px; border: 1px solid var(--border); border-radius: 6px;
    background: var(--card); font-size: 13px; cursor: pointer; color: var(--foreground);
    transition: background 0.15s;
  }
  .wa-btn:hover { background: var(--muted); }
  .wa-btn-primary { background: var(--primary); color: var(--primary-foreground); border-color: var(--primary); }
  .wa-btn-primary:hover { opacity: 0.9; }
  .wa-btn-danger { background: var(--destructive, #dc2626); color: #fff; border-color: var(--destructive, #dc2626); }
  .wa-btn-sm { padding: 3px 8px; font-size: 11.5px; }
  .wa-btn:disabled { opacity: 0.4; cursor: default; pointer-events: none; }
  .wa-qr-zone { display: flex; flex-direction: column; align-items: center; gap: 12px; }
  .wa-qr-frame {
    width: 200px; height: 200px; border: 2px dashed var(--border); border-radius: 12px;
    display: grid; place-items: center; overflow: hidden; background: var(--card);
  }
  .wa-qr-img { width: 100%; height: 100%; object-fit: contain; }
  .wa-qr-empty { font-size: 13px; color: var(--muted-foreground); }
  .wa-qr-meta { text-align: center; }
  .wa-qr-hint { font-size: 13px; font-weight: 600; margin: 0; }
  .wa-phase-tag {
    display: inline-block; margin: 6px 0 4px; padding: 2px 10px; border-radius: 10px;
    font-size: 12px; font-weight: 600;
    background: var(--muted); color: var(--muted-foreground);
  }
  .wa-phase-tag.polling { background: color-mix(in srgb, #3b82f6 14%, transparent); color: #2563eb; }
  .wa-phase-tag.success { background: color-mix(in srgb, #16a34a 14%, transparent); color: #15803d; }
  .wa-phase-tag.error { background: color-mix(in srgb, #dc2626 14%, transparent); color: #b91c1c; }
  .wa-phase-tag.verify { background: color-mix(in srgb, #d97706 14%, transparent); color: #b45309; }
  .wa-poll-status { font-size: 12px; color: var(--muted-foreground); margin: 4px 0 0; }
  .wa-poll-count { font-size: 11.5px; color: var(--muted-foreground); margin: 2px 0 0; }
  .wa-ttl { font-size: 11.5px; color: var(--warning, #d97706); margin: 4px 0 0; font-family: var(--font-mono); }
  .wa-verify-box {
    margin-top: 12px; padding: 12px; border: 1px solid var(--warning, #d97706);
    border-radius: 8px; background: color-mix(in srgb, var(--warning, #d97706) 6%, transparent);
  }
  .wa-verify-title { font-size: 14px; font-weight: 700; margin: 0 0 8px; }
  .wa-verify-hint { font-size: 12px; color: var(--muted-foreground); margin: 0 0 8px; }
  .wa-verify-link { font-size: 12px; font-family: var(--font-mono); word-break: break-all; display: block; margin-bottom: 10px; }
  .wa-login-success { display: flex; flex-direction: column; gap: 12px; }
  .wa-login-identity { display: flex; gap: 12px; align-items: center; }
  .wa-login-avatar { width: 48px; height: 48px; border-radius: 50%; object-fit: cover; }
  .wa-login-nick { font-size: 15px; font-weight: 700; margin: 0; }
  .wa-login-wxid, .wa-login-appid { font-size: 12px; color: var(--muted-foreground); margin: 2px 0 0; font-family: var(--font-mono); }
  .wa-login-waiting { text-align: center; padding: 24px 0; }
  .wa-login-waiting p { margin: 0 0 4px; }
  .wa-error-text { font-size: 14px; font-weight: 600; color: var(--destructive, #dc2626); }
  .wa-log-body {
    flex: 1; min-height: 0; overflow-y: auto; background: #1e1e1e; border-radius: 8px;
    padding: 10px; font-family: var(--font-mono); font-size: 12px; color: #a6e22e;
  }
  .wa-log-line { padding: 2px 0; white-space: pre-wrap; word-break: break-all; }
  .wa-log-empty { color: #888; }
</style>
