<script lang="ts">
  /**
   * 朋友圈（SNS）模块 — 严格遵循《朋友圈 — 业务逻辑规范》
   *
   * §4 信息流拉取与翻页（大厅 + 好友，游标管理）
   * §5 snsId 传参规则（纯数字串，长度>15用字符串）
   * §6 动态卡片渲染（XML 解析文案/缩略图）
   * §7 点赞/取消
   * §8 发布链路（含上传再发送）
   * §9 详情/删除/下载视频
   * §10 评论
   * §11 隐私设置
   * §12 模块内导航与全局状态联动
   */
  import { apiPost, isTokenInvalidPayload } from '../services/api';
  import { consoleState, lookupContactDisplayName } from '../stores/console.svelte';
  import { onMount } from 'svelte';

  // ═══════════════════════════════════════════════════════════
  // §D 可配置常量
  // ═══════════════════════════════════════════════════════════
  const SNS_ID_STRING_THRESHOLD = 15;  // §D snsId 字符串化阈值
  const THUMB_EXTRACT_MAX = 6;         // §D 缩略图抽取上限
  const XML_PARSE_MAX_LEN = 100000;    // §C.1 XML 最大扫描长度

  // ═══════════════════════════════════════════════════════════
  // §8.1 公共发布附加字段
  // ═══════════════════════════════════════════════════════════
  const PUBLISH_COMMON = {
    useProxy: true,
    allowIds: [] as string[],
    atIds: [] as string[],
    denyIds: [] as string[],
  };

  // ═══════════════════════════════════════════════════════════
  // 状态
  // ═══════════════════════════════════════════════════════════
  type TabId = 'browse' | 'publish' | 'quick' | 'more';

  let activeTab = $state<TabId>('browse');
  let logs = $state<string[]>([]);

  // §4 翻页游标
  let timelineFirstPageMd5 = $state('');
  let timelineMaxId = $state(0);
  let friendFirstPageMd5 = $state('');
  let friendMaxId = $state(0);
  let lastFriendWxid = $state('');  // §4.2 下一页回退用

  // Feed 列表
  let feed = $state<SnsCard[]>([]);

  // 浏览
  let userWxid = $state('');
  let detailSnsId = $state('');

  // 发布
  let pubContent = $state('');
  let pubImgUrls = $state('');
  let pubPrivacy = $state('0');
  let urlTitle = $state('');
  let urlDesc = $state('');
  let urlLink = $state('');
  let urlThumb = $state('');
  let urlContent = $state('');
  let videoUrl = $state('');
  let videoThumb = $state('');
  let videoContent = $state('in');
  let forwardXml = $state('');
  let forwardPrivacy = $state(false);

  // 互动
  let quickSnsId = $state('');
  let quickAuthor = $state('');
  let delSnsId = $state('');
  let commentSnsId = $state('');
  let commentOperType = $state('1');
  let commentWxid = $state('');
  let commentId = $state('');
  let commentContent = $state('');

  // 隐私
  let scopeOption = $state('1');
  let strangerEnabled = $state(true);
  let privacySnsId = $state('');
  let privacyOpen = $state(true);
  let downloadXml = $state('');

  // 加载状态
  let isTimelineLoading = $state(false);
  let isFriendLoading = $state(false);

  // ═══════════════════════════════════════════════════════════
  // §6 动态卡片结构
  // ═══════════════════════════════════════════════════════════
  interface SnsCard {
    id: string;           // §5 数字标识字符串
    nickName: string;
    userName: string;
    createTime: number;
    content: string;      // §6 从 XML 解析的文案
    thumbUrls: string[];  // §6 缩略图
    likeCount: number;
    commentCount: number;
    rawXml: string;
    type: number;
    [key: string]: unknown;
  }

  // ═══════════════════════════════════════════════════════════
  // 日志（§C.3）
  // ═══════════════════════════════════════════════════════════
  function addLog(msg: string) {
    const time = new Date().toLocaleTimeString('zh-CN', { hour12: false });
    logs = [`[${time}] ${msg}`, ...logs].slice(0, 500);
  }

  // ═══════════════════════════════════════════════════════════
  // §5 snsId 传参规则
  // ═══════════════════════════════════════════════════════════
  /**
   * §5 snsId 处理：
   * 1. 去空白
   * 2. 须为纯数字串，否则非法
   * 3. 长度 > 15 → 字符串（防精度丢失）
   * 4. 否则转数值（安全整数范围内）
   */
  function snsIdForApi(raw: string): string | number | null {
    const trimmed = String(raw || '').trim();
    if (!trimmed) return null;
    if (!/^\d+$/.test(trimmed)) return null;
    if (trimmed.length > SNS_ID_STRING_THRESHOLD) return trimmed;
    const num = Number(trimmed);
    if (!Number.isSafeInteger(num)) return trimmed;
    return num;
  }

  // ═══════════════════════════════════════════════════════════
  // §6 XML 解析（文案 + 缩略图）
  // ═══════════════════════════════════════════════════════════
  /**
   * §6 文案解析：从 snsXml 中解析 contentDesc
   * 优先结构化；失败则正则匹配；还原 HTML 实体
   */
  function parseContentFromXml(xml: string): string {
    if (!xml) return '';
    const limited = xml.slice(0, XML_PARSE_MAX_LEN);
    // 尝试匹配 <contentDesc><![CDATA[...]]></contentDesc> 或 <contentDesc>text</contentDesc>
    const m = limited.match(/<contentDesc[^>]*>(?:<!\[CDATA\[([\s\S]*?)\]\]>|([\s\S]*?))<\/contentDesc>/i);
    let text = (m?.[1] ?? m?.[2] ?? '').trim();
    // 还原 HTML 实体
    text = text.replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&#x27;/g, "'");
    return text;
  }

  /**
   * §6 缩略图解析：正则匹配 thumb 标签内 CDATA 或文本
   * 收集以 http 开头的 URL，上限 THUMB_EXTRACT_MAX
   */
  function parseThumbsFromXml(xml: string): string[] {
    if (!xml) return [];
    const limited = xml.slice(0, XML_PARSE_MAX_LEN);
    const urls: string[] = [];
    // 匹配 <thumb> 标签内内容
    const re = /<thumb[^>]*>(?:<!\[CDATA\[([\s\S]*?)\]\]>|([\s\S]*?))<\/thumb>/gi;
    let match: RegExpExecArray | null;
    while ((match = re.exec(limited)) !== null && urls.length < THUMB_EXTRACT_MAX) {
      const url = (match[1] ?? match[2] ?? '').trim();
      if (url.startsWith('http')) urls.push(url);
    }
    return urls;
  }

  /**
   * §6 时间展示：启发式判断秒级/毫秒级
   */
  function formatSnsTime(ts: number): string {
    if (!ts) return '';
    const ms = ts < 1e12 ? ts * 1000 : ts;
    return new Date(ms).toLocaleString('zh-CN', { hour12: false });
  }

  // ═══════════════════════════════════════════════════════════
  // §6 安全提取列表（§4.3 兼容主键/备用键）
  // ═══════════════════════════════════════════════════════════
  function extractList(data: unknown): unknown[] {
    if (!data || typeof data !== 'object') return [];
    const d = data as Record<string, unknown>;
    // §4.3 优先文档主字段，否则备用字段
    const list = d.snsList ?? d.list ?? d.SnsList;
    return Array.isArray(list) ? list : [];
  }

  // ═══════════════════════════════════════════════════════════
  // §6 单条卡片解析（§C.2 容错隔离）
  // ═══════════════════════════════════════════════════════════
  function parseSnsCard(item: unknown): SnsCard | null {
    try {
      const obj = item as Record<string, unknown>;
      const xml = String(obj.snsXml || obj.xml || '');
      const rawId = obj.id ?? obj.snsId ?? '';
      const id = typeof rawId === 'number' ? String(rawId) : String(rawId || '');

      return {
        id,
        nickName: String(obj.nickName || obj.nickname || ''),
        userName: String(obj.userName || ''),
        createTime: Number(obj.createTime || 0),
        content: parseContentFromXml(xml),
        thumbUrls: parseThumbsFromXml(xml),
        likeCount: Number(obj.likeCount || 0),
        commentCount: Number(obj.commentCount || 0),
        rawXml: xml,
        type: Number(obj.type || 0),
      };
    } catch {
      // §C.2 单条解析异常不得阻断其它条目
      return null;
    }
  }

  // ═══════════════════════════════════════════════════════════
  // §4 信息流拉取与翻页
  // ═══════════════════════════════════════════════════════════

  // §4.1 大厅时间线
  async function loadTimeline(more = false) {
    if (isTimelineLoading) return;
    isTimelineLoading = true;

    try {
      const body: Record<string, unknown> = {
        maxId: more ? timelineMaxId : 0,
        firstPageMd5: more ? timelineFirstPageMd5 : '',
        decrypt: true,
      };
      const res = await apiPost('/sns/snsList', body, consoleState);

      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        addLog('❌ TOKEN 已失效');
        return;
      }

      if (res.data?.ret !== 200) {
        addLog(`⚠️ 大厅加载失败: ${res.data?.msg || '未知错误'}`);
        return;
      }

      const data = res.data?.data as Record<string, unknown> | undefined;
      const rawList = extractList(data);
      const cards = rawList.map(parseSnsCard).filter(Boolean) as SnsCard[];

      // §4.1 更新游标
      if (data) {
        if (data.firstPageMd5 != null) timelineFirstPageMd5 = String(data.firstPageMd5);
        if (data.maxId != null) timelineMaxId = Number(data.maxId);
      }

      // §4.1 首次清空再渲染，翻页尾部追加
      feed = more ? [...feed, ...cards] : cards;
      addLog(`✅ 大厅加载 ${cards.length} 条`);
    } catch (e) {
      addLog(`❌ 大厅加载失败: ${(e as Error).message}`);
    } finally {
      isTimelineLoading = false;
    }
  }

  // §4.2 指定好友时间线
  async function loadUserSns(more = false) {
    // §4.2 下一页回退：输入框为空则用上次好友 wxid
    let wxid = userWxid.trim();
    if (!wxid && more) wxid = lastFriendWxid;
    if (!wxid) {
      wxid = consoleState.currentTargetWxid;
      if (wxid) userWxid = wxid;
    }
    if (!wxid) {
      addLog('⚠️ 请填写好友 wxid 或从通讯录锁定目标');
      return;
    }

    if (isFriendLoading) return;
    isFriendLoading = true;

    try {
      const body: Record<string, unknown> = {
        wxid,
        maxId: more ? friendMaxId : 0,
        firstPageMd5: more ? friendFirstPageMd5 : '',
        decrypt: true,
      };
      const res = await apiPost('/sns/contactsSnsList', body, consoleState);

      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        addLog('❌ TOKEN 已失效');
        return;
      }

      if (res.data?.ret !== 200) {
        addLog(`⚠️ 好友朋友圈加载失败: ${res.data?.msg || '未知错误'}`);
        return;
      }

      const data = res.data?.data as Record<string, unknown> | undefined;
      const rawList = extractList(data);
      const cards = rawList.map(parseSnsCard).filter(Boolean) as SnsCard[];

      // §4.2 更新好友游标
      lastFriendWxid = wxid;
      if (data) {
        if (data.firstPageMd5 != null) friendFirstPageMd5 = String(data.firstPageMd5);
        if (data.maxId != null) friendMaxId = Number(data.maxId);
      }

      feed = more ? [...feed, ...cards] : cards;
      addLog(`✅ 好友朋友圈加载 ${cards.length} 条`);
    } catch (e) {
      addLog(`❌ 好友朋友圈加载失败: ${(e as Error).message}`);
    } finally {
      isFriendLoading = false;
    }
  }

  // ═══════════════════════════════════════════════════════════
  // §7 点赞/取消点赞
  // ═══════════════════════════════════════════════════════════
  async function likeSns(oper: number) {
    const snsId = snsIdForApi(quickSnsId);
    if (snsId === null) { addLog('⚠️ snsId 必须为纯数字'); return; }
    const author = quickAuthor.trim() || consoleState.currentTargetWxid;
    if (!author) { addLog('⚠️ 请填写发布者 wxid'); return; }

    try {
      await apiPost('/sns/likeSns', {
        snsId, operType: oper, wxid: author, useProxy: true,
      }, consoleState);
      addLog(`✅ ${oper === 1 ? '点赞' : '取消点赞'}成功`);
    } catch (e) {
      addLog(`❌ ${oper === 1 ? '点赞' : '取消点赞'}失败: ${(e as Error).message}`);
    }
  }

  // ═══════════════════════════════════════════════════════════
  // §8 发布链路
  // ═══════════════════════════════════════════════════════════
  const privacyBool = $derived(pubPrivacy === '1');

  // §8.3 文本
  async function sendTextSns() {
    if (!pubContent.trim()) { addLog('⚠️ 文案不能为空'); return; }
    try {
      await apiPost('/sns/sendTextSns', {
        content: pubContent, privacy: privacyBool, ...PUBLISH_COMMON,
      }, consoleState);
      addLog('✅ 纯文本朋友圈已发布');
    } catch (e) { addLog(`❌ ${(e as Error).message}`); }
  }

  // §8.4 图文（上传再发送）
  async function sendImgSns() {
    const urls = pubImgUrls.split(',').map(s => s.trim()).filter(Boolean);
    if (!urls.length) { addLog('⚠️ 至少填写一张图片 URL'); return; }

    try {
      // 步骤 1：上传
      addLog(`正在上传 ${urls.length} 张图片...`);
      const uploadRes = await apiPost('/sns/uploadSnsImage', { imgUrls: urls }, consoleState);
      if (isTokenInvalidPayload(uploadRes.data)) { consoleState.tokenStatus = 'invalid'; addLog('❌ TOKEN 已失效'); return; }
      if (uploadRes.data?.ret !== 200) {
        addLog(`⚠️ 图片上传失败: ${uploadRes.data?.msg}`);
        return; // §E 上传失败不继续发送
      }

      const imgInfos = uploadRes.data?.data;
      if (!Array.isArray(imgInfos)) {
        addLog('⚠️ 图片上传返回数据格式异常');
        return;
      }

      // 步骤 2：发送
      await apiPost('/sns/sendImgSns', {
        content: pubContent.trim() || ' ', // §D 空文案占位空格
        privacy: privacyBool,
        imgInfos,
        ...PUBLISH_COMMON,
      }, consoleState);
      addLog(`✅ 图文朋友圈已发布 (${imgInfos.length} 张图)`);
    } catch (e) { addLog(`❌ ${(e as Error).message}`); }
  }

  // §8.4 仅上传图片
  async function uploadImgOnly() {
    const urls = pubImgUrls.split(',').map(s => s.trim()).filter(Boolean);
    if (!urls.length) { addLog('⚠️ 至少填写一张图片 URL'); return; }
    try {
      const res = await apiPost('/sns/uploadSnsImage', { imgUrls: urls }, consoleState);
      addLog(`✅ 图片上传完成: ${JSON.stringify(res.data?.data).slice(0, 200)}`);
    } catch (e) { addLog(`❌ ${(e as Error).message}`); }
  }

  // §8.5 链接卡片
  async function sendUrlSns() {
    if (!urlTitle.trim() || !urlLink.trim()) { addLog('⚠️ 标题和链接地址必填'); return; }
    try {
      await apiPost('/sns/sendUrlSns', {
        title: urlTitle, description: urlDesc, linkUrl: urlLink, thumbUrl: urlThumb,
        content: urlContent, privacy: privacyBool, ...PUBLISH_COMMON,
      }, consoleState);
      addLog('✅ 链接朋友圈已发布');
    } catch (e) { addLog(`❌ ${(e as Error).message}`); }
  }

  // §8.6 视频（上传再发送）
  async function sendVideoSns() {
    if (!videoUrl.trim()) { addLog('⚠️ 视频 URL 不能为空'); return; }

    try {
      // 步骤 1：上传
      addLog('正在上传视频...');
      const uploadRes = await apiPost('/sns/uploadSnsVideo', { videoUrl, thumbUrl: videoThumb }, consoleState);
      if (isTokenInvalidPayload(uploadRes.data)) { consoleState.tokenStatus = 'invalid'; addLog('❌ TOKEN 已失效'); return; }
      if (uploadRes.data?.ret !== 200) {
        addLog(`⚠️ 视频上传失败: ${uploadRes.data?.msg}`);
        return; // §E 上传失败不继续发送
      }

      // 步骤 2：发送
      await apiPost('/sns/sendVideoSns', {
        content: videoContent || 'in',
        privacy: privacyBool,
        videoInfo: uploadRes.data?.data,
        ...PUBLISH_COMMON,
      }, consoleState);
      addLog('✅ 视频朋友圈已发布');
    } catch (e) { addLog(`❌ ${(e as Error).message}`); }
  }

  // §8.6 仅上传视频
  async function uploadVideoOnly() {
    if (!videoUrl.trim()) { addLog('⚠️ 视频 URL 不能为空'); return; }
    try {
      const res = await apiPost('/sns/uploadSnsVideo', { videoUrl, thumbUrl: videoThumb }, consoleState);
      addLog(`✅ 视频上传完成: ${JSON.stringify(res.data?.data).slice(0, 200)}`);
    } catch (e) { addLog(`❌ ${(e as Error).message}`); }
  }

  // §8.7 转发
  async function forwardSns() {
    if (!forwardXml.trim()) { addLog('⚠️ snsXml 不能为空'); return; }
    try {
      await apiPost('/sns/forwardSns', {
        snsXml: forwardXml, privacy: forwardPrivacy, ...PUBLISH_COMMON,
      }, consoleState);
      addLog('✅ 转发成功');
    } catch (e) { addLog(`❌ ${(e as Error).message}`); }
  }

  // ═══════════════════════════════════════════════════════════
  // §9 浏览辅助与危险操作
  // ═══════════════════════════════════════════════════════════
  async function getSnsDetails() {
    const snsId = snsIdForApi(detailSnsId);
    if (snsId === null) { addLog('⚠️ snsId 必须为纯数字'); return; }
    try {
      const res = await apiPost('/sns/snsDetails', { snsId }, consoleState);
      addLog(`✅ 详情: ${JSON.stringify(res.data?.data).slice(0, 500)}`);
    } catch (e) { addLog(`❌ ${(e as Error).message}`); }
  }

  async function deleteSns() {
    const snsId = snsIdForApi(delSnsId);
    if (snsId === null) { addLog('⚠️ snsId 必须为纯数字'); return; }
    try {
      await apiPost('/sns/delSns', { snsId }, consoleState);
      addLog('✅ 朋友圈已删除');
    } catch (e) { addLog(`❌ ${(e as Error).message}`); }
  }

  async function downloadSnsVideo() {
    if (!downloadXml.trim()) { addLog('⚠️ snsXml 不能为空'); return; }
    try {
      const res = await apiPost('/sns/downloadSnsVideo', { snsXml: downloadXml }, consoleState);
      addLog(`✅ 视频下载: ${JSON.stringify(res.data?.data).slice(0, 300)}`);
    } catch (e) { addLog(`❌ ${(e as Error).message}`); }
  }

  // ═══════════════════════════════════════════════════════════
  // §10 评论
  // ═══════════════════════════════════════════════════════════
  async function commentSns() {
    const snsId = snsIdForApi(commentSnsId);
    if (snsId === null) { addLog('⚠️ snsId 必须为纯数字'); return; }
    const wxid = commentWxid.trim() || consoleState.currentTargetWxid;
    if (!wxid) { addLog('⚠️ 请填写 wxid 或锁定目标'); return; }

    const body: Record<string, unknown> = {
      snsId, operType: Number(commentOperType), wxid, content: commentContent,
    };
    if (commentId.trim()) body.commentId = commentId;

    try {
      await apiPost('/sns/commentSns', body, consoleState);
      addLog('✅ 评论操作成功');
    } catch (e) { addLog(`❌ ${(e as Error).message}`); }
  }

  // ═══════════════════════════════════════════════════════════
  // §11 隐私与全局设置
  // ═══════════════════════════════════════════════════════════
  async function setVisibleScope() {
    try {
      await apiPost('/sns/snsVisibleScope', { option: Number(scopeOption) }, consoleState);
      addLog('✅ 可见范围已设置');
    } catch (e) { addLog(`❌ ${(e as Error).message}`); }
  }

  async function setStrangerVisibility() {
    try {
      await apiPost('/sns/strangerVisibilityEnabled', { enabled: strangerEnabled }, consoleState);
      addLog('✅ 陌生人查看设置已更新');
    } catch (e) { addLog(`❌ ${(e as Error).message}`); }
  }

  async function setPrivacy() {
    const snsId = snsIdForApi(privacySnsId);
    if (snsId === null) { addLog('⚠️ snsId 必须为纯数字'); return; }
    try {
      await apiPost('/sns/snsSetPrivacy', { snsId, open: privacyOpen }, consoleState);
      addLog('✅ 单条隐私设置已更新');
    } catch (e) { addLog(`❌ ${(e as Error).message}`); }
  }

  // ═══════════════════════════════════════════════════════════
  // §12 展示名解析
  // ═══════════════════════════════════════════════════════════
  function getDisplayName(id: string): string {
    const name = lookupContactDisplayName(id);
    return name || id;
  }

  // §6.1 卡片内私聊跳转
  function cardChatRedirect(userName: string, nickName: string) {
    consoleState.currentTargetWxid = userName;
    consoleState.currentTargetDisplayName = nickName || getDisplayName(userName);
    addLog(`已锁定目标: ${nickName || userName}`);
  }

  function clearLogs() { logs = []; }

  // ═══════════════════════════════════════════════════════════
  // §12 初始化（§A.1）
  // ═══════════════════════════════════════════════════════════
  onMount(() => {
    // 同步好友 wxid 与锁定目标
    const locked = consoleState.currentTargetWxid;
    if (locked && !userWxid) userWxid = locked;
    if (locked && !quickAuthor) quickAuthor = locked;
    if (locked && !commentWxid) commentWxid = locked;
  });
</script>

<div class="wa-mod">
  <div class="wa-mod-split">
    <!-- ═══ 左侧：操作区 ═══ -->
    <div class="wa-mod-left">
      <!-- §12 分区 Tab -->
      <div class="wa-tabs">
        <button class="wa-tab" class:active={activeTab === 'browse'} onclick={() => activeTab = 'browse'}>浏览</button>
        <button class="wa-tab" class:active={activeTab === 'publish'} onclick={() => activeTab = 'publish'}>发布</button>
        <button class="wa-tab" class:active={activeTab === 'quick'} onclick={() => activeTab = 'quick'}>互动</button>
        <button class="wa-tab" class:active={activeTab === 'more'} onclick={() => activeTab = 'more'}>隐私</button>
      </div>

      {#if activeTab === 'browse'}
        <!-- §4.1 大厅时间线 -->
        <div class="wa-card">
          <h3 class="wa-card-title">朋友圈大厅</h3>
          <p class="wa-hint">首次 maxId=0, firstPageMd5=""；翻页使用上次返回值</p>
          <p class="wa-cursor">大厅游标: md5={timelineFirstPageMd5 || '—'} maxId={timelineMaxId}</p>
          <div class="wa-actions">
            <button class="wa-btn wa-btn-primary" onclick={() => loadTimeline(false)} disabled={isTimelineLoading}>
              {isTimelineLoading ? '加载中...' : '刷新大厅'}
            </button>
            <button class="wa-btn" onclick={() => loadTimeline(true)} disabled={isTimelineLoading}>加载下一页</button>
          </div>
        </div>

        <!-- §4.2 指定好友时间线 -->
        <div class="wa-card">
          <h3 class="wa-card-title">查看好友朋友圈</h3>
          <div class="wa-form-grid">
            <label class="wa-field">
              <span class="wa-label">好友 wxid</span>
              <input type="text" bind:value={userWxid} placeholder={consoleState.currentTargetWxid || '输入或锁定目标'} />
            </label>
          </div>
          <p class="wa-cursor">好友游标: md5={friendFirstPageMd5 || '—'} maxId={friendMaxId} wxid={lastFriendWxid || '—'}</p>
          <div class="wa-actions">
            <button class="wa-btn wa-btn-primary" onclick={() => loadUserSns(false)} disabled={isFriendLoading}>
              {isFriendLoading ? '加载中...' : '拉取好友圈'}
            </button>
            <button class="wa-btn" onclick={() => loadUserSns(true)} disabled={isFriendLoading}>好友圈下一页</button>
          </div>
        </div>

        <!-- §9 详情 -->
        <div class="wa-card">
          <h3 class="wa-card-title">单条详情</h3>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">snsId</span><input type="text" bind:value={detailSnsId} /></label>
          </div>
          <div class="wa-actions"><button class="wa-btn" onclick={getSnsDetails}>查询详情</button></div>
        </div>

      {:else if activeTab === 'publish'}
        <!-- §8.3 文本 -->
        <div class="wa-card">
          <h3 class="wa-card-title">发布纯文本</h3>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">文案 content *</span><textarea bind:value={pubContent} rows="3"></textarea></label>
            <label class="wa-field">
              <span class="wa-label">可见权限</span>
              <select bind:value={pubPrivacy}><option value="0">公开</option><option value="1">私密</option></select>
            </label>
          </div>
          <div class="wa-actions"><button class="wa-btn wa-btn-primary" onclick={sendTextSns}>发布纯文本</button></div>
        </div>

        <!-- §8.4 图文 -->
        <div class="wa-card">
          <h3 class="wa-card-title">发布图文（上传再发送）</h3>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">文案</span><textarea bind:value={pubContent} rows="2"></textarea></label>
            <label class="wa-field"><span class="wa-label">图片链接（逗号分隔）*</span><input type="text" bind:value={pubImgUrls} /></label>
          </div>
          <div class="wa-actions">
            <button class="wa-btn wa-btn-primary" onclick={sendImgSns}>发布图文</button>
            <button class="wa-btn" onclick={uploadImgOnly}>仅上传图片</button>
          </div>
        </div>

        <!-- §8.5 链接 -->
        <div class="wa-card">
          <h3 class="wa-card-title">发链接</h3>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">title *</span><input type="text" bind:value={urlTitle} /></label>
            <label class="wa-field"><span class="wa-label">description</span><input type="text" bind:value={urlDesc} /></label>
            <label class="wa-field"><span class="wa-label">linkUrl *</span><input type="text" bind:value={urlLink} /></label>
            <label class="wa-field"><span class="wa-label">thumbUrl</span><input type="text" bind:value={urlThumb} /></label>
            <label class="wa-field"><span class="wa-label">content</span><input type="text" bind:value={urlContent} /></label>
          </div>
          <div class="wa-actions"><button class="wa-btn wa-btn-primary" onclick={sendUrlSns}>发送链接圈</button></div>
        </div>

        <!-- §8.6 视频 -->
        <div class="wa-card">
          <h3 class="wa-card-title">发视频（上传再发送）</h3>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">videoUrl *</span><input type="text" bind:value={videoUrl} /></label>
            <label class="wa-field"><span class="wa-label">thumbUrl</span><input type="text" bind:value={videoThumb} /></label>
            <label class="wa-field"><span class="wa-label">content</span><input type="text" bind:value={videoContent} /></label>
          </div>
          <div class="wa-actions">
            <button class="wa-btn wa-btn-primary" onclick={sendVideoSns}>上传并发送</button>
            <button class="wa-btn" onclick={uploadVideoOnly}>仅上传视频</button>
          </div>
        </div>

        <!-- §8.7 转发 -->
        <div class="wa-card">
          <h3 class="wa-card-title">转发</h3>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">snsXml *</span><textarea bind:value={forwardXml} rows="4"></textarea></label>
            <label class="wa-check"><input type="checkbox" bind:checked={forwardPrivacy} /> privacy（私密）</label>
          </div>
          <div class="wa-actions"><button class="wa-btn wa-btn-primary" onclick={forwardSns}>转发</button></div>
        </div>

      {:else if activeTab === 'quick'}
        <!-- §7 点赞 -->
        <div class="wa-card">
          <h3 class="wa-card-title">点赞 / 取消</h3>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">snsId *</span><input type="text" bind:value={quickSnsId} /></label>
            <label class="wa-field"><span class="wa-label">发布者 wxid *</span><input type="text" bind:value={quickAuthor} placeholder={consoleState.currentTargetWxid} /></label>
          </div>
          <div class="wa-actions">
            <button class="wa-btn wa-btn-primary" onclick={() => likeSns(1)}>点赞</button>
            <button class="wa-btn" onclick={() => likeSns(2)}>取消点赞</button>
          </div>
        </div>

        <!-- §9 删除 -->
        <div class="wa-card">
          <h3 class="wa-card-title">删除朋友圈</h3>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">snsId</span><input type="text" bind:value={delSnsId} /></label>
          </div>
          <div class="wa-actions"><button class="wa-btn" onclick={deleteSns}>删除</button></div>
        </div>

        <!-- §10 评论 -->
        <div class="wa-card">
          <h3 class="wa-card-title">评论 / 删评论</h3>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">snsId *</span><input type="text" bind:value={commentSnsId} /></label>
            <label class="wa-field">
              <span class="wa-label">operType</span>
              <select bind:value={commentOperType}><option value="1">评论</option><option value="2">删除评论</option></select>
            </label>
            <label class="wa-field"><span class="wa-label">wxid</span><input type="text" bind:value={commentWxid} placeholder={consoleState.currentTargetWxid} /></label>
            <label class="wa-field"><span class="wa-label">commentId（可选）</span><input type="text" bind:value={commentId} /></label>
            <label class="wa-field"><span class="wa-label">content</span><input type="text" bind:value={commentContent} /></label>
          </div>
          <div class="wa-actions"><button class="wa-btn wa-btn-primary" onclick={commentSns}>提交</button></div>
        </div>

      {:else if activeTab === 'more'}
        <!-- §11 可见范围 -->
        <div class="wa-card">
          <h3 class="wa-card-title">可见范围</h3>
          <div class="wa-form-grid">
            <label class="wa-field">
              <span class="wa-label">option</span>
              <select bind:value={scopeOption}>
                <option value="1">全部</option><option value="2">最近半年</option>
                <option value="3">最近一个月</option><option value="4">最近三天</option>
              </select>
            </label>
          </div>
          <div class="wa-actions"><button class="wa-btn wa-btn-primary" onclick={setVisibleScope}>设置</button></div>
        </div>

        <!-- §11 陌生人可见 -->
        <div class="wa-card">
          <h3 class="wa-card-title">陌生人查看</h3>
          <label class="wa-check"><input type="checkbox" bind:checked={strangerEnabled} /> 允许陌生人查看</label>
          <div class="wa-actions"><button class="wa-btn" onclick={setStrangerVisibility}>提交</button></div>
        </div>

        <!-- §11 单条隐私 -->
        <div class="wa-card">
          <h3 class="wa-card-title">单条隐私/公开</h3>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">snsId</span><input type="text" bind:value={privacySnsId} /></label>
            <label class="wa-check"><input type="checkbox" bind:checked={privacyOpen} /> open（公开）</label>
          </div>
          <div class="wa-actions"><button class="wa-btn" onclick={setPrivacy}>设置</button></div>
        </div>

        <!-- §9 下载视频 -->
        <div class="wa-card">
          <h3 class="wa-card-title">下载视频</h3>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">snsXml *</span><textarea bind:value={downloadXml} rows="4"></textarea></label>
          </div>
          <div class="wa-actions"><button class="wa-btn wa-btn-primary" onclick={downloadSnsVideo}>下载</button></div>
        </div>
      {/if}
    </div>

    <!-- ═══ 右侧：Feed + 日志 ═══ -->
    <div class="wa-mod-right">
      <!-- §6 动态卡片列表 -->
      <div class="wa-card wa-card-fill">
        <h3 class="wa-card-title">朋友圈动态</h3>
        <div class="wa-feed-list">
          {#each feed as card}
            {@const safeContent = card.content || ''}
            <div class="wa-feed-item">
              <div class="wa-feed-header">
                <span class="wa-feed-nick">{card.nickName || card.userName}</span>
                <span class="wa-feed-time">{formatSnsTime(card.createTime)}</span>
              </div>
              {#if safeContent}
                <p class="wa-feed-content">{safeContent}</p>
              {/if}
              {#if card.thumbUrls.length}
                <div class="wa-feed-thumbs">
                  {#each card.thumbUrls as url}
                    <img src={url} alt="" class="wa-feed-thumb" loading="lazy" referrerpolicy="no-referrer" />
                  {/each}
                </div>
              {/if}
              <div class="wa-feed-meta">
                <span class="wa-feed-id">#{card.id}</span>
                {#if card.likeCount}<span>👍 {card.likeCount}</span>{/if}
                {#if card.commentCount}<span>💬 {card.commentCount}</span>{/if}
              </div>
              <div class="wa-feed-actions">
                <button class="wa-btn wa-btn-sm" onclick={() => { quickSnsId = card.id; quickAuthor = card.userName; likeSns(1); }}>👍 点赞</button>
                <button class="wa-btn wa-btn-sm" onclick={() => cardChatRedirect(card.userName, card.nickName)}>💬 私聊</button>
              </div>
            </div>
          {:else}
            <p class="wa-empty-hint">暂无数据，请先拉取大厅或好友朋友圈</p>
          {/each}
        </div>
      </div>

      <!-- 日志 -->
      <div class="wa-card wa-card-fill">
        <div class="wa-card-head">
          <h3 class="wa-card-title">日志</h3>
          <button class="wa-btn wa-btn-sm" onclick={clearLogs}>清空</button>
        </div>
        <div class="wa-log-body">
          {#each logs as log}<div class="wa-log-line">{log}</div>{:else}<div class="wa-log-empty">暂无</div>{/each}
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
  .wa-tabs { display: flex; gap: 2px; border-bottom: 1px solid var(--border); margin-bottom: 12px; }
  .wa-tab { padding: 6px 12px; border: none; background: none; font-size: 13px; cursor: pointer; color: var(--muted-foreground); border-bottom: 2px solid transparent; }
  .wa-tab.active { color: var(--primary); border-bottom-color: var(--primary); font-weight: 600; }
  .wa-form-grid { display: flex; flex-direction: column; gap: 10px; }
  .wa-field { display: flex; flex-direction: column; gap: 4px; }
  .wa-label { font-size: 12px; font-weight: 600; }
  .wa-field input, .wa-field select, .wa-field textarea { padding: 6px 10px; border: 1px solid var(--border); border-radius: 6px; background: var(--card); font-size: 13px; color: var(--foreground); }
  .wa-check { display: flex; align-items: center; gap: 6px; font-size: 13px; cursor: pointer; }
  .wa-actions { display: flex; gap: 8px; margin-top: 12px; flex-wrap: wrap; }
  .wa-btn { padding: 6px 14px; border: 1px solid var(--border); border-radius: 6px; background: var(--card); font-size: 13px; cursor: pointer; color: var(--foreground); }
  .wa-btn:hover { background: var(--muted); }
  .wa-btn-primary { background: var(--primary); color: var(--primary-foreground); border-color: var(--primary); }
  .wa-btn-sm { padding: 3px 8px; font-size: 11.5px; }
  .wa-btn:disabled { opacity: 0.4; cursor: default; pointer-events: none; }
  .wa-feed-list { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 10px; }
  .wa-feed-item { padding: 12px; border: 1px solid var(--border); border-radius: 8px; }
  .wa-feed-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 6px; }
  .wa-feed-nick { font-weight: 600; font-size: 13px; }
  .wa-feed-time { font-size: 11.5px; color: var(--muted-foreground); }
  .wa-feed-content { font-size: 13px; margin: 0 0 8px; white-space: pre-wrap; word-break: break-all; }
  .wa-feed-thumbs { display: flex; gap: 4px; flex-wrap: wrap; margin-bottom: 8px; }
  .wa-feed-thumb { width: 60px; height: 60px; object-fit: cover; border-radius: 4px; }
  .wa-feed-meta { display: flex; gap: 10px; font-size: 12px; color: var(--muted-foreground); margin-bottom: 6px; }
  .wa-feed-id { font-family: var(--font-mono); }
  .wa-feed-actions { display: flex; gap: 6px; }
  .wa-empty-hint { color: var(--muted-foreground); font-size: 13px; text-align: center; padding: 24px 0; }
  .wa-log-body { flex: 1; min-height: 0; overflow-y: auto; background: #1e1e1e; border-radius: 8px; padding: 10px; font-family: var(--font-mono); font-size: 12px; color: #a6e22e; }
  .wa-log-line { padding: 2px 0; white-space: pre-wrap; word-break: break-all; }
  .wa-log-empty { color: #888; }
</style>
