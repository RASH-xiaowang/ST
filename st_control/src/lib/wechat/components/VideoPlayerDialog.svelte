<!--
  视频播放器弹窗（朋友圈视频 / 文件视频共用）。
  自 WeChatPanel.svelte 抽出：两份同构模板 + scoped CSS 收敛为单组件；
  状态与错误处理由父组件传入（onVideoError 负责清源与提示）。
-->
<script lang="ts">
  let {
    open,
    src,
    title,
    error = '',
    loadingText = '视频解密中…',
    onClose,
    onLocate,
    onVideoError,
  }: {
    open: boolean;
    src: string;
    title: string;
    error?: string;
    loadingText?: string;
    onClose: () => void;
    onLocate?: () => void;
    onVideoError?: () => void;
  } = $props();
</script>

{#if open}
  <div class="wc-moment-video-player" role="dialog" aria-modal="true" aria-label="视频播放">
    <div
      class="wc-moment-video-mask"
      role="button"
      aria-label="关闭视频播放器"
      tabindex="-1"
      onclick={onClose}
      onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ' || e.key === 'Escape') { e.preventDefault(); onClose(); } }}
    ></div>
    <div class="wc-moment-video-box">
      <div class="wc-moment-video-hd">
        <span class="wc-moment-video-title" title={title}>{title}</span>
        <div class="wc-moment-video-actions">
          {#if onLocate}
            <button class="wc-img-viewer-btn" onclick={onLocate} title="在资源管理器中显示">定位</button>
          {/if}
          <button class="wc-img-viewer-btn" onclick={onClose} title="关闭 (Esc)">✕</button>
        </div>
      </div>
      <div class="wc-moment-video-stage">
        {#if src}
          <video src={src} controls autoplay playsinline onerror={onVideoError}><track kind="captions" /></video>
        {:else if error}
          <div class="wc-moment-video-err">{error}</div>
        {:else}
          <div class="wc-moment-video-loading"><span class="wc-loading-inline"></span>{loadingText}</div>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  @keyframes wc-spin { to { transform:rotate(360deg); } }
  .wc-moment-video-player { position:absolute; inset:0; z-index:80; display:flex; align-items:center; justify-content:center; }
  .wc-moment-video-mask { position:absolute; inset:0; background:rgba(0,0,0,.72); }
  .wc-moment-video-box { position:relative; width:min(860px, 92%); display:flex; flex-direction:column; gap:8px; }
  .wc-moment-video-hd { display:flex; align-items:center; gap:10px; color:#fff; font-size:13px; }
  .wc-moment-video-title { flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .wc-moment-video-actions { display:flex; gap:6px; flex-shrink:0; }
  .wc-moment-video-stage { display:flex; align-items:center; justify-content:center; background:#000; border-radius:8px; min-height:200px; max-height:78%; aspect-ratio:16/9; overflow:hidden; }
  .wc-moment-video-stage video { width:100%; height:100%; display:block; }
  .wc-moment-video-loading, .wc-moment-video-err { color:#fff; font-size:13px; display:flex; align-items:center; gap:8px; padding:20px; }
  .wc-img-viewer-btn { min-width:30px; height:30px; padding:0 8px; border:1px solid rgba(255,255,255,.25); border-radius:6px; background:rgba(255,255,255,.08); color:var(--wc-text); font-size:14px; cursor:pointer; transition:all .12s ease; }
  .wc-img-viewer-btn:hover { background:rgba(255,255,255,.2); border-color:rgba(255,255,255,.4); }
  .wc-loading-inline { display:inline-block;width:14px;height:14px;margin-right:6px;border:2px solid var(--wc-border);border-top-color:var(--wc-text);border-radius:50%;animation:wc-spin .7s linear infinite;vertical-align:middle; }
</style>
