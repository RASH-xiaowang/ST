<script lang="ts">
  // ============================================================
  // 消息行渲染（蓝图 T-蓝图-7 第一步）：自 WeChatPanel.svelte 下沉
  // - 纯渲染：不做任何状态持有，读 ctx / 写 actions（回调注入）
  // - 样式：wc-msg-* / wc-rich-* / 各富媒体卡片 scoped CSS 随模板迁移
  // ============================================================
  import type { WeChatMessage, WeChatSession } from '../types';
  import { renderEmojiText } from '../utils';
  import { extTone } from '../utils/misc';
  import {
    avatarLetter,
    chatlogPreview,
    colorFromName,
    fmtDur,
    fmtFileSize,
    iconSvg,
    ICON_PATHS,
    payStateClass,
    redPacketLabel,
    resolveStaticEmojiPath,
  } from '../utils/format';
  import { editKey } from '../utils/panel';
  import { messageImageUrl } from '../services/mediaApi.svelte';
  import WechatHoverButton from './WechatHoverButton.svelte';

  /** 消息行读取的父级状态（只读；可变 map 仍由父级持有，变更经 actions 回调） */
  export interface MessageRowCtx {
    curSession: string;
    isOfficialChat: boolean;
    curSessionInfo: WeChatSession | null;
    avatarCache: Record<string, string>;
    staticEmoticonMap: Map<string, string>;
    imageCache: Record<string, string>;
    /** 图片失败原因（key → 诊断信息），失效占位符展示 */
    imageFailedReasons: Record<string, string>;
    apiMediaBlocked: Set<string>;
    apiMediaBase: string;
    apiToken: string;
    fileOpening: Record<string, boolean>;
    voiceLoadingKey: string;
    voiceMap: Record<string, string>;
    voiceText: Record<string, string>;
    voiceTextFailed: Record<string, boolean>;
    voiceTranscribing: Record<string, boolean>;
    videoPlaying: Record<string, boolean>;
    videoMissing: Record<string, boolean>;
    videoCoverFail: Record<string, boolean>;
    editedSet: Set<string>;
  }

  /** 消息行交互回调（统一经 props 注入，不在子组件持有业务逻辑） */
  export interface MessageRowActions {
    onContextMenu(e: MouseEvent, m: WeChatMessage): void;
    openImage(m: WeChatMessage): void;
    onImageError(m: WeChatMessage): void;
    retryImage(m: WeChatMessage): void;
    openUrl(url?: string | null): void;
    openFile(m: WeChatMessage, r: WeChatMessage['rich']): void;
    openFileDir(m: WeChatMessage): void;
    openMiniApp(r: WeChatMessage['rich']): void;
    playVoice(username: string | null | undefined, localId: number, key: string): void;
    transcribeVoice(username: string | null | undefined, localId: number, key: string): void;
    onVoiceEnded(key: string): void;
    playVideo(key: string): void;
    onVideoEnded(key: string): void;
    onVideoError(key: string): void;
    onCoverFail(key: string): void;
  }

  let {
    m,
    divider,
    gi,
    ctx,
    actions,
  }: {
    m: WeChatMessage;
    divider: string | null;
    gi: number;
    ctx: MessageRowCtx;
    actions: MessageRowActions;
  } = $props();
</script>

{#if divider}
  <div class="wc-time-divider">{divider}</div>
{/if}
{#if m.is_notice}
  <div class="wc-notice" data-idx={gi}>{m.text}</div>
{:else}
  <div class="wc-msg" data-idx={gi} role="group" class:wc-msg-self={m.is_self} class:wc-msg-official={ctx.isOfficialChat && !m.is_self} oncontextmenu={(e) => actions.onContextMenu(e, m)}>
    {#if !m.is_self && !ctx.isOfficialChat}
      <div class="wc-msg-avatar">
        {#if ctx.avatarCache[m.sender_username]}<img src={ctx.avatarCache[m.sender_username]} alt="" />
        {:else}<div class="wc-msg-letter" style="background:{colorFromName(m.sender_name||m.sender_username||ctx.curSessionInfo?.name||'?')}">{avatarLetter(m.sender_name||ctx.curSessionInfo?.name||'?')}</div>{/if}
      </div>
    {/if}
    <div class="wc-msg-body">
      {#if !m.is_self && ctx.curSessionInfo?.is_group && m.sender_name}
        <div class="wc-msg-sender">{m.sender_name}</div>
      {/if}
      {#if !m.rich && m.type !== 3 && (m.type === 1 || m.text)}
        <!-- 纯文本分支：图片消息(type=3)带 "[图片]" 兜底文本，
             必须排除在外，否则会抢占分支永远显示占位文字 -->
        <div class="wc-msg-content">{@html renderEmojiText(m.text, ctx.staticEmoticonMap)}</div>
      {:else if m.type === 3}
        <!-- 图片消息：优先用实时推送内嵌的 data URL，否则懒加载解密缓存 -->
        {@const imgKey = ctx.curSession + ':' + m.local_id}
        {@const imgSrc = m.image_url || ctx.imageCache[imgKey] || (!ctx.apiMediaBlocked.has(imgKey)
          ? messageImageUrl(ctx.curSession, m.local_id)
          : '')}
        {#if imgSrc}
          <div class="wc-msg-content wc-msg-image">
            <button type="button" title="点击查看大图"
              style="padding:0;border:none;background:none;cursor:pointer;display:inline-flex"
              onclick={() => actions.openImage(m)}
              onkeydown={(e) => e.key === 'Enter' && actions.openImage(m)}>
              <!-- 图片气泡：轻量 img + CSS 淡入（替代每张一个 WebGL 的 NoiseReveal，
                   多图聊天性能显著提升）；加载失败沿用「阻断 URL → IPC base64 重试」链路 -->
              <img
                src={imgSrc}
                alt="图片"
                class="wc-msg-noise-img"
                loading="lazy"
                decoding="async"
                onerror={() => actions.onImageError(m)}
              />
            </button>
          </div>
        {:else if ctx.imageCache[imgKey] === '' || ctx.apiMediaBlocked.has(imgKey)}
          {@const failReason = ctx.imageFailedReasons[imgKey] || ''}
          {@const failSub = failReason.includes('解密失败')
            ? '图片解密失败 · 点击重试'
            : failReason.includes('未下载')
              ? '微信端未下载该图 · 点击重试'
              : failReason.includes('原图 Hook')
                ? '可开启原图 Hook 后重试'
                : failReason.includes('HTTP') || failReason.includes('服务')
                  ? '图片服务异常 · 点击重试'
                  : '点击重试'}
          <div class="wc-msg-content wc-msg-image-fail wc-msg-image-retry" role="button" tabindex="0"
            onclick={() => actions.retryImage(m)}
            onkeydown={(e) => e.key === 'Enter' && actions.retryImage(m)}
            title={failReason || '图片本地与 CDN 均未找到，点击重试'}>
            <svg class="wc-msg-image-fail-ico" viewBox="0 0 24 24" width="30" height="30" fill="none" stroke="currentColor" stroke-width="1.4" aria-hidden="true">
              <rect x="3" y="4" width="18" height="16" rx="2.5" />
              <circle cx="8.5" cy="9" r="1.6" />
              <path d="M3 17.5 8 12.5l3.2 3.2 3.8-4.2 6 6" stroke-linecap="round" stroke-linejoin="round" />
              <path d="M12 20v-2.4m0 2.4-1.5-1.9m1.5 1.9 1.6-1.7" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
            <span class="wc-msg-image-fail-title">图片已失效</span>
            <span class="wc-msg-image-fail-sub">{failSub}</span>
          </div>
        {:else}
          <div class="wc-msg-content wc-msg-image-loading"><span class="wc-loading-inline"></span> 图片解密中…</div>
        {/if}
      {:else if m.rich}
        {@const r = m.rich}
        {#if r.type === 'newsfeed'}
          <!-- mmreader 图文推送卡片（腾讯新闻等）：头条大图 + 子条目列表 -->
          {@const newsRows = r.top_cover ? (r.items ?? []).slice(1) : (r.items ?? [])}
          <div class="wc-msg-content wc-msg-newsfeed">
            {#if r.top_cover && (r.items ?? []).length > 0}
              <div class="wc-news-hero" onclick={() => actions.openUrl((r.items ?? [])[0]?.url)} onkeydown={(e) => e.key === 'Enter' && actions.openUrl((r.items ?? [])[0]?.url)} role="button" tabindex="0">
                <img src={r.top_cover} alt="" class="wc-news-hero-img" loading="lazy" referrerpolicy="no-referrer" />
                <div class="wc-news-hero-title">{(r.items ?? [])[0]?.title ?? ''}</div>
              </div>
            {/if}
            {#each newsRows as it}
              <div class="wc-news-row" onclick={() => actions.openUrl(it.url)} onkeydown={(e) => e.key === 'Enter' && actions.openUrl(it.url)} role="button" tabindex="0">
                <div class="wc-news-row-body">
                  <div class="wc-news-row-title">{it.title}</div>
                  {#if it.digest}<div class="wc-news-row-digest">{it.digest}</div>{/if}
                </div>
                {#if it.cover}
                  <img src={it.cover} alt="" class="wc-news-thumb" loading="lazy" referrerpolicy="no-referrer" />
                {/if}
              </div>
            {/each}
            {#if r.name}<div class="wc-news-source">{r.name}</div>{/if}
          </div>
        {:else if r.type === 'file'}
          {@const fKey = `${m.username || ctx.curSession}:${m.local_id}`}
          {@const fExt = (r.file_ext || '').toUpperCase()}
          <div class="wc-msg-content wc-card-bubble">
            <div class="wc-file-card" class:wc-file-opening={ctx.fileOpening[fKey]}
              role="button" tabindex="0"
              title="点击打开文件；打不开则打开所在目录"
              onclick={() => actions.openFile(m, r)}
              onkeydown={(e) => { if (e.key === 'Enter') actions.openFile(m, r); }}>
              <div class="wc-file-body">
                <div class="wc-file-icon-tile wc-file-tone-{extTone(r.file_ext ?? '')}">
                  <span class="wc-file-ext-label">{fExt.slice(0, 4) || 'FILE'}</span>
                </div>
                <div class="wc-file-meta">
                  <div class="wc-file-title">{r.title || '文件'}</div>
                  <div class="wc-file-sub">{fExt || '文件'} · {fmtFileSize(r.file_size)}</div>
                </div>
                <span class="wc-file-open-ico" title="打开所在目录"
                  onclick={(e) => { e.stopPropagation(); actions.openFileDir(m); }}
                  onkeydown={(e) => { if (e.key === 'Enter') { e.stopPropagation(); actions.openFileDir(m); } }}
                  role="button" tabindex="0">
                  <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true">
                    <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>
                  </svg>
                </span>
              </div>
              <div class="wc-file-bottom">
                <span>微信文件</span>
                <span class="wc-file-hint">{ctx.fileOpening[fKey] ? '查找中…' : '点击打开'}</span>
              </div>
            </div>
          </div>
        {:else if r.type === 'miniapp'}
          <div class="wc-msg-content wc-rich wc-miniapp-card" role="button" tabindex="0"
            title="点击打开小程序"
            onclick={() => actions.openMiniApp(r)}
            onkeydown={(e) => { if (e.key === 'Enter') actions.openMiniApp(r); }}>
            <div class="wc-miniapp-row">
              {#if r.icon}
              <img src={r.icon} alt="" class="wc-miniapp-icon" loading="lazy" referrerpolicy="no-referrer" onerror={(e) => ((e.currentTarget as HTMLImageElement).style.display = 'none')} />
              {:else}
                <div class="wc-miniapp-icon wc-miniapp-icon-ph">{@html iconSvg(ICON_PATHS.app, 18)}</div>
              {/if}
              <div class="wc-rich-title wc-miniapp-title">{r.title}</div>
            </div>
            {#if r.des}<div class="wc-rich-desc">{r.des}</div>{/if}
            <div class="wc-card-foot wc-miniapp-foot">
              <span>小程序{r.source ? ' · '+r.source : ''}</span>
              <span class="wc-miniapp-open">点击打开 ↗</span>
            </div>
          </div>
        {:else if r.type === 'link'}
          {@const isArticle = !!r.thumb && /mp\.weixin\.qq\.com/i.test(r.url || '')}
          <div class="wc-msg-content wc-rich wc-link-card" class:wc-article-card={isArticle}
            role="button" tabindex="0" onclick={() => actions.openUrl(r.url)}
            onkeydown={(e) => e.key === 'Enter' && actions.openUrl(r.url)}>
            {#if isArticle}
              {#if r.thumb}
                <div class="wc-article-cover">
                  <img src={r.thumb} alt="文章封面" class="wc-article-cover-img" loading="lazy" referrerpolicy="no-referrer"
                    onerror={(e) => ((e.currentTarget as HTMLImageElement).style.display = 'none')} />
                </div>
              {/if}
              <div class="wc-article-title">{r.title || '链接'}</div>
              {#if r.des}<div class="wc-article-des">{r.des}</div>{/if}
              <div class="wc-card-foot wc-article-foot">
                <span>{r.source || ctx.curSessionInfo?.name || '微信公众号'}</span>
                <span class="wc-article-open">阅读全文 ↗</span>
              </div>
              {#if r.articles?.length}
                <div class="wc-article-subs">
                  {#each r.articles as art (art.url || art.title)}
                    <div class="wc-article-sub" role="button" tabindex="0"
                      onclick={(e) => { e.stopPropagation(); actions.openUrl(art.url); }}
                      onkeydown={(e) => { if (e.key === 'Enter') { e.stopPropagation(); actions.openUrl(art.url); } }}>
                      <div class="wc-article-sub-title">{art.title}</div>
                      {#if art.cover}
                        <img src={art.cover} alt="" class="wc-article-sub-thumb" loading="lazy" referrerpolicy="no-referrer"
                          onerror={(e) => ((e.currentTarget as HTMLImageElement).style.display = 'none')} />
                      {/if}
                    </div>
                  {/each}
                </div>
              {/if}
            {:else}
              <div class="wc-rich-title wc-link-title">{r.title || '链接'}</div>
              {#if r.des}<div class="wc-rich-desc">{r.des}</div>{/if}
              <div class="wc-card-foot">{r.source || '链接'}</div>
            {/if}
          </div>
        {:else if r.type === 'quote'}
          <div class="wc-msg-content wc-quote-bubble">{r.title}
            <div class="wc-quote"><span class="wc-quote-name">{r.ref_name}: </span>{r.ref_content}</div>
          </div>
        {:else if r.type === 'transfer'}
          {@const tState = payStateClass(r.direction ?? '')}
          <div class="wc-msg-content wc-card-bubble">
            <div class="wc-transfer-card {tState}">
              <div class="wc-transfer-content">
                <div class="wc-transfer-icon">¥</div>
                <div class="wc-transfer-info">
                  <span class="wc-transfer-amount">{r.amount ? '¥' + r.amount : (r.fee_desc || r.title)}</span>
                  <span class="wc-transfer-status">{r.direction || '微信转账'}{r.pay_memo ? ' · '+r.pay_memo : ''}</span>
                </div>
              </div>
              <div class="wc-transfer-bottom"><span>微信转账</span></div>
            </div>
          </div>
        {:else if r.type === 'redpacket'}
          {@const rState = payStateClass(redPacketLabel(r.paysubtype ?? ''))}
          {@const rLabel = redPacketLabel(r.paysubtype ?? '')}
          <div class="wc-msg-content wc-card-bubble">
            <div class="wc-redpacket-card {rState}">
              <div class="wc-redpacket-content">
                <div class="wc-redpacket-icon">
                  <svg viewBox="0 0 32 36" width="30" height="34" aria-hidden="true">
                    <rect x="1.5" y="3.5" width="29" height="29" rx="3" fill="#ee4d3d"/>
                    <path d="M1.5 6.5 L16 18.5 L30.5 6.5" fill="#c93b2f"/>
                    <rect x="1.5" y="3.5" width="29" height="29" rx="3" fill="none" stroke="#d84334" stroke-width="1.2"/>
                    <circle cx="16" cy="18" r="6.6" fill="#f8c65a" stroke="#e2a43c" stroke-width="1"/>
                    <text x="16" y="21.5" text-anchor="middle" font-size="8.5" font-weight="bold" fill="#a96f1b">¥</text>
                  </svg>
                </div>
                <div class="wc-redpacket-info">
                  <span class="wc-redpacket-text">{r.title || '微信红包'}</span>
                  {#if r.amount}<span class="wc-redpacket-status">¥{r.amount}{rLabel ? ' · '+rLabel : ''}</span>
                  {:else if rLabel}<span class="wc-redpacket-status">{rLabel}</span>{/if}
                </div>
              </div>
              <div class="wc-redpacket-bottom"><span>微信红包</span></div>
            </div>
          </div>
        {:else if r.type === 'location'}
          <div class="wc-msg-content wc-rich wc-location-card" role="button" tabindex="0"
            onclick={() => r.url && actions.openUrl(r.url)}
            onkeydown={(e) => e.key === 'Enter' && r.url && actions.openUrl(r.url)}>
            <div class="wc-location-row">
              <div class="wc-location-icon">{@html iconSvg(ICON_PATHS.pin, 26)}</div>
              <div class="wc-location-main">
                <div class="wc-location-name">{r.poiname || r.label || '位置'}</div>
                {#if r.label && r.label !== r.poiname}<div class="wc-location-label">{r.label}</div>{/if}
              </div>
            </div>
            <div class="wc-card-foot">位置</div>
          </div>
        {:else if r.type === 'contact'}
          <div class="wc-msg-content wc-rich wc-contact-card">
            <div class="wc-contact-row">
              <div class="wc-contact-avatar">{r.nickname ? r.nickname[0] : '👤'}</div>
              <div class="wc-contact-main">
                <div class="wc-contact-name">{r.nickname || '联系人'}</div>
                {#if r.username}<div class="wc-contact-username">{r.username}</div>{/if}
              </div>
            </div>
            <div class="wc-card-foot">名片</div>
          </div>
        {:else if r.type === 'voice'}
          {@const vKey = `${m.username || ctx.curSession}:${m.local_id}`}
          <div class="wc-msg-content wc-msg-voice">
            <div class="wc-voice-row">
              <WechatHoverButton
                onclick={() => actions.playVoice(m.username || ctx.curSession, m.local_id, vKey)}
                title="播放语音"
                class="!px-3 !py-1 !text-xs"
              >
                {#if ctx.voiceLoadingKey === vKey}
                  <span class="wc-loading-inline-sm"></span>
                {:else if ctx.voiceMap[vKey]}
                  <svg viewBox="0 0 24 24" width="11" height="11" fill="currentColor" aria-hidden="true"><rect x="6" y="5" width="4" height="14" rx="1"/><rect x="14" y="5" width="4" height="14" rx="1"/></svg>
                {:else}
                  <svg viewBox="0 0 24 24" width="11" height="11" fill="currentColor" aria-hidden="true"><path d="M8 5v14l11-7z"/></svg>
                {/if}
                <span>{r.duration ? r.duration + '″' : ''}</span>
              </WechatHoverButton>
              {#if !ctx.voiceText[vKey] || ctx.voiceTextFailed[vKey]}
                <WechatHoverButton
                  onclick={() => actions.transcribeVoice(m.username || ctx.curSession, m.local_id, vKey)}
                  disabled={ctx.voiceTranscribing[vKey]}
                  title="语音转文字"
                  class="!px-3 !py-1 !text-xs"
                >
                  {#if ctx.voiceTranscribing[vKey]}
                    <span class="wc-loading-inline-sm"></span>
                    <span>转写中…</span>
                  {:else}
                    <svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="M12 14a3 3 0 0 0 3-3V6a3 3 0 1 0-6 0v5a3 3 0 0 0 3 3z"/><path d="M19 11a7 7 0 0 1-14 0"/><line x1="12" y1="18" x2="12" y2="21"/></svg>
                    <span>转文字</span>
                  {/if}
                </WechatHoverButton>
              {/if}
            </div>
            {#if ctx.voiceMap[vKey]}
              <audio src={ctx.voiceMap[vKey]} autoplay onended={() => actions.onVoiceEnded(vKey)}></audio>
            {/if}
            {#if ctx.voiceText[vKey]}
              <div class="wc-voice-text">{ctx.voiceText[vKey]}</div>
            {/if}
          </div>
        {:else if r.type === 'video'}
          {@const vkey = `${m.username || ctx.curSession}:${m.local_id}`}
          {@const vurl = ctx.apiMediaBase ? `${ctx.apiMediaBase}/video/${encodeURIComponent(m.username || ctx.curSession)}/${m.local_id}` + (ctx.apiToken ? `?access_token=${encodeURIComponent(ctx.apiToken)}` : '') : ''}
          {@const turl = ctx.apiMediaBase ? `${ctx.apiMediaBase}/video/thumb/${encodeURIComponent(m.username || ctx.curSession)}/${m.local_id}` + (ctx.apiToken ? `?access_token=${encodeURIComponent(ctx.apiToken)}` : '') : ''}
          <div class="wc-msg-content wc-msg-video">
            {#if !ctx.apiMediaBase}
              <span class="wc-video-fallback">{@html iconSvg(ICON_PATHS.video, 16)} [视频]{r.duration ? ' '+fmtDur(r.duration) : ''}</span>
            {:else if ctx.videoPlaying[vkey]}
              <video src={vurl} controls autoplay playsinline class="wc-msg-video-el"
                onended={() => actions.onVideoEnded(vkey)}
                onerror={() => actions.onVideoError(vkey)}><track kind="captions" /></video>
            {:else if ctx.videoMissing[vkey]}
              <span class="wc-video-missing">📭 本地视频未下载，需在微信中打开该视频后自动缓存</span>
            {:else if !ctx.videoCoverFail[vkey]}
              <div class="wc-video-cover" role="button" tabindex="0"
                onclick={() => actions.playVideo(vkey)}
                onkeydown={(e) => { if (e.key === 'Enter') actions.playVideo(vkey); }}
                title="播放视频">
                <img class="wc-video-cover-img" src={turl} alt="视频封面" loading="lazy"
                  onerror={() => actions.onCoverFail(vkey)} />
                <span class="wc-video-play-btn">▶</span>
                {#if r.duration}<span class="wc-video-dur">{fmtDur(r.duration)}</span>{/if}
              </div>
            {:else}
              <WechatHoverButton
                onclick={() => actions.playVideo(vkey)}
                class="!px-3 !py-1 !text-xs"
              >
                <svg viewBox="0 0 24 24" width="11" height="11" fill="currentColor" aria-hidden="true"><path d="M8 5v14l11-7z"/></svg>
                <span>{r.duration ? fmtDur(r.duration) : '播放视频'}</span>
              </WechatHoverButton>
            {/if}
          </div>
        {:else if r.type === 'emoji'}
          {@const emojiDesc = r.description || String(m.content ?? '')}
          {@const emojiPath = resolveStaticEmojiPath(emojiDesc, ctx.staticEmoticonMap)}
          {#if emojiPath}
            <div class="wc-msg-content wc-msg-emoji"><img src={emojiPath} alt={emojiDesc || '表情'} class="wc-msg-emoji-img" /></div>
          {:else}
            <div class="wc-msg-content">{emojiDesc ? '🙂 ' + emojiDesc : '😊 [表情]'}</div>
          {/if}
        {:else if r.type === 'channels'}
          <div class="wc-msg-content wc-rich wc-channels-card" role="button" tabindex="0"
            onclick={() => r.url && actions.openUrl(r.url)}
            onkeydown={(e) => e.key === 'Enter' && r.url && actions.openUrl(r.url)}>
            <div class="wc-channels-row">
              {#if r.cover}
                <img src={r.cover} alt="" class="wc-channels-cover" loading="lazy" referrerpolicy="no-referrer"
                  onerror={(e) => ((e.currentTarget as HTMLImageElement).style.display = 'none')} />
              {/if}
              <div class="wc-channels-main">
                <div class="wc-rich-title">{r.title}</div>
                {#if r.desc}<div class="wc-rich-desc wc-channels-desc">{r.desc}</div>{/if}
                {#if r.nickname}<div class="wc-channels-author">@{r.nickname}</div>{/if}
              </div>
            </div>
            <div class="wc-card-foot">视频号</div>
          </div>
        {:else if r.type === 'chatlog'}
          {@const logLines = chatlogPreview(r.items)}
          <div class="wc-msg-content wc-rich wc-chatlog-card">
            <div class="wc-rich-title wc-chatlog-title">🗂️ {r.title || '聊天记录'}</div>
            {#if logLines.length}
              <div class="wc-chatlog-preview">
                {#each logLines as line}
                  <div class="wc-chatlog-line">{line}</div>
                {/each}
              </div>
            {:else if r.des}
              <div class="wc-rich-desc">{r.des}</div>
            {/if}
            <div class="wc-card-foot">聊天记录</div>
          </div>
        {:else}
          <div class="wc-msg-content">{m.text || '['+m.type_label+']'}</div>
        {/if}
      {:else}
        <div class="wc-msg-content">{m.text || '['+m.type_label+']'}</div>
      {/if}
      {#if ctx.editedSet.has(editKey(ctx.curSession, m.local_id))}
        <span class="wc-edited-badge" title="该消息已被本地修改（右键可恢复）">已编辑</span>
      {/if}
    </div>
    {#if m.is_self}
      <div class="wc-msg-avatar">
        {#if ctx.avatarCache[m.sender_username]}<img src={ctx.avatarCache[m.sender_username]} alt="" />
        {:else}<div class="wc-msg-letter" style="background:{colorFromName(m.sender_username||'我')}">我</div>{/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  /* ── 消息行 scoped CSS：自 WeChatPanel.svelte 逐条迁移，保持样式等价 ── */
  .wc-time-divider { text-align:center; font-size:11.5px; color:var(--wc-muted); margin:14px 0; user-select:none; }
  .wc-notice { text-align:center; font-size:11.5px; color:var(--wc-muted); margin:10px auto; max-width:80%; line-height:1.5; }
  .wc-msg { display:flex; align-items:flex-start; gap:8px; margin-bottom:14px; padding:0 4px; content-visibility:auto; contain-intrinsic-size:auto 80px; }
  .wc-msg-body { max-width:66%; min-width:0; }
  .wc-msg-self { justify-content:flex-end; }
  .wc-msg-self .wc-msg-body { display:flex; flex-direction:column; align-items:flex-end; }
  /* 公众号/服务号会话：无头像，消息水平居中，去掉气泡小尾巴（与微信一致） */
  .wc-msg-official { justify-content:center; }
  .wc-msg-official .wc-msg-body { align-items:center; max-width:100%; }
  .wc-msg-official .wc-msg-content::after { display:none !important; }
  .wc-msg-sender { font-size:11.5px;color:var(--wc-muted);margin-bottom:2px;padding-left:2px; }
  .wc-msg-content { display:inline-block; padding:8px 13px; border-radius:4px; font-size:14px; line-height:1.55; word-break:break-word; white-space:pre-wrap; background:var(--wc-card); color:var(--wc-text); box-shadow:0 1px 2px rgba(0,0,0,0.08); position:relative; }
  .wc-msg:not(.wc-msg-self) .wc-msg-content::after { content:''; position:absolute; top:50%; left:-5px; transform:translateY(-50%) rotate(45deg); width:8px; height:8px; background:var(--wc-card); box-shadow:-1px 1px 1px rgba(0,0,0,0.04); }
  .wc-msg-self .wc-msg-content::after { content:''; position:absolute; top:50%; right:-5px; transform:translateY(-50%) rotate(45deg); width:8px; height:8px; background:#95ec69; box-shadow:1px -1px 1px rgba(0,0,0,0.04); }
  .wc-msg-voice { display:flex; flex-direction:column; gap:6px; }
  .wc-voice-row { display:flex; align-items:center; gap:8px; flex-wrap:wrap; }
  .wc-voice-transcribe { border-style:dashed; }
  .wc-voice-transcribe:disabled { opacity:.55; cursor:wait; }
  .wc-voice-text { font-size:12.5px; line-height:1.6; color:var(--wc-text2); background:color-mix(in srgb, var(--wc-text) 5%, transparent); border-radius:6px; padding:6px 9px; max-width:min(340px, 100%); white-space:pre-wrap; word-break:break-word; }
  .wc-msg-self .wc-voice-text { background:rgba(0,0,0,0.06); color:#333; }
  .wc-msg-video { padding:0; overflow:hidden; line-height:0; }
  .wc-msg-video-el { display:block; max-width:min(320px,60vw); max-height:240px; border-radius:6px; }
  .wc-video-cover { position:relative; cursor:pointer; overflow:hidden; border-radius:6px; }
  .wc-video-cover-img { display:block; width:min(320px,60vw); max-height:240px; object-fit:cover; border-radius:6px; background:var(--wc-bg2); }
  .wc-video-play-btn { position:absolute; inset:0; margin:auto; width:46px; height:46px; border-radius:50%; background:rgba(0,0,0,0.55); color:#fff; display:flex; align-items:center; justify-content:center; font-size:15px; padding-left:5px; border:1px solid rgba(255,255,255,0.28); box-shadow:0 2px 12px rgba(0,0,0,0.35); transition:transform .12s ease, background .12s ease; }
  .wc-video-cover:hover .wc-video-play-btn { background:rgba(0,0,0,0.75); transform:scale(1.1); }
  .wc-video-cover:focus-visible { outline:2px solid var(--wc-theme); outline-offset:2px; }
  .wc-video-dur { position:absolute; right:6px; bottom:6px; background:rgba(0,0,0,0.65); color:#fff; font-size:11.5px; font-weight:600; padding:2px 7px; border-radius:4px; line-height:1.3; }
  .wc-video-fallback { line-height:1.5; display:inline-flex; align-items:center; gap:6px; }
  .wc-video-missing { display:inline-block; line-height:1.5; font-size:12px; color:var(--wc-muted); padding:10px 12px; }
  :global(.wc-emoji-inline) { width:16px; height:16px; margin:0 1px; vertical-align:middle; object-fit:contain; display:inline; }
  .wc-msg-self .wc-msg-content { background:#95ec69; color:#111; }
  .wc-msg-avatar { width:36px;height:36px;border-radius:6px;flex-shrink:0;display:flex;align-items:center;justify-content:center;font-size:13px;font-weight:700;background:color-mix(in srgb,var(--wc-text) 8%,transparent);color:var(--wc-text2);overflow:hidden;margin-top:2px; }
  .wc-msg-avatar img { width:100%;height:100%;object-fit:cover;border-radius:6px; }
  .wc-msg-letter { width:100%;height:100%;display:flex;align-items:center;justify-content:center;border-radius:6px;color:#fff;font-size:14px;font-weight:700; }

  /* 富媒体消息 */
  .wc-rich { min-width:200px; }
  .wc-rich-title { font-size:13px; font-weight:600; }
  .wc-rich-desc { font-size:12px; color:var(--wc-text2); margin-top:3px; display:-webkit-box; -webkit-line-clamp:3; line-clamp:3; -webkit-box-orient:vertical; overflow:hidden; }
  .wc-msg-self .wc-rich-desc { color:#333; }
  .wc-rich-sub { font-size:11.5px; color:var(--wc-muted); margin-top:5px; padding-top:4px; border-top:1px solid var(--wc-border-light); }
  .wc-msg-self .wc-rich-sub { color:#555; border-top-color:rgba(0,0,0,0.1); }
  .wc-card-foot { font-size:11.5px; color:var(--wc-muted); margin-top:6px; padding-top:4px; border-top:1px solid var(--wc-border-light); }
  .wc-msg-self .wc-card-foot { color:#555; border-top-color:rgba(0,0,0,0.1); }
  .wc-miniapp-row { display:flex; align-items:center; gap:8px; min-width:0; }
  .wc-miniapp-icon { width:30px; height:30px; border-radius:6px; object-fit:cover; flex-shrink:0; background:var(--wc-bg2); }
  .wc-miniapp-icon-ph { display:flex; align-items:center; justify-content:center; color:var(--wc-text2); }
  .wc-miniapp-title { flex:1; min-width:0; }
  .wc-miniapp-card { cursor:pointer; transition:border-color .12s ease, box-shadow .12s ease; }
  .wc-miniapp-card:hover { box-shadow:0 2px 10px rgba(0,0,0,.12); }
  .wc-miniapp-card:focus-visible { outline:2px solid var(--wc-theme); outline-offset:2px; }
  .wc-miniapp-foot { display:flex; align-items:center; justify-content:space-between; gap:8px; }
  .wc-miniapp-open { color:var(--wc-theme); white-space:nowrap; }
  .wc-link-card { cursor:pointer; }
  .wc-link-title { color:var(--wc-theme); }
  /* ── 微信公众号/服务号文章卡片（微信同款：左文右图 + 阅读全文） ── */
  .wc-article-card { width:286px; background:var(--wc-card); border:1px solid var(--wc-border-light); border-radius:6px; padding:0; overflow:hidden; cursor:pointer; transition:border-color .12s ease, box-shadow .12s ease; }
  .wc-article-card:hover { border-color:var(--wc-border); box-shadow:0 3px 12px rgba(0,0,0,.14); }
  .wc-article-card:focus-visible { outline:2px solid var(--wc-theme); outline-offset:2px; }
  .wc-article-cover { width:100%; height:150px; overflow:hidden; background:var(--wc-bg2); }
  .wc-article-cover-img { width:100%; height:100%; object-fit:cover; display:block; }
  .wc-article-title { font-size:15px; font-weight:600; line-height:1.45; padding:11px 13px 0; display:-webkit-box; -webkit-line-clamp:2; line-clamp:2; -webkit-box-orient:vertical; overflow:hidden; word-break:break-all; }
  .wc-article-des { font-size:12px; color:var(--wc-muted); margin-top:4px; padding:0 13px; display:-webkit-box; -webkit-line-clamp:2; line-clamp:2; -webkit-box-orient:vertical; overflow:hidden; }
  .wc-article-foot { display:flex; justify-content:space-between; align-items:center; gap:8px; padding:9px 13px 11px; }
  .wc-article-open { color:var(--wc-theme); white-space:nowrap; }
  .wc-article-subs { border-top:1px solid var(--wc-border-light); padding:9px 13px 11px; display:flex; flex-direction:column; gap:8px; }
  .wc-article-sub { display:flex; align-items:center; gap:9px; cursor:pointer; border-radius:4px; padding:2px; }
  .wc-article-sub:hover { background:color-mix(in srgb, var(--wc-text) 6%, transparent); }
  .wc-article-sub-title { flex:1; min-width:0; font-size:13px; line-height:1.4; display:-webkit-box; -webkit-line-clamp:2; line-clamp:2; -webkit-box-orient:vertical; overflow:hidden; word-break:break-all; }
  .wc-article-sub-thumb { width:56px; height:56px; flex-shrink:0; border-radius:4px; object-fit:cover; background:var(--wc-bg2); }
  .wc-quote-bubble { min-width:140px; }
  /* ── 微信原版卡片：转账/红包为固定色卡片 + 气泡小尾巴（不随主题变色） ── */
  .wc-card-bubble { padding:0 !important; background:transparent !important; box-shadow:none !important; overflow:visible !important; }
  .wc-card-bubble::after { display:none !important; }
  .wc-transfer-card { width:222px; background:#f79c46; border-radius:6px; position:relative; overflow:visible; }
  .wc-transfer-card::after { content:''; position:absolute; top:16px; left:-4px; width:10px; height:10px; background:#f79c46; transform:rotate(45deg); border-radius:2px; }
  .wc-msg-self .wc-transfer-card::after { left:auto; right:-4px; }
  .wc-transfer-content { display:flex; align-items:center; padding:10px 12px; min-height:58px; }
  .wc-transfer-icon { width:36px; height:36px; border-radius:8px; background:#f5484b; color:#fff; display:flex; align-items:center; justify-content:center; font-size:20px; font-weight:700; flex-shrink:0; }
  .wc-transfer-info { flex:1; margin-left:10px; display:flex; flex-direction:column; overflow:hidden; min-width:0; }
  .wc-transfer-amount { font-size:16px; font-weight:500; color:#fff; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
  .wc-transfer-status { font-size:12px; color:#fff; margin-top:2px; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
  .wc-transfer-bottom { height:27px; display:flex; align-items:center; padding:0 12px; position:relative; }
  .wc-transfer-bottom::before { content:''; position:absolute; top:0; left:13px; right:13px; height:1px; background:rgba(255,255,255,0.2); }
  .wc-transfer-bottom span { font-size:11.5px; color:#fff; }
  .wc-transfer-card.wc-pay-received { background:#FDCE9D; }
  .wc-transfer-card.wc-pay-received::after { background:#FDCE9D; }
  .wc-transfer-card.wc-pay-returned { background:#fde1c3; }
  .wc-transfer-card.wc-pay-returned::after { background:#fde1c3; }
  .wc-transfer-card.wc-pay-overdue { background:#E9CFB3; }
  .wc-transfer-card.wc-pay-overdue::after { background:#E9CFB3; }
  .wc-redpacket-card { width:222px; background:#fa9d3b; border-radius:6px; position:relative; overflow:visible; }
  .wc-redpacket-card::after { content:''; position:absolute; top:16px; left:-4px; width:10px; height:10px; background:#fa9d3b; transform:rotate(45deg); border-radius:2px; }
  .wc-msg-self .wc-redpacket-card::after { left:auto; right:-4px; }
  .wc-redpacket-content { display:flex; align-items:center; padding:10px 12px; min-height:58px; }
  .wc-redpacket-icon { width:32px; height:36px; flex-shrink:0; display:flex; align-items:center; justify-content:center; }
  .wc-redpacket-icon svg { display:block; }
  .wc-redpacket-info { flex:1; margin-left:10px; display:flex; flex-direction:column; overflow:hidden; min-width:0; }
  .wc-redpacket-text { font-size:14px; color:#fff; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
  .wc-redpacket-status { font-size:12px; color:#fff; margin-top:2px; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
  .wc-redpacket-bottom { height:27px; display:flex; align-items:center; padding:0 12px; position:relative; }
  .wc-redpacket-bottom::before { content:''; position:absolute; top:0; left:13px; right:13px; height:1px; background:rgba(255,255,255,0.2); }
  .wc-redpacket-bottom span { font-size:11.5px; color:#faecda; }
  .wc-redpacket-card.wc-pay-received { background:#f8e2c6; }
  .wc-redpacket-card.wc-pay-received::after { background:#f8e2c6; }
  .wc-redpacket-card.wc-pay-received .wc-redpacket-text,
  .wc-redpacket-card.wc-pay-received .wc-redpacket-status { color:#b88550; }
  .wc-redpacket-card.wc-pay-received .wc-redpacket-bottom span { color:#c9a67a; }
  .wc-redpacket-card.wc-pay-received .wc-redpacket-icon svg { opacity:.72; }
  .wc-redpacket-card.wc-pay-returned { background:#f8e2c6; }
  .wc-redpacket-card.wc-pay-returned::after { background:#f8e2c6; }
  .wc-redpacket-card.wc-pay-returned .wc-redpacket-text,
  .wc-redpacket-card.wc-pay-returned .wc-redpacket-status { color:#b88550; }
  .wc-redpacket-card.wc-pay-returned .wc-redpacket-bottom span { color:#c9a67a; }
  .wc-redpacket-card.wc-pay-overdue { background:#f0d4b8; }
  .wc-redpacket-card.wc-pay-overdue::after { background:#f0d4b8; }
  .wc-redpacket-card.wc-pay-overdue .wc-redpacket-text,
  .wc-redpacket-card.wc-pay-overdue .wc-redpacket-status { color:#a98c6b; }
  .wc-redpacket-card.wc-pay-overdue .wc-redpacket-bottom span { color:#bda283; }
  /* 文件卡片（主题化：彩色类型瓦片 + 可点击打开/打开目录） */
  .wc-file-card { width:240px; background:var(--wc-card); border:1px solid var(--wc-border-light); border-radius:8px; position:relative; overflow:visible; cursor:pointer; transition:border-color .12s ease, box-shadow .12s ease, transform .08s ease; user-select:none; }
  .wc-file-card:hover { border-color:var(--wc-border); box-shadow:0 4px 14px rgba(0,0,0,.16); }
  .wc-file-card:active { transform:scale(.985); }
  .wc-file-card:focus-visible { outline:2px solid var(--wc-theme); outline-offset:2px; }
  .wc-file-card::after { content:''; position:absolute; top:16px; left:-4px; width:10px; height:10px; background:var(--wc-card); transform:rotate(45deg); border-radius:2px; }
  .wc-msg-self .wc-file-card::after { left:auto; right:-4px; }
  .wc-file-body { display:flex; align-items:center; gap:10px; padding:11px 12px; }
  .wc-file-icon-tile { width:42px; height:42px; border-radius:9px; display:flex; align-items:center; justify-content:center; flex-shrink:0; color:#fff; box-shadow:inset 0 -1px 0 rgba(0,0,0,.12); }
  .wc-file-ext-label { font-size:11.5px; font-weight:700; letter-spacing:.2px; line-height:1; }
  .wc-file-tone-doc { background:linear-gradient(135deg,#4a90e2,#3572c6); }
  .wc-file-tone-sheet { background:linear-gradient(135deg,#3cb371,#2e8b57); }
  .wc-file-tone-slide { background:linear-gradient(135deg,#f5a623,#e08a00); }
  .wc-file-tone-zip { background:linear-gradient(135deg,#9b59b6,#7d3c98); }
  .wc-file-tone-audio { background:linear-gradient(135deg,#00b8a9,#00897b); }
  .wc-file-tone-video { background:linear-gradient(135deg,#e74c3c,#c0392b); }
  .wc-file-tone-image { background:linear-gradient(135deg,#e84393,#c2185b); }
  .wc-file-tone-app { background:linear-gradient(135deg,#5c6bc0,#3f51b5); }
  .wc-file-tone-file { background:linear-gradient(135deg,#8e9aab,#6b7888); }
  .wc-file-meta { min-width:0; flex:1; }
  .wc-file-title { font-size:13px; font-weight:600; color:var(--wc-text); line-height:1.4; display:-webkit-box; -webkit-line-clamp:2; line-clamp:2; -webkit-box-orient:vertical; overflow:hidden; word-break:break-all; }
  .wc-file-sub { font-size:11.5px; color:var(--wc-muted); margin-top:3px; }
  .wc-file-open-ico { display:inline-flex; align-items:center; justify-content:center; width:24px; height:24px; border-radius:6px; color:var(--wc-muted); opacity:0; transition:opacity .12s ease, background .12s ease, color .12s ease; flex-shrink:0; }
  .wc-file-card:hover .wc-file-open-ico { opacity:1; }
  .wc-file-open-ico:hover { background:color-mix(in srgb, var(--wc-text) 10%, transparent); color:var(--wc-theme); }
  .wc-file-open-ico:focus-visible { opacity:1; outline:2px solid var(--wc-theme); outline-offset:1px; }
  .wc-file-bottom { height:27px; display:flex; align-items:center; justify-content:space-between; padding:0 12px; position:relative; font-size:11.5px; color:var(--wc-muted); }
  .wc-file-bottom::before { content:''; position:absolute; top:0; left:13px; right:13px; height:1px; background:var(--wc-border-light); }
  .wc-file-hint { color:var(--wc-muted); transition:color .12s ease; }
  .wc-file-card:hover .wc-file-hint { color:var(--wc-theme); }
  .wc-file-opening { opacity:.72; pointer-events:none; }
  .wc-location-card { min-width:200px; cursor:pointer; }
  .wc-location-row { display:flex; align-items:center; gap:10px; }
  .wc-location-icon { display:inline-flex; flex-shrink:0; color:var(--wc-text2); }
  .wc-location-main { min-width:0; }
  .wc-location-name { font-size:13px; font-weight:600; color:var(--wc-text); }
  .wc-location-label { font-size:11.5px; color:var(--wc-text2); margin-top:2px; }
  .wc-contact-card { min-width:190px; }
  .wc-contact-row { display:flex; align-items:center; gap:10px; }
  .wc-contact-avatar { width:38px; height:38px; border-radius:50%; background:color-mix(in srgb, var(--wc-theme) 18%, transparent); color:var(--wc-theme); display:flex; align-items:center; justify-content:center; font-size:17px; font-weight:700; flex-shrink:0; }
  .wc-contact-main { min-width:0; }
  .wc-contact-name { font-size:13.5px; font-weight:600; }
  .wc-contact-username { font-size:11.5px; color:var(--wc-muted); margin-top:2px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .wc-channels-card { min-width:220px; cursor:pointer; }
  .wc-channels-row { display:flex; gap:10px; align-items:flex-start; }
  .wc-channels-cover { width:74px; height:74px; object-fit:cover; border-radius:6px; flex-shrink:0; background:var(--wc-bg2); }
  .wc-channels-main { min-width:0; flex:1; }
  .wc-channels-desc { -webkit-line-clamp:2; line-clamp:2; }
  .wc-channels-author { font-size:11.5px; color:var(--wc-muted); margin-top:3px; }
  .wc-chatlog-card { min-width:220px; }
  .wc-chatlog-title { font-size:13px; }
  .wc-chatlog-preview { margin-top:6px; background:color-mix(in srgb, var(--wc-text) 5%, transparent); border-radius:5px; padding:5px 8px; }
  .wc-chatlog-line { font-size:11.5px; color:var(--wc-text2); line-height:1.55; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .wc-chatlog-line + .wc-chatlog-line { margin-top:3px; }
  .wc-quote { margin-top:6px; padding:5px 8px; border-left:1px solid var(--wc-border); background:color-mix(in srgb,var(--wc-text) 5%,transparent); border-radius:3px; font-size:12px; color:var(--wc-text2); }
  .wc-msg-self .wc-quote { background:rgba(0,0,0,0.06); color:#333; border-left-color:rgba(0,0,0,0.2); }
  .wc-quote-name { font-weight:600; }

  .wc-loading-inline { display:inline-block;width:14px;height:14px;margin-right:6px;border:2px solid var(--wc-border);border-top-color:var(--wc-text);border-radius:50%;animation:wc-spin .7s linear infinite;vertical-align:middle; }
  .wc-loading-inline-sm { display:inline-block;width:10px;height:10px;border:2px solid var(--wc-border);border-top-color:var(--wc-text);border-radius:50%;animation:wc-spin .7s linear infinite;vertical-align:middle; }

  .wc-msg-emoji { display:inline-flex; align-items:center; justify-content:center; }
  .wc-msg-emoji-img { width:24px; height:24px; object-fit:contain; vertical-align:middle; }

  /* 图片消息：气泡透明、图片圆角，最大尺寸约束 */
  .wc-msg-image { padding:3px; background:transparent !important; box-shadow:none !important; line-height:0; }
  .wc-msg-self .wc-msg-image { background:transparent !important; }
  /* 图片气泡：轻量 img（原 NoiseReveal 每图一个 WebGL 渲染器，多图聊天性能差） */
  .wc-msg-noise-img {
    width:180px;
    height:220px;
    object-fit:cover;
    border-radius:8px;
    display:block;
    cursor:zoom-in;
    background:var(--wc-bg2);
    animation:wc-img-in .35s ease;
  }
  .wc-msg-noise-img:hover { filter:brightness(.92); }
  @keyframes wc-img-in { from { opacity:0; } to { opacity:1; } }
  .wc-msg-image-loading { display:inline-flex; align-items:center; gap:6px; color:var(--wc-muted); font-size:13px; }
  /* 失效图片占位气泡：视觉补位，主题化图标 + 提示，保持聊天布局完整 */
  .wc-msg-image-fail {
    display:flex;
    flex-direction:column;
    align-items:center;
    justify-content:center;
    gap:5px;
    width:180px;
    height:118px;
    border-radius:8px;
    border:1px dashed var(--wc-border);
    background:color-mix(in srgb, var(--wc-card) 72%, var(--wc-muted));
    color:var(--wc-muted);
    user-select:none;
  }
  .wc-msg-image-fail-ico { opacity:.78; }
  .wc-msg-image-fail-title { font-size:12px; font-weight:600; color:var(--wc-text2); }
  .wc-msg-image-fail-sub { font-size:11px; opacity:.8; }
  .wc-msg-image-retry { cursor:pointer; }
  .wc-msg-image-retry:hover { border-color:color-mix(in srgb, var(--wc-theme) 55%, var(--wc-border)); color:var(--wc-theme); }
  .wc-msg-image-retry:hover .wc-msg-image-fail-ico { opacity:1; }

  /* 图文推送卡片（腾讯新闻等 mmreader 消息） */
  .wc-msg-newsfeed { padding:0; overflow:hidden; width:320px; max-width:72vw; }
  .wc-news-hero { position:relative; cursor:pointer; line-height:0; }
  .wc-news-hero-img { width:100%; height:170px; object-fit:cover; display:block; }
  .wc-news-hero-title {
    position:absolute; left:0; right:0; bottom:0; padding:22px 12px 8px;
    font-size:14px; font-weight:600; line-height:1.35; color:#fff;
    background:linear-gradient(transparent, rgba(0,0,0,.72));
    display:-webkit-box; -webkit-line-clamp:2; line-clamp:2; -webkit-box-orient:vertical; overflow:hidden;
  }
  .wc-news-row {
    display:flex; align-items:center; gap:10px; padding:10px 12px; cursor:pointer;
    border-top:1px solid rgba(128,128,128,.18);
  }
  .wc-news-row:hover { background:rgba(128,128,128,.08); }
  .wc-news-row-body { flex:1; min-width:0; }
  .wc-news-row-title {
    font-size:14px; line-height:1.4; color:var(--wc-text,#e8e8e8);
    display:-webkit-box; -webkit-line-clamp:2; line-clamp:2; -webkit-box-orient:vertical; overflow:hidden;
  }
  .wc-news-row-digest {
    margin-top:3px; font-size:12px; color:var(--wc-muted);
    white-space:nowrap; overflow:hidden; text-overflow:ellipsis;
  }
  .wc-news-thumb { width:56px; height:56px; border-radius:6px; object-fit:cover; flex-shrink:0; }
  .wc-news-source {
    padding:6px 12px 8px; font-size:11.5px; color:var(--wc-muted);
    border-top:1px solid rgba(128,128,128,.18);
  }

  /* 已编辑徽标 */
  .wc-edited-badge { align-self:flex-start; font-size:11.5px;color:var(--wc-theme,#576b95);font-weight:700;margin-top:3px;margin-left:2px;user-select:none; }
  .wc-msg-self .wc-edited-badge { align-self:flex-end;margin-right:2px; }
</style>
