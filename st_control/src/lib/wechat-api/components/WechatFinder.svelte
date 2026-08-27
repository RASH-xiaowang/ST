<script lang="ts">
  /**
   * 视频号（Finder）模块 — 严格遵循《视频号 — 业务逻辑规范》
   *
   * 核心约束：
   * - 每次 POST 之间强制随机休眠 2～5 秒
   * - 结构化日志：[ISO8601] [动作] [path] [ret] [摘要]
   * - 搜索翻页使用响应返回的 cookie/searchId/offset
   * - CDN 异步上传带随机抖动轮询
   * - 严禁伪造扫码 qrUrl/qrContent
   */
  import { apiPost, isTokenInvalidPayload } from '../services/api';
  import { consoleState } from '../stores/console.svelte';

  // ═══════════════════════════════════════════════════════════
  // §C 可配置常量
  // ═══════════════════════════════════════════════════════════
  const DELAY_MIN_MS = 2000;
  const DELAY_MAX_MS = 5000;
  const ASYNC_POLL_MAX = 10;
  const ASYNC_POLL_BASE_MS = 3000;

  // ═══════════════════════════════════════════════════════════
  // 状态
  // ═══════════════════════════════════════════════════════════
  type TabId = 'identity' | 'search' | 'interact' | 'follow' | 'publish' | 'dm' | 'scan' | 'profile';

  let activeTab = $state<TabId>('search');
  let logs = $state<string[]>([]);
  let isBusy = $state(false);

  // 身份
  let myUserName = $state('');
  let myRoleType = $state('0');
  let finderNick = $state('');
  let finderHeadImg = $state('');
  let finderSignature = $state('');
  let finderSex = $state('0');
  let scanQrContent = $state('');
  let scanUsername = $state('');

  // 搜索
  let searchContent = $state('');
  let searchCategory = $state('0');
  let searchFilter = $state('0');
  let searchPage = $state(0);
  let searchCookie = $state('');
  let searchId = $state('');
  let searchOffset = $state(0);
  let searchResults = $state<Array<Record<string, unknown>>>([]);
  let canContinue = $state(false);

  // 互动目标
  let targetId = $state('');
  let targetNonceId = $state('');
  let targetUsername = $state('');
  let targetMediaType = $state('');
  let targetUrl = $state('');
  let targetThumbUrl = $state('');
  let targetThumbUrlToken = $state('');
  let targetDescription = $state('');
  let targetVideoPlayLen = $state('');
  let targetNickname = $state('');
  let targetHeadUrl = $state('');
  let targetWidth = $state('');
  let targetHeight = $state('');

  // 互动
  let commentContent = $state('');
  let commentListResults = $state<Array<Record<string, unknown>>>([]);

  // 关注
  let followOperType = $state('1');

  // 发布
  let pubTitle = $state('');
  let pubVideoUrl = $state('');
  let pubThumbUrl = $state('');
  let pubDescription = $state('');
  let pubTopic = $state('');
  let pubWidth = $state('');
  let pubHeight = $state('');
  let pubPlayLen = $state('');

  // CDN 上传
  let cdnVideoUrl = $state('');
  let cdnCoverUrl = $state('');
  let cdnFileUrl = $state('');
  let cdnThumbUrl = $state('');
  let cdnMp4Identify = $state('');
  let cdnFileSize = $state('');
  let cdnThumbMD5 = $state('');
  let cdnFileKey = $state('');

  // 私信
  let dmToUserName = $state('');
  let dmContent = $state('');
  let dmImgUrl = $state('');
  let dmSessionId = $state('');

  // 扫码
  let scanQrUrl = $state('');

  // 资料
  let profileNick = $state('');
  let profileHeadImg = $state('');
  let profileSignature = $state('');
  let profileQrBase64 = $state('');

  // ═══════════════════════════════════════════════════════════
  // §C.3 随机延迟
  // ═══════════════════════════════════════════════════════════
  function randomDelay(): Promise<void> {
    const ms = DELAY_MIN_MS + Math.random() * (DELAY_MAX_MS - DELAY_MIN_MS);
    return new Promise(r => setTimeout(r, Math.round(ms)));
  }

  // ═══════════════════════════════════════════════════════════
  // §C.4 结构化日志
  // ═══════════════════════════════════════════════════════════
  function addLog(action: string, path: string, ret: number | string, summary: string) {
    const time = new Date().toISOString();
    logs = [`[${time}] [${action}] [${path}] [ret=${ret}] [${summary}]`, ...logs].slice(0, 500);
  }

  /** 统一请求封装 */
  async function finderPost(action: string, path: string, body: Record<string, unknown>): Promise<{ ok: boolean; data: Record<string, unknown> | null; raw: Record<string, unknown> | null }> {
    try {
      const payload = { useProxy: true, ...body };
      const res = await apiPost(path, payload, consoleState);

      if (isTokenInvalidPayload(res.data)) {
        consoleState.tokenStatus = 'invalid';
        addLog(action, path, 'ERR', 'TOKEN 已失效');
        return { ok: false, data: null, raw: null };
      }

      const ret = res.data?.ret;
      const msg = res.data?.msg || '';
      const data = res.data?.data as Record<string, unknown> | undefined;

      if (ret === 200) {
        addLog(action, path, ret, '成功');
      } else {
        // §C.2 ret=500 且带 data.code 的路径须单独分支记录
        const code = data?.code;
        addLog(action, path, ret, code ? `code=${code} ${msg}` : msg || '业务异常');
      }

      await randomDelay();
      return { ok: ret === 200, data: data || null, raw: res.data as Record<string, unknown> };
    } catch (e) {
      addLog(action, path, 'ERR', (e as Error).message || '未知错误');
      await randomDelay();
      return { ok: false, data: null, raw: null };
    }
  }

  // ═══════════════════════════════════════════════════════════
  // 身份基线
  // ═══════════════════════════════════════════════════════════
  async function getProfile() {
    isBusy = true;
    const r = await finderPost('获取视频号资料', '/finder/getProfile', {});
    if (r.ok && r.data) {
      myUserName = String(r.data.mainFinderUsername || r.data.username || '');
      addLog('获取视频号资料', '/finder/getProfile', 200, `username=${myUserName}`);
    }
    isBusy = false;
  }

  async function createFinder() {
    if (!finderNick.trim()) { addLog('创建视频号', '/finder/createFinder', 'WARN', '昵称不能为空'); return; }
    isBusy = true;
    const r = await finderPost('创建视频号', '/finder/createFinder', {
      nickName: finderNick.trim(), headImg: finderHeadImg.trim(), signature: finderSignature.trim(), sex: parseInt(finderSex, 10),
    });
    if (r.ok && r.data) {
      myUserName = String(r.data.username || '');
      addLog('创建视频号', '/finder/createFinder', 200, `username=${myUserName}`);
    }
    isBusy = false;
  }

  async function scanLoginChannels() {
    if (!scanQrContent.trim()) { addLog('扫码登录助手', '/finder/scanLoginChannels', 'WARN', 'qrContent 不能为空'); return; }
    isBusy = true;
    const r = await finderPost('扫码登录助手', '/finder/scanLoginChannels', {
      qrContent: scanQrContent.trim(), username: scanUsername.trim(),
    });
    if (r.ok && r.data) {
      addLog('扫码登录助手', '/finder/scanLoginChannels', 200, `sessionId=${String(r.data.sessionId || '').slice(0, 16)}`);
    }
    isBusy = false;
  }

  // ═══════════════════════════════════════════════════════════
  // 搜索（首屏 + 翻页）
  // ═══════════════════════════════════════════════════════════
  async function doSearch(isNextPage = false) {
    if (!searchContent.trim() && !isNextPage) { addLog('搜索', '/finder/search', 'WARN', '搜索内容不能为空'); return; }
    if (isNextPage && !canContinue) { addLog('搜索翻页', '/finder/search', 'WARN', '无更多结果'); return; }

    isBusy = true;
    const body: Record<string, unknown> = {
      content: isNextPage ? searchContent : searchContent.trim(),
      category: parseInt(searchCategory, 10),
      filter: parseInt(searchFilter, 10),
      page: isNextPage ? searchPage + 1 : 0,
      cookie: isNextPage ? searchCookie : '',
      searchId: isNextPage ? searchId : '',
      offset: isNextPage ? searchOffset : 0,
    };

    const r = await finderPost(isNextPage ? '搜索翻页' : '搜索首屏', '/finder/search', body);

    if (r.ok && r.data) {
      // §C.4 searchID 字段名以响应为准
      searchCookie = String(r.data.cookies || r.data.cookie || '');
      searchId = String(r.data.searchID || r.data.searchId || '');
      searchOffset = Number(r.data.offset || 0);
      searchPage = isNextPage ? searchPage + 1 : 0;
      canContinue = Boolean(r.data.continueFlag);

      // 解析结果列表
      const list = r.data.data ?? r.data.list ?? r.data.results;
      if (Array.isArray(list)) {
        searchResults = isNextPage ? [...searchResults, ...list] : list;
        addLog(isNextPage ? '搜索翻页' : '搜索首屏', '/finder/search', 200, `共 ${searchResults.length} 条`);
      }
    }
    isBusy = false;
  }

  // ═══════════════════════════════════════════════════════════
  // 拉取用户主页
  // ═══════════════════════════════════════════════════════════
  async function getUserPage() {
    if (!targetUsername.trim()) { addLog('用户主页', '/finder/userPage', 'WARN', 'username 不能为空'); return; }
    isBusy = true;
    const r = await finderPost('用户主页', '/finder/userPage', { username: targetUsername.trim() });
    if (r.ok && r.data) {
      addLog('用户主页', '/finder/userPage', 200, `获取成功`);
    }
    isBusy = false;
  }

  // ═══════════════════════════════════════════════════════════
  // 互动
  // ═══════════════════════════════════════════════════════════
  async function doBrowse() {
    if (!targetId.trim()) { addLog('浏览', '/finder/browse', 'WARN', 'id 不能为空'); return; }
    isBusy = true;
    await finderPost('浏览', '/finder/browse', { id: targetId.trim(), nonceId: targetNonceId.trim() });
    isBusy = false;
  }

  async function doLike() {
    if (!targetId.trim()) { addLog('点赞', '/finder/idLike', 'WARN', 'id 不能为空'); return; }
    isBusy = true;
    await finderPost('点赞', '/finder/idLike', { id: targetId.trim(), nonceId: targetNonceId.trim() });
    isBusy = false;
  }

  async function doFav() {
    if (!targetId.trim()) { addLog('收藏', '/finder/idFav', 'WARN', 'id 不能为空'); return; }
    isBusy = true;
    await finderPost('收藏', '/finder/idFav', { id: targetId.trim(), nonceId: targetNonceId.trim() });
    isBusy = false;
  }

  async function doComment() {
    if (!targetId.trim() || !commentContent.trim()) { addLog('评论', '/finder/comment', 'WARN', '参数不完整'); return; }
    isBusy = true;
    await finderPost('评论', '/finder/comment', { id: targetId.trim(), nonceId: targetNonceId.trim(), content: commentContent.trim() });
    isBusy = false;
  }

  async function getCommentList() {
    if (!targetId.trim()) { addLog('评论列表', '/finder/commentList', 'WARN', 'id 不能为空'); return; }
    isBusy = true;
    const r = await finderPost('评论列表', '/finder/commentList', { id: targetId.trim(), nonceId: targetNonceId.trim() });
    if (r.ok && r.data) {
      const list = r.data.commentList ?? r.data.list ?? r.data.comments;
      commentListResults = Array.isArray(list) ? list : [];
      addLog('评论列表', '/finder/commentList', 200, `${commentListResults.length} 条`);
    }
    isBusy = false;
  }

  // ═══════════════════════════════════════════════════════════
  // 关注
  // ═══════════════════════════════════════════════════════════
  async function doFollow() {
    if (!targetUsername.trim()) { addLog('关注', '/finder/follow', 'WARN', 'username 不能为空'); return; }
    isBusy = true;
    await finderPost('关注', '/finder/follow', { username: targetUsername.trim(), operType: parseInt(followOperType, 10) });
    isBusy = false;
  }

  async function getFollowList() {
    isBusy = true;
    const r = await finderPost('关注列表', '/finder/followList', {});
    if (r.ok && r.data) {
      addLog('关注列表', '/finder/followList', 200, `获取成功`);
    }
    isBusy = false;
  }

  async function getLikeFavList() {
    isBusy = true;
    const r = await finderPost('赞与收藏', '/finder/likeFavList', {});
    if (r.ok && r.data) {
      addLog('赞与收藏', '/finder/likeFavList', 200, `获取成功`);
    }
    isBusy = false;
  }

  // ═══════════════════════════════════════════════════════════
  // 分享
  // ═══════════════════════════════════════════════════════════
  async function sendToFriend(toWxid: string) {
    if (!toWxid.trim() || !targetId.trim()) { addLog('分享好友', '/message/sendFinderMsg', 'WARN', '参数不完整'); return; }
    isBusy = true;
    await finderPost('分享好友', '/message/sendFinderMsg', {
      toWxid: toWxid.trim(),
      id: targetId, username: targetUsername, nickname: targetNickname, headUrl: targetHeadUrl,
      nonceId: targetNonceId, mediaType: parseInt(targetMediaType || '0', 10),
      width: parseInt(targetWidth || '0', 10), height: parseInt(targetHeight || '0', 10),
      url: targetUrl, thumbUrl: targetThumbUrl, thumbUrlToken: targetThumbUrlToken,
      description: targetDescription, videoPlayLen: parseInt(targetVideoPlayLen || '0', 10),
    });
    isBusy = false;
  }

  async function sendToSns() {
    if (!targetId.trim()) { addLog('分享朋友圈', '/sns/sendFinderSns', 'WARN', '参数不完整'); return; }
    isBusy = true;
    await finderPost('分享朋友圈', '/sns/sendFinderSns', {
      allowWxIds: [], atWxIds: [], disableWxIds: [],
      id: targetId, username: targetUsername, nickname: targetNickname,
      nonceId: targetNonceId, mediaType: parseInt(targetMediaType || '0', 10),
      width: parseInt(targetWidth || '0', 10), height: parseInt(targetHeight || '0', 10),
      url: targetUrl, thumbUrl: targetThumbUrl, thumbUrlToken: targetThumbUrlToken,
      description: targetDescription, videoPlayLen: parseInt(targetVideoPlayLen || '0', 10),
    });
    isBusy = false;
  }

  // ═══════════════════════════════════════════════════════════
  // 发布（简易 + CDN 同步 + CDN 异步）
  // ═══════════════════════════════════════════════════════════
  async function publishWeb() {
    if (!pubVideoUrl.trim() || !pubDescription.trim()) { addLog('发布(Web)', '/finder/publishFinderWeb', 'WARN', '参数不完整'); return; }
    isBusy = true;
    await finderPost('发布(Web)', '/finder/publishFinderWeb', {
      title: pubTitle.trim(), videoUrl: pubVideoUrl.trim(), thumbUrl: pubThumbUrl.trim(),
      description: pubDescription.trim(), myRoleType: parseInt(myRoleType, 10),
    });
    isBusy = false;
  }

  async function publishDirect() {
    if (!pubVideoUrl.trim() || !pubDescription.trim()) { addLog('发布(直传)', '/finder/publishFinder', 'WARN', '参数不完整'); return; }
    isBusy = true;
    await finderPost('发布(直传)', '/finder/publishFinder', {
      videoUrl: pubVideoUrl.trim(), thumbUrl: pubThumbUrl.trim(),
      width: parseInt(pubWidth || '0', 10), height: parseInt(pubHeight || '0', 10),
      playLen: parseInt(pubPlayLen || '0', 10), topic: pubTopic.trim(),
      myUserName: myUserName, description: pubDescription.trim(), myRoleType: parseInt(myRoleType, 10),
    });
    isBusy = false;
  }

  // CDN 同步上传
  async function uploadCdn() {
    if (!cdnVideoUrl.trim()) { addLog('CDN上传', '/finder/uploadFinderVideo', 'WARN', 'videoUrl 不能为空'); return; }
    isBusy = true;
    const r = await finderPost('CDN上传', '/finder/uploadFinderVideo', { videoUrl: cdnVideoUrl.trim(), coverImgUrl: cdnCoverUrl.trim() });
    if (r.ok && r.data) {
      cdnFileUrl = String(r.data.fileUrl || '');
      cdnThumbUrl = String(r.data.thumbUrl || '');
      cdnMp4Identify = String(r.data.mp4Identify || '');
      cdnFileSize = String(r.data.fileSize || '');
      cdnThumbMD5 = String(r.data.thumbMD5 || '');
      cdnFileKey = String(r.data.fileKey || '');
      addLog('CDN上传', '/finder/uploadFinderVideo', 200, '已获取 CDN 字段');
    }
    isBusy = false;
  }

  async function publishCdn() {
    if (!cdnFileUrl.trim()) { addLog('CDN发布', '/finder/publishFinderCdn', 'WARN', '请先上传视频'); return; }
    isBusy = true;
    await finderPost('CDN发布', '/finder/publishFinderCdn', {
      topic: pubTopic.trim(), myUserName, myRoleType: parseInt(myRoleType, 10), description: pubDescription.trim(),
      videoCdn: {
        fileUrl: cdnFileUrl, thumbUrl: cdnThumbUrl, mp4Identify: cdnMp4Identify,
        fileSize: cdnFileSize, thumbMD5: cdnThumbMD5, fileKey: cdnFileKey,
      },
    });
    isBusy = false;
  }

  // CDN 异步上传（大文件）
  async function uploadCdnAsync() {
    if (!cdnVideoUrl.trim()) { addLog('异步上传', '/finder/uploadFinderVideoAsync', 'WARN', 'videoUrl 不能为空'); return; }
    isBusy = true;
    const r = await finderPost('异步上传', '/finder/uploadFinderVideoAsync', { videoUrl: cdnVideoUrl.trim(), coverImgUrl: cdnCoverUrl.trim() });
    if (r.ok && r.data) {
      const uuid = String(r.data.uuid || r.data.UUID || '');
      addLog('异步上传', '/finder/uploadFinderVideoAsync', 200, `uuid=${uuid}`);

      // §C.3 轮询（随机抖动 + 最大次数）
      if (uuid) {
        for (let i = 0; i < ASYNC_POLL_MAX; i++) {
          const pollMs = ASYNC_POLL_BASE_MS + Math.random() * 2000;
          await new Promise(r => setTimeout(r, pollMs));

          const qr = await finderPost('异步查询', '/finder/queryFinderVideoAsync', { uuid });
          if (qr.ok && qr.data) {
            const fileUrl = String(qr.data.fileUrl || '');
            if (fileUrl) {
              cdnFileUrl = fileUrl;
              cdnThumbUrl = String(qr.data.thumbUrl || '');
              cdnMp4Identify = String(qr.data.mp4Identify || '');
              cdnFileSize = String(qr.data.fileSize || '');
              cdnThumbMD5 = String(qr.data.thumbMD5 || '');
              cdnFileKey = String(qr.data.fileKey || '');
              addLog('异步查询', '/finder/queryFinderVideoAsync', 200, 'CDN 字段已就绪');
              isBusy = false;
              return;
            }
          }
        }
        addLog('异步查询', '/finder/queryFinderVideoAsync', 'TIMEOUT', `已轮询 ${ASYNC_POLL_MAX} 次`);
      }
    }
    isBusy = false;
  }

  // ═══════════════════════════════════════════════════════════
  // 私信
  // ═══════════════════════════════════════════════════════════
  async function getMsgSession() {
    if (!dmToUserName.trim()) { addLog('获取会话', '/finder/getMsgSessionId', 'WARN', 'toUserName 不能为空'); return; }
    isBusy = true;
    const r = await finderPost('获取会话', '/finder/getMsgSessionId', { toUserName: dmToUserName.trim() });
    if (r.ok && r.data) {
      dmSessionId = String(r.data.msgSessionId || r.data.sessionId || '');
      addLog('获取会话', '/finder/getMsgSessionId', 200, `sessionId=${dmSessionId.slice(0, 16)}`);
    }
    isBusy = false;
  }

  async function sendDmText() {
    if (!dmToUserName.trim() || !dmContent.trim() || !dmSessionId.trim()) { addLog('私信文本', '/finder/postPrivateLetter', 'WARN', '参数不完整'); return; }
    isBusy = true;
    await finderPost('私信文本', '/finder/postPrivateLetter', {
      content: dmContent.trim(), msgSessionId: dmSessionId, myUserName, toUserName: dmToUserName.trim(),
    });
    isBusy = false;
  }

  async function sendDmImg() {
    if (!dmToUserName.trim() || !dmImgUrl.trim() || !dmSessionId.trim()) { addLog('私信图片', '/finder/postPrivateLetterImg', 'WARN', '参数不完整'); return; }
    isBusy = true;
    await finderPost('私信图片', '/finder/postPrivateLetterImg', {
      imgUrl: dmImgUrl.trim(), msgSessionId: dmSessionId, myUserName, toUserName: dmToUserName.trim(),
    });
    isBusy = false;
  }

  // ═══════════════════════════════════════════════════════════
  // 扫码类能力
  // ═══════════════════════════════════════════════════════════
  async function scanAction(path: string, action: string) {
    if (!scanQrUrl.trim()) { addLog(action, path, 'WARN', 'qrUrl 不能为空（须来自真实扫码）'); return; }
    isBusy = true;
    await finderPost(action, path, { qrUrl: scanQrUrl.trim() });
    isBusy = false;
  }

  // ═══════════════════════════════════════════════════════════
  // 资料与二维码
  // ═══════════════════════════════════════════════════════════
  async function updateFinderProfile() {
    isBusy = true;
    await finderPost('更新资料', '/finder/updateProfile', {
      nickName: profileNick.trim(), headImg: profileHeadImg.trim(), signature: profileSignature.trim(),
    });
    isBusy = false;
  }

  async function getFinderQr() {
    isBusy = true;
    const r = await finderPost('获取二维码', '/finder/getQrCode', {});
    if (r.ok && r.data) {
      const raw = String(r.data.qrCode || r.data.qrBase64 || '');
      profileQrBase64 = raw.startsWith('data:') ? raw : 'data:image/jpeg;base64,' + raw;
      addLog('获取二维码', '/finder/getQrCode', 200, '成功');
    }
    isBusy = false;
  }

  // ═══════════════════════════════════════════════════════════
  // 从搜索结果填充目标
  // ═══════════════════════════════════════════════════════════
  function fillTarget(item: Record<string, unknown>) {
    targetId = String(item.id || '');
    targetNonceId = String(item.nonceId || '');
    targetUsername = String(item.username || '');
    targetMediaType = String(item.mediaType || '');
    targetUrl = String(item.url || '');
    targetThumbUrl = String(item.thumbUrl || '');
    targetThumbUrlToken = String(item.thumbUrlToken || '');
    targetDescription = String(item.description || '');
    targetVideoPlayLen = String(item.videoPlayLen || '');
    targetNickname = String(item.nickname || '');
    targetHeadUrl = String(item.headUrl || '');
    targetWidth = String(item.width || '');
    targetHeight = String(item.height || '');
    addLog('选择目标', '—', 'INFO', `id=${targetId} ${targetDescription?.slice(0, 30)}`);
  }

  function clearLogs() { logs = []; }
</script>

<div class="wa-mod">
  <div class="wa-mod-split">
    <!-- ═══ 左侧：Tab + 操作 ═══ -->
    <div class="wa-mod-left">
      <div class="wa-tabs">
        <button class="wa-tab" class:active={activeTab === 'identity'} onclick={() => activeTab = 'identity'}>身份</button>
        <button class="wa-tab" class:active={activeTab === 'search'} onclick={() => activeTab = 'search'}>搜索</button>
        <button class="wa-tab" class:active={activeTab === 'interact'} onclick={() => activeTab = 'interact'}>互动</button>
        <button class="wa-tab" class:active={activeTab === 'follow'} onclick={() => activeTab = 'follow'}>关注</button>
        <button class="wa-tab" class:active={activeTab === 'publish'} onclick={() => activeTab = 'publish'}>发布</button>
        <button class="wa-tab" class:active={activeTab === 'dm'} onclick={() => activeTab = 'dm'}>私信</button>
        <button class="wa-tab" class:active={activeTab === 'scan'} onclick={() => activeTab = 'scan'}>扫码</button>
        <button class="wa-tab" class:active={activeTab === 'profile'} onclick={() => activeTab = 'profile'}>资料</button>
      </div>

      {#if activeTab === 'identity'}
        <div class="wa-card">
          <h3 class="wa-card-title">身份基线</h3>
          <div class="wa-actions" style="margin-top:0">
            <button class="wa-btn wa-btn-primary" onclick={getProfile} disabled={isBusy}>获取视频号资料</button>
          </div>
          {#if myUserName}<p class="wa-info">当前 username: {myUserName}</p>{/if}
        </div>
        <div class="wa-card">
          <h3 class="wa-card-title">创建视频号</h3>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">昵称 *</span><input type="text" bind:value={finderNick} /></label>
            <label class="wa-field"><span class="wa-label">头像 URL</span><input type="text" bind:value={finderHeadImg} /></label>
            <label class="wa-field"><span class="wa-label">签名</span><input type="text" bind:value={finderSignature} /></label>
            <label class="wa-field"><span class="wa-label">性别</span><select bind:value={finderSex}><option value="0">未知</option><option value="1">男</option><option value="2">女</option></select></label>
          </div>
          <div class="wa-actions"><button class="wa-btn" onclick={createFinder} disabled={isBusy}>创建</button></div>
        </div>
        <div class="wa-card">
          <h3 class="wa-card-title">扫码登录助手</h3>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">qrContent *</span><input type="text" bind:value={scanQrContent} placeholder="助手官方二维码解析内容" /></label>
            <label class="wa-field"><span class="wa-label">username（空=管理员）</span><input type="text" bind:value={scanUsername} /></label>
          </div>
          <div class="wa-actions"><button class="wa-btn" onclick={scanLoginChannels} disabled={isBusy}>扫码登录</button></div>
        </div>

      {:else if activeTab === 'search'}
        <div class="wa-card">
          <h3 class="wa-card-title">搜索视频号</h3>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">搜索内容 *</span><input type="text" bind:value={searchContent} /></label>
            <div class="wa-row-2">
              <label class="wa-field"><span class="wa-label">category</span><input type="number" bind:value={searchCategory} /></label>
              <label class="wa-field"><span class="wa-label">filter</span><input type="number" bind:value={searchFilter} /></label>
            </div>
          </div>
          <div class="wa-actions">
            <button class="wa-btn wa-btn-primary" onclick={() => doSearch(false)} disabled={isBusy}>搜索首屏</button>
            <button class="wa-btn" onclick={() => doSearch(true)} disabled={isBusy || !canContinue}>加载下一页</button>
          </div>
          <p class="wa-cursor">page={searchPage} offset={searchOffset} continue={canContinue ? '是' : '否'}</p>
        </div>

        <!-- 目标预填 -->
        <div class="wa-card">
          <h3 class="wa-card-title">当前目标</h3>
          {#if targetId}
            <div class="wa-target-box">
              <p><strong>id:</strong> {targetId}</p>
              <p><strong>username:</strong> {targetUsername}</p>
              <p><strong>描述:</strong> {targetDescription?.slice(0, 60) || '—'}</p>
            </div>
          {:else}
            <p class="wa-hint">从搜索结果中选择一条以预填目标</p>
          {/if}
        </div>

        <!-- 搜索结果 -->
        <div class="wa-card wa-card-fill">
          <h3 class="wa-card-title">搜索结果 ({searchResults.length})</h3>
          <div class="wa-result-list">
            {#each searchResults as item, i}
              <button class="wa-result-item" onclick={() => fillTarget(item)}>
                <span class="wa-result-idx">#{i + 1}</span>
                <span class="wa-result-desc">{String(item.description || '').slice(0, 60) || '无描述'}</span>
              </button>
            {:else}
              <p class="wa-empty-hint">暂无结果</p>
            {/each}
          </div>
        </div>

      {:else if activeTab === 'interact'}
        <div class="wa-card">
          <h3 class="wa-card-title">互动操作</h3>
          <div class="wa-actions" style="margin-top:0">
            <button class="wa-btn" onclick={doBrowse} disabled={isBusy || !targetId}>浏览</button>
            <button class="wa-btn" onclick={doLike} disabled={isBusy || !targetId}>点赞</button>
            <button class="wa-btn" onclick={doFav} disabled={isBusy || !targetId}>收藏</button>
          </div>
        </div>
        <div class="wa-card">
          <h3 class="wa-card-title">分享</h3>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">分享给好友 wxid</span><input type="text" bind:value={dmToUserName} /></label>
          </div>
          <div class="wa-actions">
            <button class="wa-btn" onclick={() => sendToFriend(dmToUserName)} disabled={isBusy || !targetId}>分享给好友</button>
            <button class="wa-btn" onclick={sendToSns} disabled={isBusy || !targetId}>分享到朋友圈</button>
          </div>
        </div>
        <div class="wa-card">
          <h3 class="wa-card-title">评论</h3>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">评论内容</span><input type="text" bind:value={commentContent} /></label>
          </div>
          <div class="wa-actions">
            <button class="wa-btn" onclick={doComment} disabled={isBusy || !targetId}>发表评论</button>
            <button class="wa-btn" onclick={getCommentList} disabled={isBusy || !targetId}>评论列表</button>
          </div>
          {#if commentListResults.length}
            <p class="wa-info">评论 ({commentListResults.length}):</p>
            {#each commentListResults.slice(0, 10) as c}
              <p class="wa-comment-item">{String(c.content || c.comment || '')}</p>
            {/each}
          {/if}
        </div>
        <div class="wa-card">
          <h3 class="wa-card-title">用户主页</h3>
          <div class="wa-actions" style="margin-top:0">
            <button class="wa-btn" onclick={getUserPage} disabled={isBusy || !targetUsername}>拉取主页</button>
          </div>
        </div>

      {:else if activeTab === 'follow'}
        <div class="wa-card">
          <h3 class="wa-card-title">关注操作</h3>
          <div class="wa-form-grid">
            <label class="wa-field">
              <span class="wa-label">操作类型</span>
              <select bind:value={followOperType}><option value="1">关注</option><option value="2">取关</option></select>
            </label>
          </div>
          <div class="wa-actions">
            <button class="wa-btn wa-btn-primary" onclick={doFollow} disabled={isBusy || !targetUsername}>执行关注</button>
            <button class="wa-btn" onclick={getFollowList} disabled={isBusy}>关注列表</button>
            <button class="wa-btn" onclick={getLikeFavList} disabled={isBusy}>赞与收藏</button>
          </div>
        </div>

      {:else if activeTab === 'publish'}
        <div class="wa-card">
          <h3 class="wa-card-title">发布视频（简易）</h3>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">标题</span><input type="text" bind:value={pubTitle} /></label>
            <label class="wa-field"><span class="wa-label">videoUrl *</span><input type="text" bind:value={pubVideoUrl} /></label>
            <label class="wa-field"><span class="wa-label">thumbUrl</span><input type="text" bind:value={pubThumbUrl} /></label>
            <label class="wa-field"><span class="wa-label">description *</span><textarea bind:value={pubDescription} rows="2"></textarea></label>
            <label class="wa-field"><span class="wa-label">topic</span><input type="text" bind:value={pubTopic} /></label>
          </div>
          <div class="wa-actions">
            <button class="wa-btn wa-btn-primary" onclick={publishWeb} disabled={isBusy}>发布(Web)</button>
            <button class="wa-btn" onclick={publishDirect} disabled={isBusy}>发布(直传)</button>
          </div>
        </div>

        <div class="wa-card">
          <h3 class="wa-card-title">CDN 上传链路</h3>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">videoUrl *</span><input type="text" bind:value={cdnVideoUrl} /></label>
            <label class="wa-field"><span class="wa-label">coverImgUrl</span><input type="text" bind:value={cdnCoverUrl} /></label>
          </div>
          <div class="wa-actions">
            <button class="wa-btn" onclick={uploadCdn} disabled={isBusy}>CDN 同步上传</button>
            <button class="wa-btn" onclick={uploadCdnAsync} disabled={isBusy}>CDN 异步上传</button>
            <button class="wa-btn wa-btn-primary" onclick={publishCdn} disabled={isBusy || !cdnFileUrl}>CDN 发布</button>
          </div>
          {#if cdnFileUrl}
            <p class="wa-info">CDN 已就绪: {cdnFileUrl.slice(0, 40)}...</p>
          {/if}
        </div>

      {:else if activeTab === 'dm'}
        <div class="wa-card">
          <h3 class="wa-card-title">私信</h3>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">toUserName *</span><input type="text" bind:value={dmToUserName} /></label>
            <div class="wa-actions" style="margin-top:0">
              <button class="wa-btn" onclick={getMsgSession} disabled={isBusy}>获取会话ID</button>
            </div>
            {#if dmSessionId}<p class="wa-info">sessionId: {dmSessionId.slice(0, 20)}...</p>{/if}
            <label class="wa-field"><span class="wa-label">文本消息</span><input type="text" bind:value={dmContent} /></label>
            <div class="wa-actions" style="margin-top:0">
              <button class="wa-btn" onclick={sendDmText} disabled={isBusy}>发送文本</button>
            </div>
            <label class="wa-field"><span class="wa-label">图片 URL</span><input type="text" bind:value={dmImgUrl} /></label>
            <div class="wa-actions" style="margin-top:0">
              <button class="wa-btn" onclick={sendDmImg} disabled={isBusy}>发送图片</button>
            </div>
          </div>
        </div>

      {:else if activeTab === 'scan'}
        <div class="wa-card">
          <h3 class="wa-card-title">扫码类能力</h3>
          <p class="wa-warn">⚠ qrUrl 须来自真实扫码解析，严禁伪造</p>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">qrUrl *</span><input type="text" bind:value={scanQrUrl} /></label>
          </div>
          <div class="wa-actions">
            <button class="wa-btn" onclick={() => scanAction('/finder/scanFollow', '扫码关注')} disabled={isBusy}>扫码关注</button>
            <button class="wa-btn" onclick={() => scanAction('/finder/scanQrCode', '扫码解析')} disabled={isBusy}>扫码解析</button>
            <button class="wa-btn" onclick={() => scanAction('/finder/scanLike', '扫码点赞')} disabled={isBusy}>扫码点赞</button>
            <button class="wa-btn" onclick={() => scanAction('/finder/scanFav', '扫码收藏')} disabled={isBusy}>扫码收藏</button>
            <button class="wa-btn" onclick={() => scanAction('/finder/scanBrowse', '扫码浏览')} disabled={isBusy}>扫码浏览</button>
            <button class="wa-btn" onclick={() => scanAction('/finder/scanComment', '扫码评论')} disabled={isBusy}>扫码评论</button>
          </div>
        </div>

      {:else if activeTab === 'profile'}
        <div class="wa-card">
          <h3 class="wa-card-title">资料与二维码</h3>
          <div class="wa-form-grid">
            <label class="wa-field"><span class="wa-label">昵称</span><input type="text" bind:value={profileNick} /></label>
            <label class="wa-field"><span class="wa-label">头像 URL</span><input type="text" bind:value={profileHeadImg} /></label>
            <label class="wa-field"><span class="wa-label">签名</span><input type="text" bind:value={profileSignature} /></label>
          </div>
          <div class="wa-actions">
            <button class="wa-btn" onclick={updateFinderProfile} disabled={isBusy}>更新资料</button>
            <button class="wa-btn wa-btn-primary" onclick={getFinderQr} disabled={isBusy}>获取二维码</button>
          </div>
          {#if profileQrBase64}
            <div class="wa-qr-wrap"><img src={profileQrBase64} alt="视频号二维码" class="wa-qr-img" /></div>
          {/if}
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
  .wa-mod { height: 100%; display: flex; flex-direction: column; }
  .wa-mod-split { flex: 1; min-height: 0; display: flex; gap: 16px; }
  .wa-mod-left, .wa-mod-right { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 12px; overflow-y: auto; }
  .wa-card { background: var(--card); border: 1px solid var(--border); border-radius: var(--radius-lg); padding: 16px; }
  .wa-card-fill { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .wa-card-head { display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px; }
  .wa-card-title { font-size: 14px; font-weight: 600; margin: 0 0 12px; }
  .wa-card-head .wa-card-title { margin: 0; }
  .wa-hint { font-size: 12px; color: var(--muted-foreground); margin: 0 0 8px; }
  .wa-info { font-size: 12px; color: var(--primary); margin: 8px 0 0; }
  .wa-warn { font-size: 12px; color: var(--warning, #d97706); margin: 0 0 8px; }
  .wa-cursor { font-size: 11.5px; font-family: var(--font-mono); color: var(--muted-foreground); margin: 8px 0 0; }
  .wa-tabs { display: flex; gap: 2px; border-bottom: 1px solid var(--border); flex-wrap: wrap; }
  .wa-tab { padding: 6px 10px; border: none; background: none; font-size: 12px; cursor: pointer; color: var(--muted-foreground); border-bottom: 2px solid transparent; }
  .wa-tab.active { color: var(--primary); border-bottom-color: var(--primary); font-weight: 600; }
  .wa-form-grid { display: flex; flex-direction: column; gap: 10px; }
  .wa-field { display: flex; flex-direction: column; gap: 4px; }
  .wa-label { font-size: 12px; font-weight: 600; }
  .wa-field input, .wa-field select, .wa-field textarea { padding: 6px 10px; border: 1px solid var(--border); border-radius: 6px; background: var(--card); font-size: 13px; color: var(--foreground); }
  .wa-row-2 { display: flex; gap: 10px; }
  .wa-row-2 .wa-field { flex: 1; }
  .wa-actions { display: flex; gap: 8px; margin-top: 12px; flex-wrap: wrap; }
  .wa-btn { padding: 6px 14px; border: 1px solid var(--border); border-radius: 6px; background: var(--card); font-size: 13px; cursor: pointer; color: var(--foreground); }
  .wa-btn:hover { background: var(--muted); }
  .wa-btn-primary { background: var(--primary); color: var(--primary-foreground); border-color: var(--primary); }
  .wa-btn-sm { padding: 3px 8px; font-size: 11.5px; }
  .wa-btn:disabled { opacity: 0.4; cursor: default; pointer-events: none; }
  .wa-target-box { padding: 10px; border: 1px solid var(--border); border-radius: 8px; font-size: 12px; }
  .wa-target-box p { margin: 2px 0; }
  .wa-result-list { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 4px; max-height: 300px; }
  .wa-result-item { display: flex; align-items: center; gap: 8px; padding: 8px 10px; border: 1px solid var(--border); border-radius: 6px; background: none; cursor: pointer; text-align: left; font-size: 13px; color: var(--foreground); }
  .wa-result-item:hover { background: var(--muted); }
  .wa-result-idx { font-family: var(--font-mono); font-size: 11.5px; color: var(--muted-foreground); flex-shrink: 0; }
  .wa-result-desc { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .wa-comment-item { font-size: 12px; padding: 4px 0; border-bottom: 1px solid var(--border); margin: 0; }
  .wa-empty-hint { color: var(--muted-foreground); font-size: 13px; text-align: center; padding: 24px 0; }
  .wa-qr-wrap { margin-top: 10px; text-align: center; }
  .wa-qr-img { max-width: 180px; border-radius: 8px; }
  .wa-log-body { flex: 1; min-height: 0; overflow-y: auto; background: #1e1e1e; border-radius: 8px; padding: 10px; font-family: var(--font-mono); font-size: 11.5px; color: #a6e22e; }
  .wa-log-line { padding: 2px 0; white-space: pre-wrap; word-break: break-all; }
  .wa-log-empty { color: #888; }
</style>
