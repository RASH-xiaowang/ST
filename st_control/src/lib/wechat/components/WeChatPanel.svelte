<script lang="ts">
  import { onMount, onDestroy, tick, untrack } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { safeParseInt } from '../utils';
  import {
    checkupPct as calcCheckupPct,
    checkupRatePct as calcCheckupRatePct,
    countMissingChats,
    isKefuSession,
    isMiniAppKefuSession,
    miniAppPageUrl,
  } from '../utils/misc';
  import { estimateMsgHeight } from '../utils/virtualList';
  import {
  avatarLetter,
  cellText,
  cellTextSmart,
  colorFromName,
  errText,
  favFileSize,
  favIcon,
  fileIcon,
  fmtDur,
  fmtFileSize,
  iconSvg,
  ICON_PATHS,
  transferStatusLabel,
} from '../utils/format';
  import { toRealtimeMsg } from '../utils/realtimeMsg';
  import { createMsg } from '../../services/msg.svelte';
  import { buildStaticEmoticonMap, calHeat, collectSessionImages, editKey, filterByAnyKeyword, filterByKeyword, filterFavoriteItems, filterMainSessions, filterSettingsCats, filterSortCheckupChats, filterSortResourceFiles, filterStaticEmoticons, groupContactsByInitial, groupMembersByRoom, groupMomentsByDate, mergeMoments, selectedIdsFromRecord, sessionKeywordMatch, sessionMatchesKeyword, trimRecord, VIEWER_ZOOM_STEPS, zoomStepIndex } from '../utils/panel';
  import { upsertSessionOrdered } from '../utils/sessionOrder';
  import { avatarCache, preloadAvatars } from '../services/avatarQueue.svelte';
  import {
    enqueueImage,
    onImageLoadError,
    retryImage,
    clearAutoRetries as clearImageAutoRetries,
    imageQueueState,
  } from '../services/imageQueue.svelte';
  import {
    mediaApi,
    apiAssetUrl,
    loadMediaConfig as loadApiMediaConfig,
  } from '../services/mediaApi.svelte';
  import {
    momentMedia,
    momentImgKey,
    momentImgSrc,
    enqueueMomentImage,
    loadMomentOriginal,
  } from '../services/momentMedia.svelte';
  import {
    momentVideo,
    playMomentVideo,
    closeMomentVideo,
    handleVideoError,
  } from '../services/momentVideo.svelte';
  import { createWechatEventBus, type WechatEventBus } from '../events';
  import {
    batchExportSessions,
    buildWechatSearchIndex,
    clearAllSessionDrafts,
    clearSessionDraft,
    deleteConversationMessages,
    deleteFavoriteItems,
    editChatMessage,
    exportContactsCsv,
    exportFavoritesCsv,
    exportGeneralCategoryCsv,
    exportMoments,
    exportSessionMessages,
    exportWechatArchive,
    exportWechatMissingImagesCsv,
    getWechatSearchIndexStatus,
    getChatDailyCounts,
    getChatEditStatus,
    getContactProfile,
    getContacts,
    getContactsByCategory,
    getConversationMessages,
    getSessionMessageStats,
    getDbConfig,
    getEmoticons,
    getFavoriteDetail,
    getFavoriteImage,
    getFavoriteVoice,
    getFavorites,
    getGeneralSettings,
    getMessageImage,
    getMessageRawRow,
    getMessageVoice,
    getMoments,
    getMomentsInsights,
    getMonitorStatus,
    getOfficialAccounts,
    getResourceFiles,
    getSessionList,
    getStaticEmoticons,
    getWechatAccountStatus,
    getWechatDbStatus,
    getWechatMissingImages,
    listSessionEditedMessages,
    ocrIngestResource,
    openWechatAttachFolder,
    openWechatFolder,
    openWechatPath,
    openWechatProtocol,
    refreshWechatMoments,
    refreshWechatSessions,
    resetEditedMessage,
    resolveWechatFile,
    searchWechatMessages,
    setDbConfig,
    startMonitor as callStartMonitor,
    stopMonitor as callStopMonitor,
    switchWechatAccountToLive,
    transcribeMessageVoice,
    updateMessageRawFields,
  } from '../services/ipc';
  import type { MonitorStatus } from '../types';
  import type {
    EmoticonOverview,
    ExportResult,
    FavoriteEntry,
    ContactItem,
    FavoritesData,
    FavoriteDetail,
    GeneralCategory,
    MessagePage,
    MissingImagesData,
    MomentEntry,
    MomentsInsight,
    SessionMessageTypeStat,
    OfficialAccount,
    RichMedia,
    ResourceFile,
    ResourceFilesOverview,
    StaticEmoticonCategory,
    WeChatMessage,
    WeChatMessagePayload,
    WeChatOpProgress,
    WeChatSession,
    WechatAccountStatus,
  } from '../types';
  import AnnualSummary from './AnnualSummary.svelte';
  import type { WechatSearchHit } from '../../search/types';
  import DailySummary from './DailySummary.svelte';
  import DbStatusPopup from './DbStatusPopup.svelte';
  import MonitorControl from './MonitorControl.svelte';
  import VideoPlayerDialog from './VideoPlayerDialog.svelte';
  import { lsGet, lsSet } from '../../storage';
  import { copyText } from '../../clipboard';
  import { downloadBlob } from '../../download';
  import GeneralRecords from './GeneralRecords.svelte';
  import AskPanel from './AskPanel.svelte';
  import RelationshipGraph from './RelationshipGraph.svelte';
  import GroupMonitor from './GroupMonitor.svelte';
  import PrivacyScan from './PrivacyScan.svelte';
  import BackupManager from './BackupManager.svelte';
  import StorageSpace from './StorageSpace.svelte';
  import DataOverview from './DataOverview.svelte';
  import RevokedMessages from './RevokedMessages.svelte';
  import HookManager from './HookManager.svelte';
  import GargantuaBackdrop from './GargantuaBackdrop.svelte';
  import WechatHoverButton from './WechatHoverButton.svelte';
  import { type MessageRowCtx, type MessageRowActions } from './MessageRow.svelte';
  import MessageList from './MessageList.svelte';
  import WeChatConfig from './WeChatConfig.svelte';
  import WeChatSendDialog from '../../bot/WeChatSendDialog.svelte';

  // openConfigTick：外部（如微信启动页「去配置」）请求打开本面板的设置页
  let { msgCount = $bindable(0), openConfigTick = $bindable(0) } = $props();

  const imageCache = $derived(imageQueueState.cache);
  const apiMediaBlocked = $derived(imageQueueState.blocked);
  const apiMediaBase = $derived(mediaApi.mediaBase);
  const apiToken = $derived(mediaApi.token);
  const apiRoot = $derived(mediaApi.mediaBase ? mediaApi.mediaBase.replace(/\/media$/, '') : '');

  type Tab = 'overview' | 'ask' | 'graph' | 'monitor' | 'privacy' | 'backup' | 'revoked' | 'chats' | 'contacts' | 'moments' | 'favorites' | 'emoticons' | 'bizchats' | 'servicechats' | 'kefu' | 'files' | 'records' | 'settings' | 'annual' | 'dailysummary' | 'hook' | 'storage';
  let curTab = $state<Tab>('overview');

  // ── 左侧导航（顺序即产品规格，新增页签只需在此追加）──
  const NAV_GROUPS: { label: string; items: { tab: Tab; label: string; icon: string }[] }[] = [
    {
      label: '总览',
      items: [
        { tab: 'overview', label: '数据总览', icon: '<rect x="3" y="3" width="7" height="9" rx="1"/><rect x="14" y="3" width="7" height="5" rx="1"/><rect x="14" y="12" width="7" height="9" rx="1"/><rect x="3" y="16" width="7" height="5" rx="1"/>' },
      ],
    },
    {
      label: '会话',
      items: [
        { tab: 'chats', label: '聊天', icon: '<path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>' },
      ],
    },
    {
      label: '智能',
      items: [
        { tab: 'ask', label: 'AI 问答', icon: '<path d="M9 11a3 3 0 1 1 6 0c0 1.5-1.5 2-2 3h-2"/><circle cx="12" cy="12" r="10"/><line x1="12" y1="17.5" x2="12" y2="17.6"/>' },
        { tab: 'graph', label: '关系图谱', icon: '<circle cx="6" cy="6" r="3"/><circle cx="18" cy="6" r="3"/><circle cx="12" cy="18" r="3"/><line x1="8.5" y1="7.5" x2="10.5" y2="15.5"/><line x1="15.5" y1="7.5" x2="13.5" y2="15.5"/><line x1="6" y1="9" x2="6" y2="13"/><line x1="18" y1="9" x2="18" y2="13"/>' },
        { tab: 'monitor', label: '群监控', icon: '<path d="M22 12h-4l-3 9L9 3l-3 9H2"/>' },
      ],
    },
    {
      label: '数据',
      items: [
        { tab: 'contacts', label: '通讯录', icon: '<path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M23 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/>' },
        { tab: 'moments', label: '朋友圈', icon: '<rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/>' },
        { tab: 'favorites', label: '收藏', icon: '<polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/>' },
        { tab: 'emoticons', label: '表情', icon: '<circle cx="12" cy="12" r="10"/><circle cx="8" cy="10" r="1"/><circle cx="16" cy="10" r="1"/><path d="M8 15a4 4 0 0 0 8 0"/>' },
        { tab: 'files', label: '文件', icon: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/>' },
        { tab: 'records', label: '记录', icon: '<rect x="3" y="4" width="18" height="16" rx="2"/><line x1="7" y1="8" x2="17" y2="8"/><line x1="7" y1="12" x2="17" y2="12"/><line x1="7" y1="16" x2="13" y2="16"/>' },
        { tab: 'storage', label: '存储空间', icon: '<path d="M22 12H2"/><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/><line x1="6" y1="16" x2="6.01" y2="16"/><line x1="10" y1="16" x2="10.01" y2="16"/>' },
      ],
    },
    {
      label: '订阅',
      items: [
        { tab: 'bizchats', label: '公众号', icon: '<path d="M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z"/><polyline points="22,6 12,13 2,6"/>' },
        { tab: 'servicechats', label: '服务号', icon: '<path d="M3 18v-6a9 9 0 0 1 18 0v6"/><path d="M21 19a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3zM3 19a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2v-3a2 2 0 0 0-2-2H3z"/>' },
        { tab: 'kefu', label: '客服', icon: '<path d="M14 9a2 2 0 0 1-2 2H6l-4 4V4a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2z"/><path d="M18 9h2a2 2 0 0 1 2 2v11l-4-4h-6a2 2 0 0 1-2-2v-1"/>' },
      ],
    },
    {
      label: '总结',
      items: [
        { tab: 'annual', label: '年度总结', icon: '<polyline points="22 12 18 12 15 21 9 3 6 12 2 12"/>' },
        { tab: 'dailysummary', label: '每日总结', icon: '<rect x="3" y="4" width="18" height="17" rx="2"/><path d="M8 2v4M16 2v4M3 9h18"/><path d="M8 14h3M13 14h3M8 17h3M13 17h3"/>' },
      ],
    },
    {
      label: '安全',
      items: [
        { tab: 'hook', label: '原图Hook', icon: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><path d="M8 13h3M12 17H8M16 13h1M17 17h1"/>' },
        { tab: 'privacy', label: '隐私体检', icon: '<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/><path d="M9 12l2 2 4-4"/>' },
        { tab: 'revoked', label: '撤回记录', icon: '<path d="M3 7v6h6"/><path d="M21 17a9 9 0 0 0-15-6.7L3 13"/><path d="M21 3v6h-6"/>' },
        { tab: 'backup', label: '备份管家', icon: '<path d="M21 12a9 9 0 1 1-9-9"/><polyline points="21 3 21 9 15 9"/>' },
      ],
    },
  ];

  // ── 通用 ──
  let eventBus: WechatEventBus | null = null;
  let sessionsRefreshTimer: ReturnType<typeof setTimeout> | null = null;

  // ── 微信实时消息监控 ──
  let monitorStatus = $state<MonitorStatus>({ running: false, status: 'unknown' });
  let monitorLoading = $state(false);
  // dbStatus/dbStatusChecked 声明在后，须用 $derived.by 闭包避免 TDZ
  const monitorCanStart = $derived.by(
    () => dbStatusChecked && !dbStatus.some((s) => s.includes('未找到') || s.includes('失败')),
  );

  async function refreshMonitorStatus() {
    try {
    monitorStatus = await getMonitorStatus();
    } catch (e) {
      logError('refreshMonitorStatus', e);
    }
  }
  async function startMonitor() {
    if (monitorStatus.running || monitorLoading) return;
    monitorLoading = true;
    try {
    await callStartMonitor();
      await refreshMonitorStatus();
      // 监控启动时同步解密已完成，立即刷新会话列表和消息
      await refreshData();
      mgmt.show('实时消息监控已启动', true);
    } catch (e: unknown) {
      mgmt.show(`启动监控失败：${errText(e)}`, false);
      logError('startMonitor', e);
    } finally {
      monitorLoading = false;
    }
  }
  async function stopMonitor() {
    if (!monitorStatus.running || monitorLoading) return;
    monitorLoading = true;
    try {
    await callStopMonitor();
      await refreshMonitorStatus();
      mgmt.show('实时消息监控已停止', true);
    } catch (e: unknown) {
      mgmt.show(`停止监控失败：${errText(e)}`, false);
      logError('stopMonitor', e);
    } finally {
      monitorLoading = false;
    }
  }

  // 通用工具函数
  function logError(context: string, err: unknown) {
    console.error(`[WeChatPanel] ${context}:`, err);
  }
  function logDebug(context: string, data: unknown) {
    console.debug(`[WeChatPanel] ${context}:`, data);
  }

  // ── 头像加载队列（LRU + 并发受限 + 失败冷却重试）已下沉至 services/avatarQueue.svelte.ts ──

  // ── 大对象缓存上限（base64/data URL 体积大，必须裁剪，防止内存持续增长） ──
  const MAX_VOICE_CACHE = 50;        // 播放过的语音 data URL
  const MAX_VOICE_TEXT_CACHE = 200;  // 语音转写文本
  const MAX_FAV_IMAGE_CACHE = 120;   // 收藏图片 base64
  const MAX_FAV_VOICE_CACHE = 20;    // 收藏语音 data URL
  /** 点击朋友圈图片：打开查看器，并异步拉取 /0 原图 */
  async function openMomentViewer(m: MomentEntry, idx: number) {
    if (!m?.images?.length) return;
    const list = m.images.map((img) => ({
      src: momentImgSrc(img),
      time: m.time,
      _media: img,
    }));
    if (!list.some((i) => i.src)) return;
    viewerImages = list;
    viewerIndex = Math.max(0, Math.min(idx, list.length - 1));
    resetViewerTransform();
    viewerOpen = true;

    // 查看器优先展示原图（/0）：异步下载解密后替换当前图源
    const img = m.images[viewerIndex];
    if (!img?.url) return;
    const data = await loadMomentOriginal(img);
    if (viewerOpen && viewerIndex < viewerImages.length && data) {
      viewerImages[viewerIndex].src = data;
    }
  }

  /** 朋友圈列表可视时自动预加载缩略图 */
  $effect(() => {
    const items = filteredMoments;
    if (!items?.length) return;
    for (const m of items) {
      for (const img of m?.images || []) enqueueMomentImage(img);
      // 视频封面（vweixinthumb 图片）复用图片解密管线
      for (const v of m?.videos || []) {
        if (v.thumb_is_image && v.thumb) {
          enqueueMomentImage({ thumb: v.thumb, url: v.thumb, key: v.key || '' });
        }
      }
    }
  });

  /** 对当前会话中的图片消息按需触发后端解密（懒加载 + 去重 + LRU 上限） */
  function preloadMessageImages() {
    if (!curSession) return;
    // 只遍历可视窗口：虚拟滚动下 messages 可能多达 1500 条，不能全量预加载
    for (const m of msgVisibleWindow) {
      if (m.type !== 3 || m.image_url) continue;
      enqueueImage(curSession, m.local_id);
    }
  }

  // 消息列表/可视窗口变化时，自动为尚未解密的图片消息补触发懒加载
  $effect(() => {
    // 读取 msgVisibleWindow 建立响应式依赖
    msgVisibleWindow;
    preloadMessageImages();
  });

  /** 系统浏览器打开外部链接（新闻卡片/链接消息点击） */
  async function openUrl(url?: string | null) {
    // 防御：个别解析路径可能残留 CDATA 包装（<![CDATA[...]]>）
    const clean = (url || '').replace(/<!\[CDATA\[/g, '').replace(/\]\]>/g, '').trim();
    if (!clean) return;
    try {
      const { open } = await import('@tauri-apps/plugin-shell');
      await open(clean);
    } catch (e) {
      logError('openUrl', e);
    }
  }

  /** 点击文件卡片：能打开就打开（系统默认应用），打不开则打开所在目录 */
  async function openFileMsg(m: WeChatMessage, _r: WeChatMessage['rich']) {
    const username = m.username || curSession;
    const key = `${username}:${m.local_id}`;
    if (!username || fileOpening[key]) return;
    fileOpening[key] = true;
    try {
    const res = await resolveWechatFile(username, m.local_id);
      const target = res?.path;
      if (!target) {
        mgmt.show('未找到文件存储位置', false);
        return;
      }
      try {
      await openWechatPath({ path: target });
        if (!res.found) mgmt.show('未找到原文件，已打开存储目录', false);
      } catch {
        // 文件本身打不开（无默认应用/文件损坏）→ 打开所在目录
        const dir = res?.dir;
        if (dir) {
      await openWechatPath({ path: dir });
          mgmt.show('文件无法直接打开，已打开所在目录', false);
        } else {
          mgmt.show('文件打开失败', false);
        }
      }
    } catch (e: unknown) {
      logError('openFileMsg', e);
      mgmt.show(`打开文件失败：${errText(e)}`, false);
    } finally {
      fileOpening[key] = false;
    }
  }

  /** 直接打开文件所在目录（卡片右上文件夹图标） */
  async function openFileDir(m: WeChatMessage) {
    const username = m.username || curSession;
    const key = `${username}:${m.local_id}`;
    if (!username || fileOpening[key]) return;
    fileOpening[key] = true;
    try {
    const res = await resolveWechatFile(username, m.local_id);
      const dir = res?.dir;
      if (dir) {
    await openWechatPath({ path: dir });
      } else {
        mgmt.show('未找到存储目录', false);
      }
    } catch (e) {
      logError('openFileDir', e);
      mgmt.show('打开目录失败', false);
    } finally {
      fileOpening[key] = false;
    }
  }

  /** 点击小程序卡片：https 链接走浏览器；weixin:// 等协议链接交给系统/微信打开 */
  async function openMiniApp(r: WeChatMessage['rich']) {
    const url = r?.url;
    try {
      if (!url) {
        // 部分分享（腾讯文档/金山文档）把真实网页链接藏在 pagepath 的 url= 参数里
        const pageUrl = miniAppPageUrl(r);
        if (pageUrl) {
          await openUrl(pageUrl);
          return;
        }
        // 真没有外链：弹出详情，说明只能在微信内打开
        miniappDetail = r ?? null;
        return;
      }
      if (/^https?:\/\//i.test(url)) {
        // 微信返回的“小程序版本过低/不可用”错误页，浏览器打开没有意义
        if (/waerrpage/i.test(url)) {
          mgmt.show('该小程序版本过低或已不可用，无法打开', false);
          return;
        }
        await openUrl(url);
      } else if (/^(weixin|wxaurl):\/\//i.test(url)) {
    await openWechatProtocol(url);
        mgmt.show('已尝试通过微信打开小程序', true);
      } else {
        mgmt.show('暂不支持打开该链接', false);
      }
    } catch (e: unknown) {
      logError('openMiniApp', e);
      mgmt.show(`打开小程序失败：${errText(e)}`, false);
    }
  }

  async function copyMiniAppInfo(r: WeChatMessage['rich']) {
    const lines = [
      `小程序：${r?.title || ''}`,
      r?.des ? `说明：${r.des}` : '',
      r?.source ? `来源：${r.source}` : '',
      r?.appid ? `AppID：${r.appid}` : '',
      r?.pagepath ? `页面路径：${r.pagepath}` : '',
      r?.url ? `链接：${r.url}` : '',
    ].filter(Boolean);
    const text = lines.join('\n');
    const ok = await copyText(text);
    mgmt.show(ok ? '已复制小程序信息' : '复制失败', ok);
  }


  // ── 会话 ──
  let sessions = $state<WeChatSession[]>([]);
  let sessionMap = $state<Map<string, WeChatSession>>(new Map());
  let sessionsLoading = $state(false);
  let sessionsError = $state('');
  let panelLoading = $state(true);      // 面板首次加载中
let curSession = $state<string | null>(null);
let curSessionInfo = $state<WeChatSession | null>(null);
let sendDialogOpen = $state(false);
  let messages = $state<WeChatMessage[]>([]);
  let selfUsername = $state('');      // 本机 wxid，用于实时消息判断 is_self
  let msgsLoading = $state(false);
  let nextCursor = $state<number | null>(null);   // 游标分页：下一页起点 sort_seq
  let msgTotal = $state(0);
  /** 会话消息构成统计（文字/图片/语音/文件…条数，头部画像展示） */
  let msgTypeStats = $state<SessionMessageTypeStat[]>([]);
  async function loadMsgTypeStats(username: string) {
    try {
      const stats = await getSessionMessageStats(username);
      if (curSession === username) msgTypeStats = stats ?? [];
    } catch {
      // 统计是附加信息：失败静默，不影响消息浏览
      if (curSession === username) msgTypeStats = [];
    }
  }
  let hasMoreMsgs = $state(false);
  let msgsError = $state('');       // 消息加载错误
  const PAGE_SIZE = 10;             // 每页条数（懒加载）
  /** 消息加载请求序号：切换会话/刷新时递增，过期响应直接丢弃，
   *  防止快速切换会话时旧请求的结果覆盖新会话内容 */
  let msgReqSeq = 0;
  let dbStatus = $state<string[]>([]);  // 数据库状态摘要
  let showDbStatus = $state(false);      // 是否显示数据库状态弹窗
  let dbStatusLoading = $state(false);   // DB状态检查中
  let dbStatusChecked = $state(false);   // 是否已检查过

  // ── 图片缺失体检 ──
  let missingImagesOpen = $state(false);
  let missingImagesLoading = $state(false);
  let missingImagesData = $state<MissingImagesData | null>(null);
  let missingExporting = $state(false);
  // ── 账号一致性（数据来源 vs 当前登录）──
  let accountStatus = $state<WechatAccountStatus | null>(null);

  async function loadAccountStatus() {
    try {
      accountStatus = await getWechatAccountStatus();
    } catch { /* 忽略 */ }
  }

  // ── 一键切换到当前登录账号并重新获取密钥 ──
  let switchingAccount = $state(false);
  let switchAccountMsg = $state('');
  async function switchToLiveAccount() {
    if (switchingAccount) return;
    switchingAccount = true;
    switchAccountMsg = '正在切换到当前登录账号并获取密钥（可能需要 1-3 分钟）…';
    try {
      const r = await switchWechatAccountToLive(240000);
      if (r?.switched) {
        switchAccountMsg = r?.db_key_error
          ? `已切换到 ${r.live_account ?? ''}，但密钥获取失败：${r.db_key_error}`
          : `已切换到 ${r.live_account ?? ''} 并获取密钥`;
      } else {
        switchAccountMsg = '当前已是登录账号，无需切换';
      }
      if (r?.monitor_error) {
        switchAccountMsg += `（监控重启失败：${r.monitor_error}）`;
      }
      await Promise.all([loadAccountStatus(), loadSessions()]);
      // 若当前选中了会话，重新拉取其消息（数据源已切换）
      if (curSession) {
        await selectSession(curSession);
      }
    } catch (e: unknown) {
      switchAccountMsg = `切换失败：${errText(e) || '未知错误'}`;
      logError('switchToLiveAccount', e);
    } finally {
      switchingAccount = false;
      setTimeout(() => {
        if (!switchingAccount) switchAccountMsg = '';
      }, 6000);
    }
  }

  async function loadMissingImages() {
    missingImagesLoading = true;
    try {
      missingImagesData = await getWechatMissingImages();
    } catch (e: unknown) {
      missingImagesData = null;
      mgmt.show(`图片体检失败: ${errText(e)}`, false);
    } finally {
      missingImagesLoading = false;
    }
  }

  function openMissingImages() {
    missingImagesOpen = true;
    if (!missingImagesData) loadMissingImages();
  }

  async function exportMissingImages() {
    if (missingExporting) return;
    missingExporting = true;
    try {
      const r = await exportWechatMissingImagesCsv();
      mgmt.show(`已导出 ${r.count} 条缺失图片清单 → ${r.filename}`, true);
    } catch (e: unknown) {
      mgmt.show(`导出失败: ${errText(e)}`, false);
    } finally {
      missingExporting = false;
    }
  }

  /** 缺失图统计占比（total 来自当前统计快照） */
  function checkupPct(n: number): string {
    return calcCheckupPct(n, missingImagesData?.total_images ?? 0);
  }

  /** 单个会话缺失图占比 */
  function checkupRatePct(c: { missing?: number; total_images?: number }): string {
    return calcCheckupRatePct(c?.missing ?? 0, c?.total_images ?? 0);
  }

  // ── 图片体检：筛选 / 排序 / 占比（仅前端派生，不改后端） ──
  let checkupQuery = $state('');
  let checkupOnlyMissing = $state(false);
  let checkupSort = $state<'missing' | 'total' | 'name'>('missing');

  const checkupChats = $derived(
    filterSortCheckupChats(missingImagesData?.chats ?? [], {
      q: checkupQuery,
      onlyMissing: checkupOnlyMissing,
      sort: checkupSort,
    })
  );

  const checkupMissingChats = $derived(
    countMissingChats(missingImagesData?.chats ?? []),
  );

  let searchText = $state('');
  let refreshing = $state(false);

  // ── 消息列表：虚拟滚动 / 估算高度 / 滚动位置已下沉 MessageList.svelte ──
  /** 消息列表组件引用（bind:this）：metrics 与滚动方法经此调用 */
  let msgListRef = $state<MessageList | null>(null);
  /** 可视窗口快照（MessageList onVisibleChange 回填），用于图片懒加载预检 */
  let msgVisibleWindow = $state<WeChatMessage[]>([]);

  // ── 消息行渲染上下文（MessageRow.svelte）：只读状态分组注入 ──
  const isOfficialChat = $derived((curSession || '').startsWith('gh_'));
  // $derived.by 惰性求值：rowCtx 引用了后置声明的 $state（voiceMap/videoPlaying 等），
  // 与 visRange 同理，首次读取（渲染期）时所有声明已就绪，避免 TDZ。
  const rowCtx: MessageRowCtx = $derived.by(() => ({
    // 消息列表仅在 curSession 非空时渲染，此处取 ?? '' 与原文模板收窄语义一致
    curSession: curSession ?? '',
    isOfficialChat,
    curSessionInfo,
    avatarCache,
    staticEmoticonMap,
    imageCache,
    imageFailedReasons: imageQueueState.failedReasons,
    apiMediaBlocked,
    apiMediaBase,
    apiToken,
    fileOpening,
    voiceLoadingKey,
    voiceMap,
    voiceText,
    voiceTextFailed,
    voiceTranscribing,
    videoPlaying,
    videoMissing,
    videoCoverFail,
    editedSet,
  }));
  /** 消息行交互回调（MessageRow.svelte）：业务逻辑仍归属本组件 */
  const rowActions: MessageRowActions = {
    onContextMenu: (e, m) => openEditMenu(e, m),
    openImage: (m) => openImageViewer(m),
    onImageError: (m) => onImageLoadError(curSession, m.local_id),
    retryImage: (m) => retryImage(curSession, m.local_id),
    openUrl: (url) => openUrl(url),
    openFile: (m, r) => openFileMsg(m, r),
    openFileDir: (m) => openFileDir(m),
    openMiniApp: (r) => openMiniApp(r),
    playVoice: (u, id, key) => playVoice(u, id, key),
    transcribeVoice: (u, id, key) => transcribeVoice(u, id, key),
    onVoiceEnded: (key) => { voiceMap[key] = ''; },
    playVideo: (key) => { videoPlaying[key] = true; },
    onVideoEnded: (key) => { videoPlaying[key] = false; },
    onVideoError: (key) => { videoPlaying[key] = false; videoMissing[key] = true; },
    onCoverFail: (key) => { videoCoverFail[key] = true; },
  };

  /** 以下辅助函数保证 messages 与 MessageList 内部估算高度始终对齐 */
  function setMessages(next: WeChatMessage[]) {
    messages = msgListRef?.setMessages(next) ?? next;
  }
  function appendMessages(extra: WeChatMessage[]) {
    messages = msgListRef?.appendMessages([...messages, ...extra], extra) ?? [...messages, ...extra];
  }
  function prependMessages(extra: WeChatMessage[]) {
    messages = msgListRef?.prependMessages([...extra, ...messages], extra) ?? [...extra, ...messages];
  }
  function clearMessages() {
    messages = [];
    msgListRef?.clearMessages();
  }

  // 会话列表只保留群聊与单聊（好友/企业微信）；公众号/服务号在独立页签查看
  let filteredSessions = $derived(filterMainSessions(sessions, searchText));
  /** 主列表统计：好友数 / 群聊数 / 未读合计（会话侧栏信息密度） */
  const chatListStats = $derived.by(() => {
    const base = filterMainSessions(sessions ?? [], '');
    let friends = 0;
    let groups = 0;
    let unread = 0;
    for (const s of base) {
      if (s.is_group || (s.username ?? '').endsWith('@chatroom')) groups += 1;
      else friends += 1;
      unread += s.unread_count ?? 0;
    }
    return { friends, groups, unread };
  });
  /** 客服会话分组（客服消息 / 小程序客服消息） */
  const kefuSessions = $derived((sessions || []).filter((s: WeChatSession) => isKefuSession(s.username)));
  const miniappKefuSessions = $derived((sessions || []).filter((s: WeChatSession) => isMiniAppKefuSession(s.username)));
  let kefuSearch = $state('');
  const kefuSearchMatch = (s: WeChatSession) => sessionMatchesKeyword(s, kefuSearch);
  const kefuSessionsFiltered = $derived(kefuSessions.filter(kefuSearchMatch));
  const miniappKefuSessionsFiltered = $derived(miniappKefuSessions.filter(kefuSearchMatch));
  /** 客服会话未读合计（头部统计展示） */
  const kefuUnread = $derived(
    [...kefuSessions, ...miniappKefuSessions].reduce((a, s) => a + (s.unread_count ?? 0), 0),
  );

  /** 置顶会话折叠状态（持久化到 localStorage，刷新/重进应用后保持） */
  function loadPinnedCollapsed(): boolean {
    return lsGet('wc_pinned_collapsed') === '1';
  }
  let pinnedCollapsed = $state(loadPinnedCollapsed());
  /** 置顶聊天：全部置顶会话（含公众号/客服/无消息记录），与真实微信一致 */
  const allPinnedSessions = $derived((sessions || []).filter((s: WeChatSession) => s.pinned));
  const pinnedSessions = $derived(allPinnedSessions.filter((s) => sessionKeywordMatch(s, searchText)));
  const normalSessions = $derived(filteredSessions.filter((s: WeChatSession) => !s.pinned));
  /** 实际展示的会话（批量全选口径）：置顶 + 普通 */
  const visibleSessions = $derived.by(() => {
    const out: WeChatSession[] = [];
    if (!pinnedCollapsed) out.push(...pinnedSessions);
    out.push(...normalSessions);
    return out;
  });
  function togglePinnedCollapsed() {
    pinnedCollapsed = !pinnedCollapsed;
    lsSet('wc_pinned_collapsed', pinnedCollapsed ? '1' : '0');
  }

  async function loadSessions() {
    sessionsLoading = true;
    try {
      const list = await Promise.race([
    getSessionList(),
        new Promise<never>((_, reject) =>
          setTimeout(() => reject(new Error('会话列表加载超时')), 10000)
        )
      ]);
      sessions = list ?? [];
      rebuildSessionMap(sessions);
      sessionsError = '';
    } catch (e: unknown) {
      sessionsError = errText(e) || '加载失败';
      logError('loadSessions', e);
    } finally {
      sessionsLoading = false;
      if (sessions.length) {
        preloadAvatars(sessions.map((s) => s.username).filter((u): u is string => !!u));
      }
    }
  }

  /** 从会话数组重建 Map，用于实时更新的 O(1) 查找 */
  function rebuildSessionMap(list: WeChatSession[]) {
    const next = new Map<string, WeChatSession>();
    for (const s of list) {
      if (s?.username) next.set(s.username, s);
    }
    sessionMap = next;
  }

  /** 事件驱动的会话列表刷新（120ms 尾部防抖）。
   *  消息爆发时把多次刷新合并为一次 IPC，既保证会话列表毫秒级更新，
   *  又避免每条消息都触发一次全量加载造成的卡顿。 */
  function scheduleSessionsRefresh() {
    if (sessionsRefreshTimer) clearTimeout(sessionsRefreshTimer);
    sessionsRefreshTimer = setTimeout(() => {
      sessionsRefreshTimer = null;
      loadSessions();
    }, 120);
  }

  // ── 清除会话草稿（仅写解密副本；微信源库草稿需在微信客户端清除） ──
  let clearingDraft = $state<string | null>(null);
  /** 用户已清除的草稿：username → 清除时的草稿文本。
   *  监控会不断用源库恢复旧草稿，这里按内容匹配隐藏残留，
   *  微信写入的新草稿（内容不同）仍会正常显示。持久化到 control.db。 */
  const CLEARED_DRAFTS_KEY = 'wechat_cleared_drafts';
  let clearedDrafts = $state<Record<string, string>>({});

  async function loadClearedDrafts() {
    try {
    const items = await getDbConfig();
      const raw = items.find((i) => i.key === CLEARED_DRAFTS_KEY)?.value;
      if (raw) {
        const parsed = JSON.parse(raw);
        if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
          clearedDrafts = parsed;
        }
      }
    } catch (e) {
      logError('loadClearedDrafts', e);
    }
  }

  async function saveClearedDrafts() {
    try {
      await setDbConfig(CLEARED_DRAFTS_KEY, JSON.stringify(clearedDrafts));
    } catch (e) {
      logError('saveClearedDrafts', e);
    }
  }

  async function clearDraft(s: WeChatSession) {
    if (clearingDraft) return;
    clearingDraft = s.username;
    try {
      const clearedText = s.draft || '';
    await clearSessionDraft(s.username);
      // 记录已清除的草稿内容：监控恢复的同一内容将被隐藏
      clearedDrafts = { ...clearedDrafts, [s.username]: clearedText };
      saveClearedDrafts();
      // 本地立即清除，避免等待全量刷新
      const updated = { ...s, draft: '' };
      sessionMap.set(s.username, updated);
      sessions = sessions.map((x) => (x.username === s.username ? updated : x));
      mgmt.show(`已清除「${s.name || s.username}」的草稿`, true);
      scheduleSessionsRefresh();
    } catch (e: unknown) {
      mgmt.show(`清除草稿失败：${errText(e)}`, false);
      logError('clearDraft', e);
    } finally {
      clearingDraft = null;
    }
  }

  /** 仍显示中的草稿会话数（已清除的记录不再计入） */
  let visibleDraftCount = $derived(
    sessions.filter((s: WeChatSession) => s.draft && clearedDrafts[s.username] !== s.draft).length,
  );
  let clearingAllDrafts = $state(false);

  /** 一键清除全部草稿：清解密副本 + 记录所有草稿为已清除（源库恢复的残留将隐藏） */
  async function clearAllDrafts() {
    if (clearingAllDrafts || visibleDraftCount === 0) return;
    if (!confirm(`确定清除全部 ${visibleDraftCount} 个会话草稿吗？（仅清除本机解密副本并隐藏显示）`)) return;
    clearingAllDrafts = true;
    try {
    const r = await clearAllSessionDrafts();
      const next: Record<string, string> = { ...clearedDrafts };
      for (const d of r?.drafts ?? []) {
        if (d?.username) next[d.username] = d.draft || '';
      }
      clearedDrafts = next;
      saveClearedDrafts();
      // 本地清空所有草稿
      const cleared = sessions.map((x: WeChatSession) => ({ ...x, draft: '' }));
      sessions = cleared;
      rebuildSessionMap(cleared);
      mgmt.show(`已清除 ${r?.updated ?? 0} 个会话草稿`, true);
      scheduleSessionsRefresh();
    } catch (e: unknown) {
      mgmt.show(`清除草稿失败：${errText(e)}`, false);
      logError('clearAllDrafts', e);
    } finally {
      clearingAllDrafts = false;
    }
  }

  /** 检查解密数据库可用状态 */
  async function checkDbStatus() {
    dbStatusLoading = true;
    try {
    dbStatus = await getWechatDbStatus();
      dbStatusChecked = true;
    } catch (e: unknown) {
      dbStatus = [`检查失败: ${errText(e)}`];
    } finally {
      dbStatusLoading = false;
    }
  }

  /** 手动刷新：强制重新解密 session.db → 刷新会话列表 + 当前会话消息 */
  async function refreshData() {
    if (refreshing) return;
    refreshing = true;
    try {
      // 刷新后重试自定义表情（API 可能刚启用/网络恢复）
      emoImgFailed = {};
      // 调用后端强制解密 + 返回最新会话列表（比普通 get_session_list 多一次解密）
      const list = await Promise.race([
    refreshWechatSessions(),
        new Promise<never>((_, reject) =>
          setTimeout(() => reject(new Error('刷新会话列表超时')), 10000)
        )
      ]);
      sessions = list ?? [];
      rebuildSessionMap(sessions);
      sessionsError = '';
      if (sessions.length) {
        preloadAvatars(sessions.map((s: WeChatSession) => s.username).filter((u): u is string => !!u));
      }

      // 如果当前有打开的会话，重新加载其消息
      if (curSession) {
        const seq = ++msgReqSeq;
        msgsLoading = true;
        // 记录阅读位置：刷新后恢复到原位置，避免把翻历史的人拽回底部
        const prevStick = msgListRef?.isStickToBottom() ?? true;
        const prevScroll = msgListRef?.getScrollTop() ?? 0;
        clearMessages();
        nextCursor = null;
        msgTotal = 0;
        hasMoreMsgs = false;
        msgsError = '';
        try {
          const r = await loadLatestMessages(curSession);
          if (seq !== msgReqSeq) return; // 期间切换了会话，丢弃过期响应
          setMessages(r?.messages ?? []);
          selfUsername = r?.self_username ?? '';
          msgTotal = r?.total ?? 0;
          hasMoreMsgs = r?.has_more ?? false;
          nextCursor = r?.next_cursor ?? null;
          msgCount = msgTotal;
          loadMsgTypeStats(curSession).catch((e) => logError('loadMsgTypeStats', e));
          if (curSessionInfo?.is_group) {
            preloadAvatars(messages.map((m: WeChatMessage) => m.sender_username).filter((u): u is string => !!u));
          }
          // 恢复阅读位置：原本贴底则贴底，否则按原滚动位置定位
          await msgListRef?.restorePosition(prevStick, prevScroll);
        } catch (e: unknown) {
          if (seq !== msgReqSeq) return;
          msgsError = errText(e) || '刷新消息失败';
          logError('refreshData messages', e);
        }
        if (seq === msgReqSeq) msgsLoading = false;
      }
      mgmt.show('已刷新', true);
    } catch (e) {
      logError('refreshData', e);
      mgmt.show('刷新失败', false);
    } finally {
      refreshing = false;
    }
  }

  /** 打开/关闭 DB 状态弹窗 */
  function toggleDbStatus() {
    showDbStatus = !showDbStatus;
    if (showDbStatus && !dbStatusChecked) {
      checkDbStatus();
    }
  }

  /** 点击弹窗外部关闭（排除 toggle 按钮，避免点开按钮后弹窗被立刻关掉） */
  function closeDbStatusPopup(e: MouseEvent) {
    const popup = document.querySelector('.wc-db-status-popup');
    const trigger = e.target as Element | null;
    // 点击 toggle 按钮本身（DB 状态）时跳过关闭，交由按钮自己的 onclick 处理 toggle
    if (trigger?.closest('.wc-db-status-btn')) return;
    if (popup && !popup.contains(e.target as Node)) {
      showDbStatus = false;
    }
  }

  /** 直接调用 Tauri IPC，不经过任何包装 */
  async function loadMessages(username: string, beforeSortSeq: number | null): Promise<MessagePage> {
    return getConversationMessages({
      username,
      page: 0,
      pageSize: PAGE_SIZE,
      beforeSortSeq,
    });
  }

  /** 加载最新一页消息；若整页都是被后端合并掉的转账状态更新行（返回空页），
   *  继续向前翻页直到拿到有内容的页或确认没有更早的历史。
   *  转账卡片永远位于发起行位置，空页本身没有可展示内容，跳过不会丢消息。 */
  async function loadLatestMessages(username: string): Promise<MessagePage> {
    let r = await loadMessages(username, null);
    let guard = 0;
    while ((r?.messages?.length ?? 0) === 0 && r?.has_more && guard < 10) {
      r = await loadMessages(username, r?.next_cursor ?? null);
      guard++;
    }
    return r;
  }

    /** 用实时推送内容直接更新会话列表，无需等待 120ms 后的 IPC 刷新。
     *  使用 sessionMap 实现 O(1) 查找，避免消息爆发时 O(n) 扫描。 */
    function mergeSessionUpdate(payload: WeChatMessagePayload) {
      const username = payload?.username;
      if (!username) return;
      const ts = Math.floor((payload.timestamp ?? 0) / 1_000_000);
      const existing = sessionMap.get(username);
      if (!existing) {
        // 新会话：触发一次 IPC 刷新补齐元数据
        scheduleSessionsRefresh();
        return;
      }
      if (ts < (existing.ts ?? 0)) return;

      const updated = {
        ...existing,
        ts,
        // 保留置顶优先级：置顶会话 sort_ts 大于消息时间，新消息不应把它降到普通位置
        sort_ts: Math.max(existing.sort_ts ?? 0, ts),
        summary: payload.content ?? existing.summary,
        last_msg_type: payload.msg_type ?? existing.last_msg_type,
        // 未读以微信 SessionTable 真实值为准：不在此手动 +1，
        // 120ms 后的全量刷新会带回真实未读数（避免已打开会话也虚增）
        unread_count: existing.unread_count ?? 0,
      } as WeChatSession;
      sessionMap.set(username, updated);
      // 局部重排：sessions 恒保持（置顶优先 + sort_ts 降序），消息爆发时只有被
      // 更新的会话需要移动位置（命中头部 O(1) / 二分插入 O(log n) / 追加）。
      sessions = upsertSessionOrdered(sessions, username, updated);
      preloadAvatars([username, payload.sender_username].filter((u): u is string => !!u));
    }

    /** 实时消息命中当前会话时，追加（而非重新加载替换），保留已加载的历史 */
    function appendRealtimeMessage(payload: WeChatMessagePayload) {
      if (!payload || !payload.username || payload.username !== curSession) return;
      const incoming = toRealtimeMsg(payload, selfUsername);
      // 转账状态更新行（paysubtype≠1，如已收款/已退还）单向出现，不新增气泡：
      // - 已存在同 transfer_id 的卡片 → 原地更新状态文案/颜色；
      // - 本地没有发起卡片（发起行不在已加载页）→ 忽略本次推送，卡片会在加载
      //   历史页时由后端合并逻辑出现在发起行时间位置（与微信客户端一致）。
      if (incoming.rich?.type === 'transfer' && incoming.rich?.transfer_id) {
        const ps = String(incoming.rich.paysubtype || '');
        if (ps !== '1' && ps !== '') {
          const tid = String(incoming.rich.transfer_id);
          const idx = messages.findIndex((m) =>
            m.rich?.type === 'transfer' && String(m.rich?.transfer_id) === tid
          );
          if (idx >= 0) {
            messages[idx] = {
              ...messages[idx],
              rich: {
                ...(messages[idx].rich ?? {}),
                paysubtype: ps,
                direction: transferStatusLabel(!!messages[idx].is_self, ps),
              } as WeChatMessage['rich'],
            };
          }
          return; // 状态更新行绝不作为新气泡出现
        }
      }
      // 去重：首轮监控可能回放近期消息，避免与已显示消息重复
      // 以 (local_id, sort_seq) 为准：跨分库时 local_id 可能重复；
      // fallback 摘要 local_id=0 且 sort_seq=0，不作为去重键
      if (incoming.local_id > 0) {
        const dupIdx = messages.findIndex((m: WeChatMessage) =>
          m.local_id === incoming.local_id &&
          (m.sort_seq === incoming.sort_seq || !m.sort_seq || !incoming.sort_seq)
        );
        if (dupIdx >= 0) {
          // 同一条消息的实时推送更新（如 fallback 摘要先到、DB 行后到）：
          // 原位替换，保留顺序与滚动位置，避免「同一条消息两个气泡 / 方向错乱」
          messages[dupIdx] = { ...messages[dupIdx], ...incoming };
          msgListRef?.updateEstimate(dupIdx, estimateMsgHeight(messages[dupIdx]));
          if (msgListRef?.isStickToBottom()) msgListRef?.scrollToBottom();
          return;
        }
      }
      if (incoming.local_id > 0 &&
          messages.some((m: WeChatMessage) => m.local_id === incoming.local_id && m.sort_seq === incoming.sort_seq)) return;
      appendMessages([incoming]);
      // 单聊与群聊都预载发送者头像：单聊实时消息的 sender_username 即会话对方
      preloadAvatars([incoming.sender_username].filter(Boolean));
      // 仅在用户本就位于底部时跟随吸底；正在翻历史时不打断阅读位置。
      // （msgTotal 保持数据库总条数语义，不被已加载数覆盖）
      if (msgListRef?.isStickToBottom()) {
        msgListRef?.scrollToBottom();
      }
    }

    /** 游标分页查询历史消息（向上滑动加载） */
  async function selectSession(s: WeChatSession | string) {
    const username = typeof s === 'string' ? s : s?.username;
    if (!username || username === curSession) return;
    // 公众号从通讯录直接进入时，提前加载“查看历史消息”链接（空消息时兜底显示）
    if (username.startsWith('gh_')) ensureOfficialHistory();
    const seq = ++msgReqSeq; // 新请求使所有在途旧请求过期
    curSession = username;
    clearImageAutoRetries();
    videoPlaying = {};
    videoCoverFail = {};
    videoMissing = {};
    voiceText = {};
    voiceTextFailed = {};
    voiceTranscribing = {};
    curSessionInfo = typeof s === 'string'
      ? (sessionMap.get(s) ?? {
          username: s,
          // 会话列表没有该会话（如通讯录点入）时，按 username 后缀兜底判断群聊
          is_group: s.endsWith('@chatroom') || s.includes('@im.chatroom'),
        })
      : (s ?? null);
    msgsLoading = true;
    clearMessages();
    nextCursor = null;
    msgTotal = 0;
    msgTypeStats = [];
    hasMoreMsgs = false;
    msgsError = '';
    try {
      const r = await loadLatestMessages(username);
      if (seq !== msgReqSeq) return; // 过期响应（已切换会话），丢弃
      setMessages(r?.messages ?? []);
      selfUsername = r?.self_username ?? '';
      msgTotal = r?.total ?? 0;
      hasMoreMsgs = r?.has_more ?? false;
      nextCursor = r?.next_cursor ?? null;
      msgCount = msgTotal;
      // 消息构成统计（文字/图片/语音…），后台拉取不阻塞消息展示
      loadMsgTypeStats(username).catch((e) => logError('loadMsgTypeStats', e));
      // 加载该会话已编辑消息标记（“已编辑”徽标）
      loadEditedSet(username).catch((e) => logError('loadEditedSet', e));
      if (curSessionInfo?.is_group) {
        preloadAvatars(messages.map((m: WeChatMessage) => m.sender_username).filter((u): u is string => !!u));
      }
      // 切换会话后强制贴底（虚拟滚动窗口覆盖到最后一条）
      msgListRef?.setStickToBottom(true);
      await tick();
      msgListRef?.scrollToBottom();
    } catch (e: unknown) {
      if (seq !== msgReqSeq) return;
      msgsError = errText(e) || '消息加载失败';
      logError('selectSession', e);
    }
    if (seq === msgReqSeq) msgsLoading = false;
  }

  let loadMsgsTimeoutId: ReturnType<typeof setTimeout> | null = null;

  // ── 导出弹窗 ──
  let exportOpen = $state(false);
  let exportFormat = $state<'txt' | 'csv' | 'html' | 'excel'>('txt');
  // ── 联系人资料卡（聊天窗口「资料」弹窗 / 通讯录右侧面板）──
  let profileOpen = $state(false);
  /** 通讯录内嵌模式：点击联系人在右侧内容区显示资料卡（非弹窗） */
  let inlineProfile = $state(false);
  let profileUsername = $state('');
  let profileData = $state<ContactItem | null>(null);
  let profileLoading = $state(false);
  // ── 语音播放 ──
  let voiceLoadingKey = $state<string>('');
  async function playVoice(username: string | null | undefined, localId: number, key: string) {
    if (!username) return; // 无会话则无法定位语音（IPC 参数要求非空 username）
    if (voiceLoadingKey) return;
    if (voiceMap[key]) {
      voiceMap[key] = ''; // 已加载 → 停止并清除
      return;
    }
    voiceLoadingKey = key;
    try {
    const r = await getMessageVoice({ username, localId });
      const url = r?.kind === 'data' && r.data ? r.data : '';
      voiceMap[key] = url;
      trimRecord(voiceMap, MAX_VOICE_CACHE);
    } catch (e: unknown) {
      logError('playVoice', e);
    } finally {
      voiceLoadingKey = '';
    }
  }
  let voiceMap = $state<Record<string, string>>({});
  // ── 语音转写 ──
  let voiceText = $state<Record<string, string>>({});
  let voiceTextFailed = $state<Record<string, boolean>>({});
  let voiceTranscribing = $state<Record<string, boolean>>({});
  // ── 文件消息打开状态（防止重复点击，显示“查找中…”）──
  let fileOpening = $state<Record<string, boolean>>({});
  // ── 小程序详情弹窗（无外链时展示）──
  let miniappDetail = $state<RichMedia | null>(null);
  async function transcribeVoice(username: string | null | undefined, localId: number, key: string) {
    if (voiceTranscribing[key] || (voiceText[key] && !voiceTextFailed[key])) return;
    voiceTranscribing[key] = true;
    voiceText[key] = '';
    voiceTextFailed[key] = false;
    try {
    const r = await transcribeMessageVoice({ username, localId });
      if (r?.kind === 'data' && r.data) {
        voiceText[key] = r.data;
      } else if (r?.kind === 'none') {
        voiceText[key] = '（语音内容暂不可用，请确认微信已下载该语音）';
        voiceTextFailed[key] = true;
      }
    } catch (e: unknown) {
      logError('transcribeVoice', e);
      const msg = errText(e);
      if (/未配置|无可用|未找到.*(模型|提供方)|not configured/i.test(msg)) {
        voiceText[key] = '（未配置可用的大模型，无法转写；可在 设置 → 大模型 中接入 SenseVoice/Whisper/TeleSpeechASR（如硅基流动）后重试）';
      } else {
        // 透出真实原因，避免「点击重试」反复失败却不知为何
        const short = msg.length > 100 ? `${msg.slice(0, 100)}…` : msg;
        voiceText[key] = `（转写失败：${short}。点击可重试）`;
      }
      voiceTextFailed[key] = true;
    } finally {
      voiceTranscribing[key] = false;
      trimRecord(voiceText, MAX_VOICE_TEXT_CACHE);
    }
  }
  // ── 视频播放 ──
  let videoPlaying = $state<Record<string, boolean>>({});
  let videoCoverFail = $state<Record<string, boolean>>({});
  let videoMissing = $state<Record<string, boolean>>({});
  // ── 收藏语音播放 ──
  let favVoiceMap = $state<Record<number, string>>({});
  async function playFavoriteVoice(serverId: number) {
    if (favVoiceMap[serverId]) {
      favVoiceMap[serverId] = '';
      return;
    }
    try {
    const r = await getFavoriteVoice({ serverId });
      if (r?.kind === 'data' && r.data) {
        favVoiceMap[serverId] = r.data;
        trimRecord(favVoiceMap, MAX_FAV_VOICE_CACHE);
      }
    } catch (e: unknown) {
      logError('playFavoriteVoice', e);
    }
  }
  // ── 收藏图片显示（按 md5 懒加载）──
  let favImageMap = $state<Record<string, string>>({});
  async function loadFavoriteImage(md5: string) {
    if (favImageMap[md5] || favImageMap[md5] === 'loading') return;
    favImageMap[md5] = 'loading';
    try {
    const r = await getFavoriteImage(md5);
      favImageMap[md5] = r?.kind === 'data' && r.data ? r.data : '';
      trimRecord(favImageMap, MAX_FAV_IMAGE_CACHE);
    } catch (e: unknown) {
      favImageMap[md5] = '';
      trimRecord(favImageMap, MAX_FAV_IMAGE_CACHE);
      logError('loadFavoriteImage', e);
    }
  }
  function lazyLoadFavImage(node: HTMLElement, md5: string) {
    const io = new IntersectionObserver((entries) => {
      if (entries[0]?.isIntersecting) {
        loadFavoriteImage(md5);
        io.disconnect();
      }
    });
    io.observe(node);
    return { destroy: () => io.disconnect() };
  }
  // ── 消息日历（每日消息数热力图）──
  let calendarOpen = $state(false);
  let calYear = $state(new Date().getFullYear());
  let calMonth = $state(new Date().getMonth() + 1);
  let calCounts = $state<Record<string, number>>({});
  let calLoading = $state(false);
  async function openCalendar() {
    calendarOpen = true;
    calLoading = true;
    calCounts = {};
    try {
    const r = await getChatDailyCounts({
        username: curSession,
        year: calYear,
        month: calMonth,
      });
      calCounts = r?.counts ?? {};
    } catch (e: unknown) {
      logError('openCalendar', e);
    } finally {
      calLoading = false;
    }
  }
  async function switchCalendarMonth(delta: number) {
    let m = calMonth + delta;
    let y = calYear;
    if (m < 1) { m = 12; y--; }
    if (m > 12) { m = 1; y++; }
    calMonth = m;
    calYear = y;
    await openCalendar();
  }
  function jumpToDay(day: number) {
    const target = `${calYear}-${String(calMonth).padStart(2, '0')}-${String(day).padStart(2, '0')}`;
    const idx = messages.findIndex((m: WeChatMessage) => String(m.time).startsWith(target));
    if (idx >= 0) {
      msgListRef?.scrollToIdx(idx);
      calendarOpen = false;
    }
  }
  const calFirstDow = $derived((new Date(calYear, calMonth - 1, 1).getDay() + 6) % 7);
  const calDays = $derived(new Date(calYear, calMonth, 0).getDate());
  // 日历统计：本月消息总量 / 活跃天数 / 日均 / 最活跃日（沟通量一眼可见）
  const calTotal = $derived(Object.values(calCounts).reduce((a, b) => a + b, 0));
  const calActiveDays = $derived(Object.values(calCounts).filter((n) => n > 0).length);
  const calAvg = $derived(calActiveDays > 0 ? Math.round(calTotal / calActiveDays) : 0);
  const calTop = $derived.by(() => {
    let best = 0;
    let bestDay = 0;
    for (const [k, v] of Object.entries(calCounts)) {
      const d = Number(k);
      if (Number.isFinite(d) && v > best) {
        best = v;
        bestDay = d;
      }
    }
    return best > 0 ? { day: bestDay, count: best } : null;
  });

  // 未选会话空状态背景：Gargantua 黑洞光线追踪（见 GargantuaBackdrop.svelte）
  async function openContactProfile(username?: string, seed?: ContactItem | null, inline = false) {
    const target = username || curSession;
    if (!target) return;
    profileUsername = target;
    // 通讯录内嵌模式：直接显示在右侧内容区；聊天头部「资料」保持弹窗
    inlineProfile = inline;
    profileOpen = !inline;
    if (seed) {
      // 列表条目已有完整数据（群成员数/群主/类型等）：立即上屏，避免加载闪烁
      profileData = { ...seed, username: seed.username || target };
      profileLoading = false;
    } else {
      profileLoading = true;
      profileData = null;
    }
    try {
      preloadAvatars([target]);
      const fetched = await getContactProfile(target);
      // 拉取结果只覆盖非空字段，避免把已上屏的种子数据冲掉
      if (fetched) {
        const rich = { ...(seed ?? {}) } as ContactItem;
        if (!rich.username) rich.username = target;
        for (const [k, v] of Object.entries(fetched)) {
          if (v !== '' && v != null) rich[k] = v;
        }
        profileData = rich;
      }
    } catch (e: unknown) {
      logError('openContactProfile', e);
    } finally {
      profileLoading = false;
    }
  }

  /** 记录跳转：打开会话并尝试定位到指定消息 */
  async function openRecordSession(username: string, localId?: number) {
    if (!username) return;
    if (curTab !== 'chats') curTab = 'chats';
    const s = sessionMap.get(username) ?? {
      username,
      is_group: username.endsWith('@chatroom') || username.includes('@im.chatroom'),
    };
    await selectSession(s);
    if (localId && messages.length) {
      const idx = messages.findIndex((m: WeChatMessage) => Number(m.local_id) === Number(localId));
      if (idx >= 0) msgListRef?.scrollToIdx(idx);
    }
  }
  let exportCount = $state(100);
  let exportCountCustom = $state(false);
  let exportLoading = $state(false);
  let exportError = $state('');
  let exportSuccess = $state<{path: string, count: number} | null>(null);

  /** 加载更多历史消息：仅负责数据获取与消息维护，滚动位置恢复由 MessageList.loadMore 处理。
   *  返回 false 表示会话已切换等场景，调用方不应恢复滚动位置。 */
  async function loadMoreMsgs(): Promise<boolean> {
    if (!curSession || !hasMoreMsgs || msgsLoading) return false;
    const sessionAtCall = curSession;
    const seq = msgReqSeq; // 与当前会话共享序号；切换会话后序号变化
    msgsLoading = true;
    msgsError = '';
    loadMsgsTimeoutId = setTimeout(() => {
      if (msgsLoading) {
        const tip = document.querySelector('.wc-msgs-loading-tip');
        if (tip) (tip as HTMLElement).textContent = '加载时间较长，请稍候…';
      }
    }, 3000);
    try {
      let r = await loadMessages(sessionAtCall, nextCursor);
      // 整页都是被后端合并掉的转账状态更新行时会返回空页：
      // 继续向前翻页，直到拿到有内容的页或确认没有更早历史
      //（转账卡片位于发起行位置，跳过空页不会丢消息）。
      let guard = 0;
      while ((r?.messages?.length ?? 0) === 0 && r?.has_more && guard < 10) {
        r = await loadMessages(sessionAtCall, r?.next_cursor ?? null);
        guard++;
      }
      if (seq !== msgReqSeq || sessionAtCall !== curSession) {
        return false; // 已切换会话：调用方不复位滚动位置
      }
      const older = r?.messages ?? [];
      if (older.length === 0) {
        hasMoreMsgs = false;
        nextCursor = null;
        return true;
      }
      prependMessages(older);
      hasMoreMsgs = r?.has_more ?? false;
      nextCursor = r?.next_cursor ?? null;
      msgCount = msgTotal;
      if (curSessionInfo?.is_group) {
        preloadAvatars(older.map((m: WeChatMessage) => m.sender_username).filter((u): u is string => !!u));
      }
      return true;
    } catch (e: unknown) {
      if (seq !== msgReqSeq) return false;
      msgsError = errText(e) || '加载更多消息失败';
      logError('loadMoreMsgs', e);
      return false;
    } finally {
      if (seq === msgReqSeq) {
        msgsLoading = false;
        if (loadMsgsTimeoutId) { clearTimeout(loadMsgsTimeoutId); loadMsgsTimeoutId = null; }
      }
    }
  }

  /** 导出消息记录 */
  async function doExport() {
    if (!curSession || exportLoading) return;
    exportError = '';
    exportSuccess = null;
    const count = safeParseInt(exportCount, 100, 1, 100000);
    exportCount = count;
    try {
      // 先弹出保存位置选择，用户取消则不做任何事
      const { save } = await import('@tauri-apps/plugin-dialog');
      const ext = exportFormat === 'html' ? 'html' : exportFormat === 'excel' ? 'xls' : exportFormat === 'csv' ? 'csv' : 'txt';
      const base = curSession.endsWith('@chatroom') ? curSession.slice(0, -9) : curSession;
      const path = await save({
        title: '导出消息记录',
        defaultPath: `${base.slice(0, 16)}_${count}.${ext}`,
        filters: ext === 'html'
          ? [{ name: 'HTML 报告', extensions: ['html'] }]
          : ext === 'xls'
            ? [{ name: 'Excel 表格', extensions: ['xls'] }]
          : ext === 'csv'
            ? [{ name: 'CSV 表格', extensions: ['csv'] }]
            : [{ name: '文本文件', extensions: ['txt'] }],
      });
      if (!path) return; // 用户取消
      exportLoading = true;
    const r = await exportSessionMessages(
        curSession,
        exportFormat,
        count,
        path,
      );
      // 成功 → 弹窗内联显示结果，不再用原生 alert
      exportSuccess = { path: r.path, count: r.count };
    } catch (e: unknown) {
      console.error('[export] 失败:', e);
      exportError = errText(e) || '导出失败';
      logError('doExport', e);
    } finally {
      exportLoading = false;
    }
  }

  // ═══════════════ 数据管理：批量导出 / 删除 / 搜索筛选 ═══════════════

  /** 全局操作结果轻提示（5 秒自动消失，收敛自本地 showMgmtMsg，T-291） */
  const mgmt = createMsg(5000);

  /** 通用无参导出命令（联系人/收藏 CSV） */
  let genericExporting = $state(false);
  const EXPORT_CSV_DEFAULTS: Record<string, { name: string; label: string }> = {
    export_contacts_csv: { name: 'contacts', label: '联系人' },
    export_favorites_csv: { name: 'favorites', label: '收藏' },
  };
  async function runExportCmd(cmd: string, label: string) {
    if (genericExporting) return;
    genericExporting = true;
    try {
      // 先弹出保存位置选择，用户取消则不做任何事
      const meta = EXPORT_CSV_DEFAULTS[cmd];
      const { save } = await import('@tauri-apps/plugin-dialog');
      const path = await save({
        title: `导出${label}为 CSV`,
        defaultPath: `${meta?.name ?? 'export'}_${Date.now()}.csv`,
        filters: [{ name: 'CSV 表格', extensions: ['csv'] }],
      });
      if (!path) return; // 用户取消
      const r = await ({ export_contacts_csv: exportContactsCsv, export_favorites_csv: exportFavoritesCsv } as Record<string, (path?: string) => Promise<ExportResult>>)[cmd](path);
      mgmt.show(`${label}导出成功：${r.count} 条 → ${r.path}`, true);
    } catch (e: unknown) {
      mgmt.show(`${label}导出失败：${errText(e)}`, false);
      logError(`runExportCmd ${cmd}`, e);
    } finally {
      genericExporting = false;
    }
  }

  // ── 朋友圈导出（多格式 + 当前作者过滤 + HTML 全资源）──
  let momentExportFormat = $state<'csv' | 'json' | 'txt' | 'html'>('csv');
  let momentExporting = $state(false);
  /** 导出朋友圈：格式可选 CSV/JSON/TXT/HTML；正在看某位好友时只导出 TA 的动态；
   *  HTML 格式会连同全部图片/视频资源一起导出 */
  async function runMomentsExport() {
    if (momentExporting || genericExporting) return;
    momentExporting = true;
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const ext = momentExportFormat;
      const author = momentAuthor;
      const safe = author
        ? (author.name || author.username).replace(/[\\/:*?"<>|]/g, '_').trim().slice(0, 24)
        : '';
      const base = author ? `moments_${safe || 'author'}` : 'moments_all';
      const path = await save({
        title: author ? `导出「${author.name}」的朋友圈` : '导出全部朋友圈',
        defaultPath: `${base}_${Date.now()}.${ext}`,
        filters: [{
          name: ext === 'csv'
            ? 'CSV 表格'
            : ext === 'json'
              ? 'JSON 数据'
              : ext === 'html'
                ? 'HTML 报告（含图片/视频资源）'
                : '文本文件',
          extensions: [ext],
        }],
      });
      if (!path) return; // 用户取消
      const r = await exportMoments({
        format: momentExportFormat,
        authorUsername: author?.username ?? null,
        path,
      });
      const mediaNote = ext === 'html' && (r.media ?? 0) > 0 ? `，含 ${r.media} 个图片/视频资源` : '';
      mgmt.show(`${author ? `「${author.name}」的` : ''}朋友圈导出成功：${r.count} 条${mediaNote} → ${r.path}`, true);
    } catch (e: unknown) {
      mgmt.show(`朋友圈导出失败：${errText(e)}`, false);
      logError('runMomentsExport', e);
    } finally {
      momentExporting = false;
    }
  }

  // ── 会话批量导出（多选模式）──
  let batchMode = $state(false);
  let selectedSessions = $state<Record<string, boolean>>({});
  let batchExporting = $state(false);
  let selectedSessionList = $derived(filteredSessions.filter((s: WeChatSession) => selectedSessions[s.username]));

  function toggleBatchMode() {
    batchMode = !batchMode;
    if (!batchMode) selectedSessions = {};
  }
  function toggleSelectSession(username: string) {
    selectedSessions = { ...selectedSessions, [username]: !selectedSessions[username] };
  }
  function selectAllFiltered() {
    const all: Record<string, boolean> = {};
    // 只全选当前可见的会话（置顶分组折叠时隐藏的会话不参与）
    for (const s of visibleSessions) all[s.username] = true;
    selectedSessions = all;
  }
  async function doBatchExport(format: 'txt' | 'csv') {
    const usernames = selectedSessionList.map((s: WeChatSession) => s.username).filter((u): u is string => !!u);
    if (!usernames.length) { mgmt.show('请至少选择一个会话', false); return; }
    if (batchExporting) return;
    try {
      // 先选择保存目录，用户取消则不做任何事
      const { open } = await import('@tauri-apps/plugin-dialog');
      const dir = await open({ directory: true, multiple: false, title: '选择批量导出保存目录' });
      if (typeof dir !== 'string' || !dir.trim()) return; // 用户取消
      batchExporting = true;
    const r = await batchExportSessions(usernames, format, dir);
      mgmt.show(`批量导出完成：${r.sessions} 个会话 / ${r.total_messages} 条消息 → ${r.dir}`, true);
      batchMode = false;
      selectedSessions = {};
    } catch (e: unknown) {
      mgmt.show(`批量导出失败：${errText(e)}`, false);
      logError('doBatchExport', e);
    } finally {
      batchExporting = false;
    }
  }

  // ── 清空当前会话聊天记录 ──
  let clearConfirmOpen = $state(false);
  let clearing = $state(false);
  async function doClearHistory() {
    if (!curSession || clearing) return;
    clearing = true;
    try {
    const r = await deleteConversationMessages(curSession);
      mgmt.show(`已清空本地聊天记录（${r.deleted} 条，仅删除本机副本）`, true);
      clearConfirmOpen = false;
      clearMessages();
      msgTotal = 0;
      msgCount = 0;
      hasMoreMsgs = false;
      nextCursor = null;
      loadSessions();
    } catch (e: unknown) {
      mgmt.show(`清空失败：${errText(e)}`, false);
      logError('doClearHistory', e);
    } finally {
      clearing = false;
    }
  }

  // ── 收藏：搜索 / 类型筛选 / 多选删除 ──
  let favSearch = $state('');
  let favType = $state<string>('all');
  let favSelectMode = $state(false);
  let favSelected = $state<Record<string, boolean>>({});
  let favDeleting = $state(false);

  let favTypes = $derived.by(() => {
    const set = new Set<string>();
    for (const f of (favData.items ?? [])) if (f.type_label) set.add(f.type_label);
    return [...set];
  });
  /** 各类型收藏数（tab 上显示计数，一目了然收藏构成） */
  let favTypeCounts = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const f of (favData.items ?? [])) {
      const k = f.type_label ?? '其他';
      counts.set(k, (counts.get(k) ?? 0) + 1);
    }
    return counts;
  });
  // favData 声明在其后：保持惰性求值（与原始 $derived.by 语义一致）
  let filteredFavItems = $derived.by(() =>
    filterFavoriteItems(favData.items ?? [], { type: favType, q: favSearch })
  );
  let favSelectedIds = $derived(selectedIdsFromRecord(favSelected));
  function toggleFavSelectMode() {
    favSelectMode = !favSelectMode;
    if (!favSelectMode) favSelected = {};
  }
  function toggleFavSelect(id: string | number) {
    const key = String(id);
    favSelected = { ...favSelected, [key]: !favSelected[key] };
  }
  async function doDeleteFavorites() {
    if (!favSelectedIds.length || favDeleting) return;
    if (!confirm(`确定删除选中的 ${favSelectedIds.length} 条收藏吗？（仅删除本机副本，不可恢复）`)) return;
    favDeleting = true;
    try {
    const r = await deleteFavoriteItems(favSelectedIds);
      mgmt.show(`已删除 ${r.deleted} 条收藏`, true);
      favSelected = {};
      favSelectMode = false;
      await loadFavorites();
    } catch (e: unknown) {
      mgmt.show(`删除收藏失败：${errText(e)}`, false);
      logError('doDeleteFavorites', e);
    } finally {
      favDeleting = false;
    }
  }

  // ── 朋友圈搜索 / 作者过滤 ──
  let momentSearch = $state('');
  /** 「专门看某位好友的朋友圈」：按作者 username 后端过滤（分页/总数精确），name 用于展示 */
  let momentAuthor = $state<{ username: string; name: string } | null>(null);
  /** 只看我发布的动态（作用于已加载分页） */
  let momentSelfOnly = $state(false);
  // momentsPage 声明在后，须用 $derived.by 闭包避免 TDZ
  let filteredMoments = $derived.by(() => {
    let byKeyword = filterByAnyKeyword(
      momentsPage.items,
      momentSearch,
      (m) => m.author || '',
      (m) => m.text || '',
      (m) => m.location || '',
    );
    if (momentSelfOnly) {
      byKeyword = byKeyword.filter((m) => m.is_self);
    }
    return byKeyword;
  });

  // ── 表情搜索 ──
  let emoSearch = $state('');
  let emoTab = $state<'all' | 'custom' | 'static' | 'packages'>('all');
  /** 自定义表情 CDN 图加载失败的 md5（回退占位） */
  let emoImgFailed = $state<Record<string, boolean>>({});
  function emoImgError(md5: string) {
    emoImgFailed = { ...emoImgFailed, [md5]: true };
  }
  // emoticons 声明在其后：保持惰性求值（与原始 $derived.by 语义一致）
  let filteredEmoPackages = $derived.by(() =>
    filterByKeyword(emoticons.packages ?? [], emoSearch, (p) => p.name || '')
  );
  let filteredEmoCustom = $derived.by(() =>
    filterByKeyword(emoticons.custom ?? [], emoSearch, (e) => e.md5 || '')
  );
  /** 当前页签下实际展示的表情数量（用于空状态判断） */
  let emoActiveCount = $derived.by(() => {
    if (emoTab === 'custom') return filteredEmoCustom.length;
    if (emoTab === 'static') return filteredStaticEmoticons.length;
    if (emoTab === 'packages') return filteredEmoPackages.length;
    return filteredEmoCustom.length + filteredEmoPackages.length + filteredStaticEmoticons.length;
  });
  /** 复制文本到剪贴板（表情 MD5 等） */
  async function copyTextToClipboard(text: string) {
    const ok = await copyText(text);
    mgmt.show(ok ? '已复制' : '复制失败', ok);
  }



  // ── 联系人（懒加载分页）──
  // 联系人列表改为按 category 分页拉取，避免一次加载全部 / 不加载非好友。
  let contactsPage = $state<{ items: ContactItem[]; total: number; hasMore: boolean; loading: boolean }>({
    items: [], total: 0, hasMore: true, loading: false,
  });
  const CONTACTS_PAGE_SIZE = 200;
  let contactsLoading = $state(false);
  let contactsError = $state('');
  let contactSearch = $state('');
  // 已提交给后端的搜索词：非空时后端跨全库搜索（不限于已加载分页）
  let contactSearchQuery = $state('');
  // 分页加载进行中时又有新的搜索/分类请求 → 完成后重载一次
  let contactsPendingReload = false;
  // 各分类计数（来自 get_contacts 全量 stats，通讯录分类 tab 上显示）
  let contactStats = $state<Record<string, number> | null>(null);
  /** 全部（可见六分类合计）计数 */
  const contactStatsTotal = $derived.by(() => {
    if (!contactStats) return null;
    let sum = 0;
    for (const k of ['friend', 'member', 'enterprise', 'group', 'official', 'service']) {
      sum += contactStats[k] ?? 0;
    }
    return sum;
  });
  async function loadContactStats() {
    if (contactStats) return;
    try {
      const book = await getContacts();
      const s = book?.stats;
      const next: Record<string, number> = {};
      if (s && typeof s === 'object') {
        for (const [k, v] of Object.entries(s as Record<string, unknown>)) {
          if (typeof v === 'number') next[k] = v;
        }
      }
      contactStats = next;
    } catch (e) {
      logError('loadContactStats', e);
    }
  }
  // 默认只加载"朋友"分类，避免一次性把陌生人/群成员等非好友也加载进来
  let contactCat = $state<'all'|'friend'|'group'|'member'|'enterprise'|'official'|'service'>('friend');

  // 后端已经按 category 过滤好了，这里只需处理搜索关键字
  let filteredContacts = $derived(
    filterByAnyKeyword(
      contactsPage.items,
      contactSearch,
      (c) => c.display_name || '',
      (c) => c.nick_name || '',
      (c) => c.remark || '',
      (c) => c.username || '',
      (c) => c.alias || '',
    ),
  );

  /** 通讯录分组：群成员按所在群聊分组，其余分类按拼音首字母（PC 通讯录效果） */
  let groupedContacts = $derived(
    contactCat === 'member'
      ? groupMembersByRoom(filteredContacts)
      : groupContactsByInitial(filteredContacts),
  );

  /** 加载通讯录（按当前 contactCat 分页拉取，可带搜索词跨全库搜索） */
  async function loadContacts(reset = true) {
    if (contactsPage.loading) return;
    if (!reset && !contactsPage.hasMore) return;
    const q = contactSearchQuery.trim();
    if (reset) {
      // 搜索时保留旧列表做即时过滤展示，避免清空闪烁；正常重置才清空
      contactsPage = { items: q ? contactsPage.items : [], total: 0, hasMore: true, loading: false };
      contactsLoading = !q;
    }
    contactsPage.loading = true;
    const replace = !!q && reset;
    try {
      const r = await getContactsByCategory({
        category: contactCat,
        offset: replace ? 0 : contactsPage.items.length,
        limit: CONTACTS_PAGE_SIZE,
        query: q || undefined,
      });
      const incoming: ContactItem[] = r?.contacts ?? [];
      contactsPage.items = replace ? incoming : [...contactsPage.items, ...incoming];
      contactsPage.total = r?.total ?? contactsPage.items.length;
      contactsPage.hasMore = !!r?.has_more;
      contactsError = '';
      preloadAvatars(incoming.map((c) => c.username).filter((u): u is string => !!u));
      // 预载公众号历史消息链接，通讯录里点击公众号可立即进入
      ensureOfficialHistory();
    } catch (e) {
      // 滚动加载更多失败时不覆盖已显示的数据，仅首屏失败展示错误页
      if (contactsPage.items.length === 0) contactsError = errText(e) || '通讯录加载失败';
      logError('loadContacts', e);
    } finally {
      contactsPage.loading = false;
      if (reset) contactsLoading = false;
      // 加载期间又有新的搜索/分类切换请求：立即重载一次
      if (contactsPendingReload) {
        contactsPendingReload = false;
        loadContacts(true);
      }
    }
  }

  /** 切换分类：重置并加载新分类的第一页 */
  // untrack 读取：effect 仅依赖 contactCat 变化，避免初始化重复触发
  let lastContactCat = untrack(() => contactCat);
  $effect(() => {
    if (contactCat !== lastContactCat) {
      lastContactCat = contactCat;
      if (contactsPage.loading) {
        contactsPendingReload = true;
      } else {
        loadContacts(true);
      }
    }
  });

  /** 搜索词防抖：输入停顿 300ms 后提交后端跨全库搜索（通讯录全量，不限于已加载分页） */
  let contactSearchTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const v = contactSearch.trim();
    if (contactSearchTimer) clearTimeout(contactSearchTimer);
    contactSearchTimer = setTimeout(() => {
      if (contactSearchQuery !== v) {
        contactSearchQuery = v;
        if (contactsPage.loading) {
          contactsPendingReload = true;
        } else {
          loadContacts(true);
        }
      }
    }, 300);
    return () => {
      if (contactSearchTimer) clearTimeout(contactSearchTimer);
    };
  });

  /** 滚动到底部时增量加载更多 */
  function onContactsScroll(e: Event) {
    const el = e.target as HTMLElement;
    if (el.scrollTop + el.clientHeight >= el.scrollHeight - 80) {
      loadContacts(false);
    }
  }

  // ── 朋友圈（懒加载分页）──
  let momentsPage = $state<{ items: MomentEntry[]; total: number; hasMore: boolean; loading: boolean }>({
    items: [], total: 0, hasMore: true, loading: false,
  });
  let momentsError = $state('');
  const MOMENTS_PAGE_LIMIT = 6;
  /** 朋友圈洞察（作者活跃榜/月度热力/媒体构成），进入朋友圈视图时拉取 */
  let momentsInsight = $state<MomentsInsight | null>(null);
  async function loadMomentsInsight() {
    try {
      momentsInsight = await getMomentsInsights();
    } catch (e) {
      // 洞察是附加信息：失败不打断朋友圈浏览
      momentsInsight = null;
      logError('loadMomentsInsight', e);
    }
  }
  /** 朋友圈自动刷新周期（微信端持续写入 sns.db，解密副本需轮询跟进） */
  const MOMENTS_REFRESH_MS = 15000;
  /** tid 为服务端自增 ID：按 64 位整数降序（最新在前）。
   *  注意不能用字符串比较——tid 常为负数（如 -3463300…），
   *  字典序会把更负（更旧）的排到前面，导致懒加载后顺序颠倒。 */
  let momentsRefreshTimer: ReturnType<typeof setInterval> | null = null;
  let momentsRefreshing = $state(false);
  /** 加载/刷新进行中又有新的作者切换请求 → 完成后重载一次 */
  let momentsPendingReload = false;
  async function loadMoments(reset = true) {
    if (momentsPage.loading) return;
    if (!reset && !momentsPage.hasMore) return;
    if (reset) momentsPage = { items: [], total: 0, hasMore: true, loading: false };
    momentsPage.loading = true;
    try {
      const r = await getMoments(momentsPage.items.length, MOMENTS_PAGE_LIMIT, momentAuthor?.username);
      const incoming: MomentEntry[] = r?.items ?? [];
      momentsPage.items = [...momentsPage.items, ...incoming];
      momentsPage.total = r?.total ?? momentsPage.items.length;
      momentsPage.hasMore = !!r?.has_more;
      momentsError = '';
      preloadAvatars(incoming.map((m) => m.username).filter(Boolean));
    } catch (e) {
      if (momentsPage.items.length === 0) momentsError = errText(e) || '朋友圈加载失败';
      logError('loadMoments', e);
    } finally {
      momentsPage.loading = false;
      if (momentsPendingReload) {
        momentsPendingReload = false;
        loadMoments(true);
      }
    }
  }

  /** 设置「专门看某位好友的朋友圈」：按 username 后端过滤，切换后重载列表 */
  function setMomentAuthor(a: { username: string; name: string } | null) {
    const changed = momentAuthor?.username !== (a?.username ?? null);
    momentAuthor = a;
    // 作者模式与「只看我」叠加必然为空，切作者时复位
    momentSelfOnly = false;
    if (curTab === 'moments' && changed) {
      if (momentsPage.loading || momentsRefreshing) {
        momentsPendingReload = true;
      } else {
        loadMoments(true);
      }
    }
  }

  /**
   * 增量刷新朋友圈：后端按 mtime 判断源 sns.db 是否变新，变新才重解密。
   * - reset=true：整页重置（首次进入 / 手动刷新），同时触发一次源库解密
   * - reset=false：新条目按 tid 去重合并到列表头部，无更新时不打扰
   */
  async function refreshMomentsAuto(reset = false) {
    if (momentsRefreshing || momentsPage.loading) return;
    momentsRefreshing = true;
    if (reset) momentsPage.loading = true;
    try {
      const r = await refreshWechatMoments(0, MOMENTS_PAGE_LIMIT * 3, momentAuthor?.username);
      const incoming: MomentEntry[] = r?.items ?? [];
      if (reset) {
        momentsPage.items = incoming;
        momentsPage.total = r?.total ?? incoming.length;
        momentsPage.hasMore = !!r?.has_more;
        momentsError = '';
        preloadAvatars(incoming.map((m) => m.username).filter(Boolean));
      } else {
        // 1) 已存在条目用最新页数据更新（时间/点赞/评论等），新条目置顶；
        // 2) tid 降序（合并逻辑下沉 mergeMoments 纯函数）
        const { items: next, fresh } = mergeMoments(momentsPage.items, incoming);
        if (fresh.length > 0) {
          momentsPage.total = r?.total ?? next.length;
          momentsError = '';
          preloadAvatars(fresh.map((m) => m.username).filter(Boolean));
          mgmt.show(`朋友圈已更新 ${fresh.length} 条`, true);
        }
        if (next.length) momentsPage.items = next;
      }
    } catch (e) {
      if (reset) momentsError = errText(e) || '朋友圈加载失败';
      logError('refreshMomentsAuto', e);
    } finally {
      momentsPage.loading = false;
      momentsRefreshing = false;
      // 刷新期间有新的作者切换请求：完成后按新作者重载
      if (momentsPendingReload) {
        momentsPendingReload = false;
        loadMoments(true);
      }
    }
  }

  // ── 图片查看器（lightbox：点击放大 / 滚轮缩放 / 拖拽平移 / 前后切换）──
  let viewerOpen = $state(false);
  let viewerImages = $state<
    {
      src: string;
      time: string;
      username?: string;
      local_id?: number;
      sender_username?: string;
      is_group?: boolean;
    }[]
  >([]);
  let viewerIndex = $state(0);
  /** 高清原图 src（加载中为空；加载失败回退缩略图） */
  let viewerHdSrc = $state('');
  let viewerHdLoading = $state(false);
  /** 查看模式：true=原图/高清，false=缩略图（手动切换） */
  let viewerShowHd = $state(true);
  /** 等待微信下载原图的重试状态（微信 PC 端需打开该图片/会话才会下载高清） */
  let viewerHdRetryTimer: ReturnType<typeof setTimeout> | null = null;
  let viewerHdRetryCount = $state(0);
  const VIEWER_HD_RETRY_INTERVAL = 3000;
  const VIEWER_HD_MAX_RETRY = 20; // 20 × 3s ≈ 60s
  let viewerZoom = $state(1);
  let viewerOffset = $state({ x: 0, y: 0 });
  let viewerDragActive = $state(false);
  let viewerDragStart = { x: 0, y: 0, ox: 0, oy: 0 };

  /** 点击聊天图片消息：打开查看器并定位到该消息 */
  function openImageViewer(m: WeChatMessage) {
    const imgs = collectSessionImages(messages, imageCache, curSession);
    if (!imgs.length) return;
    viewerImages = imgs.map(({ src, time, local_id, sender_username, is_group }) => ({
      src,
      time,
      username: curSession ?? undefined,
      local_id,
      sender_username,
      is_group,
    }));
    const idx = imgs.findIndex((i) => i.local_id === m.local_id);
    viewerIndex = idx >= 0 ? idx : 0;
    resetViewerTransform();
    viewerOpen = true;
    loadViewerHd(viewerIndex);
  }

  /** 把当前查看的聊天图片发送到图文识别（携带微信五要素） */
  async function sendViewerToOcr() {
    const item = viewerImages[viewerIndex];
    if (!item || item.username === undefined || item.local_id === undefined) {
      mgmt.show('仅聊天图片支持发送到图文识别', false);
      return;
    }
    let mediaUrl = '';
    if (apiMediaBase) {
      // 优先 HTTP API 直链：图文识别后端可直接下载解密后的图片
      mediaUrl =
        `${apiMediaBase}/${encodeURIComponent(item.username)}/${item.local_id}?size=hd` +
        (apiToken ? `&access_token=${encodeURIComponent(apiToken)}` : '');
    } else if (item.src.startsWith('data:')) {
      // 未启用 HTTP API：直接把已解密的 data URL 交给后端
      mediaUrl = item.src;
    } else {
      mgmt.show('图片尚未解密，请先查看原图后再发送', false);
      return;
    }
    try {
      const id = await ocrIngestResource({
        senderUsername: item.sender_username || item.username,
        sessionType: item.is_group ? 'group' : 'single',
        timestamp: item.time || '',
        username: item.username,
        mediaUrl,
      });
      mgmt.show(`已发送到图文识别（资源 #${id}）`, true);
    } catch (e) {
      mgmt.show(`发送失败: ${e}`, false);
    }
  }

  /** 加载当前查看图片的高清原图：URL 直链（?size=hd）或 IPC 高清解码 */
  async function loadViewerHd(idx: number) {
    viewerHdLoading = false;
    viewerHdSrc = '';
    if (!viewerShowHd) {
      stopViewerHdRetry();
      return; // 缩略图模式：不加载原图
    }
    const item = viewerImages[idx];
    if (!item || item.username === undefined || item.local_id === undefined) {
      stopViewerHdRetry();
      return; // 朋友圈等无 local_id
    }
    viewerHdLoading = true;
    if (apiMediaBase) {
      // URL 直链：img 自行加载，onload 结束 loading
      const url = `${apiMediaBase}/${encodeURIComponent(item.username)}/${item.local_id}?size=hd` +
        (apiToken ? `&access_token=${encodeURIComponent(apiToken)}` : '');
      viewerHdSrc = url;
    } else {
      // IPC：后端按高清（原图/_h）解密
      try {
        const r = await getMessageImage({
          username: item.username,
          localId: item.local_id,
          size: 'hd',
        });
        if (viewerIndex !== idx) return; // 已切换图片，丢弃过期结果
        const data = r?.kind === 'data' && r.data ? r.data : '';
        if (data) {
          viewerHdSrc = data;
          stopViewerHdRetry();
        } else {
          startViewerHdRetry(); // 本地暂无高清，等待微信下载后重试
        }
      } catch (e) {
        logError('loadViewerHd', e);
        startViewerHdRetry();
      }
      viewerHdLoading = false;
    }
  }

  /** 进入"等待微信下载原图"重试循环 */
  function startViewerHdRetry() {
    stopViewerHdRetry();
    if (viewerHdRetryCount >= VIEWER_HD_MAX_RETRY) return;
    viewerHdRetryCount++;
    viewerHdRetryTimer = setTimeout(() => {
      viewerHdRetryTimer = null;
      loadViewerHd(viewerIndex); // 重试：微信下载完成后将命中
    }, VIEWER_HD_RETRY_INTERVAL);
  }

  function stopViewerHdRetry() {
    if (viewerHdRetryTimer) {
      clearTimeout(viewerHdRetryTimer);
      viewerHdRetryTimer = null;
    }
    viewerHdRetryCount = 0;
  }

  function resetViewerTransform() {
    viewerZoom = 1;
    viewerOffset = { x: 0, y: 0 };
  }

  function closeImageViewer() {
    viewerOpen = false;
    stopViewerHdRetry();
  }

  function prevImage() {
    if (viewerImages.length > 1) {
      viewerIndex = (viewerIndex - 1 + viewerImages.length) % viewerImages.length;
      resetViewerTransform();
      stopViewerHdRetry();
      loadViewerHd(viewerIndex);
    }
  }

  /** 手动切换 原图/缩略图 查看模式 */
  function toggleViewerHd() {
    viewerShowHd = !viewerShowHd;
    if (viewerShowHd) {
      loadViewerHd(viewerIndex);
    } else {
      viewerHdSrc = '';
      viewerHdLoading = false;
      stopViewerHdRetry();
    }
  }

  function nextImage() {
    if (viewerImages.length > 1) {
      viewerIndex = (viewerIndex + 1) % viewerImages.length;
      resetViewerTransform();
      stopViewerHdRetry();
      loadViewerHd(viewerIndex);
    }
  }

  function cycleZoom() {
    viewerZoom = VIEWER_ZOOM_STEPS[zoomStepIndex(VIEWER_ZOOM_STEPS, viewerZoom, 1, 'cycle')];
    if (viewerZoom === 1) viewerOffset = { x: 0, y: 0 };
  }

  function onViewerWheel(e: WheelEvent) {
    e.preventDefault();
    const dir = e.deltaY < 0 ? 1 : -1;
    viewerZoom = VIEWER_ZOOM_STEPS[zoomStepIndex(VIEWER_ZOOM_STEPS, viewerZoom, dir, 'clamp')];
    if (viewerZoom === 1) viewerOffset = { x: 0, y: 0 };
  }

  function onViewerMouseDown(e: MouseEvent) {
    if (viewerZoom <= 1) return;
    viewerDragActive = true;
    viewerDragStart = { x: e.clientX, y: e.clientY, ox: viewerOffset.x, oy: viewerOffset.y };
  }

  function onViewerMouseMove(e: MouseEvent) {
    if (!viewerDragActive) return;
    viewerOffset = {
      x: viewerDragStart.ox + (e.clientX - viewerDragStart.x),
      y: viewerDragStart.oy + (e.clientY - viewerDragStart.y),
    };
  }

  function onViewerMouseUp() {
    viewerDragActive = false;
  }

  /** 查看器键盘：Esc 关闭、←/→ 切换 */
  $effect(() => {
    if (!viewerOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        closeImageViewer();
        closeMomentVideo();
      }
      else if (e.key === 'ArrowLeft') prevImage();
      else if (e.key === 'ArrowRight') nextImage();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  /** 放大后拖动平移（mousemove/mouseup 挂 window，拖动到图片外不丢） */
  $effect(() => {
    if (!viewerOpen) return;
    const move = (e: MouseEvent) => onViewerMouseMove(e);
    const up = () => onViewerMouseUp();
    window.addEventListener('mousemove', move);
    window.addEventListener('mouseup', up);
    return () => {
      window.removeEventListener('mousemove', move);
      window.removeEventListener('mouseup', up);
    };
  });

  function startMomentsAutoRefresh() {
    stopMomentsAutoRefresh();
    momentsRefreshTimer = setInterval(refreshMomentsAuto, MOMENTS_REFRESH_MS);
  }
  function stopMomentsAutoRefresh() {
    if (momentsRefreshTimer) {
      clearInterval(momentsRefreshTimer);
      momentsRefreshTimer = null;
    }
  }
  /** 页面隐藏时暂停朋友圈轮询，恢复可见时若仍在朋友圈 tab 则续跑 */
  function handleVisibility() {
    if (document.hidden) {
      stopMomentsAutoRefresh();
    } else if (curTab === 'moments') {
      startMomentsAutoRefresh();
    }
  }

  /** 滚动到底部时增量加载更多动态 */
  function onMomentsScroll(e: Event) {
    const el = e.target as HTMLElement;
    if (el.scrollTop + el.clientHeight >= el.scrollHeight - 80) {
      loadMoments(false);
    }
  }

  // ── 收藏 ──
  let favData = $state<FavoritesData>({ items: [], tags: [] });
  let favoritesLoading = $state(false);
  let favoritesError = $state('');
  let favDetail = $state<FavoriteDetail | null>(null);
  let favDetailLoading = $state(false);
  async function loadFavorites() {
    favoritesLoading = true;
    try {
      favData = await getFavorites();
      favoritesError = '';
    } catch (e) {
      favData = { items: [], tags: [] };
      favoritesError = errText(e) || '收藏加载失败';
      logError('loadFavorites', e);
    } finally { favoritesLoading = false; }
  }
  /** 打开收藏详情（微信样式：左侧列表 + 右侧详情） */
  async function openFavDetail(f: FavoriteEntry) {
    if (favSelectMode || !f?.local_id) return;
    favDetailLoading = true;
    try {
      favDetail = await getFavoriteDetail(f.local_id);
      // 预取详情里的图片
      for (const md5 of favDetail?.images ?? []) loadFavoriteImage(md5);
    } catch (e: unknown) {
      logError('openFavDetail', e);
      mgmt.show(`收藏详情加载失败：${errText(e)}`, false);
    } finally {
      favDetailLoading = false;
    }
  }
  /** 收藏图片点击 → 全屏查看 */
  function openFavImageViewer(md5s: string[], idx: number) {
    const imgs = (md5s || []).map((md5: string) => ({
      src: favImageMap[md5] || '',
      time: '',
    }));
    if (!imgs.length) return;
    viewerImages = imgs;
    viewerIndex = Math.max(0, Math.min(idx, imgs.length - 1));
    viewerOpen = true;
    resetViewerTransform();
  }

  // ── 表情 ──
  let emoticons = $state<EmoticonOverview>({ packages: [], custom: [], store_files: [] });
  let emoticonsLoading = $state(false);
  let emoticonsError = $state('');
  async function loadEmoticons() {
    emoticonsLoading = true;
    try {
      emoticons = await getEmoticons();
      emoticonsError = '';
    } catch (e) {
      emoticons = { packages: [], custom: [], store_files: [] };
      emoticonsError = errText(e) || '表情数据加载失败';
      logError('loadEmoticons', e);
    }
    finally { emoticonsLoading = false; }
  }

  // 本地静态表情包（随应用打包）
  let staticEmoticons = $state<StaticEmoticonCategory[]>([]);
  let staticEmoticonsLoading = $state(false);
  let staticEmoticonsError = $state('');
  let staticEmoCat = $state<string>('all');
  let staticEmoSearch = $state<string>('');
  let staticEmoticonMap = $derived(buildStaticEmoticonMap(staticEmoticons));
  let staticEmoCategories = $derived([{ key: 'all', label: '全部' }, ...(staticEmoticons.map((c) => ({ key: c.category, label: c.label })))]);
  let filteredStaticEmoticons = $derived(filterStaticEmoticons(staticEmoticons, staticEmoCat, staticEmoSearch));
  async function loadStaticEmoticons() {
    if (staticEmoticons.length > 0) return;
    staticEmoticonsLoading = true;
    try {
      staticEmoticons = await getStaticEmoticons();
      staticEmoticonsError = '';
    } catch (e) {
      staticEmoticons = [];
      staticEmoticonsError = errText(e) || '静态表情加载失败';
      logError('loadStaticEmoticons', e);
    } finally { staticEmoticonsLoading = false; }
  }

  // ── 公众号 ──
  let bizchats = $state<OfficialAccount[]>([]);
  let bizchatsLoading = $state(false);
  let bizchatsError = $state('');
  /** 公众号 username → “查看历史消息”网页链接（无本地消息时使用） */
  let officialHistory = $state<Record<string, string>>({});
  /** 公众号/服务号分组：服务号（ServiceType=1）与其余（订阅号/企业号/未知） */
  const bizServices = $derived((bizchats || []).filter((b) => b.official_kind === 'service'));
  const bizSubscriptions = $derived((bizchats || []).filter((b) => b.official_kind && b.official_kind !== 'service'));
  let bizSearch = $state('');
  const bizServicesFiltered = $derived(
    filterByAnyKeyword(bizServices, bizSearch, (b) => b.name || '', (b) => b.username || ''),
  );
  const bizSubscriptionsFiltered = $derived(
    filterByAnyKeyword(bizSubscriptions, bizSearch, (b) => b.name || '', (b) => b.username || ''),
  );
  async function loadBizchats() {
    bizchatsLoading = true;
    try {
      // 完整公众号/服务号列表（含无本地消息的已订阅账号），口径与微信一致
    const list = await getOfficialAccounts();
      bizchats = list ?? [];
      const h: Record<string, string> = {};
      for (const b of bizchats) {
        if (b.history_url) h[b.username] = b.history_url;
      }
      officialHistory = h;
      bizchatsError = '';
      preloadAvatars(bizchats.map((b) => b.username).filter(Boolean));
    } catch (e) {
      bizchats = [];
      bizchatsError = errText(e) || '公众号加载失败';
      logError('loadBizchats', e);
    }
    bizchatsLoading = false;
  }

  /** 懒加载公众号“查看历史消息”链接（从通讯录直接进入公众号聊天时也需要） */
  let officialHistoryLoaded = false;
  async function ensureOfficialHistory() {
    if (officialHistoryLoaded) return;
    officialHistoryLoaded = true;
    try {
    const list = await getOfficialAccounts();
      const h: Record<string, string> = {};
      for (const b of list ?? []) {
        if (b.history_url) h[b.username] = b.history_url;
      }
      officialHistory = h;
    } catch (e) {
      logError('ensureOfficialHistory', e);
    }
  }

  // ── 文件 ──
  const EMPTY_FILES_DATA: ResourceFilesOverview = {
    images: [], videos: [], files: [],
    total_size: 0, total_size_label: '',
    images_total: 0, videos_total: 0, files_total: 0,
  };
  let fileData = $state<ResourceFilesOverview>(EMPTY_FILES_DATA);
  let filesLoading = $state(false);
  let filesError = $state('');
  let fileCat = $state<'all'|'image'|'video'|'file'>('all');
  let fileSearch = $state('');
  /** 列表分页：每页加载条数 */
  const FILE_PAGE = 100;
  let fileListLimit = $state(FILE_PAGE);
  /** 图片/视频预览加载失败的 md5 集合（回退占位） */
  let fileImgFailed = $state<Record<string, boolean>>({});
  let fileVideoFailed = $state<Record<string, boolean>>({});
  /** 文件图片查看器 */
  interface FileViewerItem { src: string; name: string; meta: string; path: string; }
  let fileViewer = $state<{ open: boolean; items: FileViewerItem[]; index: number }>({ open: false, items: [], index: 0 });
  /** 文件视频播放器 */
  let fileVideoOpen = $state(false);
  let fileVideoSrc = $state('');
  let fileVideoName = $state('');
  let fileVideoPath = $state('');
  async function loadFiles() {
    filesLoading = true;
    try {
    fileData = await getResourceFiles();
      filesError = '';
      fileListLimit = FILE_PAGE;
    } catch (e) {
      fileData = EMPTY_FILES_DATA;
      filesError = errText(e) || '文件数据加载失败';
      logError('loadFiles', e);
    }
    finally { filesLoading = false; }
  }
  let shownFiles = $derived(filterSortResourceFiles(fileData, fileCat, fileSearch));
  /** 当前分页窗口（首屏只渲染前 N 条，避免几千行 DOM） */
  let filePageItems = $derived(shownFiles.slice(0, fileListLimit));
  let fileImages = $derived(filePageItems.filter((f) => f.category === 'image'));
  let fileVideos = $derived(filePageItems.filter((f) => f.category === 'video'));
  let fileDocs = $derived(filePageItems.filter((f) => f.category === 'file'));
  /** 未分页的过滤总数（用于页签计数） */
  let fileImagesTotal = $derived(shownFiles.filter((f) => f.category === 'image').length);
  let fileVideosTotal = $derived(shownFiles.filter((f) => f.category === 'video').length);
  let fileDocsTotal = $derived(shownFiles.filter((f) => f.category === 'file').length);
  /** 打开文件图片查看器（带左右切换） */
  function openFileImageViewer(f: ResourceFile) {
    if (!apiRoot) return;
    const list = fileImages;
    const index = Math.max(0, list.findIndex((x) => x.md5 === f.md5));
    fileViewer = {
      open: true,
      index,
      items: list.map((x) => ({
        src: apiAssetUrl(`/file/image/${x.md5}`),
        name: x.file_name || x.md5,
        meta: `${x.size_label} · ${x.time}`,
        path: x.path || '',
      })),
    };
  }
  function closeFileViewer() { fileViewer = { open: false, items: [], index: 0 }; }
  function fileViewerPrev() {
    if (!fileViewer.items.length) return;
    fileViewer.index = (fileViewer.index - 1 + fileViewer.items.length) % fileViewer.items.length;
  }
  function fileViewerNext() {
    if (!fileViewer.items.length) return;
    fileViewer.index = (fileViewer.index + 1) % fileViewer.items.length;
  }
  /** 播放文件管理中的视频 */
  function openFileVideo(f: ResourceFile) {
    if (!apiRoot) return;
    fileVideoSrc = apiAssetUrl(`/file/video/${f.md5}`);
    fileVideoName = f.file_name || f.md5;
    fileVideoPath = f.path || '';
    fileVideoOpen = true;
  }
  function closeFileVideo() { fileVideoOpen = false; fileVideoSrc = ''; fileVideoPath = ''; }
  /** 在资源管理器中定位文件所在目录 */
  async function openFileFolder(p: string) {
    const idx = p.lastIndexOf('\\');
    const dir = idx > 0 ? p.slice(0, idx) : p;
    try {
    await openWechatFolder(dir);
    } catch (e) {
      mgmt.show(errText(e) || '打开目录失败', false);
    }
  }

  // ── 设置 ──
  let settingsData = $state<GeneralCategory[]>([]);
  let settingsLoading = $state(false);
  let settingsError = $state('');
  /** 分类折叠状态：key → 是否展开 */
  let settingsOpen = $state<Record<string, boolean>>({});
  let settingsSearch = $state('');
  /** 分类总数（真实行数合计）与有数据分类数 */
  let settingsStats = $derived.by(() => {
    const total = settingsData.reduce((a: number, c) => a + (Number(c.total) || 0), 0);
    const withData = settingsData.filter((c) => Number(c.total) > 0).length;
    return { cats: settingsData.length, withData, total };
  });
  /** 搜索过滤：分类名/表名匹配，或行内任意单元格命中 */
  let settingsFilteredCats = $derived(filterSettingsCats(settingsData, settingsSearch));
  async function loadSettings() {
    settingsLoading = true;
    try {
    settingsData = await getGeneralSettings();
      settingsError = '';
      // 默认展开有数据的分类，收起空分类
      const open: Record<string, boolean> = {};
      for (const c of settingsData) open[c.key] = Number(c.total) > 0;
      settingsOpen = open;
    } catch (e) {
      settingsData = [];
      settingsError = errText(e) || '通用数据加载失败';
      logError('loadSettings', e);
    }
    finally { settingsLoading = false; }
  }
  const settingIcons: Record<string, string> = {
    FMessageTable: ICON_PATHS.chat, transferTable: ICON_PATHS.card, redEnvelopeTable: ICON_PATHS.gift, groupPayTable: ICON_PATHS.users,
    revokemessage: ICON_PATHS.rewind, reddot: ICON_PATHS.dot, SearchRecent: ICON_PATHS.search, ForwardRecent: ICON_PATHS.corner,
    autoDownloadFileTable: ICON_PATHS.download, VoiceToTextTable: ICON_PATHS.mic, AuthInfo: ICON_PATHS.lock, LoginDeviceInfo: ICON_PATHS.monitor,
  };
  function toggleSettingsCat(key: string) {
    settingsOpen = { ...settingsOpen, [key]: !settingsOpen[key] };
  }
  /** 导出单个分类为 CSV */
  async function exportSettingsCat(cat: GeneralCategory) {
    try {
    const r = await exportGeneralCategoryCsv({ table: cat.table });
      const csv = r?.csv ?? '';
      if (!csv) { mgmt.show('无数据可导出', false); return; }
      downloadBlob(new Blob(["\uFEFF" + csv], { type: 'text/csv;charset=utf-8' }), `wechat_${cat.table}_${new Date().toISOString().slice(0, 10)}.csv`);
      mgmt.show('已导出 CSV', true);
    } catch (e) {
      mgmt.show(errText(e) || '导出失败', false);
    }
  }

  function switchTab(t: Tab) {
    curTab = t; curSession = null; curSessionInfo = null; clearMessages();
    // 离开通讯录时清空内嵌资料卡，下次进入显示列表提示
    inlineProfile = false;
    profileOpen = false;
    if (t === 'contacts') { loadContacts(); loadContactStats(); }
    if (t === 'moments') { refreshMomentsAuto(true); loadMomentsInsight(); startMomentsAutoRefresh(); }
    else stopMomentsAutoRefresh();
    if (t === 'favorites') loadFavorites();
    if (t === 'emoticons') { loadEmoticons(); loadStaticEmoticons(); }
    if (t === 'bizchats' || t === 'servicechats') loadBizchats();
    if (t === 'files') loadFiles();
    if (t === 'settings') loadSettings();
  }

  // 外部请求（微信启动页「去配置」等）→ 打开本面板的「设置」
  $effect(() => {
    if (openConfigTick > 0) switchTab('settings');
  });

  // ── 消息编辑（迁移自 WeChatDataAnalysis：本地修改解密副本，支持恢复）──
  let editMenu = $state<{
    open: boolean; x: number; y: number;
    username: string | null; localId: number; text: string;
    canEdit: boolean; modified: boolean; loading: boolean; msg: WeChatMessage | null;
  }>({ open: false, x: 0, y: 0, username: null, localId: 0, text: '', canEdit: false, modified: false, loading: false, msg: null });
  let editModal = $state<{ open: boolean; username: string; localId: number; text: string; saving: boolean; error: string }>(
    { open: false, username: '', localId: 0, text: '', saving: false, error: '' }
  );
  /** 已编辑消息标记集合（key = `${username}:${local_id}`） */
  let editedSet = $state<Set<string>>(new Set());

  /** 会话加载时获取该会话已编辑消息列表，用于渲染“已编辑”徽标 */
  async function loadEditedSet(username: string) {
    try {
    const r = await listSessionEditedMessages({ username });
      const next = new Set(editedSet);
      for (const it of (r?.items ?? [])) {
        if (it?.local_id != null) next.add(editKey(username, it.local_id));
      }
      editedSet = next;
    } catch (e) { logError('loadEditedSet', e); }
  }

  function openEditMenu(e: MouseEvent, m: WeChatMessage) {
    e.preventDefault();
    e.stopPropagation();
    const canEdit = !!curSession && (m.type === 1 || (!!m.text && !m.rich));
    editMenu = {
      open: true, x: e.clientX, y: e.clientY,
      username: curSession, localId: m.local_id, text: m.text ?? '',
      canEdit, modified: editedSet.has(editKey(curSession, m.local_id)), loading: false, msg: m,
    };
    if (!curSession) return;
    // 异步刷新真实编辑状态
    getChatEditStatus({ username: curSession, localId: m.local_id })
      .then((r) => {
        if (!editMenu.open || editMenu.localId !== m.local_id) return;
        const modified = !!r?.modified;
        editMenu.modified = modified;
        editMenu.loading = false;
        const s = new Set(editedSet);
        if (modified) s.add(editKey(curSession, m.local_id)); else s.delete(editKey(curSession, m.local_id));
        editedSet = s;
      })
      .catch(() => { if (editMenu.open && editMenu.localId === m.local_id) editMenu.loading = false; });
  }

  function closeEditMenu() {
    editMenu = { ...editMenu, open: false };
  }

  function openEditModal() {
    if (!editMenu.canEdit || !editMenu.username) return;
    editModal = {
      open: true, username: editMenu.username, localId: editMenu.localId,
      text: editMenu.text, saving: false, error: '',
    };
    closeEditMenu();
  }

  async function saveEdit() {
    if (editModal.saving || !editModal.username) return;
    editModal.saving = true;
    editModal.error = '';
    try {
      await editChatMessage({
        username: editModal.username, localId: editModal.localId, newText: editModal.text,
      });
      // 本地立即更新气泡内容
      const idx = messages.findIndex((m) => m.local_id === editModal.localId);
      if (idx >= 0) {
        messages[idx] = { ...messages[idx], text: editModal.text, rich: null };
        msgListRef?.updateEstimate(idx, estimateMsgHeight(messages[idx]));
      }
      const key = editKey(editModal.username, editModal.localId);
      const s = new Set(editedSet); s.add(key); editedSet = s;
      editModal.open = false;
      mgmt.show('消息已修改（仅解密副本，微信源库不受影响）', true);
    } catch (e: unknown) {
      editModal.error = errText(e);
    } finally {
      editModal.saving = false;
    }
  }

  async function resetEdit() {
    if (!editMenu.username) return;
    const username = editMenu.username;
    const localId = editMenu.localId;
    try {
    await resetEditedMessage({ username, localId });
      const key = editKey(username, localId);
      const s = new Set(editedSet); s.delete(key); editedSet = s;
      // 重新加载当前会话消息以恢复原文
      const seq = ++msgReqSeq;
      const r = await loadLatestMessages(username);
      if (seq !== msgReqSeq) return;
      setMessages(r?.messages ?? []);
      hasMoreMsgs = r?.has_more ?? false;
      nextCursor = r?.next_cursor ?? null;
      msgListRef?.setStickToBottom(true);
      await tick();
      msgListRef?.scrollToBottom();
      mgmt.show('已恢复原始消息', true);
    } catch (e: unknown) {
      mgmt.show(`恢复失败：${errText(e)}`, false);
    } finally {
      closeEditMenu();
    }
  }

  // ── 消息原始字段编辑（迁移自 WeChatDataAnalysis 的字段编辑弹窗）──
  let rawEditModal = $state<{ open: boolean; username: string; localId: number; json: string; unsafe: boolean; saving: boolean; error: string }>(
    { open: false, username: '', localId: 0, json: '', unsafe: false, saving: false, error: '' }
  );

  async function openRawEditModal() {
    if (!editMenu.username) return;
    const username = editMenu.username;
    const localId = editMenu.localId;
    rawEditModal = { open: true, username, localId, json: '', unsafe: false, saving: false, error: '' };
    closeEditMenu();
    try {
    const r = await getMessageRawRow({ username, localId });
      const row = r?.row ?? {};
      const seed: Record<string, unknown> = {};
      for (const k of ['message_content', 'local_type', 'create_time', 'server_id', 'sort_seq', 'real_sender_id', 'compress_content']) {
        if (row[k] !== undefined) seed[k] = row[k];
      }
      rawEditModal.json = JSON.stringify(seed, null, 2);
    } catch (e: unknown) {
      rawEditModal.error = errText(e);
    }
  }

  async function saveRawEdit() {
    if (rawEditModal.saving) return;
    let edits: Record<string, unknown> | null = null;
    try {
      edits = JSON.parse(rawEditModal.json);
    } catch {
      rawEditModal.error = 'JSON 格式错误';
      return;
    }
    if (!edits || typeof edits !== 'object' || Array.isArray(edits)) {
      rawEditModal.error = 'edits 必须是 JSON 对象';
      return;
    }
    if (!Object.keys(edits).length) {
      rawEditModal.error = 'edits 不能为空';
      return;
    }
    rawEditModal.saving = true;
    rawEditModal.error = '';
    try {
      await updateMessageRawFields({
        username: rawEditModal.username, localId: rawEditModal.localId, edits, unsafeEdit: rawEditModal.unsafe,
      });
      // 重新加载当前会话
      const seq = ++msgReqSeq;
      const r = await loadLatestMessages(rawEditModal.username);
      if (seq === msgReqSeq) {
        setMessages(r?.messages ?? []);
        hasMoreMsgs = r?.has_more ?? false;
        nextCursor = r?.next_cursor ?? null;
        msgListRef?.setStickToBottom(true);
        await tick();
        msgListRef?.scrollToBottom();
      }
      const key = editKey(rawEditModal.username, rawEditModal.localId);
      const s = new Set(editedSet); s.add(key); editedSet = s;
      rawEditModal.open = false;
      mgmt.show('消息字段已修改（仅解密副本）', true);
    } catch (e: unknown) {
      rawEditModal.error = errText(e);
    } finally {
      rawEditModal.saving = false;
    }
  }

  function copyMessageJson() {
    try {
      const msg = editMenu.msg ?? null;
      if (msg) void copyText(JSON.stringify(msg, null, 2));
    } catch {}
    closeEditMenu();
  }

  // ── 导出归档（迁移自 WeChatDataAnalysis 的账号归档导出）──
  let archiveOpen = $state(false);
  let archiveDir = $state('');
  let archiveIncludeResources = $state(true);
  let archiveRunning = $state(false);
  let archiveResult = $state<{ path: string; filename: string; file_count: number; total_bytes: number } | null>(null);
  let archiveError = $state('');
  let archiveProgress = $state<{ label: string; percent: number } | null>(null);
  let archiveUnlisten: (() => void) | null = null;

  async function pickArchiveDir() {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const sel = await open({ directory: true, multiple: false, title: '选择归档保存目录' });
      if (typeof sel === 'string' && sel.trim()) archiveDir = sel.trim();
    } catch (e: unknown) {
      archiveError = errText(e);
    }
  }

  async function startArchive() {
    if (archiveRunning) return;
    archiveRunning = true;
    archiveError = '';
    archiveResult = null;
    archiveProgress = null;
    try {
      archiveUnlisten = await listen<WeChatOpProgress>('wechat-op-progress', (e) => {
        if (e?.payload?.op !== 'archive') return;
        archiveProgress = { label: e.payload.message ?? '', percent: e.payload.percent ?? 0 };
      });
    } catch {}
    try {
      const r = await exportWechatArchive({
        outputDir: archiveDir || null,
        includeResources: archiveIncludeResources,
      });
      archiveResult = r;
      archiveProgress = { label: '归档完成', percent: 100 };
    } catch (e: unknown) {
      archiveError = errText(e);
    } finally {
      archiveRunning = false;
      try { archiveUnlisten?.(); archiveUnlisten = null; } catch {}
    }
  }

  // ── 全局消息搜索（迁移自 WeChatDataAnalysis 的聊天记录搜索）──
  let searchMode = $state<'session' | 'message'>('session');
  let msgSearchResults = $state<WechatSearchHit[]>([]);
  let msgSearchLoading = $state(false);
  let msgSearchError = $state('');
  let msgSearchTimer: ReturnType<typeof setTimeout> | null = null;
  let msgSearchSearched = $state(false);
  let msgSearchIndexed = $state(true);
  let searchIndexBuilding = $state(false);
  async function buildSearchIndex(silent = false) {
    searchIndexBuilding = true;
    try {
      const r = await buildWechatSearchIndex(false);
      msgSearchIndexed = true;
      if (!silent) msgSearchError = `搜索索引已就绪（${r?.rows ?? 0} 条），重新搜索即可加速`;
    } catch (e: unknown) {
      if (!silent) msgSearchError = `索引构建失败: ${e}`;
    } finally {
      searchIndexBuilding = false;
    }
  }

  async function checkSearchIndexStatus() {
    try {
      const st = await getWechatSearchIndexStatus();
      msgSearchIndexed = st?.exists === true && (st?.rows ?? 0) > 0;
    } catch { /* 保持默认 */ }
  }

  function onSearchInput() {
    if (searchMode === 'message') {
      if (msgSearchTimer) clearTimeout(msgSearchTimer);
      const q = searchText.trim();
      if (q.length < 1) {
        msgSearchResults = [];
        msgSearchSearched = false;
        msgSearchError = '';
        return;
      }
      // 首次消息搜索且索引未构建：后台自动构建，本次先用全表扫描出结果
      if (!msgSearchIndexed && !searchIndexBuilding) {
        buildSearchIndex(true);
      }
      msgSearchTimer = setTimeout(async () => {
        msgSearchLoading = true;
        msgSearchError = '';
        try {
          const r = await searchWechatMessages({ query: q, limit: 200 });
          msgSearchResults = r?.hits ?? [];
          msgSearchIndexed = r?.indexed !== false;
          msgSearchSearched = true;
        } catch (e: unknown) {
          msgSearchError = errText(e);
          msgSearchResults = [];
        } finally {
          msgSearchLoading = false;
        }
      }, 350);
    }
  }

  async function openSearchHit(hit: WechatSearchHit) {
    if (hit?.username === curSession) {
      await tryJumpToMessage(hit.local_id);
      return;
    }
    await selectSession({ username: hit.username, name: hit.name });
    await tryJumpToMessage(hit.local_id);
  }

  /** AI 问答引用跳转：切到聊天页并定位到指定消息 */
  async function openAskCitation(c: { username: string; local_id?: number; name?: string }) {
    if (!c?.username || !c?.local_id) return;
    switchTab('chats');
    await tick();
    if (c.username === curSession) {
      await tryJumpToMessage(c.local_id);
      return;
    }
    await selectSession({ username: c.username, name: c.name });
    await tryJumpToMessage(c.local_id);
  }

  /** 关系图谱打开聊天：切到聊天页并选中会话 */
  async function openGraphChat(username: string) {
    if (!username) return;
    switchTab('chats');
    await tick();
    if (username === curSession) return;
    await selectSession({ username });
  }

  /** 定位搜索命中的消息：逐步向上翻页直至找到（上限 15 页） */
  async function tryJumpToMessage(localId: number) {
    await tick();
    for (let i = 0; i < 15 && hasMoreMsgs; i++) {
      const idx = messages.findIndex((m) => m.local_id === localId);
      if (idx >= 0) {
        if (!msgListRef?.scrollToIdx(idx)) {
          // 目标行不在可视窗口：退出吸底让虚拟滚动窗口定位到该位置
          msgListRef?.setStickToBottom(false);
        }
        return;
      }
      await msgListRef?.loadMore();
    }
    const idx = messages.findIndex((m) => m.local_id === localId);
    if (idx >= 0) {
      msgListRef?.scrollToIdx(idx);
    } else {
      mgmt.show('未能在已加载消息中找到该条，请继续向上滑动', false);
    }
  }

  onMount(async () => {
    panelLoading = true;

    // 已清除草稿记录（持久化），避免监控恢复旧草稿后再次显示
    loadClearedDrafts().catch((e) => logError('loadClearedDrafts', e));

    // 图片 URL 直链配置（HTTP API 端口/令牌），失败则回退 IPC base64
    loadApiMediaConfig().catch((e) => logError('loadApiMediaConfig', e));
    // 账号一致性（数据来源 vs 当前登录微信）
    loadAccountStatus().catch((e) => logError('loadAccountStatus', e));
    // 搜索索引状态（未构建时提示并支持自动/手动构建）
    checkSearchIndexStatus().catch((e) => logError('checkSearchIndexStatus', e));

    // 关键路径：先加载会话列表（用户最先看到的内容）
    // checkDbStatus 是非关键路径，后台加载即可
    const sessionsPromise = loadSessions();
    const dbStatusPromise = checkDbStatus();
    // 静态表情（用于聊天消息中的 [表情名] → 图片 渲染）
    // 必须与会话并行预加载：否则用户不切到"表情"tab 就直接看群聊，
    // 聊天文本里的 [微笑][捂脸] 等都会显示为原文而不是图片
    loadStaticEmoticons().catch((e) => logError('onMount loadStaticEmoticons', e));

    // 等待会话加载，但最多 15 秒超时（防止 IPC 卡住导致永远 "微信数据加载中…"）
    const timeoutId = setTimeout(() => {
      if (panelLoading) {
        sessionsError = '会话列表加载超时，请检查数据库路径或重新配置';
        panelLoading = false;
        logError('onMount', new Error('会话列表加载超时'));
      }
    }, 15000);
    try {
      await sessionsPromise;
    } catch (e) {
      logError('onMount loadSessions', e);
    }
    clearTimeout(timeoutId);
    panelLoading = false;

    // 启动事件总线：Tauri Event 主通道 + WebSocket 回退 + ACK 去重
    // 注意：必须先完成 eventBus.start()（注册 listen）再启动后端监控，
    // 否则监控启动瞬间推送的首批消息会因前端尚未 listen 而丢失。
    try {
      eventBus = createWechatEventBus({
        onMessage: (payload) => {
          try {
            const username = payload?.username?.trim();
            if (!username) return;
            // 与会话列表中的 username 对齐（去除空格/大小写差异）
            const matched = sessionMap.get(username);
            const resolved = matched?.username?.trim() ?? username;
            const active = curSession?.trim();
            const isActive = active && resolved === active;
            logDebug('wechat-message', {
              raw: payload?.username,
              resolved,
              active,
              isActive,
              channel: payload?.channel,
              content: payload?.content,
            });

            // 先直接合并到会话列表，实现零延迟 UI 反馈；再排队一次 IPC 刷新补全元数据
            mergeSessionUpdate({ ...payload, username: resolved });
            scheduleSessionsRefresh();
            // 实时刷新当前会话
            if (isActive) {
              // 实时消息命中当前会话：追加新消息而非用“最新一页”替换，
              // 避免已加载的历史消息被清空、只显示少量最新消息（群聊显示不全）。
              appendRealtimeMessage({ ...payload, username: resolved });
            }
          } catch (e) {
            logError('wechat-message handler', e);
          }
        },
        onStatus: (payload) => {
          try {
            monitorStatus = {
              running: !!payload?.running,
              status: String(payload?.status ?? 'unknown'),
              ws_port: payload?.ws_port,
              pending_acks: payload?.pending_acks,
              sent_total: payload?.sent_total,
              sent_batch_count: payload?.sent_batch_count,
              sent_ws_count: payload?.sent_ws_count,
              latency: payload?.latency,
            };
          } catch (e) {
            logError('wechat-status handler', e);
          }
        },
        // 健康看门狗：检测到后端监控任务死亡/假死时自动重启并补拉遗漏消息
        onNeedRestart: () => {
          logError('wechat watchdog', new Error('监控心跳丢失，自动重启'));
          startMonitor()
            .then(() => eventBus?.resync())
            .catch((e) => logError('wechat watchdog restart', e));
        },
      });
      await eventBus.start();
    } catch (e) {
      logError('createWechatEventBus', e);
    }

    // dbStatusPromise 在后台继续，完成后再更新 UI；若数据库就绪且监控未运行，自动启动
    // 延迟 800ms，等待 Tauri IPC 注入完成，避免 dev 模式下 IPC custom protocol 回退前的调用失败
    // 该逻辑放在 eventBus.start() 之后，确保 listen 已注册，首批推送不丢失
    dbStatusPromise
      .then(async () => {
        // 只要完成检查就尝试启动监控：个别库"空目录"不代表监控不可用，
        // 监控启动本身会按需解密最新数据。启动失败会通过 toast 明确提示。
        if (!dbStatusChecked) return;
        await refreshMonitorStatus();
        if (!monitorStatus.running) {
          await new Promise((resolve) => setTimeout(resolve, 800));
          if (!monitorStatus.running) await startMonitor();
        }
      })
      .catch((e) => logError('onMount checkDbStatus', e));

    // 获取初始监控状态
    await refreshMonitorStatus();

    // 点击页面其他位置关闭 DB 状态弹窗
    document.addEventListener('click', closeDbStatusPopup);
    document.addEventListener('visibilitychange', handleVisibility);
  });
  onDestroy(() => {
    stopMomentsAutoRefresh();
    document.removeEventListener('visibilitychange', handleVisibility);
    // 事件总线接口方法名为 destroy()（不存在 stop()）。
    // 之前调用 stop() 会抛 TypeError，导致 unlisten 永不执行、
    // 监听器泄漏、组件重建后同一消息被重复推送。
    eventBus?.destroy();
    clearImageAutoRetries();
    if (loadMsgsTimeoutId) clearTimeout(loadMsgsTimeoutId);
    document.removeEventListener('click', closeDbStatusPopup);
  });
</script>

<div class="wc-root">
  {#snippet profileCard(pd: ContactItem)}
    <div class="wc-profile">
      <div class="wc-profile-avatar">
        {#if avatarCache[profileUsername]}<img src={avatarCache[profileUsername]} alt="" class="wc-profile-avatar-img" />{:else}<div class="wc-avatar wc-avatar-lg">{avatarLetter(pd.display_name || pd.nick_name || pd.username || profileUsername || '')}</div>{/if}
      </div>
      <div class="wc-profile-name">{pd.display_name || pd.nick_name || '未知联系人'}</div>
      <div class="wc-profile-items">
        {#if pd.username}<div class="wc-profile-item"><span>微信号</span><span class="wc-mono">{pd.username}</span></div>{/if}
        {#if pd.remark}<div class="wc-profile-item"><span>备注</span><span>{pd.remark}</span></div>{/if}
        {#if pd.alias}<div class="wc-profile-item"><span>别名</span><span>{pd.alias}</span></div>{/if}
        {#if pd.member_count != null}<div class="wc-profile-item"><span>群成员</span><span>{pd.member_count} 人</span></div>{/if}
        {#if pd.owner_name}<div class="wc-profile-item"><span>群主</span><span>{pd.owner_name}</span></div>{/if}
        {#if pd.group_name}
          <div class="wc-profile-item">
            <span>所在群</span>
            {#if pd.group_username}
              <button type="button" class="wc-profile-link" title="打开该群聊天"
                onclick={() => {
                  profileOpen = false;
                  inlineProfile = false;
                  openRecordSession(pd.group_username || '');
                }}>{pd.group_name}</button>
            {:else}
              <span>{pd.group_name}</span>
            {/if}
          </div>
        {/if}
        {#if pd.description}<div class="wc-profile-item"><span>签名</span><span>{pd.description}</span></div>{/if}
        {#if pd.local_type_label}<div class="wc-profile-item"><span>类型</span><span>{pd.local_type_label}</span></div>{/if}
      </div>
      <div class="wc-profile-actions">
        <WechatHoverButton
          text="TA 的朋友圈"
          title="专门查看这位好友的朋友圈动态"
          onclick={() => {
            const u = pd.username || profileUsername || '';
            profileOpen = false;
            inlineProfile = false;
            setMomentAuthor({ username: u, name: pd.display_name || pd.nick_name || u });
            switchTab('moments');
          }}
          class="!px-3 !py-1 !text-xs"
        />
        <WechatHoverButton
          text={(pd.username || profileUsername || '').endsWith('@chatroom') ? '群发消息' : '发消息'}
          onclick={() => {
            const u = pd.username || profileUsername;
            profileOpen = false;
            inlineProfile = false;
            curTab = 'chats';
            selectSession(u);
          }}
          class="!px-3 !py-1 !text-xs"
        />
        <WechatHoverButton
          text="复制用户名"
          onclick={() => void copyTextToClipboard(pd.username || profileUsername)}
          class="!px-3 !py-1 !text-xs"
        />
      </div>
    </div>
  {/snippet}
  {#snippet bizItem(b: OfficialAccount)}
    <button class="wc-chat-item" class:wc-chat-active={curSession === b.username} class:wc-chat-pinned={b.pinned} onclick={() => selectSession(b)}>
      <div class="wc-avatar wc-avatar-official">
        {#if avatarCache[b.username]}<img src={avatarCache[b.username]} alt="" class="wc-avatar-img" />{:else}{avatarLetter(b.name||b.username)}{/if}
      </div>
      <div class="wc-chat-info">
        <div class="wc-chat-top">
          <span class="wc-chat-name-group">
            <span class="wc-chat-name">{b.name||b.username}</span>
            {#if b.official_kind === 'service'}
              <span class="wc-official-badge wc-official-badge-service">服务号</span>
            {:else if b.official_kind === 'enterprise'}
              <span class="wc-official-badge wc-official-badge-ent">企业号</span>
            {:else}
              <span class="wc-official-badge">公众号</span>
            {/if}
          </span>
          <span class="wc-chat-time-group">
            {#if b.pinned}
              <span class="wc-chat-pin" title="置顶会话">
                <svg viewBox="0 0 24 24" width="12" height="12" fill="currentColor" aria-hidden="true">
                  <path d="M16 3l5 5-4 1-3 4 1 5-2 2-4-4-4 4-1-1 4-4-4-4 2-2 5 1 4-3 1-4z"/>
                </svg>
              </span>
            {/if}
            <span class="wc-chat-time">{b.time}</span>
          </span>
        </div>
        <div class="wc-chat-bottom"><span class="wc-chat-preview">{b.summary||'暂无消息'}</span></div>
      </div>
    </button>
  {/snippet}
  {#snippet sessionItem(s: WeChatSession)}
    <button class="wc-chat-item" class:wc-chat-active={curSession === s.username} class:wc-chat-pinned={s.pinned}
      onclick={() => batchMode ? toggleSelectSession(s.username) : selectSession(s)}>
      {#if batchMode}
        <span class="wc-checkbox" class:wc-checkbox-on={selectedSessions[s.username]}>
          {selectedSessions[s.username] ? '✓' : ''}
        </span>
      {/if}
      <div class="wc-avatar" class:wc-avatar-official={s.is_official}>
        {#if avatarCache[s.username]}<img src={avatarCache[s.username]} alt="" class="wc-avatar-img" />
        {:else}<div class="wc-msg-letter" style="background:{colorFromName(s.name||s.username)}">{avatarLetter(s.name||s.username)}</div>{/if}
      </div>
      <div class="wc-chat-info">
        <div class="wc-chat-top">
          <span class="wc-chat-name-group">
            <span class="wc-chat-name">{s.name||s.username}</span>
            {#if s.is_hidden}
              <span class="wc-chat-hidden-badge" title="该会话在微信中被隐藏（折叠群聊/不显示），仍可查看聊天记录">已隐藏</span>
            {/if}
          </span>
          <span class="wc-chat-time-group">
            {#if s.pinned}
              <span class="wc-chat-pin" title="置顶会话">
                <svg viewBox="0 0 24 24" width="12" height="12" fill="currentColor" aria-hidden="true">
                  <path d="M16 3l5 5-4 1-3 4 1 5-2 2-4-4-4 4-1-1 4-4-4-4 2-2 5 1 4-3 1-4z"/>
                </svg>
              </span>
            {/if}
            <span class="wc-chat-time">{s.time}</span>
          </span>
        </div>
        <div class="wc-chat-bottom">
          <span class="wc-chat-preview">
            {#if s.draft && clearedDrafts[s.username] !== s.draft}
              <span class="wc-draft-tag">[草稿]</span>
              <span class="wc-draft-text">{s.draft}</span>
              <span class="wc-draft-clear" role="button" tabindex="0"
                onclick={(e) => { e.stopPropagation(); clearDraft(s); }}
                onkeydown={(e) => { if (e.key === 'Enter') { e.stopPropagation(); clearDraft(s); } }}
                title="清除草稿">✕</span>
            {:else}{s.summary}{/if}
          </span>
          {#if (s.unread_count ?? 0) > 0}<span class="wc-badge">{(s.unread_count ?? 0) > 99 ? '99+' : s.unread_count ?? 0}</span>{/if}
        </div>
      </div>
    </button>
  {/snippet}
  {#if panelLoading}
    <div class="wc-loading-overlay">
      <div class="wc-loading-spinner"></div>
      <div class="wc-loading-text">微信数据加载中…</div>
      <div class="wc-loading-sub">正在读取解密数据库</div>
    </div>
  {:else}
  <!-- 顶栏 -->
  <header class="wc-header">
    <div class="wc-header-left">
      <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" style="margin-right:6px">
        <path d="M8 12.5c0-1.5 2-2.5 4-2.5s4 1 4 2.5"/><circle cx="8" cy="8.5" r="1"/><circle cx="16" cy="8.5" r="1"/>
        <path d="M17.5 12c2.5 0 4.5 1.5 4.5 3.5S20 19 17.5 19"/><path d="M6.5 12C4 12 2 13.5 2 15.5S4 19 6.5 19"/>
      </svg>
      <span class="wc-header-title">微信数据</span>
      <span class="wc-dot wc-dot-on"></span>
      <span class="wc-status">运行中</span>
    </div>
    <div class="wc-header-right">
      <!-- 数据操作组：状态检查 / 图片体检 / 刷新 -->
      <div class="wc-header-group">
        <!-- DB 状态按钮 -->
        <WechatHoverButton
          text={dbStatusLoading ? '检查中' : 'DB 状态'}
          onclick={(e) => { e.stopPropagation(); toggleDbStatus(); }}
          class={showDbStatus ? 'wc-ihb-active' : ''}
        />

        <!-- 图片体检按钮：统计各会话缺失图片并导出清单 -->
        <WechatHoverButton
          text="图片体检"
          onclick={(e) => { e.stopPropagation(); openMissingImages(); }}
          title="统计各会话缺失图片（本地与 CDN 均无法获取）并导出清单"
        />

        <!-- 手动刷新按钮 -->
        <WechatHoverButton
          text={refreshing ? '刷新中…' : '刷新'}
          onclick={refreshData}
          disabled={refreshing}
          title="刷新会话与消息（重新解密最新数据）"
        />
      </div>

      <span class="wc-header-divider" aria-hidden="true"></span>

      <!-- 监控组：状态与启停控制 + 指标面板 -->
      <MonitorControl
        status={monitorStatus}
        loading={monitorLoading}
        canStart={monitorCanStart}
        onStart={startMonitor}
        onStop={stopMonitor}
      />
    </div>

    <!-- DB 状态弹窗 -->
    {#if showDbStatus}
      <DbStatusPopup
        loading={dbStatusLoading}
        lines={dbStatus}
        onClose={() => showDbStatus = false}
        onRefresh={checkDbStatus}
      />
    {/if}

    <!-- 图片体检弹窗 -->
    {#if missingImagesOpen}
      <div
        class="wc-checkup-overlay"
        onclick={(e) => { if (e.target === e.currentTarget) missingImagesOpen = false; }}
        role="dialog"
        aria-modal="true"
        aria-label="图片体检"
        tabindex="-1"
        onkeydown={(e) => e.key === 'Escape' && (missingImagesOpen = false)}
      >
        <div class="wc-checkup-dialog">
          <div class="wc-checkup-hd">
            <div class="wc-checkup-hd-left">
              <span
                class="wc-checkup-led"
                class:wc-checkup-led-bad={(missingImagesData?.missing ?? 0) > 0}
                class:wc-checkup-led-good={missingImagesData && (missingImagesData.missing ?? 0) === 0}
              ></span>
              <div class="wc-checkup-hd-text">
                <div class="wc-checkup-title">图片体检</div>
                {#if missingImagesData}
                  <div class="wc-checkup-meta">
                    扫描于 <b class="wc-mono">{missingImagesData.scanned_at}</b>
                    · {(missingImagesData.chats ?? []).length} 个会话 · {checkupMissingChats} 个存在缺失
                  </div>
                {:else}
                  <div class="wc-checkup-meta">扫描全部会话的图片可用性</div>
                {/if}
              </div>
            </div>
            <button class="wc-checkup-close" onclick={() => missingImagesOpen = false} aria-label="关闭" title="关闭">
              <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="M18 6 6 18M6 6l12 12"/></svg>
            </button>
          </div>

          <div class="wc-checkup-body">
            {#if missingImagesLoading && !missingImagesData}
              <div class="wc-checkup-skeleton" aria-label="正在扫描图片">
                <div class="wc-checkup-meter-grid">
                  {#each [1, 2, 3, 4] as _n}<div class="wc-checkup-meter wc-checkup-sk"></div>{/each}
                </div>
                <div class="wc-checkup-sk-bar"></div>
                <div class="wc-checkup-sk-line" style="width:46%"></div>
                <div class="wc-checkup-sk-line" style="width:30%"></div>
                <div class="wc-checkup-sk-table">
                  {#each [1, 2, 3, 4] as _n}<div class="wc-checkup-sk-row"></div>{/each}
                </div>
              </div>
            {:else if missingImagesData}
              <div class="wc-checkup-meter-grid">
                <div class="wc-checkup-meter">
                  <span class="wc-checkup-meter-label">总图片</span>
                  <span class="wc-checkup-meter-value">{missingImagesData.total_images}</span>
                </div>
                <div class="wc-checkup-meter wc-checkup-meter-ok">
                  <span class="wc-checkup-meter-label">本地可解</span>
                  <span class="wc-checkup-meter-value">{missingImagesData.local_ok}</span>
                </div>
                <div class="wc-checkup-meter wc-checkup-meter-cdn">
                  <span class="wc-checkup-meter-label">CDN 可下</span>
                  <span class="wc-checkup-meter-value">{missingImagesData.cdn_possible}</span>
                </div>
                <div class="wc-checkup-meter wc-checkup-meter-bad">
                  <span class="wc-checkup-meter-label">缺失</span>
                  <span class="wc-checkup-meter-value">{missingImagesData.missing}</span>
                </div>
              </div>

              <div class="wc-checkup-bar-wrap">
                <div class="wc-checkup-bar" role="img" aria-label="图片可用性占比：本地、CDN 与缺失">
                  <span class="wc-checkup-bar-ok" style="width:{checkupPct(missingImagesData.local_ok)}%"></span>
                  <span class="wc-checkup-bar-cdn" style="width:{checkupPct(missingImagesData.cdn_possible)}%"></span>
                  <span class="wc-checkup-bar-bad" style="width:{checkupPct(missingImagesData.missing)}%"></span>
                </div>
                <div class="wc-checkup-bar-legend">
                  <span class="wc-checkup-legend-item wc-legend-ok">本地 {checkupPct(missingImagesData.local_ok)}%</span>
                  <span class="wc-checkup-legend-item wc-legend-cdn">CDN {checkupPct(missingImagesData.cdn_possible)}%</span>
                  <span class="wc-checkup-legend-item wc-legend-bad">缺失 {checkupPct(missingImagesData.missing)}%</span>
                </div>
              </div>

              <p class="wc-checkup-hint">「缺失」= 本地无缓存且 CDN 网关无法获取（历史数据缺失）。微信稍后下载到本地会自动补显，也可导出清单人工核对。</p>

              {#if (missingImagesData.missing ?? 0) === 0}
                <div class="wc-checkup-healthy">
                  <svg viewBox="0 0 24 24" width="30" height="30" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="m8.5 12 2.4 2.4 4.6-4.8"/></svg>
                  <div>
                    <div class="wc-checkup-healthy-title">全部图片可用</div>
                    <div class="wc-checkup-healthy-sub">本地缓存与 CDN 均能取到，无需处理。</div>
                  </div>
                </div>
              {:else}
                <div class="wc-checkup-tools">
                  <div class="wc-checkup-search">
                    <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><circle cx="11" cy="11" r="7"/><path d="m21 21-4.35-4.35"/></svg>
                    <input bind:value={checkupQuery} placeholder="搜索会话名称或 wxid" aria-label="搜索会话" />
                  </div>
                  <label class="wc-checkup-chip" title="只显示存在缺失图片的会话">
                    <input type="checkbox" bind:checked={checkupOnlyMissing} />
                    <span>仅看缺失</span>
                  </label>
                  <select bind:value={checkupSort} class="wc-checkup-sort" aria-label="排序方式">
                    <option value="missing">缺失最多</option>
                    <option value="total">图片最多</option>
                    <option value="name">按名称</option>
                  </select>
                </div>

                <div class="wc-checkup-table-wrap">
                  <table class="wc-checkup-table">
                    <thead>
                      <tr>
                        <th>会话</th>
                        <th class="num">总图</th>
                        <th class="num">本地</th>
                        <th class="num">CDN</th>
                        <th class="num">缺失</th>
                        <th class="num">缺失率</th>
                      </tr>
                    </thead>
                    <tbody>
                      {#each checkupChats as c}
                        <tr class:wc-checkup-row-bad={(c.missing ?? 0) > 0}>
                          <td>
                            <div class="wc-checkup-name">{c.name || '—'}</div>
                            <div class="wc-checkup-wxid wc-mono">{c.username}</div>
                          </td>
                          <td class="num wc-mono">{c.total_images}</td>
                          <td class="num wc-mono">{c.local_ok}</td>
                          <td class="num wc-mono">{c.cdn_possible}</td>
                          <td class="num wc-mono wc-checkup-cell-bad">{c.missing}</td>
                          <td class="num">
                            <span class="wc-checkup-rate">
                              <span class="wc-checkup-rate-bar">
                                <span class="wc-checkup-rate-fill" style="width:{checkupRatePct(c)}%"></span>
                              </span>
                              <span class="wc-mono">{checkupRatePct(c)}%</span>
                            </span>
                          </td>
                        </tr>
                      {:else}
                        <tr><td colspan="6" class="wc-checkup-empty-row">没有匹配的会话</td></tr>
                      {/each}
                    </tbody>
                  </table>
                </div>
              {/if}
            {:else}
              <div class="wc-checkup-error" role="alert">
                <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="M10.3 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.7 3.86a2 2 0 0 0-3.4 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>
                <span>扫描失败或暂无数据</span>
                <WechatHoverButton text="重试" onclick={loadMissingImages} disabled={missingImagesLoading} />
              </div>
            {/if}
          </div>

          {#if missingImagesData}
            <div class="wc-checkup-ft">
              <span class="wc-checkup-ft-stat">缺失 <b class="wc-mono">{missingImagesData.missing}</b> 张</span>
              <div class="wc-checkup-ft-actions">
                <WechatHoverButton
                  text={missingExporting ? '导出中…' : '导出缺失清单 CSV'}
                  onclick={exportMissingImages}
                  disabled={missingExporting || (missingImagesData.missing ?? 0) === 0}
                  class="wc-checkup-primary"
                />
                <WechatHoverButton text={missingImagesLoading ? '扫描中…' : '重新扫描'} onclick={loadMissingImages} disabled={missingImagesLoading} />
                <WechatHoverButton text="关闭" onclick={() => missingImagesOpen = false} />
              </div>
            </div>
          {/if}
        </div>
      </div>
    {/if}
  </header>

  {#if accountStatus?.mismatch}
    <div class="wc-account-banner" role="alert">
      <span class="wc-account-banner-dot"></span>
      <span class="wc-account-banner-text">
        数据来源账号 <b class="wc-mono">{accountStatus.analysis_account}</b> 与当前微信登录账号
        <b class="wc-mono">{accountStatus.live_account}</b>{accountStatus.weixin_running ? '（运行中）' : ''} 不一致；
        当前展示的是历史账号数据。
      </span>
      <WechatHoverButton
        text={switchingAccount ? '切换中…' : '一键切换到当前账号并获取密钥'}
        onclick={switchToLiveAccount}
        disabled={switchingAccount}
        class="!px-3 !py-1 !text-xs shrink-0"
        title="将分析数据源切换到当前登录微信账号，并自动重新获取数据库密钥"
      />
      {#if switchAccountMsg}
        <span class="wc-account-banner-msg" class:wc-account-banner-err={switchAccountMsg.includes('失败')}>
          {switchAccountMsg}
        </span>
      {/if}
    </div>
  {/if}

  <div class="wc-body">
    <!-- 左侧导航 + 内容 -->
    <div class="wc-sidebar">
      <nav class="wc-nav">
        {#each NAV_GROUPS as group (group.label)}
          <div class="wc-nav-group">
            <span class="wc-nav-label">{group.label}</span>
            {#each group.items as item (item.tab)}
              <WechatHoverButton
                class={curTab === item.tab ? 'wc-ihb-active' : ''}
                onclick={() => switchTab(item.tab)}
              >
                <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true">{@html item.icon}</svg>
                <span>{item.label}</span>
              </WechatHoverButton>
            {/each}
          </div>
        {/each}
        <WechatHoverButton class="mt-auto {curTab === 'settings' ? 'wc-ihb-active' : ''}" onclick={() => switchTab('settings')}>
          <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="12" cy="12" r="3"/><path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42"/></svg>
          <span>微信配置</span>
        </WechatHoverButton>
      </nav>

      {#if curTab === 'chats'}
        <div class="wc-chat-list">
        <div class="wc-search">
          <input type="text" placeholder="搜索会话" bind:value={searchText} oninput={onSearchInput} />
          {#if visibleDraftCount > 0}
            <WechatHoverButton
    text={clearingAllDrafts ? '清除中…' : `草稿 ${visibleDraftCount}`}
              onclick={clearAllDrafts}
              disabled={clearingAllDrafts}
              title="清除全部草稿"
              class="!px-3 !py-1 !text-xs"
            />
          {/if}
          <WechatHoverButton
    text={batchMode ? '退出批量' : '批量'}
            onclick={toggleBatchMode}
            title="批量导出会话"
            class={batchMode ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'}
          />
        </div>
        {#if searchMode === 'message'}
          <div class="wc-search-results">
            {#if !msgSearchIndexed}
              <div class="wc-search-index-hint">
                <span>消息搜索索引尚未构建（当前为全表扫描）。首次搜索将自动构建，也可手动立即构建。</span>
                <WechatHoverButton
                  text={searchIndexBuilding ? '构建中…' : '构建搜索索引'}
                  onclick={() => buildSearchIndex()}
                  disabled={searchIndexBuilding}
                  class="!px-3 !py-1 !text-xs"
                />
              </div>
            {/if}
            {#if msgSearchLoading}
              <div class="wc-empty"><span class="wc-loading-inline"></span> 搜索中…</div>
            {:else if msgSearchError}
              <div class="wc-empty wc-error-hint"><p>⚠️ {msgSearchError}</p></div>
            {:else if msgSearchSearched && msgSearchResults.length === 0}
              <div class="wc-empty">未找到相关消息</div>
            {:else if msgSearchResults.length > 0}
              <div class="wc-search-hit-count">命中 {msgSearchResults.length} 条{msgSearchResults.length >= 200 ? '（已达单次上限 200 条，建议加长关键词缩小范围）' : ''} · 点击定位到原消息</div>
              {#each msgSearchResults as hit (hit.username + ':' + hit.local_id)}
                <button class="wc-search-hit" onclick={() => openSearchHit(hit)}>
                  <div class="wc-search-hit-top">
                    <span class="wc-search-hit-name">{hit.name}</span>
                    <span class="wc-search-hit-time">{hit.time}</span>
                  </div>
                  <div class="wc-search-hit-snippet">{hit.snippet}</div>
                </button>
              {/each}
            {/if}
          </div>
        {:else}
          <div class="wc-session-stats">
            <span>好友 {chatListStats.friends}</span>
            <span>群聊 {chatListStats.groups}</span>
            {#if chatListStats.unread > 0}<span class="wc-session-stats-unread">未读 {chatListStats.unread}</span>{/if}
            {#if searchText.trim()}<span>匹配 {filteredSessions.length}</span>{/if}
          </div>
          {#if batchMode}
            <div class="wc-batch-bar">
              <span class="wc-batch-count">已选 {selectedSessionList.length} 个</span>
              <WechatHoverButton text="全选" onclick={selectAllFiltered} class="!px-3 !py-1 !text-xs" />
              <WechatHoverButton text="导出TXT" onclick={() => doBatchExport('txt')} disabled={batchExporting || !selectedSessionList.length} class="!px-3 !py-1 !text-xs" />
              <WechatHoverButton text="导出CSV" onclick={() => doBatchExport('csv')} disabled={batchExporting || !selectedSessionList.length} class="!px-3 !py-1 !text-xs" />
            </div>
          {/if}
          {#if pinnedSessions.length > 0}
            {#if !pinnedCollapsed}
              {#each pinnedSessions as s (s.username)}
                {@render sessionItem(s)}
              {/each}
            {/if}
            <button class="wc-pinned-hd" onclick={togglePinnedCollapsed}
              title={pinnedCollapsed ? '展开置顶聊天' : '折叠置顶聊天'}
              aria-expanded={!pinnedCollapsed}>
              <span class="wc-pinned-hd-label">置顶聊天</span>
              <span class="wc-pinned-hd-count">{pinnedSessions.length}</span>
              <svg class="wc-pinned-hd-arrow" class:wc-pinned-hd-collapsed={pinnedCollapsed}
                viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
                <polyline points="6 9 12 15 18 9"/>
              </svg>
            </button>
          {/if}
          {#each normalSessions as s (s.username)}
            {@render sessionItem(s)}
          {/each}
          {#if sessionsError}
            <div class="wc-session-warn">
              <span class="wc-session-warn-icon">⚠️</span>
              <span>{sessionsError}</span>
            </div>
          {/if}
          {#if filteredSessions.length===0}
            {#if !sessionsLoading && !sessionsError}
              <div class="wc-empty">暂无会话</div>
            {:else if sessionsLoading}
              <div class="wc-empty">加载中…</div>
            {/if}
          {/if}
        {/if}
        </div>
      {:else if curTab === 'contacts'}
        <div class="wc-contact-list">
          <div class="wc-search">
            <input type="text" placeholder="全库搜索：昵称 / 备注 / 微信号 / 拼音" bind:value={contactSearch} />
<WechatHoverButton text="导出" onclick={() => runExportCmd('export_contacts_csv', '联系人')} disabled={genericExporting} title="导出全部联系人为CSV" class="!px-3 !py-1 !text-xs" />
          </div>
          <div class="wc-cat-bar">
            {#each ([['all','全部',null],['friend','好友','friend'],['group','群聊','group'],['official','公众号','official'],['service','服务号','service'],['enterprise','企业微信联系人','enterprise'],['member','群成员','member']] as const) as [k, label, statKey]}
              <WechatHoverButton
                text={label + (k === 'all' ? (contactStatsTotal != null ? ` (${contactStatsTotal})` : '') : (statKey && contactStats?.[statKey] != null) ? ` (${contactStats[statKey]})` : '')}
                onclick={() => contactCat = k}
                class={contactCat === k ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'}
              />
            {/each}
          </div>
          <div class="wc-contact-scroll" onscroll={onContactsScroll}>
            {#if contactsLoading}
              <div class="wc-empty"><span class="wc-loading-inline"></span> 加载中…</div>
            {:else if contactsError}
              <div class="wc-empty wc-error-hint">
                <p>⚠️ 通讯录加载失败</p>
                <p class="wc-error-text">{contactsError}</p>
                <WechatHoverButton text="重试" onclick={() => loadContacts(true)} class="!px-3 !py-1 !text-xs" />
              </div>
            {:else if groupedContacts.length===0}
              <div class="wc-empty">{contactSearch ? '无匹配联系人' : contactCat === 'all' ? '暂无联系人' : contactCat === 'friend' ? '暂无好友' : contactCat === 'group' ? '暂无群聊' : contactCat === 'member' ? '暂无群成员' : contactCat === 'enterprise' ? '暂无企业微信联系人' : contactCat === 'service' ? '暂无服务号' : '暂无公众号'}</div>
            {:else}
              {#each groupedContacts as [letter, list] (letter)}
                <div class="wc-letter-hd">{letter}{contactCat === 'member' ? `（${list.length}人）` : ''}</div>
                {#each list as c (c.username)}
                  <button class="wc-contact-item" class:wc-contact-active={inlineProfile && profileUsername === c.username} onclick={() => openContactProfile(c.username, c, true)}>
                    <div class="wc-avatar" class:wc-avatar-official={c.category==='official'}>
                      {#if avatarCache[c.username]}<img src={avatarCache[c.username]} alt="" class="wc-avatar-img" />
                      {:else}<div class="wc-msg-letter" style="background:{colorFromName(c.display_name||c.username)}">{avatarLetter(c.display_name||c.username)}</div>{/if}
                    </div>
                    <div class="wc-chat-info">
                      <div class="wc-chat-top">
                        <span class="wc-chat-name">{c.display_name}</span>
                        <span class="wc-contact-type">{c.local_type_label}</span>
                      </div>
                      <div class="wc-contact-sub">
                        {#if c.category==='group' && c.member_count != null}<span>{c.member_count}人</span>{/if}
                        {#if c.category==='group' && c.owner_name}<span>群主: {c.owner_name}</span>{/if}
                        {#if c.alias}<span>微信号: {c.alias}</span>{/if}
                        {#if !c.alias && c.category!=='group'}<span class="wc-contact-desc">{c.nick_name && c.remark ? '昵称: '+c.nick_name : c.username}</span>{/if}
                      </div>
                    </div>
                  </button>
                {/each}
              {/each}
              <!-- 底部加载状态 / 已加载数 -->
              <div class="wc-contact-footer">
                {#if contactsPage.loading}
                  <span class="wc-loading-inline-sm"></span> 加载中…
                {:else if contactsPage.hasMore}
                  <span class="wc-contact-hint">已显示 {contactsPage.items.length} / {contactsPage.total}，滚动加载更多</span>
                {:else if contactsPage.total > 0}
                  <span class="wc-contact-hint">已显示全部 {contactsPage.total} 项</span>
                {/if}
              </div>
            {/if}
          </div>
        </div>
      {:else if curTab === 'bizchats' || curTab === 'servicechats'}
        <div class="wc-chat-list">
          <div class="wc-favs-hd">
            <span>{curTab === 'bizchats' ? '公众号' : '服务号'}</span>
            <span class="wc-favs-count">{curTab === 'bizchats' ? bizSubscriptions.length : bizServices.length} 个{bizSearch ? ` · 匹配 ${curTab === 'bizchats' ? bizSubscriptionsFiltered.length : bizServicesFiltered.length}` : ''}</span>
          </div>
          <div class="wc-search">
            <input type="text" placeholder={curTab === 'bizchats' ? '搜索公众号' : '搜索服务号'} bind:value={bizSearch} />
          </div>
          {#if bizchatsLoading}<div class="wc-empty">加载中…</div>
          {:else if bizchatsError}<div class="wc-empty wc-error-hint">
            <p>⚠️ 加载失败</p>
            <p class="wc-error-text">{bizchatsError}</p>
            <WechatHoverButton text="重试" onclick={() => loadBizchats()} class="!px-3 !py-1 !text-xs" />
          </div>
          {:else if (curTab === 'bizchats' ? bizSubscriptionsFiltered.length : bizServicesFiltered.length)===0}
            <div class="wc-empty">{curTab === 'bizchats' ? '暂无公众号' : '暂无服务号'}</div>
          {:else}
            {#if curTab === 'bizchats'}
              {#each bizSubscriptionsFiltered as b (b.username)}
                {@render bizItem(b)}
              {/each}
            {:else}
              {#each bizServicesFiltered as b (b.username)}
                {@render bizItem(b)}
              {/each}
            {/if}
          {/if}
        </div>
      {:else if curTab === 'kefu'}
        <div class="wc-chat-list">
          <div class="wc-favs-hd">
            <span>客服会话</span>
            <span class="wc-favs-count">客服 {kefuSessions.length} 个 · 小程序客服 {miniappKefuSessions.length} 个 · 未读 {kefuUnread} 条</span>
          </div>
          <div class="wc-search">
            <input type="text" placeholder="搜索客服消息" bind:value={kefuSearch} />
          </div>
          {#if kefuSessionsFiltered.length === 0 && miniappKefuSessionsFiltered.length === 0}
            <div class="wc-empty">暂无客服消息</div>
          {:else}
            {#if kefuSessionsFiltered.length > 0}
              <div class="wc-sec-title">客服消息 ({kefuSessionsFiltered.length})</div>
              {#each kefuSessionsFiltered as s (s.username)}
                {@render sessionItem(s)}
              {/each}
            {/if}
            {#if miniappKefuSessionsFiltered.length > 0}
              <div class="wc-sec-title">小程序客服消息 ({miniappKefuSessionsFiltered.length})</div>
              {#each miniappKefuSessionsFiltered as s (s.username)}
                {@render sessionItem(s)}
              {/each}
            {/if}
          {/if}
        </div>
      {:else if curTab === 'favorites'}
        <div class="wc-chat-list">
          <div class="wc-favs-hd">
            <span>收藏</span>
            <span class="wc-favs-count">{filteredFavItems.length}{(favSearch || favType !== 'all') ? ` / ${favData.items?.length ?? 0}` : ''} 项</span>
            <WechatHoverButton
    text={favSelectMode ? '退出多选' : '多选'}
              onclick={toggleFavSelectMode}
              title="多选删除"
              class={favSelectMode ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'}
            />
<WechatHoverButton text="导出" onclick={() => runExportCmd('export_favorites_csv', '收藏')} disabled={genericExporting} title="导出全部收藏为CSV" class="!px-3 !py-1 !text-xs" />
          </div>
          <div class="wc-search wc-search-pad"><input type="text" placeholder="搜索标题 / 描述 / 来源" bind:value={favSearch} /></div>
          {#if favTypes.length > 1}
            <div class="wc-cat-bar wc-cat-bar-pad">
              <WechatHoverButton text={`全部 (${favData.items?.length ?? 0})`} onclick={() => favType = 'all'} class={favType === 'all' ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'} />
              {#each favTypes as t}
                <WechatHoverButton text={`${t} (${favTypeCounts.get(t) ?? 0})`} onclick={() => favType = t} class={favType === t ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'} />
              {/each}
            </div>
          {/if}
          {#if favSelectMode}
            <div class="wc-batch-bar wc-batch-bar-pad">
              <span class="wc-batch-count">已选 {favSelectedIds.length} 项</span>
              <WechatHoverButton text="删除所选" onclick={doDeleteFavorites} disabled={favDeleting || !favSelectedIds.length} class="!px-3 !py-1 !text-xs" />
            </div>
          {/if}
          {#if favoritesLoading}<div class="wc-empty">加载中…</div>
          {:else if favoritesError}<div class="wc-empty wc-error-hint">
            <p>⚠️ 收藏加载失败</p>
            <p class="wc-error-text">{favoritesError}</p>
            <WechatHoverButton text="重试" onclick={() => loadFavorites()} class="!px-3 !py-1 !text-xs" />
          </div>
          {:else if filteredFavItems.length===0}<div class="wc-empty">{(favSearch || favType !== 'all') ? '无匹配收藏' : '暂无收藏'}</div>
          {:else}
            {#each filteredFavItems as f (f.local_id)}
              <button class="wc-chat-item wc-fav-item" class:wc-chat-active={favDetail?.local_id === f.local_id}
                onclick={() => favSelectMode ? toggleFavSelect(f.local_id) : openFavDetail(f)}>
                {#if favSelectMode}
                  <span class="wc-checkbox" class:wc-checkbox-on={favSelected[f.local_id]}>{favSelected[f.local_id] ? '✓' : ''}</span>
                {/if}
                <div class="wc-fav-icon">{@html favIcon(f.type_label ?? '')}</div>
                <div class="wc-chat-info">
                  <div class="wc-chat-top">
                    <span class="wc-chat-name">{f.title || f.desc || '(无内容)'}</span>
                    <span class="wc-chat-time">{f.time}</span>
                  </div>
                  <div class="wc-chat-bottom">
                    <span class="wc-chat-preview">{f.type_label}{f.source ? ' · '+f.source : ''}</span>
                    {#if f.desc && f.title}<span class="wc-fav-list-desc">{f.desc}</span>{/if}
                  </div>
                </div>
              </button>
            {/each}
          {/if}
        </div>
      {/if}
    </div>

    <!-- 右侧主区域 -->
    <div class="wc-main">
      <WeChatSendDialog
        bind:open={sendDialogOpen}
        defaultPeer={curSession ?? ''}
        defaultName={curSessionInfo?.name ?? ''}
      />
      {#if curTab === 'ask'}
        <AskPanel onJump={openAskCitation} />
      {:else if curTab === 'graph'}
        <RelationshipGraph onOpenChat={openGraphChat} />
      {:else if curTab === 'monitor'}
        <GroupMonitor onJump={openAskCitation} />
      {:else if curTab === 'privacy'}
        <PrivacyScan onJump={openAskCitation} />
      {:else if curTab === 'revoked'}
        <RevokedMessages />
      {:else if curTab === 'backup'}
        <BackupManager />
      {:else if curTab === 'storage'}
        <StorageSpace onOpenChat={openRecordSession} />
      {:else if curTab === 'overview'}
        <DataOverview
          onNavigate={(t) => switchTab(t as Tab)}
          onOpenAuthor={(a) => {
            // 先设作者（此时不在朋友圈页，不会触发加载），再切页签：
            // refreshMomentsAuto 同步读取 momentAuthor，单次请求直达该好友的朋友圈
            setMomentAuthor({ username: a.username, name: a.name });
            switchTab('moments');
          }}
        />
      {:else if curTab === 'annual'}
        <AnnualSummary />
      {:else if curTab === 'dailysummary'}
        <DailySummary />
      {:else if curTab === 'hook'}
        <HookManager />
      {:else if curTab === 'records'}
        <GeneralRecords onopen={openRecordSession} />
      {:else if curTab === 'chats' || curTab === 'bizchats' || curTab === 'servicechats' || curTab === 'kefu'}
        {#if curSession}
          <div class="wc-chat-hd">
            <div class="wc-chat-hd-info">
              <span class="wc-chat-hd-name">{curSessionInfo?.name || curSession}</span>
              <span class="wc-chat-hd-user">{curSession}{msgTotal ? ` · 共 ${msgTotal} 条消息` : ''}</span>
              {#if msgTypeStats.length > 0}
                <span class="wc-msg-type-chips">
                  {#each msgTypeStats.slice(0, 5) as t}
                    <span class="wc-msg-type-chip" title="{t.label}共 {t.count} 条">{t.label} {t.count}</span>
                  {/each}
                  {#if msgTypeStats.length > 5}
                    <span class="wc-msg-type-chip" title="其余类型合计">{'其他 +' + msgTypeStats.slice(5).reduce((a, t) => a + t.count, 0)}</span>
                  {/if}
                </span>
              {/if}
            </div>
<WechatHoverButton text="日历" onclick={(e) => { e.stopPropagation(); openCalendar(); }} title="消息日历（每日消息数）" class="!px-3 !py-1 !text-xs" />
<WechatHoverButton text="发消息" onclick={(e) => { e.stopPropagation(); sendDialogOpen = true; }} title={curSessionInfo?.is_group ? '通过 ClawBot 给该群发消息' : '通过 ClawBot 给该好友发消息'} class="!px-3 !py-1 !text-xs" />
<WechatHoverButton text="附件" onclick={(e) => { e.stopPropagation(); openWechatAttachFolder({ username: curSession ?? '' }).catch((err) => logError('openAttach', err)); }} title="打开该会话的附件文件夹" class="!px-3 !py-1 !text-xs" />
<WechatHoverButton text="资料" onclick={(e) => { e.stopPropagation(); openContactProfile(); }} title="查看联系人资料" class="!px-3 !py-1 !text-xs" />
<WechatHoverButton text="导出" onclick={(e) => { e.stopPropagation(); exportOpen = true; exportError = ''; exportSuccess = null; }} title="导出消息记录" class="!px-3 !py-1 !text-xs" />
<WechatHoverButton text="清空" onclick={(e) => { e.stopPropagation(); clearConfirmOpen = true; }} title="清空本地聊天记录" class="!px-3 !py-1 !text-xs" />
          </div>
          <MessageList
            bind:this={msgListRef}
            {messages}
            loading={msgsLoading}
            error={msgsError}
            hasMore={hasMoreMsgs}
            {curSession}
            {officialHistory}
            {rowCtx}
            {rowActions}
            onLoadMore={loadMoreMsgs}
            onOpenUrl={openUrl}
            onVisibleChange={(msgs) => { msgVisibleWindow = msgs; }}
          />
        {:else}
          <div class="wc-no-session">
            <GargantuaBackdrop />
            <div class="wc-ns-hint">
              <span class="wc-ns-hint-title">从左侧选择一个会话</span>
              <span class="wc-ns-hint-sub">支持搜索、批量操作与消息导出</span>
            </div>
          </div>
        {/if}


      {:else if curTab === 'contacts'}
        {#if inlineProfile && profileData}
          <div class="wc-contact-profile-pane">
            <div class="wc-contact-profile-hd">
              <span class="wc-contact-profile-title">{(profileData.username || profileUsername || '').endsWith('@chatroom') ? '群聊资料' : '联系人资料'}</span>
              <button class="wc-export-close" onclick={() => inlineProfile = false} title="返回列表">×</button>
            </div>
            <div class="wc-contact-profile-body">
              {@render profileCard(profileData)}
            </div>
          </div>
        {:else}
          <div class="wc-empty" style="height:100%"><p style="color:var(--wc-muted)">{contactCat === 'all' ? '全部' : contactCat === 'friend' ? '好友' : contactCat === 'group' ? '群聊' : contactCat === 'member' ? '群成员' : contactCat === 'enterprise' ? '企业微信联系人' : contactCat === 'service' ? '服务号' : '公众号'}共 {contactsPage.total} 项{contactsPage.hasMore ? `（已加载 ${contactsPage.items.length}）` : ''} · 点击左侧联系人查看资料卡，可在资料卡中发消息</p></div>
        {/if}

      {:else if curTab === 'moments'}
        {@const groupedMoments = groupMomentsByDate(filteredMoments)}
        <div class="wc-moments">
          <div class="wc-moments-toolbar">
            <div class="wc-moments-title">
              <span class="wc-moments-name">朋友圈</span>
              {#if momentAuthor}
                <span class="wc-moments-filtered">正在看「{momentAuthor.name}」</span>
              {/if}
              <span class="wc-moments-count">{momentSearch ? `${filteredMoments.length} / ${momentsPage.total}` : momentsPage.total} 条</span>
            </div>
            <div class="wc-moments-actions">
              <input class="wc-moments-search" type="text" placeholder="搜索作者 / 内容 / 位置" bind:value={momentSearch} />
              {#if momentAuthor}
                <WechatHoverButton text="返回全部" onclick={() => setMomentAuthor(null)} class="!px-3 !py-1 !text-xs" />
              {/if}
              <WechatHoverButton text={momentsRefreshing ? '刷新中…' : '刷新'} onclick={() => refreshMomentsAuto(true)} disabled={momentsRefreshing} title="同步微信端最新朋友圈（每 15 秒自动刷新）" class="!px-3 !py-1 !text-xs" />
              <select
                bind:value={momentExportFormat}
                class="wc-moments-fmt"
                aria-label="导出格式"
                title="导出格式"
                disabled={momentExporting || genericExporting}
              >
                <option value="csv">CSV</option>
                <option value="json">JSON</option>
                <option value="txt">TXT</option>
                <option value="html">HTML</option>
              </select>
              <WechatHoverButton text={momentExporting ? '导出中…' : '导出'} onclick={() => runMomentsExport()} disabled={momentExporting || genericExporting} title={momentAuthor ? `导出「${momentAuthor.name}」的朋友圈（当前筛选，格式 ${momentExportFormat.toUpperCase()}）` : `导出全部朋友圈（格式 ${momentExportFormat.toUpperCase()}）`} class="!px-3 !py-1 !text-xs" />
            </div>
          </div>
          {#if momentsInsight}
            {@const mi = momentsInsight}
            {@const maxPosts = Math.max(1, ...mi.monthly.map((x) => x.posts))}
            {@const rawMax = Math.max(0, ...mi.monthly.map((x) => x.posts))}
            {@const peakMonth = rawMax > 0 ? mi.monthly.find((x) => x.posts === rawMax) : undefined}
            {@const monthsTotal = mi.monthly.reduce((s, x) => s + x.posts, 0)}
            <div class="wc-moments-insight">
              <div class="wc-mi-stats">
                <div class="wc-mi-stat"><span class="wc-mi-num">{mi.total}</span><span class="wc-mi-label">总动态</span></div>
                <div class="wc-mi-stat"><span class="wc-mi-num">{mi.with_images}</span><span class="wc-mi-label">含图片</span></div>
                <div class="wc-mi-stat"><span class="wc-mi-num">{mi.with_videos}</span><span class="wc-mi-label">含视频</span></div>
                <div class="wc-mi-stat"><span class="wc-mi-num">{mi.with_location}</span><span class="wc-mi-label">带位置</span></div>
                <div class="wc-mi-stat"><span class="wc-mi-num">{mi.with_link}</span><span class="wc-mi-label">分享链接</span></div>
                <button type="button" class="wc-mi-stat" class:wc-mi-stat-on={momentSelfOnly} title="点击只看自己发布的动态"
                  onclick={() => momentSelfOnly = !momentSelfOnly}>
                  <span class="wc-mi-num">{mi.self_posts}</span><span class="wc-mi-label">我发布的</span>
                </button>
              </div>
              <div class="wc-mi-body">
                <div class="wc-mi-authors">
                  <span class="wc-mi-hd">活跃作者 Top 5（点击只看 TA 的朋友圈）</span>
                  <div class="wc-mi-author-list">
                    {#each mi.top_authors.slice(0, 5) as a, i (a.username)}
                      <button type="button" class="wc-mi-author" class:wc-mi-author-on={momentAuthor?.username === a.username}
                        title="只看 {a.name} 的朋友圈"
                        onclick={() => setMomentAuthor(momentAuthor?.username === a.username ? null : { username: a.username, name: a.name })}>
                        <span class="wc-mi-rank">{i + 1}</span><span class="wc-mi-author-name" title={a.name}>{a.name}</span><span class="wc-mi-posts">{a.posts}</span>
                      </button>
                    {/each}
                  </div>
                </div>
                <div class="wc-mi-months">
                  <div class="wc-mi-months-hd">
                    <span class="wc-mi-hd">近 12 个月发圈热度</span>
                    {#if mi.monthly.length > 0}
                      <span class="wc-mi-months-meta">
                        {mi.monthly[0].month} ~ {mi.monthly[mi.monthly.length - 1].month} · 合计 {monthsTotal} 条
                        {#if peakMonth && peakMonth.posts > 0}· 峰值 {Number(peakMonth.month.slice(5))}月 {peakMonth.posts} 条{/if}
                      </span>
                    {/if}
                  </div>
                  <div class="wc-mi-bars">
                    {#each mi.monthly as m (m.month)}
                      <div
                        class="wc-mi-bar-col"
                        class:wc-mi-bar-col-peak={peakMonth?.month === m.month}
                        title="{m.month} · {m.posts} 条"
                      >
                        <span class="wc-mi-bar-val" class:wc-mi-bar-val-zero={m.posts === 0}>{m.posts}</span>
                        <div class="wc-mi-bar">
                          <div class="wc-mi-bar-fill" style="height:{Math.round((m.posts / maxPosts) * 100)}%"></div>
                        </div>
                        <span class="wc-mi-bar-label">{Number(m.month.slice(5))}月</span>
                      </div>
                    {/each}
                  </div>
                </div>
              </div>
            </div>
          {/if}
          <div class="wc-moments-scroll" onscroll={onMomentsScroll}>
          {#if momentsPage.loading && momentsPage.items.length === 0}<div class="wc-empty">加载中…</div>
          {:else if momentsError}<div class="wc-empty wc-error-hint">
            <p>⚠️ 朋友圈加载失败</p>
            <p class="wc-error-text">{momentsError}</p>
            <WechatHoverButton text="重试" onclick={() => loadMoments(true)} class="!px-3 !py-1 !text-xs" />
          </div>
          {:else if filteredMoments.length===0}<div class="wc-empty">{momentSearch || momentAuthor || momentSelfOnly ? '无匹配动态' : '暂无动态'}</div>
          {:else}
            {#each groupedMoments as g (g.dateKey)}
              <div class="wc-moment-day">
                <span class="wc-moment-day-label">{g.label}</span>
                <span class="wc-moment-day-count">{g.items.length} 条</span>
              </div>
              {#each g.items as m, i (m.tid || i)}
                <div class="wc-moment-card">
                  <div class="wc-moment-avatar">
                    {#if avatarCache[m.username]}<img src={avatarCache[m.username]} alt="" />
                    {:else}<div class="wc-msg-letter" style="background:{colorFromName(m.author||'?')}">{avatarLetter(m.author||'?')}</div>{/if}
                  </div>
                  <div class="wc-moment-body">
                    <div class="wc-moment-meta">
                      <span class="wc-moment-author">{m.author}</span>
                      <span class="wc-moment-time">{m.time}</span>
                    </div>
                    {#if m.text}<div class="wc-moment-content">{m.text}</div>{/if}
                    {#if m.images && m.images.length > 0}
                      <div class="wc-moment-images" class:wc-moment-images-single={m.images.length === 1} class:wc-moment-images-four={m.images.length === 4}>
                        {#each m.images as img, idx (img.url || img.thumb || idx)}
                          {@const imgState = momentMedia.imgCache[momentImgKey(img.thumb || img.url, img.key || '')]}
                          <div class="wc-moment-img-wrap" class:wc-moment-img-fail={imgState === ''} class:wc-moment-img-loading={imgState === undefined}>
                            {#if imgState}
                              <button type="button"
                                style="padding:0;border:none;background:none;cursor:pointer;display:inline-flex"
                                onclick={() => openMomentViewer(m, idx)}
                                onkeydown={(e) => e.key === 'Enter' && openMomentViewer(m, idx)}>
                                <img src={imgState} alt="朋友圈图片"
                                  onerror={(e) => { const t = e.target as HTMLElement; t.style.display='none'; t.parentElement?.classList.add('wc-moment-img-fail'); }} />
                              </button>
                            {:else if imgState === undefined}
                              <span class="wc-loading-inline-sm" title="图片加载中…"></span>
                            {/if}
                          </div>
                        {/each}
                      </div>
                    {/if}
                    {#if m.videos && m.videos.length > 0}
                      <div class="wc-moment-videos" class:wc-moment-videos-single={m.videos.length === 1}>
                        {#each m.videos as v, vi (v.url || vi)}
                          {@const vCover = v.thumb_is_image ? momentMedia.imgCache[momentImgKey(v.thumb, v.key || '')] : ''}
                          <div class="wc-moment-video-tile" role="button" tabindex="0" title="点击播放视频"
                            onclick={() => playMomentVideo(m, vi)}
                            onkeydown={(e) => e.key === 'Enter' && playMomentVideo(m, vi)}>
                            {#if v.thumb_is_image && vCover}
                              <img src={vCover} alt="朋友圈视频封面" />
                            {:else}
                              <span class="wc-moment-video-ph"></span>
                            {/if}
                            <span class="wc-moment-video-badge">▶</span>
                            {#if v.duration > 0}<span class="wc-moment-video-dur">{fmtDur(v.duration)}</span>{/if}
                          </div>
                        {/each}
                      </div>
                    {/if}
                    {#if m.media_desc || m.link_title || m.location}
                      <div class="wc-moment-tags">
                        {#if m.media_desc && (!m.images || m.images.length === 0) && (!m.videos || m.videos.length === 0)}<span class="wc-moment-media">{@html iconSvg(ICON_PATHS.image, 14)} {m.media_desc}</span>{/if}
                        {#if m.link_title}<span class="wc-moment-media">{@html iconSvg(ICON_PATHS.link, 14)} {m.link_title}</span>{/if}
                        {#if m.location}<span class="wc-moment-media">{@html iconSvg(ICON_PATHS.pin, 14)} {m.location}</span>{/if}
                      </div>
                    {/if}
                    {#if (m.likes && m.likes.length > 0) || (m.comments && m.comments.length > 0)}
                      <div class="wc-moment-actions">
                        {#if m.likes && m.likes.length > 0}
                          <div class="wc-moment-likes">
                            <span class="wc-moment-like-icon">❤</span>
                            <span>{m.likes.map((l) => l.nickname || l.username || '未知').join('、')}</span>
                          </div>
                        {/if}
                        {#if m.comments && m.comments.length > 0}
                          <div class="wc-moment-comments">
                            {#each m.comments as c}
                              <div class="wc-moment-comment-item">
                                <span class="wc-moment-comment-name">{c.nickname || c.username || '未知'}</span>
                                {#if c.to_username && c.to_username !== m.username}
                                  <span class="wc-moment-comment-reply">回复 {c.to_nickname || c.to_username}</span>
                                {/if}
                                <span class="wc-moment-comment-text">{c.content || ''}</span>
                              </div>
                            {/each}
                          </div>
                        {/if}
                      </div>
                    {/if}
                  </div>
                </div>
              {/each}
            {/each}
              <!-- 底部加载状态 -->
              <div class="wc-moment-footer">
                {#if momentsPage.loading}
                  <span class="wc-loading-inline-sm"></span> 加载中…
                {:else if momentsPage.hasMore}
                  <span class="wc-moment-hint">已加载 {momentsPage.items.length} / {momentsPage.total}，滚动加载更多</span>
                {:else if momentsPage.total > 0}
                  <span class="wc-moment-hint">已加载全部 {momentsPage.total} 条动态</span>
                {/if}
              </div>
            {/if}
          </div>
        </div>

      {:else if curTab === 'favorites'}
        {#if favDetailLoading}<div class="wc-empty" style="height:100%"><span class="wc-loading-inline"></span> 加载详情…</div>
        {:else if favDetail}
          {@const fd = favDetail}
          <div class="wc-fav-detail">
            <div class="wc-fav-detail-hd">
              <div class="wc-fav-detail-hd-info">
                <WechatHoverButton text="← 返回列表" onclick={() => (favDetail = null)} title="返回收藏列表" class="!px-3 !py-1 !text-xs" />
                <div class="wc-fav-detail-title">{fd.title || fd.text || '收藏'}</div>
              </div>
              <div class="wc-fav-detail-meta">
                <span class="wc-contact-type">{fd.type_label}</span>
                {#if fd.source}<span>来源: {fd.source}</span>{/if}
                <span>{fd.time}</span>
              </div>
            </div>
            <div class="wc-fav-detail-body">
              {#if fd.text}<div class="wc-fav-detail-text">{fd.text}</div>{/if}
              {#if fd.images?.length}
                <div class="wc-fav-detail-imgs">
                  {#each fd.images as md5, i (md5)}
                    <div class="wc-fav-detail-img" use:lazyLoadFavImage={md5} role="button" tabindex="0"
                      onclick={() => openFavImageViewer(fd.images, i)}
                      onkeydown={(e) => e.key === 'Enter' && openFavImageViewer(fd.images, i)}>
                      {#if favImageMap[md5] && favImageMap[md5] !== 'loading'}
                        <img src={favImageMap[md5]} alt="收藏图片" loading="lazy" />
                      {:else if favImageMap[md5] === 'loading'}
                        <span class="wc-fav-img-ph">加载图片…</span>
                      {/if}
                    </div>
                  {/each}
                </div>
              {/if}
              {#if fd.voice_server_id}
                {@const vsid = fd.voice_server_id}
                <div class="wc-fav-detail-row">
                  <WechatHoverButton
                    onclick={() => playFavoriteVoice(vsid)}
                    class="!px-3 !py-1 !text-xs"
                  >
                    {#if favVoiceMap[vsid]}
                      <svg viewBox="0 0 24 24" width="11" height="11" fill="currentColor" aria-hidden="true"><rect x="6" y="5" width="4" height="14" rx="1"/><rect x="14" y="5" width="4" height="14" rx="1"/></svg>
                    {:else}
                      <svg viewBox="0 0 24 24" width="11" height="11" fill="currentColor" aria-hidden="true"><path d="M8 5v14l11-7z"/></svg>
                    {/if}
                    <span>播放收藏语音</span>
                  </WechatHoverButton>
                  {#if favVoiceMap[vsid]}
                    <audio src={favVoiceMap[vsid]} autoplay onended={() => (favVoiceMap[vsid] = '')}></audio>
                  {/if}
                </div>
              {/if}
              {#if fd.video}
                {@const vd = fd.video}
                <div class="wc-fav-detail-row">
                  {@html iconSvg(ICON_PATHS.video, 15)} 视频{vd.duration ? ` · ${Math.round(vd.duration)}″` : ''}
                  {#if apiRoot && vd.md5}
                    <WechatHoverButton
                      text="播放视频"
                      onclick={() => {
                        fileVideoSrc = apiAssetUrl(`/file/video/${vd.md5}`);
                        fileVideoPath = '';
                        fileVideoOpen = true;
                      }}
                      class="!px-3 !py-1 !text-xs"
                    />
                  {:else if !apiRoot}
                    <span class="wc-fav-detail-note">未启用本地 API，视频预览不可用</span>
                  {/if}
                </div>
              {/if}
              {#if fd.link}
                {@const lk = fd.link}
                <div class="wc-fav-detail-link">
                  <div class="wc-fav-detail-link-title">{lk.title || '链接'}</div>
                  {#if lk.url}<div class="wc-fav-detail-link-url">{lk.url}</div>{/if}
{#if lk.url}<WechatHoverButton text="打开链接" onclick={() => openUrl(lk.url)} class="!px-3 !py-1 !text-xs" />{/if}
                </div>
              {/if}
              {#if fd.location}
                <div class="wc-fav-detail-row">{@html iconSvg(ICON_PATHS.pin, 15)} {fd.location.name}{fd.location.label && fd.location.label !== fd.location.name ? ` · ${fd.location.label}` : ''}</div>
              {/if}
              {#if fd.file}
                <div class="wc-fav-detail-file">
                  <span class="wc-fav-detail-file-name">📄 {fd.file.name || '文件'}</span>
                  <span class="wc-fav-detail-file-meta">{fd.file.ext?.toUpperCase() || '文件'}{fd.file.size ? ' · ' + favFileSize(fd.file.size) : ''}</span>
                </div>
              {/if}
              {#if fd.items?.length}
                <div class="wc-fav-detail-items">
                  {#each fd.items as it, i (i)}
                    <div class="wc-fav-detail-item">
                      {#if it.type === 'text'}
                        <span>{it.text}</span>
                      {:else}
                        <span class="wc-fav-detail-item-label">{@html iconSvg(ICON_PATHS.link, 14)} {it.text}{it.des ? ' ' + it.des : ''}</span>
                      {/if}
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
          </div>
        {:else}
          <div class="wc-empty" style="height:100%"><p style="color:var(--wc-muted)">点击左侧收藏查看详细内容</p></div>
        {/if}

      {:else if curTab === 'emoticons'}
        <div class="wc-db-view">
          <div class="wc-db-hd"><span>表情</span><span class="wc-db-count">自定义 {filteredEmoCustom.length} · 本地静态 {filteredStaticEmoticons.length} · 表情包 {filteredEmoPackages.length}</span></div>
          <div class="wc-search wc-search-pad"><input type="text" placeholder="搜索表情包名称 / 表情 MD5 / 本地表情名称" bind:value={emoSearch} /></div>
          {#if emoticonsError || staticEmoticonsError}<div class="wc-empty wc-error-hint">
            <p>⚠️ 表情数据加载失败</p>
            <p class="wc-error-text">{emoticonsError || staticEmoticonsError}</p>
            <WechatHoverButton text="重试" onclick={() => { loadEmoticons(); loadStaticEmoticons(); }} class="!px-3 !py-1 !text-xs" />
          </div>
          {:else if (emoticonsLoading || staticEmoticonsLoading) && !((emoticons.custom?.length || 0) > 0 || (emoticons.packages?.length || 0) > 0 || staticEmoticons.length > 0)}<div class="wc-empty"><span class="wc-loading-inline"></span> 加载中…</div>
          {:else}
            <div class="wc-emo-tabs">
              <WechatHoverButton text="全部" onclick={() => emoTab = 'all'} class={emoTab === 'all' ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'} />
              {#if filteredEmoCustom.length > 0}
                <WechatHoverButton text={`自定义 (${filteredEmoCustom.length})`} onclick={() => emoTab = 'custom'} class={emoTab === 'custom' ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'} />
              {/if}
              {#if filteredStaticEmoticons.length > 0}
                <WechatHoverButton text={`本地静态 (${filteredStaticEmoticons.length})`} onclick={() => emoTab = 'static'} class={emoTab === 'static' ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'} />
              {/if}
              {#if filteredEmoPackages.length > 0}
                <WechatHoverButton text={`表情包 (${filteredEmoPackages.length})`} onclick={() => emoTab = 'packages'} class={emoTab === 'packages' ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'} />
              {/if}
            </div>
            {#if emoTab === 'custom' || emoTab === 'all'}
              {#if filteredEmoCustom.length > 0}
                <div class="wc-sec-title">自定义表情 <span class="wc-sec-count">（点击复制 MD5）</span></div>
                {#if !apiRoot}
                  <p class="wc-emo-api-hint">未启用本地 API 服务，自定义表情暂无法加载图片（可在「微信配置」中开启 HTTP API）。</p>
                {/if}
                <div class="wc-emo-grid">
                  {#each filteredEmoCustom.slice(0, 400) as e (e.md5)}
                    {@const emoUrl = apiAssetUrl(`/emoticon/${e.md5}`)}
                    <button class="wc-emo-cell" title="{e.md5}{e.size_label ? ' · '+e.size_label : ''} 点击复制"
                      onclick={() => copyTextToClipboard(e.md5)}>
                      {#if e.item_type === 3}<span class="wc-emo-gif">动图</span>{/if}
                      {#if emoUrl && !emoImgFailed[e.md5]}
                        <img src={emoUrl} alt="" class="wc-emo-img" loading="lazy" referrerpolicy="no-referrer"
                          onerror={() => emoImgError(e.md5)} />
                      {:else}
                        <span class="wc-emo-ph">{@html iconSvg(e.item_type === 3 ? ICON_PATHS.film : ICON_PATHS.image, 24)}</span>
                        <span class="wc-emo-md5">{e.md5.slice(0, 8)}</span>
                      {/if}
                    </button>
                  {/each}
                </div>
              {/if}
            {/if}
            {#if emoTab === 'static' || emoTab === 'all'}
              {#if filteredStaticEmoticons.length > 0}
                <div class="wc-sec-title">本地静态表情</div>
                <div class="wc-emo-tabs">
                  {#each staticEmoCategories as cat}
                    <WechatHoverButton text={cat.label} onclick={() => staticEmoCat = cat.key} class={staticEmoCat === cat.key ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'} />
                  {/each}
                </div>
                <div class="wc-search wc-search-pad"><input type="text" class="wc-static-emo-search" placeholder="搜索本地表情" bind:value={staticEmoSearch} /></div>
                <div class="wc-static-emo-grid">
                  {#each filteredStaticEmoticons as item (`${item.category}-${item.file.name}`)}
                    <div class="wc-static-emo-cell" title="{item.file.name.replace(/\.png$/i,'')}">
                      <img src={item.file.path} alt="" class="wc-static-emo-img" loading="lazy" />
                      <span class="wc-static-emo-name">{item.file.name.replace(/\.png$/i,'').slice(0, 10)}</span>
                    </div>
                  {/each}
                </div>
              {/if}
            {/if}
            {#if emoTab === 'packages' || emoTab === 'all'}
              {#if filteredEmoPackages.length > 0}
                <div class="wc-sec-title">表情包</div>
                <div class="wc-pkg-grid">
                  {#each filteredEmoPackages as p (p.package_id || p.name)}
                    <div class="wc-pkg-card">
                      <div class="wc-pkg-icon">🙂</div>
                      <div class="wc-pkg-name">{p.name || p.package_id || '未命名'}</div>
                      <div class="wc-pkg-sub">{p.count ? p.count+' 个表情' : ''}</div>
                    </div>
                  {/each}
                </div>
              {/if}
            {/if}
            {#if emoActiveCount === 0}
              <div class="wc-empty">{emoSearch || staticEmoSearch ? '无匹配表情' : '暂无表情数据'}</div>
            {/if}
          {/if}
        </div>

      {:else if curTab === 'files'}
        <div class="wc-db-view">
          <div class="wc-db-hd">
            <span>文件管理</span>
            <span class="wc-db-count">共 {(fileData.images_total ?? 0) + (fileData.videos_total ?? 0) + (fileData.files_total ?? 0)} 项 · {fileData.total_size_label}{fileSearch ? ` · 匹配 ${shownFiles.length} 项` : ''}</span>
<WechatHoverButton text="刷新" onclick={() => loadFiles()} title="刷新文件列表" class="!px-3 !py-1 !text-xs" />
          </div>
          <div class="wc-search wc-search-pad"><input type="text" placeholder="搜索文件名 / MD5" bind:value={fileSearch} oninput={() => fileListLimit = FILE_PAGE} /></div>
          <div class="wc-file-tabs">
            {#each ([['all','全部', shownFiles.length],['image','图片', fileImagesTotal],['video','视频', fileVideosTotal],['file','文件', fileDocsTotal]] as const) as [k, label, cnt]}
              <WechatHoverButton
                text={`${label} (${cnt})`}
                onclick={() => { fileCat = k; fileListLimit = FILE_PAGE; }}
                class={fileCat === k ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'}
              />
            {/each}
          </div>
          {#if !apiRoot}
            <p class="wc-emo-api-hint">未启用本地 API 服务，图片/视频预览不可用（可在「微信配置」中开启 HTTP API），文件仍可打开/定位。</p>
          {/if}
          {#if filesLoading}<div class="wc-empty">加载中…</div>
          {:else if filesError}<div class="wc-empty wc-error-hint">
            <p>⚠️ 文件数据加载失败</p>
            <p class="wc-error-text">{filesError}</p>
            <WechatHoverButton text="重试" onclick={() => loadFiles()} class="!px-3 !py-1 !text-xs" />
          </div>
          {:else if shownFiles.length===0}<div class="wc-empty">{fileSearch ? '无匹配文件' : fileCat === 'image' ? '暂无图片' : fileCat === 'video' ? '暂无视频' : fileCat === 'file' ? '暂无文件' : '暂无文件记录'}</div>
          {:else}
            {#if fileCat === 'all' || fileCat === 'image'}
              {#if fileImages.length > 0}
                <div class="wc-sec-title">图片 <span class="wc-sec-count">（显示最近 {fileImages.length} / 共 {fileData.images_total ?? 0}）</span></div>
                <div class="wc-file-img-grid">
                  {#each fileImages as f (f.md5)}
                    <button class="wc-file-img-card" class:wc-file-img-card-off={!apiRoot} title="{f.file_name || f.md5} · 点击查看大图"
                      onclick={() => openFileImageViewer(f)}>
                      {#if apiRoot && !fileImgFailed[f.md5]}
                        <img src={apiAssetUrl(`/file/image/${f.md5}`)} alt="" class="wc-file-img-thumb" loading="lazy"
                          onerror={() => { fileImgFailed = { ...fileImgFailed, [f.md5]: true }; }} />
                      {:else}
                        <span class="wc-file-img-ph">{@html iconSvg(ICON_PATHS.image, 34)}</span>
                      {/if}
                      <span class="wc-file-img-name">{f.file_name || f.md5}</span>
                      <span class="wc-file-img-meta">{f.size_label}</span>
                    </button>
                  {/each}
                </div>
              {/if}
            {/if}
            {#if fileCat === 'all' || fileCat === 'video'}
              {#if fileVideos.length > 0}
                <div class="wc-sec-title">视频 <span class="wc-sec-count">（显示最近 {fileVideos.length} / 共 {fileData.videos_total ?? 0}）</span></div>
                <div class="wc-file-video-grid">
                  {#each fileVideos as f (f.md5)}
                    <button class="wc-file-video-card" title="{f.file_name || f.md5} · 点击播放"
                      onclick={() => openFileVideo(f)}>
                      <span class="wc-file-video-cover">
                        {#if apiRoot && !fileVideoFailed[f.md5]}
                          <img src={apiAssetUrl(`/file/video/thumb/${f.md5}`)} alt="" class="wc-file-video-thumb" loading="lazy"
                            onerror={() => { fileVideoFailed = { ...fileVideoFailed, [f.md5]: true }; }} />
                        {:else}
                          <span class="wc-file-video-ph">{@html iconSvg(ICON_PATHS.video, 32)}</span>
                        {/if}
                        <span class="wc-file-video-play">▶</span>
                      </span>
                      <span class="wc-file-video-name">{f.file_name || f.md5}</span>
                      <span class="wc-file-video-meta">{f.size_label} · {f.time}</span>
                    </button>
                  {/each}
                </div>
              {/if}
            {/if}
            {#if fileCat === 'all' || fileCat === 'file'}
              {#if fileDocs.length > 0}
                <div class="wc-sec-title">文件 <span class="wc-sec-count">（显示最近 {fileDocs.length} / 共 {fileData.files_total ?? 0}）</span></div>
                <div class="wc-file-list">
                  {#each fileDocs as f (f.md5)}
                    <div class="wc-file-item" title="{f.md5}">
                      <span class="wc-file-icon">{@html fileIcon(f.ext)}</span>
                      <div class="wc-file-info">
                        <div class="wc-file-name">{f.file_name || f.md5}</div>
                        <div class="wc-file-sub">{f.ext ? f.ext.toUpperCase() + ' 文件' : '文件'} · {f.size_label} · {f.time}</div>
                      </div>
                      <div class="wc-file-actions">
                        {#if f.path}
                          <WechatHoverButton text="打开" onclick={() => openWechatPath({ path: f.path ?? '' }).catch((e) => mgmt.show(errText(e) || '打开失败', false))} title="用系统默认程序打开" class="!px-3 !py-1 !text-xs" />
                          <WechatHoverButton text="定位" onclick={() => openFileFolder(f.path ?? '')} title="在资源管理器中显示" class="!px-3 !py-1 !text-xs" />
                        {:else}
                          <span class="wc-file-missing">文件不在本地</span>
                        {/if}
                      </div>
                    </div>
                  {/each}
                </div>
              {/if}
            {/if}
            {#if shownFiles.length > fileListLimit}
              <div class="wc-file-more">
                <WechatHoverButton text={`加载更多（还剩 ${shownFiles.length - fileListLimit} 条）`} onclick={() => fileListLimit += FILE_PAGE} class="!px-3 !py-1 !text-xs" />
              </div>
            {/if}
          {/if}
        </div>

      {:else if curTab === 'settings'}
        <div class="wc-settings">
          <section class="wc-settings-section">
            <div class="wc-settings-hd">
              <span>微信配置</span>
              <span class="wc-settings-count">数据库路径 · 解密密钥 · 加密 API · 本地语音转写</span>
            </div>
            <WeChatConfig />
          </section>
          <div class="wc-settings-divider" aria-hidden="true"></div>
          <section class="wc-settings-section">
            <div class="wc-settings-hd">
              <span>通用数据</span>
              <span class="wc-settings-actions">
                <span class="wc-settings-count">{settingsStats.withData} 个分类有数据 · 共 {settingsStats.total} 条记录</span>
<WechatHoverButton text="刷新" onclick={() => loadSettings()} title="刷新通用数据" class="!px-3 !py-1 !text-xs" />
                <WechatHoverButton
                  text="导出归档"
                  onclick={() => { archiveOpen = true; archiveResult = null; archiveError = ''; archiveProgress = null; }}
                  title="导出账号归档（数据库 + 资源 ZIP 打包）"
                  class="!px-3 !py-1 !text-xs"
                />
              </span>
            </div>
            <div class="wc-search wc-search-pad"><input type="text" placeholder="搜索分类 / 表名 / 记录内容" bind:value={settingsSearch} /></div>
            {#if settingsLoading}<div class="wc-empty">加载中…</div>
            {:else if settingsError}<div class="wc-empty wc-error-hint">
              <p>⚠️ 通用数据加载失败</p>
              <p class="wc-error-text">{settingsError}</p>
              <WechatHoverButton text="重试" onclick={() => loadSettings()} class="!px-3 !py-1 !text-xs" />
            </div>
            {:else if settingsFilteredCats.length===0}<div class="wc-empty">{settingsSearch ? '无匹配分类' : '暂无数据'}</div>
            {:else}
              <div class="wc-settings-list">
              {#each settingsFilteredCats as cat (cat.key)}
                <div class="wc-settings-cat" class:wc-settings-cat-open={settingsOpen[cat.key]}>
                  <button class="wc-settings-cat-hd" onclick={() => toggleSettingsCat(cat.key)}
                    aria-expanded={!!settingsOpen[cat.key]}>
                  <span class="wc-settings-cat-icon">{@html iconSvg(settingIcons[cat.key] || ICON_PATHS.file, 16)}</span>
                    <span class="wc-settings-cat-name">{cat.label}</span>
                    <span class="wc-settings-cat-table">{cat.table}</span>
                    {#if cat.total > 0}
                      <span class="wc-settings-cat-count">{cat.total} 条</span>
                    {:else}
                      <span class="wc-settings-cat-empty">暂无数据</span>
                    {/if}
                    <span class="wc-settings-cat-arrow">{settingsOpen[cat.key] ? '▾' : '▸'}</span>
                  </button>
                  {#if settingsOpen[cat.key]}
                    <div class="wc-settings-cat-body">
                      {#if cat.count > 0}
                        <div class="wc-table-wrap">
                          <table class="wc-table">
                            <thead>
                              <tr>{#each cat.column_labels as label, ci}<th title={cat.columns[ci]}>{label}</th>{/each}</tr>
                            </thead>
                            <tbody>
                              {#each cat.rows as row}
                                <tr>
                                  {#each row as cell, ci}
                                    <td title={cellText(cell)}>{cellTextSmart(cell, cat.columns[ci])}</td>
                                  {/each}
                                </tr>
                              {/each}
                            </tbody>
                          </table>
                        </div>
                      {:else}
                        <div class="wc-settings-cat-nodata">该分类暂无记录</div>
                      {/if}
                      {#if cat.total > 0}
                        <div class="wc-settings-cat-foot">
                          <span>已显示 {cat.count} / 共 {cat.total} 条{cat.count < cat.total ? '（仅展示前 ' + cat.count + ' 条）' : ''}</span>
                          <WechatHoverButton text="导出 CSV" onclick={() => exportSettingsCat(cat)} title="导出该分类全部数据为 CSV" class="!px-3 !py-1 !text-xs" />
                        </div>
                      {/if}
                    </div>
                  {/if}
                </div>
              {/each}
              </div>
            {/if}
          </section>
        </div>
      {/if}

    {#if exportOpen}
          <div class="wc-export-overlay" onclick={(e) => { if (e.target === e.currentTarget && !exportLoading) exportOpen = false; }} role="dialog" tabindex="-1" onkeydown={(e) => e.key === 'Escape' && !exportLoading && (exportOpen = false)}>
            <div class="wc-export-modal" role="document">
              <div class="wc-export-header">
                <span>{exportSuccess ? '✅ 导出成功' : '导出消息记录'}</span>
                <button class="wc-export-close" onclick={() => { if (!exportLoading) exportOpen = false; }} disabled={exportLoading}>×</button>
              </div>

              {#if exportSuccess}
                <div class="wc-export-result">
                  <div class="wc-export-result-info">{exportSuccess.count} 条消息已导出到：</div>
                  <div class="wc-export-result-path" title={exportSuccess.path}>{exportSuccess.path}</div>
                  <div class="wc-export-result-hint">你可以在文件管理器中打开该文件，或直接拖入查阅。</div>
                </div>
                <div class="wc-export-footer">
                  <WechatHoverButton text="关闭" onclick={() => { exportOpen = false; exportSuccess = null; }} class="!px-3 !py-1 !text-xs" />
                </div>
              {:else}
                <div class="wc-export-body">
                  <div class="wc-export-field">
                    <label class="wc-export-label" for="export-fmt-txt">导出格式</label>
                    <div class="wc-export-options">
                      <label class="wc-export-radio" class:wc-export-radio-on={exportFormat === 'txt'}>
                        <input id="export-fmt-txt" type="radio" name="exportFmt" value="txt" bind:group={exportFormat} disabled={exportLoading} />
                        TXT（纯文本）
                      </label>
                      <label class="wc-export-radio" class:wc-export-radio-on={exportFormat === 'csv'}>
                        <input id="export-fmt-csv" type="radio" name="exportFmt" value="csv" bind:group={exportFormat} disabled={exportLoading} />
                        CSV（表格）
                      </label>
                      <label class="wc-export-radio" class:wc-export-radio-on={exportFormat === 'html'}>
                        <input id="export-fmt-html" type="radio" name="exportFmt" value="html" bind:group={exportFormat} disabled={exportLoading} />
                        HTML（聊天报告，含图片）
                      </label>
                      <label class="wc-export-radio" class:wc-export-radio-on={exportFormat === 'excel'}>
                        <input id="export-fmt-excel" type="radio" name="exportFmt" value="excel" bind:group={exportFormat} disabled={exportLoading} />
                        Excel（表格）
                      </label>
                    </div>
                  </div>

                  <div class="wc-export-field">
                    <span class="wc-export-label">导出条数</span>
                    <div class="wc-export-presets">
                      {#each [
                        { label: '最近 50 条', value: 50 },
                        { label: '最近 100 条', value: 100 },
                        { label: '最近 500 条', value: 500 },
                        { label: '全部消息', value: 0 },
                      ] as preset}
                        <WechatHoverButton
                          text={preset.label}
                          onclick={() => { exportCount = preset.value; exportCountCustom = false; }}
                          disabled={exportLoading}
                          class={!exportCountCustom && exportCount === preset.value ? 'wc-ihb-active !px-3 !py-1 !text-xs' : '!px-3 !py-1 !text-xs'}
                        />
                      {/each}
                    </div>
                    <div class="wc-export-custom">
                      <label>
                        <input type="checkbox" bind:checked={exportCountCustom} disabled={exportLoading} />
                        自定义条数
                      </label>
                      {#if exportCountCustom}
                        <input type="number" class="wc-export-input" min="1" max="50000" bind:value={exportCount} disabled={exportLoading} />
    {/if}
                    </div>
                  </div>

                  {#if exportError}
                    <div class="wc-export-error">{exportError}</div>
                  {/if}
                </div>

                <div class="wc-export-footer">
                  <WechatHoverButton text="取消" onclick={() => { if (!exportLoading) exportOpen = false; }} disabled={exportLoading} class="!px-3 !py-1 !text-xs" />
                  <WechatHoverButton text={exportLoading ? '导出中…' : '确认导出'} onclick={() => doExport()} disabled={exportLoading} />
                </div>
              {/if}
            </div>
          </div>
        {/if}

    {#if profileOpen}
      <div class="wc-export-overlay" onclick={(e) => { if (e.target === e.currentTarget) profileOpen = false; }} role="dialog" tabindex="-1" onkeydown={(e) => e.key === 'Escape' && (profileOpen = false)}>
        <div class="wc-export-dialog wc-profile-dialog">
          <div class="wc-export-hd">
            <span class="wc-export-title">{(profileData?.username || profileUsername || '').endsWith('@chatroom') ? '群聊资料' : '联系人资料'}</span>
            <button class="wc-export-close" onclick={() => profileOpen = false}>×</button>
          </div>
          <div class="wc-export-body">
            {#if profileLoading}
              <div class="wc-empty">加载中…</div>
            {:else if profileData}
              {@render profileCard(profileData)}
            {:else}
              <div class="wc-empty">未找到联系人资料</div>
            {/if}
          </div>
        </div>
      </div>
    {/if}

    {#if calendarOpen}
      <div class="wc-export-overlay" onclick={(e) => { if (e.target === e.currentTarget) calendarOpen = false; }} role="dialog" tabindex="-1" onkeydown={(e) => e.key === 'Escape' && (calendarOpen = false)}>
        <div class="wc-export-dialog wc-calendar-dialog">
          <div class="wc-export-hd">
            <span class="wc-export-title">消息日历</span>
            <button class="wc-export-close" onclick={() => calendarOpen = false}>×</button>
          </div>
          <div class="wc-export-body">
            <div class="wc-calendar-nav">
              <button class="wc-btn" onclick={() => switchCalendarMonth(-1)}>‹</button>
              <span class="wc-calendar-title">{calYear} 年 {calMonth} 月</span>
              <button class="wc-btn" onclick={() => switchCalendarMonth(1)}>›</button>
            </div>
            {#if calLoading}
              <div class="wc-empty">加载中…</div>
            {:else}
              <div class="wc-cal-stats">
                <span class="wc-cal-stat">本月共 <b>{calTotal}</b> 条消息</span>
                <span class="wc-cal-stat">活跃 <b>{calActiveDays}</b> 天</span>
                <span class="wc-cal-stat">日均 <b>{calAvg}</b> 条</span>
                {#if calTop}<span class="wc-cal-stat">最活跃：{calMonth}月{calTop.day}日（{calTop.count} 条）</span>{/if}
              </div>
              <div class="wc-calendar-grid">
                {#each ['一','二','三','四','五','六','日'] as wd}
                  <div class="wc-cal-wd">{wd}</div>
                {/each}
                {#each Array(calFirstDow) as _}<div class="wc-cal-empty"></div>{/each}
                {#each Array(calDays) as _, i}
                  {@const day = i + 1}
                  {@const cnt = calCounts[String(day)] ?? 0}
                  <button
                    class="wc-cal-day"
                    style="background:{calHeat(cnt)}"
                    title={cnt ? `${calMonth}月${day}日：${cnt} 条消息` : `${calMonth}月${day}日：无消息`}
                    onclick={() => jumpToDay(day)}
                  >
                    <span class="wc-cal-day-num">{day}</span>
                    {#if cnt}<span class="wc-cal-day-cnt">{cnt}</span>{/if}
                  </button>
                {/each}
              </div>
              <p class="wc-calendar-hint">点击日期跳转到当天消息（色块深浅表示消息量）</p>
            {/if}
          </div>
        </div>
      </div>
    {/if}

    {#if miniappDetail}
      <div class="wc-export-overlay" onclick={(e) => { if (e.target === e.currentTarget) miniappDetail = null; }} role="dialog" tabindex="-1" onkeydown={(e) => e.key === 'Escape' && (miniappDetail = null)}>
        <div class="wc-export-dialog wc-miniapp-dialog">
          <div class="wc-export-hd">
            <span class="wc-export-title">小程序</span>
            <button class="wc-export-close" onclick={() => miniappDetail = null}>×</button>
          </div>
          <div class="wc-export-body">
            <div class="wc-miniapp-detail-row">
              {#if miniappDetail.icon}
                <img src={miniappDetail.icon} alt="" class="wc-miniapp-detail-icon" />
              {:else}
                <div class="wc-miniapp-detail-icon wc-miniapp-detail-icon-ph">🟢</div>
              {/if}
              <div class="wc-miniapp-detail-head">
                <div class="wc-miniapp-detail-name">{miniappDetail.title || '微信小程序'}</div>
                {#if miniappDetail.des}<div class="wc-miniapp-detail-des">{miniappDetail.des}</div>{/if}
              </div>
            </div>
            <div class="wc-miniapp-detail-tip">该小程序没有可直接打开的网页链接，只能在微信客户端内打开。</div>
            {#if miniappDetail.appid}
              <div class="wc-miniapp-detail-kv"><span>AppID</span><code>{miniappDetail.appid}</code></div>
            {/if}
            {#if miniappDetail.source}
              <div class="wc-miniapp-detail-kv"><span>来源</span><code>{miniappDetail.source}</code></div>
            {/if}
            {#if miniappDetail.pagepath}
              <div class="wc-miniapp-detail-kv"><span>页面路径</span><code class="wc-miniapp-detail-path">{miniappDetail.pagepath}</code></div>
            {/if}
            <div class="wc-miniapp-detail-actions">
<WechatHoverButton text="复制信息" onclick={() => copyMiniAppInfo(miniappDetail)} class="!px-3 !py-1 !text-xs" />
              <WechatHoverButton text="关闭" onclick={() => miniappDetail = null} class="!px-3 !py-1 !text-xs" />
            </div>
          </div>
        </div>
      </div>
    {/if}

        {#if editMenu.open}
          <div
            class="wc-edit-mask"
            role="button"
            aria-label="关闭编辑菜单"
            tabindex="-1"
            onclick={closeEditMenu}
            oncontextmenu={(e) => { e.preventDefault(); closeEditMenu(); }}
            onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ' || e.key === 'Escape') { e.preventDefault(); closeEditMenu(); } }}
          ></div>
          <div class="wc-edit-menu" style="left:{editMenu.x}px;top:{editMenu.y}px">
            {#if editMenu.loading}
              <div class="wc-edit-menu-loading"><span class="wc-loading-inline"></span> 检查编辑状态…</div>
            {/if}
            {#if editMenu.canEdit}
              <WechatHoverButton text="✏️ 编辑消息内容" onclick={openEditModal} class="!w-full !justify-start !px-3 !py-1 !text-xs" />
              <WechatHoverButton text="🔧 编辑原始字段…" onclick={openRawEditModal} class="!w-full !justify-start !px-3 !py-1 !text-xs" />
            {/if}
            {#if editMenu.modified}
              <WechatHoverButton text="↩️ 恢复原始消息" onclick={resetEdit} class="!w-full !justify-start !px-3 !py-1 !text-xs" />
            {/if}
            <WechatHoverButton text="📋 复制文本" onclick={() => { void copyText(editMenu.text); closeEditMenu(); }} class="!w-full !justify-start !px-3 !py-1 !text-xs" />
            <WechatHoverButton text="📋 复制消息 JSON" onclick={copyMessageJson} class="!w-full !justify-start !px-3 !py-1 !text-xs" />
            <WechatHoverButton text="关闭" onclick={closeEditMenu} class="!w-full !justify-start !px-3 !py-1 !text-xs" />
          </div>
        {/if}

        {#if editModal.open}
          <div class="wc-export-overlay" onclick={(e) => { if (e.target === e.currentTarget && !editModal.saving) editModal.open = false; }} role="dialog" tabindex="-1" onkeydown={(e) => e.key === 'Escape' && !editModal.saving && (editModal.open = false)}>
            <div class="wc-export-modal" role="document">
              <div class="wc-export-header">
                <span>✏️ 编辑消息</span>
                <button class="wc-export-close" onclick={() => { if (!editModal.saving) editModal.open = false; }} disabled={editModal.saving}>×</button>
              </div>
              <div class="wc-edit-modal-body">
                <p class="wc-edit-modal-tip">修改仅写入本地解密副本，微信源库不受影响；保存后可随时右键该消息恢复原文。</p>
                <textarea class="wc-edit-modal-input" bind:value={editModal.text} rows={5} disabled={editModal.saving} placeholder="输入新的消息内容"></textarea>
                {#if editModal.error}
                  <div class="wc-edit-modal-error">{editModal.error}</div>
                {/if}
                <div class="wc-edit-modal-actions">
                  <WechatHoverButton text="取消" onclick={() => { editModal.open = false; }} disabled={editModal.saving} class="!px-3 !py-1 !text-xs" />
                  <WechatHoverButton text={editModal.saving ? '保存中…' : '保存'} onclick={saveEdit} disabled={editModal.saving || !editModal.text.trim()} />
                </div>
              </div>
            </div>
          </div>
        {/if}

        {#if rawEditModal.open}
          <div class="wc-export-overlay" onclick={(e) => { if (e.target === e.currentTarget && !rawEditModal.saving) rawEditModal.open = false; }} role="dialog" tabindex="-1" onkeydown={(e) => e.key === 'Escape' && !rawEditModal.saving && (rawEditModal.open = false)}>
            <div class="wc-export-modal" role="document">
              <div class="wc-export-header">
                <span>🔧 编辑消息原始字段</span>
                <button class="wc-export-close" onclick={() => { if (!rawEditModal.saving) rawEditModal.open = false; }} disabled={rawEditModal.saving}>×</button>
              </div>
              <div class="wc-edit-modal-body">
                <p class="wc-edit-modal-tip">JSON 对象键为字段名；BLOB 字段用 <code>0x..</code> 十六进制表示。修改仅写入本地解密副本，可右键恢复。</p>
                <textarea class="wc-edit-modal-input wc-mono" bind:value={rawEditModal.json} rows={10} disabled={rawEditModal.saving} spellcheck="false"></textarea>
                <label class="wc-raw-unsafe">
                  <input type="checkbox" bind:checked={rawEditModal.unsafe} disabled={rawEditModal.saving} />
                  高级（危险）模式：允许修改白名单以外的字段
                </label>
                {#if rawEditModal.error}
                  <div class="wc-edit-modal-error">{rawEditModal.error}</div>
                {/if}
                <div class="wc-edit-modal-actions">
                  <WechatHoverButton text="取消" onclick={() => { rawEditModal.open = false; }} disabled={rawEditModal.saving} class="!px-3 !py-1 !text-xs" />
                  <WechatHoverButton text={rawEditModal.saving ? '保存中…' : '保存'} onclick={saveRawEdit} disabled={rawEditModal.saving || !rawEditModal.json.trim()} />
                </div>
              </div>
            </div>
          </div>
        {/if}

        {#if archiveOpen}
          <div class="wc-export-overlay" onclick={(e) => { if (e.target === e.currentTarget && !archiveRunning) archiveOpen = false; }} role="dialog" tabindex="-1" onkeydown={(e) => e.key === 'Escape' && !archiveRunning && (archiveOpen = false)}>
            <div class="wc-export-modal" role="document">
              <div class="wc-export-header">
                <span>📦 导出账号归档（ZIP）</span>
                <button class="wc-export-close" onclick={() => { if (!archiveRunning) archiveOpen = false; }} disabled={archiveRunning}>×</button>
              </div>
              <div class="wc-export-body">
                <p class="wc-export-tip">打包解密数据库与本地资源目录，适合备份、迁移或重新分析；数据较大时请耐心等待。</p>
                <div class="wc-export-field">
                  <label class="wc-export-label" for="archive-dir">保存目录</label>
                  <div class="wc-row">
                    <input id="archive-dir" class="wc-input wc-mono" bind:value={archiveDir} placeholder="默认：&lt;st_result&gt;/exports" />
                    <WechatHoverButton text="选择目录" onclick={pickArchiveDir} disabled={archiveRunning} class="!px-3 !py-1 !text-xs" />
                  </div>
                </div>
                <label class="wc-raw-unsafe">
                  <input type="checkbox" bind:checked={archiveIncludeResources} disabled={archiveRunning} />
                  包含资源文件（图片/视频/文件缓存；不勾选仅打包 .db 数据库）
                </label>
                {#if archiveProgress}
                  <div class="wc-archive-progress">
                    <div class="wc-archive-progress-track">
                      <div class="wc-archive-progress-fill" style="--p:{archiveProgress.percent}"></div>
                    </div>
                    <span class="wc-archive-progress-label">{archiveProgress.label}</span>
                  </div>
                {/if}
                {#if archiveError}
                  <div class="wc-edit-modal-error">{archiveError}</div>
                {/if}
                {#if archiveResult}
                  <div class="wc-export-result">
                    <div class="wc-export-result-info">归档完成：{archiveResult.file_count} 个文件（{fmtFileSize(archiveResult.total_bytes)}）</div>
                    <div class="wc-export-result-path" title={archiveResult.path}>{archiveResult.path}</div>
                  </div>
                {/if}
              </div>
              <div class="wc-export-footer">
                <WechatHoverButton text="关闭" onclick={() => { if (!archiveRunning) archiveOpen = false; }} disabled={archiveRunning} class="!px-3 !py-1 !text-xs" />
                <WechatHoverButton text={archiveRunning ? '打包中…' : '开始导出'} onclick={startArchive} disabled={archiveRunning} />
              </div>
            </div>
          </div>
        {/if}

        {#if clearConfirmOpen}
          <div class="wc-export-overlay" onclick={(e) => { if (e.target === e.currentTarget && !clearing) clearConfirmOpen = false; }} role="dialog" tabindex="-1" onkeydown={(e) => e.key === 'Escape' && !clearing && (clearConfirmOpen = false)}>
            <div class="wc-export-modal" role="document">
              <div class="wc-export-header">
                <span>清空聊天记录</span>
                <button class="wc-export-close" onclick={() => { if (!clearing) clearConfirmOpen = false; }} disabled={clearing}>×</button>
              </div>
              <div class="wc-export-body">
                <div class="wc-clear-warn">
                  <p>⚠️ 将清空「{curSessionInfo?.name || curSession}」的<strong>本地全部聊天记录</strong>。</p>
                  <p class="wc-clear-warn-sub">仅删除本机解密副本中的数据，不影响微信官方数据；删除后不可恢复，建议先导出备份。</p>
                </div>
              </div>
              <div class="wc-export-footer">
                <WechatHoverButton text="取消" onclick={() => { if (!clearing) clearConfirmOpen = false; }} disabled={clearing} class="!px-3 !py-1 !text-xs" />
                <WechatHoverButton text={clearing ? '清空中…' : '确认清空'} onclick={() => doClearHistory()} disabled={clearing} />
              </div>
            </div>
          </div>
        {/if}

    </div>
    </div>
  {/if}

  <!-- 数据管理操作结果轻提示 -->
  {#if mgmt.state.text}
    <div class="wc-mgmt-toast" class:wc-mgmt-toast-err={!mgmt.state.ok} role="status">{mgmt.state.text}</div>
  {/if}

  <!-- 图片查看器（聊天 / 朋友圈共用） -->
  {#if viewerOpen}
    {@const vImg = viewerImages[viewerIndex]}
    <div class="wc-img-viewer" role="dialog" aria-modal="true" aria-label="图片查看">
      <div
        class="wc-img-viewer-mask"
        role="button"
        aria-label="关闭图片查看器"
        tabindex="-1"
        onclick={closeImageViewer}
        onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ' || e.key === 'Escape') { e.preventDefault(); closeImageViewer(); } }}
      ></div>
      <div class="wc-img-viewer-toolbar">
        <span class="wc-img-viewer-count">{viewerImages.length > 1 ? `${viewerIndex + 1} / ${viewerImages.length}` : ''}</span>
        {#if vImg?.time}<span class="wc-img-viewer-time">{vImg.time}</span>{/if}
        <div class="wc-img-viewer-actions">
          {#if viewerImages.length > 1}
            <button class="wc-img-viewer-btn" onclick={prevImage} title="上一张 (←)">‹</button>
            <button class="wc-img-viewer-btn" onclick={nextImage} title="下一张 (→)">›</button>
          {/if}
          <button class="wc-img-viewer-btn" class:wc-img-viewer-btn-on={viewerShowHd}
            onclick={toggleViewerHd} title="切换 原图/缩略图">
            {viewerShowHd ? '原图' : '缩略图'}
          </button>
          <button class="wc-img-viewer-btn" onclick={cycleZoom} title="缩放 (滚轮)">{Math.round(viewerZoom * 100)}%</button>
          {#if vImg?.username && vImg?.local_id != null}
            <button class="wc-img-viewer-btn" onclick={sendViewerToOcr} title="把这张图片发送到图文识别，自动携带发送人/会话/时间">图文识别</button>
          {/if}
          <button class="wc-img-viewer-btn" onclick={closeImageViewer} title="关闭 (Esc)">✕</button>
        </div>
      </div>
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions —— 画布指针交互容器，键盘等价由工具栏按钮提供 -->
      <div
        class="wc-img-viewer-stage"
        role="application"
        aria-label="图片预览（滚轮缩放、拖拽平移、双击复位；+ 键放大）"
        tabindex="-1"
        class:wc-img-viewer-dragging={viewerDragActive}
        onwheel={onViewerWheel}
        onmousedown={onViewerMouseDown}
        ondblclick={() => { viewerZoom = 1; viewerOffset = { x: 0, y: 0 }; }}
        onkeydown={(e) => { if (e.key === '+' || e.key === '=') { e.preventDefault(); cycleZoom(); } }}
        style={`cursor:${viewerZoom > 1 ? (viewerDragActive ? 'grabbing' : 'grab') : 'zoom-in'}`}
      >
        {#if vImg}
          {#if viewerHdLoading && !viewerHdSrc}
            <div class="wc-img-viewer-loading"><span class="wc-loading-inline"></span>加载高清原图中…</div>
          {/if}
          {#if viewerHdRetryCount > 0}
            <div class="wc-img-viewer-loading">
              <span class="wc-loading-inline"></span>
              本地暂无高清原图，程序正在等待微信同步（微信下载完成后将在此自动显示）…
              <button class="wc-img-viewer-btn" onclick={stopViewerHdRetry} title="停止等待">停止</button>
            </div>
          {/if}
          <img
            src={viewerHdSrc || vImg.src}
            alt="图片预览"
            class="wc-img-viewer-img"
            style={`transform: translate(${viewerOffset.x}px, ${viewerOffset.y}px) scale(${viewerZoom})`}
            ondragstart={(e) => e.preventDefault()}
            onload={() => { viewerHdLoading = false; stopViewerHdRetry(); }}
            onerror={() => {
              viewerHdSrc = '';
              viewerHdLoading = false;
              // 本地无高清（微信尚未下载）：进入等待重试，下载完成后自动获取
              if (viewerShowHd) startViewerHdRetry();
            }}
          />
        {/if}
      </div>
      <div class="wc-img-viewer-hint">滚轮缩放 · 拖拽平移 · 双击复位 · Esc 关闭</div>
    </div>
  {/if}

  <!-- 朋友圈视频播放器 -->
  <VideoPlayerDialog
    open={momentVideo.open}
    src={momentVideo.src}
    title={momentVideo.title}
    error={momentVideo.error}
    onClose={closeMomentVideo}
    onVideoError={handleVideoError}
  />

  <!-- 文件管理：图片查看器 -->
  {#if fileViewer.open && fileViewer.items.length > 0}
    {@const fv = fileViewer.items[fileViewer.index]}
    <div class="wc-file-viewer" role="dialog" aria-modal="true" aria-label="文件图片查看" tabindex="-1"
      onkeydown={(e) => {
        if (e.key === 'Escape') closeFileViewer();
        else if (e.key === 'ArrowLeft') fileViewerPrev();
        else if (e.key === 'ArrowRight') fileViewerNext();
      }}>
      <div
        class="wc-file-viewer-mask"
        role="button"
        aria-label="关闭文件查看器"
        tabindex="-1"
        onclick={closeFileViewer}
        onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ' || e.key === 'Escape') { e.preventDefault(); closeFileViewer(); } }}
      ></div>
      <div class="wc-file-viewer-toolbar">
        <span class="wc-file-viewer-name" title={fv.name}>{fv.name}</span>
        <span class="wc-file-viewer-meta">{fileViewer.items.length > 1 ? `${fileViewer.index + 1} / ${fileViewer.items.length} · ` : ''}{fv.meta}</span>
        <div class="wc-file-viewer-actions">
          {#if fileViewer.items.length > 1}
            <button class="wc-img-viewer-btn" onclick={fileViewerPrev} title="上一张 (←)">‹</button>
            <button class="wc-img-viewer-btn" onclick={fileViewerNext} title="下一张 (→)">›</button>
          {/if}
          {#if fv.path}
            <button class="wc-img-viewer-btn" onclick={() => openWechatPath({ path: fv.path }).catch((e) => mgmt.show(errText(e) || '打开失败', false))} title="用系统默认程序打开原文件">打开文件</button>
            <button class="wc-img-viewer-btn" onclick={() => openFileFolder(fv.path)} title="在资源管理器中显示">定位</button>
          {/if}
          <button class="wc-img-viewer-btn" onclick={closeFileViewer} title="关闭 (Esc)">✕</button>
        </div>
      </div>
      <div class="wc-file-viewer-stage">
        <img src={fv.src} alt="图片预览" class="wc-file-viewer-img"
          onerror={() => closeFileViewer()} />
      </div>
      <div class="wc-file-viewer-hint">← → 切换 · Esc 关闭</div>
    </div>
  {/if}

  <!-- 文件管理：视频播放器 -->
  <VideoPlayerDialog
    open={fileVideoOpen}
    src={fileVideoSrc}
    title={fileVideoName}
    loadingText="视频加载失败"
    onClose={closeFileVideo}
    onLocate={() => openFileFolder(fileVideoPath)}
    onVideoError={() => { fileVideoSrc = ''; mgmt.show('视频播放失败（文件可能已失效）', false); }}
  />
</div>

<style>
  /* ===== 启动加载覆盖层 ===== */
  .wc-loading-overlay { position:absolute; inset:0; z-index:50; display:flex; flex-direction:column; align-items:center; justify-content:center; gap:12px; background:var(--wc-bg); }
  .wc-loading-spinner { width:32px;height:32px;border:3px solid var(--wc-border);border-top-color:var(--wc-text);border-radius:50%;animation:wc-spin .7s linear infinite; }
  @keyframes wc-spin { to { transform:rotate(360deg); } }
  .wc-loading-text { font-size:14px;font-weight:600;color:var(--wc-text); }
  .wc-loading-sub { font-size:11.5px;color:var(--wc-muted); }

  :where(.wc-root) { position:relative;
    --wc-bg: color-mix(in srgb, var(--app-bg-color) 96%, var(--app-color-text));
    --wc-sidebar-bg: color-mix(in srgb, var(--app-bg-color) 92%, var(--app-color-text));
    --wc-nav-bg: color-mix(in srgb, var(--app-bg-color) 86%, var(--app-color-text));
    --wc-nav-hover: color-mix(in srgb, var(--app-bg-color) 80%, var(--app-color-text));
    --wc-nav-active: color-mix(in srgb, var(--app-bg-color) 74%, var(--app-color-text));
    --wc-item-hover: color-mix(in srgb, var(--app-bg-color) 88%, var(--app-color-text));
    --wc-item-active: color-mix(in srgb, var(--app-bg-color) 83%, var(--app-color-text));
    --wc-border: color-mix(in srgb, var(--app-bg-color) 76%, var(--app-color-text));
    --wc-border-light: color-mix(in srgb, var(--app-bg-color) 85%, var(--app-color-text));
    --wc-text: var(--app-color-text);
    --wc-text2: color-mix(in srgb, var(--app-color-text) 72%, var(--app-bg-color));
    --wc-muted: color-mix(in srgb, var(--app-color-text) 45%, var(--app-bg-color));
    --wc-card: var(--app-color-card-bg);
    --wc-bg2: color-mix(in srgb, var(--app-bg-color) 90%, var(--app-color-text));
  --wc-theme: var(--app-wc-accent, #576b95);
    --wc-header-h: 48px;
    --wc-nav-w: 176px;
    --wc-sidebar-w: 260px;
    color:var(--wc-text); display:flex; flex-direction:column; height:100%; overflow:hidden; font-size:13px;
    background:var(--wc-bg); transition:all .2s ease;
  }
  .wc-header { height:var(--wc-header-h); flex-shrink:0; display:flex; align-items:center; justify-content:space-between; padding:0 18px; border-bottom:1px solid var(--wc-border); user-select:none; gap:8px; }
  .wc-header-left { display:flex; align-items:center; gap:4px; min-width:0; flex-shrink:1; }
  .wc-header-title { font-size:16px; font-weight:600; white-space:nowrap; }
  .wc-dot { width:6px;height:6px;border-radius:50%;background:color-mix(in srgb,#0a0 40%,var(--wc-muted));margin-left:8px;flex-shrink:0; }
  .wc-dot-on { background:#0a0; box-shadow:0 0 6px color-mix(in srgb,#0a0 60%,transparent); }
  .wc-status { font-size:11.5px; color:var(--wc-muted); white-space:nowrap; }
  .wc-header-right { display:flex; align-items:center; gap:6px; flex-shrink:0; }
  /* 工具栏分组：数据操作 ｜ 监控控制 */
  .wc-header-group { display:flex; align-items:center; gap:6px; }
  .wc-header-divider {
    width:1px; height:18px; flex:none;
    background: var(--wc-border);
    margin: 0 4px;
  }
  .wc-loading-inline-sm { display:inline-block;width:10px;height:10px;border:2px solid var(--wc-border);border-top-color:var(--wc-text);border-radius:50%;animation:wc-spin .7s linear infinite;vertical-align:middle; }
  .wc-export-overlay { position:fixed;inset:0;z-index:9999;display:flex;align-items:center;justify-content:center;background:rgba(0,0,0,0.45); }
  .wc-export-modal { background:var(--wc-card);border:1px solid var(--wc-border);border-radius:10px;width:380px;max-width:90vw;box-shadow:0 8px 32px rgba(0,0,0,0.2);overflow:hidden;animation:wc-fade-in .15s ease; }
  .wc-export-dialog { background:var(--wc-card);border:1px solid var(--wc-border);border-radius:10px;width:380px;max-width:90vw;box-shadow:0 8px 32px rgba(0,0,0,0.2);overflow:hidden;animation:wc-fade-in .15s ease; }
  .wc-export-hd { display:flex;align-items:center;justify-content:space-between;padding:12px 16px;border-bottom:1px solid var(--wc-border);font-size:14px;font-weight:600; }
  .wc-export-title { font-size:14px;font-weight:600;color:var(--wc-text); }
  .wc-profile { display:flex;flex-direction:column;align-items:center;gap:12px;padding:8px 0; }
  /* 通讯录内嵌资料卡面板（右侧内容区） */
  .wc-contact-profile-pane { height:100%; overflow-y:auto; display:flex; flex-direction:column; padding:18px 24px; }
  .wc-contact-profile-hd { display:flex; align-items:center; justify-content:space-between; padding-bottom:12px; border-bottom:1px solid var(--wc-border-light); margin-bottom:16px; }
  .wc-contact-profile-title { font-size:14px; font-weight:600; color:var(--wc-text); }
  .wc-contact-profile-body { display:flex; justify-content:center; }
  .wc-contact-profile-body .wc-profile { width:100%; max-width:460px; }
  .wc-contact-active { background:color-mix(in srgb,var(--wc-theme) 10%,transparent); }
  .wc-profile-avatar { display:flex;align-items:center;justify-content:center; }
  .wc-profile-avatar-img { width:72px;height:72px;border-radius:12px;object-fit:cover; }
  .wc-avatar-lg { width:72px;height:72px;border-radius:12px;font-size:26px;display:flex;align-items:center;justify-content:center;background:var(--wc-theme);color:#fff; }
  .wc-profile-name { font-size:16px;font-weight:700;color:var(--wc-text); }
  .wc-profile-items { width:100%;display:flex;flex-direction:column;gap:8px; }
  .wc-profile-actions { width:100%;display:flex;align-items:center;gap:8px;margin-top:14px;padding-top:12px;border-top:1px solid var(--wc-border-light); }
  .wc-profile-item { display:flex;justify-content:space-between;gap:12px;font-size:12px;color:var(--wc-text2);padding:7px 10px;background:var(--wc-bg2);border-radius:6px; }
  .wc-profile-link { border:none;background:none;padding:0;margin:0;cursor:pointer;font-size:12px;color:var(--wc-theme);text-decoration:underline;text-underline-offset:2px; }
  .wc-profile-item span:first-child { color:var(--wc-muted);flex-shrink:0; }
  .wc-profile-item span:last-child { text-align:right;word-break:break-all;color:var(--wc-text); }
  .wc-fav-img-ph { padding:12px;border:1px dashed var(--wc-border);border-radius:6px;font-size:11.5px;color:var(--wc-muted);text-align:center;max-width:260px; }
  .wc-calendar-dialog { width: 420px; }
  .wc-calendar-nav { display:flex;align-items:center;justify-content:space-between;gap:8px;margin-bottom:10px; }
  /* 日历月度统计条（本月消息量 / 活跃天数 / 日均 / 最活跃日） */
  .wc-cal-stats { display:flex; flex-wrap:wrap; gap:6px 14px; margin-bottom:10px; padding:8px 10px; border-radius:8px; background:var(--wc-bg2); border:1px solid var(--wc-border-light); font-size:12px; color:var(--wc-text2); }
  .wc-cal-stat b { color:var(--wc-theme); font-weight:700; font-variant-numeric:tabular-nums; }
  .wc-calendar-title { font-size:14px;font-weight:600;color:var(--wc-text); }
  .wc-calendar-grid { display:grid;grid-template-columns:repeat(7,1fr);gap:4px; }
  .wc-cal-wd { text-align:center;font-size:11.5px;color:var(--wc-muted);padding:4px 0; }
  .wc-cal-empty { aspect-ratio:1; }
  .wc-cal-day { aspect-ratio:1;border-radius:6px;border:1px solid var(--wc-border);display:flex;flex-direction:column;align-items:center;justify-content:center;cursor:pointer;transition:transform .1s ease;background:var(--wc-bg2); }
  .wc-cal-day:hover { transform:scale(1.08);border-color:var(--wc-theme); }
  .wc-cal-day-num { font-size:11.5px;color:var(--wc-text); }
  .wc-cal-day-cnt { font-size:11.5px;color:var(--wc-muted);margin-top:1px; }
  .wc-calendar-hint { font-size:11.5px;color:var(--wc-muted);margin:10px 0 0;text-align:center; }
  /* 图片体检 */
  .wc-checkup-overlay { position:fixed;inset:0;z-index:9999;display:flex;align-items:center;justify-content:center;background:rgba(0,0,0,0.45); }
  .wc-checkup-dialog { display:flex;flex-direction:column;width:860px;max-width:94vw;max-height:86vh;background:var(--wc-card);border:1px solid var(--wc-border);border-radius:16px;box-shadow:0 18px 50px rgba(35,48,44,0.22);overflow:hidden;animation:wc-fade-in .15s ease; }
  .wc-checkup-hd { display:flex;align-items:center;justify-content:space-between;gap:12px;padding:13px 18px;border-bottom:1px solid var(--wc-border); }
  .wc-checkup-hd-left { display:flex;align-items:center;gap:11px;min-width:0; }
  .wc-checkup-led { width:8px;height:8px;border-radius:50%;flex-shrink:0;background:var(--wc-muted); }
  .wc-checkup-led-bad { background:var(--app-warning);box-shadow:0 0 8px var(--app-warning); }
  .wc-checkup-led-good { background:var(--app-success);box-shadow:0 0 8px var(--app-success); }
  .wc-checkup-hd-text { min-width:0; }
  .wc-checkup-title { font-size:15px;font-weight:700;color:var(--wc-text);line-height:1.2; }
  .wc-checkup-meta { font-size:11px;color:var(--wc-muted);margin-top:2px;line-height:1.5; }
  .wc-checkup-meta b { color:var(--wc-text2);font-weight:600; }
  .wc-checkup-close { display:inline-flex;align-items:center;justify-content:center;width:28px;height:28px;border:1px solid transparent;border-radius:8px;background:transparent;color:var(--wc-muted);cursor:pointer;transition:all .12s ease;flex-shrink:0; }
  .wc-checkup-close:hover { color:var(--wc-text);border-color:var(--wc-border);background:var(--wc-item-hover); }
  .wc-checkup-close:focus-visible { outline:2px solid color-mix(in srgb,var(--wc-theme) 55%,transparent);outline-offset:1px; }
  .wc-checkup-body { display:flex;flex-direction:column;gap:14px;padding:16px 18px;overflow:auto; }
  .wc-checkup-meter-grid { display:grid;grid-template-columns:repeat(4,1fr);gap:10px; }
  .wc-checkup-meter { display:flex;flex-direction:column;gap:6px;padding:11px 14px;background:var(--wc-bg2);border:1px solid var(--wc-border);border-radius:12px;min-width:0; }
  .wc-checkup-meter-label { font-size:11px;font-weight:600;letter-spacing:0.08em;color:var(--wc-muted);white-space:nowrap; }
  .wc-checkup-meter-value { font-family:var(--font-mono,ui-monospace,monospace);font-size:26px;font-weight:700;line-height:1;color:var(--wc-text);font-variant-numeric:tabular-nums; }
  .wc-checkup-meter-ok .wc-checkup-meter-value { color:var(--app-success); }
  .wc-checkup-meter-cdn .wc-checkup-meter-value { color:var(--brand); }
  .wc-checkup-meter-bad { background:color-mix(in srgb,var(--app-warning) 7%,var(--wc-bg2));border-color:color-mix(in srgb,var(--app-warning) 32%,var(--wc-border)); }
  .wc-checkup-meter-bad .wc-checkup-meter-value { color:var(--app-warning); }
  .wc-checkup-bar-wrap { display:flex;flex-direction:column;gap:7px; }
  .wc-checkup-bar { display:flex;height:4px;border-radius:9999px;background:var(--wc-border-light);overflow:hidden; }
  .wc-checkup-bar span { display:block;height:100%; }
  .wc-checkup-bar-ok { background:var(--app-success); }
  .wc-checkup-bar-cdn { background:var(--brand); }
  .wc-checkup-bar-bad { background:var(--app-warning); }
  .wc-checkup-bar-legend { display:flex;gap:16px;font-size:11px;color:var(--wc-muted); }
  .wc-checkup-legend-item { display:inline-flex;align-items:center;gap:5px; }
  .wc-checkup-legend-item::before { content:"";width:6px;height:6px;border-radius:50%;background:currentColor; }
  .wc-legend-ok { color:var(--app-success); }
  .wc-legend-cdn { color:var(--brand); }
  .wc-legend-bad { color:var(--app-warning); }
  .wc-checkup-hint { font-size:11.5px;color:var(--wc-muted);line-height:1.6;margin:0; }
  .wc-checkup-healthy { display:flex;align-items:center;gap:13px;padding:18px;border:1px solid color-mix(in srgb,var(--app-success) 30%,var(--wc-border));border-radius:12px;background:color-mix(in srgb,var(--app-success) 6%,var(--wc-card));color:var(--app-success); }
  .wc-checkup-healthy-title { font-size:15px;font-weight:700;color:var(--wc-text); }
  .wc-checkup-healthy-sub { font-size:12px;color:var(--wc-muted);margin-top:3px; }
  .wc-checkup-tools { display:flex;align-items:center;gap:8px; }
  .wc-checkup-search { display:flex;align-items:center;gap:7px;flex:1;min-width:0;height:30px;padding:0 10px;border:1px solid var(--wc-border);border-radius:8px;background:transparent;color:var(--wc-muted);transition:border-color .12s ease; }
  .wc-checkup-search:focus-within { border-color:color-mix(in srgb,var(--wc-theme) 55%,var(--wc-border)); }
  .wc-checkup-search input { flex:1;min-width:0;background:transparent;border:none;outline:none;color:var(--wc-text);font-size:12px; }
  .wc-checkup-search input::placeholder { color:var(--wc-muted); }
  .wc-checkup-chip { display:inline-flex;align-items:center;gap:6px;height:30px;padding:0 10px;border:1px solid var(--wc-border);border-radius:8px;background:transparent;color:var(--wc-text2);font-size:12px;cursor:pointer;white-space:nowrap;user-select:none;transition:all .12s ease; }
  .wc-checkup-chip:hover { background:var(--wc-item-hover); }
  .wc-checkup-chip:has(input:checked) { border-color:color-mix(in srgb,var(--app-warning) 45%,var(--wc-border));background:color-mix(in srgb,var(--app-warning) 8%,var(--wc-card));color:var(--wc-text); }
  .wc-checkup-chip input { accent-color:var(--wc-theme); }
  .wc-checkup-sort { height:30px;padding:0 8px;border:1px solid var(--wc-border);border-radius:8px;background:var(--wc-card);color:var(--wc-text);font-size:12px;cursor:pointer; }
  .wc-checkup-sort:focus-visible { outline:2px solid color-mix(in srgb,var(--wc-theme) 55%,transparent);outline-offset:1px; }
  .wc-checkup-table-wrap { border:1px solid var(--wc-border);border-radius:12px;overflow:auto;max-height:340px; }
  .wc-checkup-table { width:100%;border-collapse:collapse;font-size:12px; }
  .wc-checkup-table th { text-align:left;padding:8px 12px;font-size:11px;font-weight:600;letter-spacing:0.06em;color:var(--wc-muted);background:var(--wc-item-active);border-bottom:1px solid var(--wc-border);white-space:nowrap;position:sticky;top:0; }
  .wc-checkup-table td { padding:8px 12px;border-top:1px solid var(--wc-border-light);color:var(--wc-text2);vertical-align:middle; }
  .wc-checkup-table .num { text-align:right;font-variant-numeric:tabular-nums; }
  .wc-checkup-table tbody tr { transition:background .1s ease; }
  .wc-checkup-table tbody tr:hover { background:var(--wc-item-hover); }
  .wc-checkup-row-bad { background:color-mix(in srgb,var(--app-warning) 5%,transparent); }
  .wc-checkup-name { font-size:12px;font-weight:600;color:var(--wc-text);line-height:1.35;max-width:240px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap; }
  .wc-checkup-wxid { font-size:10.5px;color:var(--wc-muted);margin-top:1px;max-width:240px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap; }
  .wc-checkup-cell-bad { color:var(--app-warning);font-weight:700; }
  .wc-checkup-rate { display:inline-flex;align-items:center;justify-content:flex-end;gap:7px;min-width:86px; }
  .wc-checkup-rate-bar { width:44px;height:4px;border-radius:9999px;background:var(--wc-border-light);overflow:hidden; }
  .wc-checkup-rate-fill { display:block;height:100%;background:var(--app-warning); }
  .wc-checkup-empty-row { padding:18px 12px;text-align:center;color:var(--wc-muted); }
  .wc-checkup-ft { display:flex;align-items:center;justify-content:space-between;gap:12px;padding:12px 18px;border-top:1px solid var(--wc-border); }
  .wc-checkup-ft-stat { font-size:11.5px;color:var(--wc-muted); }
  .wc-checkup-ft-stat b { color:var(--app-warning);font-weight:700;font-size:12px; }
  .wc-checkup-ft-actions { display:flex;gap:8px; }
  :global(.wc-ihb.wc-checkup-primary) { background:var(--primary);border-color:var(--primary);color:var(--primary-foreground);font-weight:600; }
  :global(.wc-ihb.wc-checkup-primary:hover:not(:disabled)) { background:color-mix(in oklab,var(--primary) 86%,var(--background));border-color:var(--primary);color:var(--primary-foreground); }
  .wc-checkup-error { display:flex;align-items:center;gap:10px;padding:14px;border:1px solid color-mix(in srgb,var(--app-danger) 32%,var(--wc-border));border-radius:10px;background:color-mix(in srgb,var(--app-danger) 7%,var(--wc-card));color:var(--app-danger);font-size:12.5px; }
  .wc-checkup-error span { flex:1; }
  /* 骨架（首次加载，不用居中的转圈） */
  .wc-checkup-sk { position:relative;overflow:hidden;background:var(--wc-item-active); }
  .wc-checkup-sk-bar { height:4px;border-radius:9999px;background:var(--wc-item-active); }
  .wc-checkup-sk-line { height:11px;border-radius:6px;background:var(--wc-item-active); }
  .wc-checkup-sk-table { display:flex;flex-direction:column;gap:8px; }
  .wc-checkup-sk-row { height:34px;border-radius:8px;background:var(--wc-item-active); }
  .wc-checkup-sk::after, .wc-checkup-sk-bar::after, .wc-checkup-sk-line::after, .wc-checkup-sk-row::after { content:"";position:absolute;inset:0;background:linear-gradient(90deg,transparent,color-mix(in srgb,var(--wc-card) 55%,transparent),transparent);animation:wc-sk-shimmer 1.4s ease-in-out infinite; }
  @keyframes wc-sk-shimmer { from { transform:translateX(-100%); } to { transform:translateX(100%); } }
  @media (prefers-reduced-motion: reduce) {
    .wc-checkup-sk::after, .wc-checkup-sk-bar::after, .wc-checkup-sk-line::after, .wc-checkup-sk-row::after { animation:none; }
    .wc-checkup-dialog { animation:none; }
  }
  /* 账号不一致警示条 */
  .wc-account-banner {
    display:flex;
    flex-wrap:wrap;
    align-items:center;
    gap:8px 12px;
    padding:8px 14px;
    font-size:11.5px;
    line-height:1.6;
    color:color-mix(in srgb, #b9770c 85%, #000);
    background:color-mix(in srgb, #f5a623 12%, transparent);
    border-bottom:1px solid color-mix(in srgb, #f5a623 35%, transparent);
  }
  .wc-account-banner-dot { width:7px;height:7px;border-radius:50%;flex-shrink:0;background:#f5a623;box-shadow:0 0 8px #f5a623; }
  .wc-account-banner-text { flex:1;min-width:340px; }
  .wc-account-banner-text b { font-weight:700; }
  .wc-account-banner-msg { font-size:11px;color:color-mix(in srgb, #b9770c 75%, #000);flex-shrink:0; }
  .wc-account-banner-msg.wc-account-banner-err { color:#d64545; }
  .wc-export-header { display:flex;align-items:center;justify-content:space-between;padding:12px 16px;border-bottom:1px solid var(--wc-border);font-size:14px;font-weight:600; }
  .wc-export-close { background:none;border:none;font-size:20px;cursor:pointer;color:var(--wc-muted);padding:0 4px;line-height:1; }
  .wc-export-close:hover { color:var(--wc-text); }
  .wc-export-close:disabled { opacity:.3;cursor:not-allowed; }
  .wc-export-body { padding:16px;display:flex;flex-direction:column;gap:16px; }
  .wc-export-field { display:flex;flex-direction:column;gap:8px; }
  .wc-export-label { font-size:12px;font-weight:600;color:var(--wc-text); }
  .wc-export-options { display:flex;gap:12px; }
  .wc-export-radio { display:flex;align-items:center;gap:6px;font-size:12px;color:var(--wc-text2);cursor:pointer;padding:4px 10px;border:1px solid var(--wc-border);border-radius:6px;transition:all .12s ease; }
  .wc-export-radio input { accent-color:var(--wc-theme); }
  .wc-export-radio-on { border-color:var(--wc-theme);color:var(--wc-theme);background:color-mix(in srgb,var(--wc-theme) 10%,transparent); }
  .wc-export-presets { display:flex;flex-wrap:wrap;gap:6px; }
  .wc-export-custom { display:flex;align-items:center;gap:8px;font-size:12px;color:var(--wc-text2); }
  .wc-export-custom input[type="checkbox"] { accent-color:var(--wc-theme); }
  .wc-export-input { width:100px;padding:4px 8px;border:1px solid var(--wc-border);border-radius:4px;background:transparent;color:var(--wc-text);font-size:12px; }
  .wc-export-error { padding:8px 12px;border-radius:6px;background:color-mix(in srgb,#fa5151 12%,transparent);border:1px solid color-mix(in srgb,#fa5151 25%,transparent);color:#fa5151;font-size:11.5px; }
  .wc-export-footer { display:flex;justify-content:flex-end;gap:8px;padding:12px 16px;border-top:1px solid var(--wc-border); }
  .wc-export-result { padding:28px 20px;text-align:center; }
  .wc-export-result-info { font-size:14px;font-weight:600;color:var(--wc-text);margin-bottom:10px; }
  .wc-export-result-path { font-size:11.5px;color:var(--wc-muted);word-break:break-all;background:var(--wc-bg2);padding:10px 12px;border-radius:6px;border:1px solid var(--wc-border);user-select:all;cursor:text;margin-bottom:8px; }
  .wc-export-result-hint { font-size:11.5px;color:var(--wc-muted); }
  @keyframes wc-fade-in { from { opacity:0;transform:translateY(-4px); } to { opacity:1;transform:translateY(0); } }
  .wc-btn { font-size:11.5px; height:26px; padding:0 10px; border:1px solid var(--wc-border); border-radius:4px; background:transparent; color:var(--wc-text2); cursor:pointer; transition:all .12s ease; display:inline-flex; align-items:center; gap:4px; white-space:nowrap; }
  .wc-btn:hover { background:var(--wc-item-hover); }
  .wc-btn:disabled { opacity:.4; cursor:default; }
  .wc-body { flex:1; display:flex; overflow:hidden; }

  /* 左侧导航 + 面板 */
  .wc-sidebar { display:flex; border-right:1px solid var(--wc-border); background:var(--wc-sidebar-bg); }
  .wc-nav { width:var(--wc-nav-w); flex-shrink:0; display:flex; flex-direction:column; align-items:stretch; padding:6px 6px 8px; gap:2px; background:var(--wc-nav-bg); overflow-y:auto; scrollbar-width:thin; }
  .wc-nav-group { display:flex; flex-direction:column; gap:1px; padding-bottom:6px; }
  .wc-nav-label { font-size:11.5px; font-weight:600; letter-spacing:0.14em; color:var(--wc-muted); padding:8px 8px 4px; text-transform:uppercase; }
  /* 标题导航按钮整行靠左：图标+文字从左侧对齐 */
  .wc-nav :global(.wc-ihb) { width:100%; justify-content:flex-start; border:none; background:transparent; box-shadow:none; }
  .wc-nav :global(.wc-ihb:hover:not(:disabled)) { background:var(--wc-nav-hover); }
  .wc-nav :global(.wc-ihb.wc-ihb-active) { background:var(--wc-nav-active); color:var(--wc-text); box-shadow: inset 2px 0 0 var(--wc-theme); }
  .wc-chat-list, .wc-contact-list { width:var(--wc-sidebar-w); display:flex; flex-direction:column; overflow:hidden; }
  .wc-chat-list { overflow-y:auto; scrollbar-width:thin; }
  .wc-contact-scroll { flex:1; overflow-y:auto; scrollbar-width:thin; }
  .wc-search { padding:8px 10px; flex-shrink:0; display:flex; align-items:center; gap:6px; }
  .wc-search input { flex:1; min-width:0; padding:5px 10px; border:1px solid var(--wc-border); border-radius:4px; background:var(--wc-card); font-size:12px; color:var(--wc-text); outline:none; }
  .wc-search input::placeholder { color:var(--wc-muted); }
  .wc-search-pad { border-bottom:1px solid var(--wc-border-light); }

  .wc-batch-bar { display:flex;align-items:center;gap:6px;padding:6px 12px;border-bottom:1px solid var(--wc-border-light);background:var(--wc-bg2);flex-shrink:0; }
  /* 会话侧栏统计条（好友/群聊/未读） */
  .wc-session-stats { display:flex;align-items:center;gap:12px;padding:5px 12px;font-size:11.5px;color:var(--wc-muted);border-bottom:1px solid var(--wc-border-light);flex-shrink:0;background:var(--wc-bg); }
  .wc-session-stats-unread { color:var(--wc-theme);font-weight:600; }
  .wc-session-stats span { font-variant-numeric:tabular-nums; }
  .wc-batch-bar-pad { margin:0 0 8px; }
  /* 工具栏按钮组向右靠齐（收藏/朋友圈/表情/文件头部、批量栏） */
  .wc-favs-hd :global(.wc-ihb:first-of-type),
  .wc-db-hd :global(.wc-ihb:first-of-type),
  .wc-batch-bar :global(.wc-ihb:first-of-type) { margin-left: auto; }
  .wc-batch-count { font-size:11.5px;color:var(--wc-text2);flex:1; }
  .wc-checkbox { display:inline-flex;align-items:center;justify-content:center;width:16px;height:16px;border:1.5px solid var(--wc-border);border-radius:4px;background:var(--wc-card);color:#fff;font-size:11.5px;flex-shrink:0;user-select:none; }
  .wc-checkbox-on { background:var(--wc-theme);border-color:var(--wc-theme); }
  .wc-cat-bar-pad { padding:0 10px;margin-bottom:8px; }
  .wc-clear-warn p { font-size:13px;color:var(--wc-text);margin:0 0 8px;line-height:1.6; }
  .wc-clear-warn-sub { font-size:11.5px !important;color:var(--wc-muted) !important; }
  .wc-mgmt-toast { position:fixed;left:50%;bottom:28px;transform:translateX(-50%);z-index:10000;max-width:72vw;padding:9px 16px;border-radius:8px;background:rgba(20,20,20,.92);color:#fff;font-size:12px;line-height:1.5;box-shadow:0 6px 24px rgba(0,0,0,.3);word-break:break-all;animation:wc-fade-in .15s ease; }
  .wc-mgmt-toast-err { background:rgba(160,32,32,.94); }
  .wc-chat-item,.wc-contact-item { display:flex; align-items:center; gap:10px; width:100%; padding:9px 12px; border:none; background:transparent; cursor:pointer; text-align:left; transition:background .1s ease, box-shadow .12s ease; flex-shrink:0; position:relative; }
  .wc-chat-item:hover,.wc-contact-item:hover { background:var(--wc-item-hover); }
  .wc-chat-active { background:var(--wc-item-active) !important; box-shadow: inset 2px 0 0 var(--app-wc-accent); }
  .wc-chat-pinned { background:color-mix(in srgb, var(--wc-item-active) 82%, transparent); }
  .wc-pinned-hd { display:flex; align-items:center; gap:6px; width:100%; padding:7px 12px; border:none; border-top:1px solid var(--wc-border-light); border-bottom:1px solid var(--wc-border-light); background:color-mix(in srgb, var(--wc-item-active) 86%, transparent); cursor:pointer; text-align:left; font-size:11.5px; font-weight:600; letter-spacing:.4px; color:var(--wc-muted); transition:background .12s ease,color .12s ease; flex-shrink:0; }
  .wc-pinned-hd:hover { color:var(--wc-text2); background:color-mix(in srgb, var(--wc-item-active) 92%, transparent); }
  .wc-pinned-hd-count { min-width:16px; padding:0 5px; border-radius:8px; background:color-mix(in srgb, var(--wc-text) 12%, transparent); font-size:11.5px; font-weight:600; line-height:16px; text-align:center; color:var(--wc-muted); }
  .wc-pinned-hd-arrow { margin-left:auto; opacity:.65; transition:transform .18s ease; }
  .wc-pinned-hd-collapsed { transform:rotate(-90deg); }
  .wc-avatar { width:40px;height:40px;flex-shrink:0;border-radius:6px;display:flex;align-items:center;justify-content:center;font-size:16px;font-weight:700;color:var(--wc-text2);background:color-mix(in srgb,var(--wc-text) 8%,transparent);user-select:none;overflow:hidden; }
  .wc-avatar-img { width:100%;height:100%;object-fit:cover;border-radius:6px; }
  .wc-chat-info { flex:1; min-width:0; display:flex; flex-direction:column; gap:2px; }
  .wc-chat-top { display:flex; align-items:center; justify-content:space-between; gap:4px; }
  .wc-chat-name-group { display:flex; align-items:center; gap:4px; min-width:0; flex:1; }
  .wc-chat-name { font-size:13px;font-weight:600;color:var(--wc-text);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;min-width:0; }
  .wc-chat-hidden-badge { flex-shrink:0; font-size:10px; line-height:1; padding:2px 4px; border-radius:3px; background:color-mix(in srgb,var(--wc-muted) 16%,transparent); color:var(--wc-muted); }
  .wc-chat-pin { display:inline-flex; align-items:center; flex-shrink:0; color:var(--wc-theme); opacity:.9; }
  .wc-chat-time-group { display:inline-flex; align-items:center; gap:3px; flex-shrink:0; margin-left:6px; }
  .wc-chat-time { font-size:11.5px;color:var(--wc-muted);white-space:nowrap;flex-shrink:0; font-variant-numeric:tabular-nums; }
  .wc-chat-bottom { display:flex; align-items:center; gap:4px; }
  .wc-avatar-official { background:color-mix(in srgb,var(--app-success,#2e7d32) 62%,var(--wc-text)) !important; color:#fff !important; font-size:14px; }
  .wc-loading-inline { display:inline-block;width:14px;height:14px;margin-right:6px;border:2px solid var(--wc-border);border-top-color:var(--wc-text);border-radius:50%;animation:wc-spin .7s linear infinite;vertical-align:middle; }
  .wc-chat-preview { flex:1; min-width:0; font-size:12px; line-height:1.45; color:var(--wc-text2); overflow:hidden; display:-webkit-box; -webkit-line-clamp:2; line-clamp:2; -webkit-box-orient:vertical; word-break:break-all; }
  .wc-draft-tag { color:#fa5151; margin-right:2px; }
  .wc-draft-text { flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .wc-draft-clear { display:inline-flex; align-items:center; justify-content:center; width:16px; height:16px; margin-left:4px; border-radius:50%; color:var(--wc-muted); cursor:pointer; font-size:11.5px; line-height:1; vertical-align:middle; flex-shrink:0; }
  .wc-draft-clear:hover { background:color-mix(in srgb,var(--app-danger,#d32f2f) 15%,transparent); color:var(--app-danger,#d32f2f); }
  .wc-badge { flex-shrink:0;min-width:18px;height:18px;border-radius:9px;display:flex;align-items:center;justify-content:center;font-size:11.5px;font-weight:700;color:#fff;background:color-mix(in srgb,#f44 80%,#c00);padding:0 5px; }
  .wc-empty { display:flex;align-items:center;justify-content:center;color:var(--wc-muted);font-size:13px;padding:40px;text-align:center; }
  /* ── 未选会话空状态：Gargantua 黑洞背景 + 底部提示 ── */
  .wc-no-session { position:relative; flex:1; overflow:hidden; min-height:100%; background:color-mix(in srgb, var(--wc-card) 92%, var(--wc-muted)); }
  .wc-ns-hint {
    position:absolute;
    left:0;
    right:0;
    bottom:0;
    z-index:1;
    display:flex;
    flex-direction:column;
    align-items:center;
    gap:4px;
    padding-bottom:48px;
    pointer-events:none;
    background:linear-gradient(180deg, transparent, color-mix(in srgb, var(--wc-card) 82%, transparent) 70%);
  }
  .wc-ns-hint-title { font-size:13.5px; font-weight:600; color:var(--wc-text); }
  .wc-ns-hint-sub { font-size:12px; color:var(--wc-text2); }
  .wc-error-hint { flex-direction:column; gap:6px; padding:24px 16px; }
  .wc-error-text { font-size:11.5px; color:#fa5151; word-break:break-all; }
  .wc-session-warn { display:flex; align-items:center; gap:6px; padding:8px 12px; margin:6px 8px; border-radius:6px; background:color-mix(in srgb,#fa5151 10%,transparent); border:1px solid color-mix(in srgb,#fa5151 25%,transparent); font-size:11.5px; color:#fa5151; word-break:break-all; flex-shrink:0; }
  .wc-session-warn-icon { font-size:13px; flex-shrink:0; }

  /* 通讯录 */
  .wc-cat-bar { display:grid; grid-template-columns:repeat(2,1fr); gap:6px; padding:0 10px 8px; flex-shrink:0; }
  .wc-letter-hd { padding:6px 12px 3px; font-size:11.5px; font-weight:700; color:var(--wc-muted); position:sticky; top:0; background:var(--wc-sidebar-bg); z-index:1; }
  .wc-contact-type { font-size:11.5px;padding:1px 6px;border-radius:4px;background:color-mix(in srgb,var(--wc-theme) 14%,transparent);color:var(--wc-theme);font-weight:600;white-space:nowrap;flex-shrink:0; }
  .wc-official-badge { flex-shrink:0; font-size:11.5px; padding:1px 6px; border-radius:4px; font-weight:600; white-space:nowrap; background:color-mix(in srgb,var(--app-success,#07c160) 14%,transparent); color:var(--app-success,#07c160); }
  .wc-official-badge-service { background:color-mix(in srgb,var(--wc-theme) 14%,transparent); color:var(--wc-theme); }
  .wc-official-badge-ent { background:color-mix(in srgb,#9c27b0 18%,transparent); color:#9c27b0; }
  .wc-contact-sub { display:flex;font-size:11.5px;color:var(--wc-muted);gap:6px;overflow:hidden; }
  .wc-contact-desc { overflow:hidden;text-overflow:ellipsis;white-space:nowrap; }
  .wc-contact-footer { display:flex; align-items:center; justify-content:center; gap:6px; padding:14px 12px; font-size:11.5px; color:var(--wc-muted); }
  .wc-contact-hint { opacity:0.7; }

  /* 聊天窗口 */
  .wc-main { flex:1; display:flex; flex-direction:column; min-width:0; }
  .wc-chat-hd { display:flex; align-items:center; gap:10px; padding:8px 16px; border-bottom:1px solid var(--wc-border); flex-shrink:0; min-height:48px; }
  .wc-chat-hd-info { flex:1; min-width:0; }
  .wc-chat-hd-name { font-size:14px;font-weight:600; }
  .wc-chat-hd-user { font-size:11.5px;color:var(--wc-muted);margin-top:1px;word-break:break-all; }
  /* 聊天头部消息构成画像 chips */
  .wc-msg-type-chips { display:flex; flex-wrap:wrap; gap:4px; margin-top:3px; }
  .wc-msg-type-chip { font-size:10.5px; padding:1px 7px; border-radius:999px; background:color-mix(in srgb,var(--wc-theme) 10%,transparent); color:var(--wc-text2); border:1px solid var(--wc-border-light); font-variant-numeric:tabular-nums; }

  .wc-miniapp-dialog { width:360px; max-width:90vw; }
  .wc-miniapp-detail-row { display:flex; align-items:center; gap:10px; margin-bottom:10px; }
  .wc-miniapp-detail-icon { width:44px; height:44px; border-radius:10px; object-fit:cover; flex-shrink:0; background:var(--wc-bg2); }
  .wc-miniapp-detail-icon-ph { display:flex; align-items:center; justify-content:center; font-size:22px; }
  .wc-miniapp-detail-head { min-width:0; }
  .wc-miniapp-detail-name { font-size:15px; font-weight:700; }
  .wc-miniapp-detail-des { font-size:12px; color:var(--wc-muted); margin-top:2px; word-break:break-all; }
  .wc-miniapp-detail-tip { font-size:12px; color:var(--wc-muted); background:color-mix(in srgb, var(--wc-text) 5%, transparent); border-radius:6px; padding:8px 10px; line-height:1.5; margin-bottom:10px; }
  .wc-miniapp-detail-kv { display:flex; gap:8px; font-size:12px; margin-bottom:6px; word-break:break-all; }
  .wc-miniapp-detail-kv span { color:var(--wc-muted); flex-shrink:0; width:64px; }
  .wc-miniapp-detail-kv code { color:var(--wc-text2); font-family:inherit; }
  .wc-miniapp-detail-path { white-space:pre-wrap; }
  .wc-miniapp-detail-actions { display:flex; justify-content:flex-end; gap:8px; margin-top:14px; }

  /* 朋友圈 */
  .wc-moments { flex:1; display:flex; flex-direction:column; padding:0; min-height:0; background:var(--wc-bg); }
  /* 顶部工具条：标题+计数 左侧，搜索/返回/刷新/导出 右侧 */
  .wc-moments-toolbar { flex-shrink:0; display:flex; align-items:center; justify-content:space-between; gap:12px; padding:11px 16px; border-bottom:1px solid var(--wc-border-light); }
  .wc-moments-title { display:flex; align-items:baseline; gap:10px; min-width:0; }
  .wc-moments-name { font-size:15px; font-weight:700; color:var(--wc-text); }
  .wc-moments-filtered { font-size:12px; color:var(--wc-theme); font-weight:600; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; max-width:200px; }
  .wc-moments-count { font-size:12px; font-weight:400; color:var(--wc-muted); }
  .wc-moments-actions { display:flex; align-items:center; gap:8px; flex-shrink:0; }
  .wc-moments-search { width:190px; padding:5px 10px; border-radius:8px; border:1px solid var(--wc-border); background:var(--wc-bg2); color:var(--wc-text); font-size:12px; outline:none; transition:border-color .12s ease; }
  .wc-moments-search:focus { border-color:var(--wc-theme); }
  .wc-moments-search::placeholder { color:var(--wc-muted); }
  .wc-moments-fmt { height:28px; padding:0 8px; border-radius:7px; border:1px solid var(--wc-border); background:var(--wc-card,var(--card)); color:var(--wc-text2,var(--foreground)); font-size:12px; font-family:inherit; outline:none; cursor:pointer; transition:border-color .14s ease, background .14s ease; }
  .wc-moments-fmt:hover:not(:disabled) { border-color:color-mix(in srgb,var(--wc-theme,var(--brand)) 48%,var(--wc-border)); }
  .wc-moments-fmt:disabled { opacity:0.48; cursor:not-allowed; }
  /* 朋友圈洞察条：统计卡 + 作者榜 + 月度热度 */
  .wc-moments-insight { flex-shrink:0; display:flex; flex-direction:column; gap:10px; padding:12px 16px; border-bottom:1px solid var(--wc-border-light); background:color-mix(in srgb,var(--wc-theme) 4%,var(--wc-bg)); }
  .wc-mi-stats { display:flex; gap:10px; flex-wrap:wrap; }
  .wc-mi-stat { display:flex; flex-direction:column; align-items:center; gap:2px; min-width:76px; padding:8px 12px; border-radius:8px; background:var(--wc-bg2); border:1px solid var(--wc-border-light); }
  button.wc-mi-stat { cursor:pointer; font:inherit; transition:border-color .12s ease, background .12s ease; }
  button.wc-mi-stat:hover { border-color:var(--wc-theme); }
  .wc-mi-stat-on { background:color-mix(in srgb,var(--wc-theme) 12%,transparent); border-color:var(--wc-theme); }
  .wc-mi-num { font-size:16px; font-weight:700; color:var(--wc-theme); font-variant-numeric:tabular-nums; }
  .wc-mi-label { font-size:11px; color:var(--wc-muted); }
  .wc-mi-body { display:flex; gap:16px; flex-wrap:wrap; }
  .wc-mi-authors { flex:1; min-width:200px; display:flex; flex-direction:column; gap:6px; }
  .wc-mi-hd { font-size:11.5px; font-weight:600; color:var(--wc-muted); }
  .wc-mi-author-list { display:flex; flex-direction:column; gap:3px; }
  .wc-mi-author { display:flex; align-items:center; gap:6px; font-size:12.5px; color:var(--wc-text); border:none; background:transparent; padding:2px 4px; border-radius:6px; cursor:pointer; text-align:left; }
  .wc-mi-author:hover { background:var(--wc-nav-hover, var(--wc-bg2)); }
  .wc-mi-author-on { background:color-mix(in srgb,var(--wc-theme) 12%,transparent); }
  .wc-mi-author-on .wc-mi-posts { color:var(--wc-theme); font-weight:700; }
  .wc-mi-rank { width:16px; height:16px; border-radius:4px; background:color-mix(in srgb,var(--wc-theme) 14%,transparent); color:var(--wc-theme); font-size:10.5px; font-weight:700; display:inline-flex; align-items:center; justify-content:center; flex-shrink:0; }
  .wc-mi-author-name { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; max-width:140px; }
  .wc-mi-posts { margin-left:auto; font-size:11px; color:var(--wc-muted); font-variant-numeric:tabular-nums; }
  .wc-mi-months { flex:1.4; min-width:280px; display:flex; flex-direction:column; gap:6px; }
  .wc-mi-months-hd { display:flex; align-items:baseline; gap:8px; flex-wrap:wrap; }
  .wc-mi-months-meta { margin-left:auto; font-size:10.5px; color:var(--wc-muted); white-space:nowrap; font-variant-numeric:tabular-nums; }
  .wc-mi-bars { display:flex; gap:5px; align-items:flex-end; height:86px; }
  .wc-mi-bar-col { flex:1; display:flex; flex-direction:column; align-items:center; gap:3px; min-width:0; height:100%; justify-content:flex-end; }
  /* 数值（柱顶）：12 个月并排，字号克制避免拥挤 */
  .wc-mi-bar-val { height:13px; line-height:13px; font-size:9.5px; font-weight:600; color:var(--wc-text2, var(--muted-foreground)); font-variant-numeric:tabular-nums; transition:color .15s ease; }
  .wc-mi-bar-val-zero { color:transparent; }
  /* 轨道：低饱和主题色底，圆角胶囊 */
  .wc-mi-bar { width:100%; max-width:24px; height:52px; border-radius:999px; background:color-mix(in srgb,var(--wc-theme) 9%,transparent); display:flex; align-items:flex-end; overflow:hidden; }
  .wc-mi-bar-fill { width:100%; border-radius:999px; background:linear-gradient(180deg,color-mix(in srgb,var(--wc-theme) 92%,#fff 8%),color-mix(in srgb,var(--wc-theme) 55%,transparent)); min-height:3px; transform-origin:bottom; animation:wc-mi-grow .6s cubic-bezier(.22,.9,.36,1) both; transition:filter .15s ease; }
  @keyframes wc-mi-grow { from { transform:scaleY(0); } to { transform:scaleY(1); } }
  /* 悬停：柱体增亮 + 数值转主题色 */
  .wc-mi-bar-col:hover .wc-mi-bar-fill { filter:brightness(1.22); }
  .wc-mi-bar-col:hover .wc-mi-bar-val { color:var(--wc-theme); }
  .wc-mi-bar-col:hover .wc-mi-bar-val-zero { color:var(--wc-muted); }
  /* 峰值月：数值/月份高亮，柱体更亮 + 内描边强调（外层 overflow 会裁掉外发光） */
  .wc-mi-bar-col-peak .wc-mi-bar-val { color:var(--wc-theme); font-weight:700; }
  .wc-mi-bar-col-peak .wc-mi-bar-fill { background:linear-gradient(180deg,color-mix(in srgb,var(--wc-theme) 68%,#fff 32%),var(--wc-theme)); box-shadow:inset 0 0 0 1px color-mix(in srgb,var(--wc-theme) 65%,transparent); }
  .wc-mi-bar-col-peak .wc-mi-bar-label { color:var(--wc-theme); font-weight:700; }
  .wc-mi-bar-label { font-size:10px; color:var(--wc-muted); white-space:nowrap; transition:color .15s ease; }
  /* 时间线滚动区：日期分组 + 动态卡片 */
  .wc-moments-scroll { flex:1; overflow-y:auto; overflow-x:hidden; scrollbar-width:thin; padding:4px 14px 16px; max-width:100%; }
  /* 日期分组条 */
  .wc-moment-day { display:flex; align-items:center; gap:8px; margin:16px 0 10px; padding:0 4px; }
  .wc-moment-day:first-child { margin-top:10px; }
  .wc-moment-day-label { font-size:12.5px; font-weight:700; color:var(--wc-text2); }
  .wc-moment-day-count { font-size:11px; color:var(--wc-muted); font-variant-numeric:tabular-nums; }
  .wc-moment-day::after { content:''; flex:1; height:1px; background:var(--wc-border-light); }
  /* 动态卡片 */
  .wc-moment-card { display:flex; gap:12px; padding:14px 14px; margin:0 0 10px; border:1px solid var(--wc-border-light); border-radius:12px; background:var(--wc-card, var(--wc-bg2)); transition:border-color .15s ease, box-shadow .15s ease; }
  .wc-moment-card:hover { border-color:color-mix(in srgb,var(--wc-theme) 35%,transparent); box-shadow:0 2px 10px rgba(0,0,0,0.12); }
  .wc-moment-avatar { width:42px; height:42px; border-radius:50%; display:flex; align-items:center; justify-content:center; font-size:16px; font-weight:600; background:color-mix(in srgb,var(--wc-text) 10%,transparent); color:var(--wc-text2); flex-shrink:0; overflow:hidden; }
  .wc-moment-avatar img { width:100%; height:100%; object-fit:cover; }
  .wc-moment-avatar .wc-msg-letter { width:100%; height:100%; font-size:16px; }
  .wc-moment-body { flex:1; min-width:0; max-width:100%; display:flex; flex-direction:column; gap:6px; }
  .wc-moment-meta { display:flex; align-items:center; gap:8px; }
  .wc-moment-author { font-size:14px; font-weight:600; color:#576b95; }
  .wc-moment-time { font-size:11.5px; color:var(--wc-muted); margin-left:auto; flex-shrink:0; }
  .wc-moment-content { font-size:13.5px; line-height:1.7; color:var(--wc-text); word-break:break-word; overflow-wrap:break-word; white-space:pre-wrap; hyphens:auto; }
  .wc-moment-images { display:grid; grid-template-columns:repeat(3,1fr); gap:4px; margin-top:6px; border-radius:8px; overflow:hidden; max-width:min(100%,340px); }
  .wc-moment-images-single { grid-template-columns:1fr; max-width:200px; }
  .wc-moment-images-four { grid-template-columns:repeat(2,1fr); max-width:224px; }
  .wc-moment-img-wrap { position:relative; width:100%; aspect-ratio:1; overflow:hidden; background:color-mix(in srgb,var(--wc-text) 8%,transparent); border-radius:4px; }
  .wc-moment-img-wrap img { width:100%; height:100%; object-fit:cover; display:block; cursor:pointer; transition:transform 0.2s; }
  .wc-moment-img-wrap img:hover { transform:scale(1.05); }
  .wc-moment-img-wrap.wc-moment-img-loading { display:flex; align-items:center; justify-content:center; }
  .wc-moment-img-wrap.wc-moment-img-fail { display:flex; align-items:center; justify-content:center; }
  .wc-moment-img-wrap.wc-moment-img-fail::after { content:'图片加载失败'; font-size:11.5px; color:var(--wc-muted); }
  .wc-moment-videos { display:grid; grid-template-columns:repeat(3,1fr); gap:4px; margin-top:6px; max-width:min(100%,340px); }
  .wc-moment-videos-single { grid-template-columns:1fr; max-width:200px; }
  .wc-moment-video-tile { position:relative; width:100%; aspect-ratio:1; overflow:hidden; background:#1a1a1f; border-radius:4px; cursor:pointer; }
  .wc-moment-video-tile img, .wc-moment-video-ph { width:100%; height:100%; object-fit:cover; display:block; }
  .wc-moment-video-ph { background:linear-gradient(145deg,#26262e,#15151a); }
  .wc-moment-video-badge { position:absolute; inset:0; margin:auto; width:30px; height:30px; display:flex; align-items:center; justify-content:center; background:rgba(0,0,0,.55); border:2px solid rgba(255,255,255,.85); border-radius:50%; color:#fff; font-size:12px; padding-left:3px; box-sizing:border-box; pointer-events:none; }
  .wc-moment-video-dur { position:absolute; right:4px; bottom:4px; padding:1px 5px; border-radius:4px; background:rgba(0,0,0,.65); color:#fff; font-size:11.5px; line-height:1.5; pointer-events:none; }
  .wc-moment-tags { display:flex; gap:6px; flex-wrap:wrap; margin-top:4px; }
  .wc-moment-media { display:inline-flex; align-items:center; gap:6px; padding:3px 10px; border-radius:12px; background:color-mix(in srgb,var(--wc-accent, #4a9eff) 12%, transparent); color:var(--wc-text2); font-size:11.5px; border:1px solid color-mix(in srgb,var(--wc-accent, #4a9eff) 20%, transparent); }
  /* 点赞 / 评论：卡片内嵌套灰底互动区 */
  .wc-moment-actions { display:flex; flex-direction:column; gap:5px; margin-top:8px; padding:8px 11px; background:color-mix(in srgb,var(--wc-text) 5%,transparent); border-radius:8px; }
  .wc-moment-likes { display:flex; align-items:baseline; gap:6px; font-size:12.5px; line-height:1.55; color:var(--wc-text); flex-wrap:wrap; word-break:break-word; }
  .wc-moment-like-icon { font-size:11.5px; color:#f44; flex-shrink:0; }
  .wc-moment-comments { display:flex; flex-direction:column; gap:2px; margin-top:5px; padding-top:5px; border-top:1px solid color-mix(in srgb,var(--wc-text) 8%,transparent); }
  .wc-moment-comment-item { font-size:12.5px; line-height:1.6; color:var(--wc-text); word-break:break-word; overflow-wrap:break-word; }
  .wc-moment-comment-name { color:#576b95; font-weight:600; margin-right:3px; }
  .wc-moment-comment-reply { color:#576b95; margin-right:3px; }
  .wc-moment-comment-text { color:var(--wc-text); }
  /* 加载状态/底部 */
  .wc-moment-footer { display:flex; align-items:center; justify-content:center; gap:6px; padding:16px 12px; font-size:11.5px; color:var(--wc-muted); }
  .wc-moment-hint { opacity:0.7; }

  .wc-favs-hd { font-size:14px;font-weight:700;margin-bottom:12px;display:flex;align-items:center;gap:8px; }
  .wc-favs-count { font-size:12px;font-weight:400;color:var(--wc-muted); }
  .wc-fav-item { display:flex;align-items:center;gap:12px;padding:12px 0; }
  .wc-fav-icon { width:40px; display:inline-flex; align-items:center; justify-content:center; flex-shrink:0; color:var(--wc-text2); }
  .wc-chat-item.wc-fav-item { gap:10px; padding:9px 12px; }
  .wc-fav-list-desc { font-size:11.5px; color:var(--wc-muted); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; min-width:0; }
  /* 收藏详情（微信样式：右侧阅读器） */
  .wc-fav-detail { flex:1; overflow-y:auto; scrollbar-width:thin; padding:20px 28px; }
  .wc-fav-detail-hd { border-bottom:1px solid var(--wc-border-light); padding-bottom:14px; margin-bottom:16px; }
  .wc-fav-detail-hd-info { display:flex; align-items:center; gap:12px; }
  .wc-fav-detail-title { font-size:17px; font-weight:700; line-height:1.5; word-break:break-all; }
  .wc-fav-detail-meta { display:flex; align-items:center; gap:8px; font-size:11.5px; color:var(--wc-muted); margin-top:6px; flex-wrap:wrap; }
  .wc-fav-detail-row { display:flex; align-items:center; gap:8px; font-size:13px; color:var(--wc-text); }
  .wc-fav-detail-note { font-size:11.5px; color:var(--wc-muted); }
  .wc-fav-detail-body { display:flex; flex-direction:column; gap:14px; }
  .wc-fav-detail-text { font-size:14px; line-height:1.75; white-space:pre-wrap; word-break:break-word; color:var(--wc-text); }
  .wc-fav-detail-imgs { display:grid; grid-template-columns:repeat(auto-fill,minmax(130px,1fr)); gap:8px; }
  .wc-fav-detail-img { aspect-ratio:1; border-radius:6px; overflow:hidden; background:var(--wc-bg2); cursor:pointer; display:flex; align-items:center; justify-content:center; border:1px solid var(--wc-border-light); }
  .wc-fav-detail-img:hover { border-color:var(--wc-border); }
  .wc-fav-detail-img img { width:100%; height:100%; object-fit:cover; display:block; }
  .wc-fav-detail-row { font-size:13px; color:var(--wc-text2); display:flex; align-items:center; gap:8px; }
  .wc-fav-detail-link { border:1px solid var(--wc-border-light); border-radius:8px; padding:12px 14px; display:flex; flex-direction:column; gap:6px; background:var(--wc-bg2); }
  .wc-fav-detail-link-title { font-size:14px; font-weight:600; word-break:break-all; }
  .wc-fav-detail-link-url { font-size:11.5px; color:var(--wc-muted); word-break:break-all; }
  .wc-fav-detail-file { display:flex; align-items:center; gap:10px; border:1px solid var(--wc-border-light); border-radius:8px; padding:12px 14px; background:var(--wc-bg2); }
  .wc-fav-detail-file-name { font-size:13px; font-weight:600; word-break:break-all; }
  .wc-fav-detail-file-meta { font-size:11.5px; color:var(--wc-muted); flex-shrink:0; }
  .wc-fav-detail-items { border-left:1px solid var(--wc-border-light); padding-left:12px; display:flex; flex-direction:column; gap:8px; }
  .wc-fav-detail-item { font-size:13px; line-height:1.6; word-break:break-word; }
  .wc-fav-detail-item-label { color:var(--wc-text2); }

  /* 表情 */
  .wc-sec-title { font-size:13px;font-weight:700;margin:14px 0 8px;color:var(--wc-text); }
  .wc-sec-count { font-size:11.5px; font-weight:400; color:var(--wc-muted); }
  .wc-emo-api-hint { margin:-2px 0 10px; font-size:11.5px; color:var(--wc-muted); line-height:1.5; }
  .wc-emo-tabs { display:flex; flex-wrap:wrap; gap:6px; margin-bottom:12px; }
  .wc-emo-grid { display:grid; grid-template-columns:repeat(auto-fill,minmax(64px,1fr)); gap:8px; }
  .wc-emo-cell { aspect-ratio:1; position:relative; display:flex; flex-direction:column; align-items:center; justify-content:center; gap:2px; background:var(--wc-card); border:1px solid var(--wc-border-light); border-radius:8px; cursor:pointer; overflow:hidden; transition:border-color .12s ease, box-shadow .12s ease, transform .08s ease; }
  .wc-emo-gif { position:absolute; top:3px; right:3px; font-size:9px; line-height:1; font-weight:700; color:#fff; background:color-mix(in srgb,var(--wc-theme) 85%,transparent); border-radius:3px; padding:2px 4px; pointer-events:none; }
  .wc-emo-cell:hover { border-color:var(--wc-border); box-shadow:0 2px 8px rgba(0,0,0,.12); transform:translateY(-1px); }
  .wc-emo-ph { display:inline-flex; line-height:1; color:var(--wc-text2); }
  .wc-emo-img { width:44px; height:44px; object-fit:contain; }
  .wc-emo-md5 { font-size:11.5px; color:var(--wc-muted); font-family:ui-monospace,Consolas,monospace; letter-spacing:.3px; }
  .wc-pkg-grid { display:grid; grid-template-columns:repeat(auto-fill,minmax(160px,1fr)); gap:10px; }
  .wc-pkg-card { display:flex; flex-direction:column; align-items:center; gap:4px; padding:14px 8px; background:var(--wc-card); border:1px solid var(--wc-border-light); border-radius:8px; transition:border-color .12s ease, box-shadow .12s ease; }
  .wc-pkg-card:hover { border-color:var(--wc-border); box-shadow:0 2px 8px rgba(0,0,0,.12); }
  .wc-pkg-icon { font-size:28px; }
  .wc-pkg-name { font-size:12px; font-weight:600; text-align:center; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; max-width:100%; }
  .wc-pkg-sub { font-size:11.5px; color:var(--wc-muted); }
  .wc-static-emo-search { font-size:12px; padding:4px 8px; border:1px solid var(--wc-border); border-radius:4px; background:var(--wc-bg); color:var(--wc-text); outline:none; min-width:120px; }
  .wc-static-emo-search:focus { border-color:var(--wc-primary); }
  .wc-static-emo-grid { display:grid; grid-template-columns:repeat(auto-fill,minmax(72px,1fr)); gap:8px; }
  .wc-static-emo-cell { display:flex; flex-direction:column; align-items:center; gap:3px; padding:7px 4px; background:var(--wc-card); border:1px solid var(--wc-border-light); border-radius:8px; cursor:default; transition:border-color .12s ease, background .12s ease; }
  .wc-static-emo-cell:hover { border-color:var(--wc-border); background:var(--wc-item-hover); }
  .wc-static-emo-img { width:46px; height:46px; object-fit:contain; }
  .wc-static-emo-name { font-size:11.5px; color:var(--wc-muted); text-align:center; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; max-width:100%; }


  /* ── 图片查看器 ── */
  .wc-img-viewer { position:fixed; inset:0; z-index:20000; display:flex; flex-direction:column; align-items:center; }
  .wc-img-viewer-mask { position:absolute; inset:0; background:rgba(8,8,12,.88); }
  .wc-img-viewer-toolbar { position:relative; z-index:2; display:flex; align-items:center; gap:14px; width:100%; max-width:860px; padding:10px 18px; color:var(--wc-text); font-size:13px; user-select:none; }
  .wc-img-viewer-count { font-variant-numeric:tabular-nums; opacity:.85; }
  .wc-img-viewer-time { font-size:12px; opacity:.6; }
  .wc-img-viewer-actions { margin-left:auto; display:flex; gap:6px; }
  .wc-img-viewer-btn { min-width:30px; height:30px; padding:0 8px; border:1px solid rgba(255,255,255,.25); border-radius:6px; background:rgba(255,255,255,.08); color:var(--wc-text); font-size:14px; cursor:pointer; transition:all .12s ease; }
  .wc-img-viewer-btn:hover { background:rgba(255,255,255,.2); border-color:rgba(255,255,255,.4); }
  .wc-img-viewer-btn-on { background:var(--wc-theme); border-color:var(--wc-theme); color:#fff; }
  .wc-img-viewer-btn-on:hover { background:var(--wc-theme); color:#fff; }
  .wc-img-viewer-stage { position:relative; z-index:1; flex:1; width:100%; display:flex; align-items:center; justify-content:center; overflow:hidden; touch-action:none; }
  .wc-img-viewer-img { max-width:92%; max-height:92%; object-fit:contain; border-radius:4px; box-shadow:0 8px 40px rgba(0,0,0,.55); transition:transform .12s ease; user-select:none; -webkit-user-drag:none; }
  .wc-img-viewer-loading { position:relative; z-index:2; display:flex; align-items:center; gap:8px; padding:10px 16px; border-radius:8px; background:rgba(255,255,255,.08); color:var(--wc-text); font-size:13px; }
  .wc-img-viewer-hint { position:relative; z-index:2; padding:8px 16px; font-size:11.5px; color:var(--wc-muted); user-select:none; }


  /* 文件 */
  .wc-file-tabs { display:flex; flex-wrap:wrap; gap:6px; margin-bottom:12px; }
  .wc-file-img-grid { display:grid; grid-template-columns:repeat(auto-fill,minmax(118px,1fr)); gap:10px; }
  .wc-file-img-card { display:flex; flex-direction:column; align-items:center; gap:4px; padding:8px; background:var(--wc-card); border:1px solid var(--wc-border-light); border-radius:10px; cursor:zoom-in; overflow:hidden; transition:border-color .12s ease, box-shadow .12s ease, transform .08s ease; }
  .wc-file-img-card-off { cursor:default; }
  .wc-file-img-card:hover { border-color:var(--wc-border); box-shadow:0 3px 10px rgba(0,0,0,.14); transform:translateY(-1px); }
  .wc-file-img-thumb { width:96px; height:96px; object-fit:cover; border-radius:6px; background:var(--wc-bg2); display:block; }
  .wc-file-img-ph { width:96px; height:96px; display:flex; align-items:center; justify-content:center; color:var(--wc-muted); background:var(--wc-bg2); border-radius:6px; }
  .wc-file-img-name { width:100%; font-size:11.5px; color:var(--wc-text2); text-align:center; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .wc-file-img-meta { font-size:11.5px; color:var(--wc-muted); }
  .wc-file-video-grid { display:grid; grid-template-columns:repeat(auto-fill,minmax(210px,1fr)); gap:10px; }
  .wc-file-video-card { display:flex; flex-direction:column; gap:4px; padding:8px; background:var(--wc-card); border:1px solid var(--wc-border-light); border-radius:10px; cursor:pointer; overflow:hidden; transition:border-color .12s ease, box-shadow .12s ease, transform .08s ease; }
  .wc-file-video-card:hover { border-color:var(--wc-border); box-shadow:0 3px 10px rgba(0,0,0,.14); transform:translateY(-1px); }
  .wc-file-video-cover { position:relative; width:100%; aspect-ratio:16/9; border-radius:6px; overflow:hidden; background:var(--wc-bg2); display:flex; align-items:center; justify-content:center; }
  .wc-file-video-thumb { width:100%; height:100%; object-fit:cover; display:block; }
  .wc-file-video-ph { display:inline-flex; color:var(--wc-muted); }
  .wc-file-video-play { position:absolute; inset:0; display:flex; align-items:center; justify-content:center; color:#fff; font-size:26px; background:rgba(0,0,0,.28); opacity:0; transition:opacity .15s ease; text-shadow:0 1px 6px rgba(0,0,0,.6); }
  .wc-file-video-card:hover .wc-file-video-play { opacity:1; }
  .wc-file-video-name { width:100%; font-size:11.5px; color:var(--wc-text2); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .wc-file-video-meta { font-size:11.5px; color:var(--wc-muted); }
  .wc-file-list { display:flex; flex-direction:column; gap:6px; }
  .wc-file-item { display:flex; align-items:center; gap:10px; padding:8px 12px; background:var(--wc-card); border:1px solid var(--wc-border-light); border-radius:8px; transition:border-color .12s ease, background .12s ease; }
  .wc-file-item:hover { border-color:var(--wc-border); background:var(--wc-item-hover); }
  .wc-file-icon { flex-shrink:0; width:34px; height:34px; display:flex; align-items:center; justify-content:center; color:var(--wc-text2); background:var(--wc-bg2); border-radius:8px; }
  .wc-file-info { flex:1; min-width:0; display:flex; flex-direction:column; gap:2px; }
  .wc-file-info .wc-file-name { font-size:12px; font-weight:600; color:var(--wc-text); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .wc-file-sub { font-size:11.5px; color:var(--wc-muted); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .wc-file-actions { flex-shrink:0; display:flex; gap:6px; }
  .wc-file-missing { font-size:11.5px; color:var(--wc-muted); white-space:nowrap; }
  .wc-file-more { display:flex; justify-content:center; padding:14px 0 4px; }
  .wc-file-viewer { position:fixed; inset:0; z-index:20000; display:flex; flex-direction:column; align-items:center; }
  .wc-file-viewer-mask { position:absolute; inset:0; background:rgba(8,8,12,.9); }
  .wc-file-viewer-toolbar { position:relative; z-index:2; display:flex; align-items:center; gap:12px; width:100%; max-width:900px; padding:10px 18px; color:var(--wc-text); font-size:13px; }
  .wc-file-viewer-name { font-weight:600; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .wc-file-viewer-meta { font-size:11.5px; opacity:.65; white-space:nowrap; }
  .wc-file-viewer-actions { margin-left:auto; display:flex; gap:6px; }
  .wc-file-viewer-stage { position:relative; z-index:1; flex:1; width:100%; display:flex; align-items:center; justify-content:center; overflow:hidden; }
  .wc-file-viewer-img { max-width:92%; max-height:92%; object-fit:contain; border-radius:4px; box-shadow:0 8px 40px rgba(0,0,0,.55); }
  .wc-file-viewer-hint { position:relative; z-index:2; padding:8px 16px; font-size:11.5px; color:var(--wc-muted); }

  /* 设置 */
  .wc-settings { flex:1; overflow-y:auto; scrollbar-width:thin; padding:16px 18px 24px; display:flex; flex-direction:column; gap:12px; }
  .wc-settings-section { display:flex; flex-direction:column; gap:10px; min-width:0; }
  .wc-settings-divider { height:1px; background:var(--wc-border-light); margin:2px 0; flex-shrink:0; }
  .wc-settings-hd { font-size:16px; font-weight:700; display:flex; align-items:center; justify-content:space-between; gap:10px; flex-wrap:wrap; }
  .wc-settings-count { font-size:12px; font-weight:400; color:var(--wc-muted); }
  .wc-settings-actions { display:inline-flex; align-items:center; gap:8px; }
  .wc-settings-list { display:flex; flex-direction:column; gap:10px; }
  .wc-settings-cat { border:1px solid var(--wc-border-light); border-radius:12px; background:var(--wc-card); overflow:hidden; transition:border-color .15s ease; }
  .wc-settings-cat-open { border-color:var(--wc-border); }
  .wc-settings-cat-hd { display:flex; align-items:center; gap:10px; width:100%; text-align:left; padding:12px 14px; border:none; background:transparent; color:var(--wc-text); cursor:pointer; font-size:13px; font-weight:700; transition:background .12s ease; }
  .wc-settings-cat-hd:hover { background:var(--wc-item-hover); }
  .wc-settings-cat-icon { font-size:17px; flex-shrink:0; }
  .wc-settings-cat-name { min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .wc-settings-cat-table { font-size:11.5px; color:var(--wc-muted); font-family:'Cascadia Code',Consolas,monospace; background:var(--wc-bg2); border:1px solid var(--wc-border-light); border-radius:5px; padding:1px 7px; white-space:nowrap; }
  .wc-settings-cat-count { margin-left:auto; font-size:11.5px; color:var(--wc-text2); font-weight:600; font-variant-numeric:tabular-nums; flex-shrink:0; }
  .wc-settings-cat-empty { margin-left:auto; font-size:11.5px; color:var(--wc-muted); flex-shrink:0; }
  .wc-settings-cat-arrow { flex-shrink:0; color:var(--wc-muted); font-size:12px; transition:transform .15s ease; }
  .wc-settings-cat-open .wc-settings-cat-arrow { transform:rotate(180deg); }
  .wc-settings-cat-body { padding:0 14px 14px; display:flex; flex-direction:column; gap:10px; }
  .wc-settings-cat-nodata { padding:18px 10px; text-align:center; font-size:12px; color:var(--wc-muted); border:1px dashed var(--wc-border-light); border-radius:8px; }
  .wc-settings-cat-foot { display:flex; align-items:center; justify-content:space-between; gap:10px; font-size:11.5px; color:var(--wc-muted); }
  .wc-table-wrap { overflow-x:auto; border:1px solid var(--wc-border-light); border-radius:6px; }
  .wc-table { width:100%; border-collapse:collapse; font-size:12px; }
  .wc-table th { text-align:left; padding:7px 10px; background:var(--wc-item-active); color:var(--wc-text2); font-weight:600; white-space:nowrap; font-size:11.5px; position:sticky; top:0; }
  .wc-table td { padding:7px 10px; border-top:1px solid var(--wc-border-light); color:var(--wc-text); max-width:240px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-size:12px; }

  /* 通用数据展示 */
  .wc-db-view { flex:1; overflow-y:auto; padding:16px; scrollbar-width:thin; }
  .wc-db-hd { display:flex; align-items:center; gap:8px; margin-bottom:12px; font-size:16px; font-weight:700; }
  .wc-db-count { font-size:12px; color:var(--wc-muted); font-weight:400; }

  /* 消息搜索结果 */
  .wc-search-results { flex:1;overflow-y:auto;scrollbar-width:thin;padding:4px 0; }
  .wc-search-index-hint { display:flex;align-items:center;justify-content:space-between;gap:10px;padding:8px 10px;border:1px dashed var(--wc-border);border-radius:6px;background:var(--wc-bg2);font-size:11.5px;color:var(--wc-muted);margin-bottom:6px; }
  .wc-search-hit { display:flex;flex-direction:column;gap:4px;width:100%;text-align:left;padding:8px 12px;border:none;border-bottom:1px solid var(--wc-border-light);background:transparent;color:var(--wc-text);cursor:pointer;transition:background .12s ease; }
  .wc-search-hit-count { font-size:11.5px; color:var(--wc-muted); padding:6px 12px 2px; }
  .wc-search-hit:hover { background:var(--wc-item-hover); }
  .wc-search-hit-top { display:flex;align-items:center;justify-content:space-between;gap:8px; }
  .wc-search-hit-name { font-size:12px;font-weight:600;overflow:hidden;text-overflow:ellipsis;white-space:nowrap; }
  .wc-search-hit-time { font-size:11.5px;color:var(--wc-muted);flex-shrink:0; }
  .wc-search-hit-snippet { font-size:11.5px;color:var(--wc-text2);line-height:1.5;display:-webkit-box;-webkit-line-clamp:2;line-clamp:2;-webkit-box-orient:vertical;overflow:hidden;word-break:break-all; }


  /* 消息右键菜单 */
  .wc-edit-mask { position:fixed;inset:0;z-index:9000; }
  .wc-edit-menu { position:fixed;z-index:9001;min-width:170px;background:var(--wc-card);border:1px solid var(--wc-border);border-radius:8px;box-shadow:0 8px 28px rgba(0,0,0,0.22);padding:5px;display:flex;flex-direction:column;gap:2px; }
  .wc-edit-menu-loading { font-size:11.5px;color:var(--wc-muted);padding:6px 10px;display:flex;align-items:center;gap:6px; }

  /* 编辑弹窗 */
  .wc-edit-modal-body { padding:14px 16px;display:flex;flex-direction:column;gap:10px; }
  .wc-edit-modal-tip { font-size:11.5px;color:var(--wc-muted);line-height:1.6;margin:0; }
  .wc-edit-modal-input { width:100%;box-sizing:border-box;min-height:96px;padding:9px 11px;border:1px solid var(--wc-border);border-radius:8px;background:var(--wc-bg2);color:var(--wc-text);font-size:13px;line-height:1.6;resize:vertical;outline:none; }
  .wc-edit-modal-input:focus { border-color:var(--wc-theme,#576b95); }
  .wc-edit-modal-error { font-size:11.5px;color:#f87171;background:#ef44441a;border:1px solid #ef444433;border-radius:6px;padding:6px 10px;word-break:break-all; }
  .wc-edit-modal-actions { display:flex;justify-content:flex-end;gap:8px; }

  /* 原始字段编辑 / 归档导出 */
  .wc-raw-unsafe { display:flex;align-items:center;gap:7px;font-size:11.5px;color:var(--wc-text2);cursor:pointer;line-height:1.5; }
  .wc-raw-unsafe input { accent-color:var(--wc-theme,#576b95);width:14px;height:14px;cursor:pointer;flex-shrink:0; }
  .wc-export-tip { font-size:11.5px;color:var(--wc-muted);line-height:1.7;margin:0; }
  .wc-archive-progress { display:flex;flex-direction:column;gap:5px; }
  .wc-archive-progress-track { height:8px;border-radius:4px;background:var(--wc-bg2);overflow:hidden; }
  .wc-archive-progress-fill {
    height:100%;width:100%;border-radius:4px;background:var(--wc-theme,#576b95);
    transform-origin:left;transform:scaleX(calc(var(--p,0) / 100));transition:transform .25s ease;
  }
  .wc-archive-progress-label { font-size:11.5px;color:var(--wc-muted); }
</style>
