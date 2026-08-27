<!--
  ResourcePreview：统一资源预览组件
  支持：图片（缩放/旋转/全屏）、PDF（翻页/缩放）、音视频（播放）、
  Office 文档（HTML 渲染）、Markdown、代码高亮、纯文本
-->
<script lang="ts">
  import KbIcon from './KbIcon.svelte';
  import { renderMd } from './markdown';
  import { Button } from '../components/ui/button';
  import { Badge } from '../components/ui/badge';
  import { Empty, EmptyTitle, EmptyDescription } from '../components/ui/empty';

  interface Props {
    title: string;
    fileType: string | null;
    dataBase64: string | null;
    textContent?: string | null;
    loading?: boolean;
    onClose: () => void;
    onDownload?: () => void;
  }

  let { title, fileType, dataBase64, textContent = null, loading = false, onClose, onDownload }: Props = $props();

  // ── 图片预览状态 ──
  let imgScale = $state(1);
  let imgRotation = $state(0);
  let imgFullscreen = $state(false);

  // ── PDF 预览状态 ──
  let pdfUrl = $state('');

  // ── 音视频状态 ──
  let mediaUrl = $state('');

  // ── 预览类型判断 ──
  const ft = $derived((fileType ?? '').toLowerCase());
  const previewType = $derived.by(() => {
    if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp'].includes(ft)) return 'image';
    if (ft === 'pdf') return 'pdf';
    if (['mp3', 'wav', 'm4a', 'ogg', 'flac', 'aac'].includes(ft)) return 'audio';
    if (['mp4', 'avi', 'mov', 'mkv', 'webm'].includes(ft)) return 'video';
    if (['md', 'markdown'].includes(ft)) return 'markdown';
    if (['py', 'js', 'ts', 'rs', 'json', 'xml', 'html', 'css', 'yaml', 'yml', 'toml'].includes(ft)) return 'code';
    if (['docx', 'doc', 'xlsx', 'xls', 'pptx', 'ppt', 'odt', 'ods', 'odp', 'rtf', 'epub'].includes(ft)) return 'office';
    return 'text';
  });

  // ── 文件类型 MIME 映射 ──
  function getMime(ext: string): string {
    const map: Record<string, string> = {
      png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif',
      webp: 'image/webp', bmp: 'image/bmp',
      pdf: 'application/pdf',
      mp3: 'audio/mpeg', wav: 'audio/wav', m4a: 'audio/mp4', ogg: 'audio/ogg',
      mp4: 'video/mp4', avi: 'video/x-msvideo', mov: 'video/quicktime',
      mkv: 'video/x-matroska', webm: 'video/webm',
    };
    return map[ext] ?? 'application/octet-stream';
  }

  // ── 创建 Blob URL ──
  $effect(() => {
    if (!dataBase64) return;
    try {
      const bin = Uint8Array.from(atob(dataBase64), (c) => c.charCodeAt(0));
      const blob = new Blob([bin], { type: getMime(ft) });
      const url = URL.createObjectURL(blob);
      if (previewType === 'pdf') pdfUrl = url;
      else if (previewType === 'audio' || previewType === 'video') mediaUrl = url;
      return () => { URL.revokeObjectURL(url); };
    } catch { /* base64 解码失败时忽略 */ }
  });

  // ── 图片控制 ──
  function zoomIn() { imgScale = Math.min(imgScale + 0.25, 5); }
  function zoomOut() { imgScale = Math.max(imgScale - 0.25, 0.25); }
  function rotate() { imgRotation = (imgRotation + 90) % 360; }
  function fitWindow() { imgScale = 1; imgRotation = 0; }

  // ── 图片 URL ──
  let imgUrl = $state('');
  $effect(() => {
    if (previewType !== 'image' || !dataBase64) return;
    try {
      const bin = Uint8Array.from(atob(dataBase64), (c) => c.charCodeAt(0));
      const blob = new Blob([bin], { type: getMime(ft) });
      imgUrl = URL.createObjectURL(blob);
      return () => { URL.revokeObjectURL(imgUrl); };
    } catch { /* ignore */ }
  });
</script>

<div class="resource-preview" class:fullscreen={imgFullscreen}>
  <!-- 工具栏 -->
  <div class="rp-toolbar">
    <div class="rp-title">
      <KbIcon name={previewType === 'image' ? 'image' : previewType === 'pdf' ? 'file' : previewType === 'audio' || previewType === 'video' ? 'play' : 'file'} size={16} />
      <span class="rp-title-text" title={title}>{title}</span>
      <Badge variant="outline" class="text-[10px]">{ft.toUpperCase()}</Badge>
    </div>
    <div class="rp-actions">
      {#if previewType === 'image'}
        <Button variant="ghost" size="icon-sm" onclick={zoomOut} title="缩小"><KbIcon name="minus" size={14} /></Button>
        <span class="rp-zoom-label">{Math.round(imgScale * 100)}%</span>
        <Button variant="ghost" size="icon-sm" onclick={zoomIn} title="放大"><KbIcon name="plus" size={14} /></Button>
        <Button variant="ghost" size="icon-sm" onclick={fitWindow} title="适应窗口"><KbIcon name="arrowsOut" size={14} /></Button>
        <Button variant="ghost" size="icon-sm" onclick={rotate} title="旋转"><KbIcon name="refresh" size={14} /></Button>
      {/if}
      {#if onDownload}
        <Button variant="ghost" size="icon-sm" onclick={onDownload} title="下载"><KbIcon name="download" size={14} /></Button>
      {/if}
      <Button variant="ghost" size="icon-sm" onclick={onClose} title="关闭"><KbIcon name="close" size={14} /></Button>
    </div>
  </div>

  <!-- 预览内容 -->
  <div class="rp-content">
    {#if loading}
      <div class="rp-loading">
        <div class="rp-spinner"></div>
        <span>加载中…</span>
      </div>
    {:else if previewType === 'image' && imgUrl}
      <div class="rp-image-wrap" role="img" aria-label={title}>
        <img
          src={imgUrl}
          alt={title}
          style="transform: scale({imgScale}) rotate({imgRotation}deg)"
          draggable="false"
        />
      </div>
    {:else if previewType === 'pdf' && pdfUrl}
      <iframe src={pdfUrl} title={title} class="rp-pdf"></iframe>
    {:else if previewType === 'video' && mediaUrl}
      <div class="rp-media-wrap">
        <video controls autoplay={false} style="max-width:100%;max-height:100%;border-radius:8px">
          <source src={mediaUrl} type={getMime(ft)} />
          您的浏览器不支持视频播放
        </video>
      </div>
    {:else if previewType === 'audio' && mediaUrl}
      <div class="rp-audio-wrap">
        <div class="rp-audio-icon">
          <KbIcon name="play" size={48} color="var(--kb-accent-bright)" />
        </div>
        <audio controls autoplay={false} style="width:80%;max-width:500px">
          <source src={mediaUrl} type={getMime(ft)} />
          您的浏览器不支持音频播放
        </audio>
      </div>
    {:else if previewType === 'markdown' && textContent}
      <div class="rp-markdown">{@html renderMd(textContent)}</div>
    {:else if previewType === 'office' && textContent}
      <div class="rp-office">
        <div class="rp-office-header">
          <KbIcon name={ft.includes('xls') ? 'table' : ft.includes('ppt') ? 'presentation' : 'file'} size={18} />
          <span class="rp-office-type">{ft.toUpperCase()} 文档预览</span>
          <Badge variant="outline" class="text-[10px]">提取文本</Badge>
        </div>
        <div class="rp-office-content">
          {#if ft.includes('xls')}
            <!-- Excel: 尝试表格渲染 -->
            {@const lines = textContent.split('\n').filter((l: string) => l.trim())}
            {#if lines.length > 0 && lines[0].includes('\t')}
              <div class="rp-table-wrap">
                <table class="rp-table">
                  <tbody>
                  {#each lines.slice(0, 200) as line, i}
                    {@const cells = line.split('\t')}
                    <tr class={i === 0 ? 'rp-table-header' : ''}>
                      {#each cells as cell}
                        <td>{cell}</td>
                      {/each}
                    </tr>
                  {/each}
                  </tbody>
                </table>
                {#if lines.length > 200}
                  <div class="rp-table-more">显示前 200 行，共 {lines.length} 行</div>
                {/if}
              </div>
            {:else}
              <pre class="rp-office-text">{textContent}</pre>
            {/if}
          {:else if ft.includes('ppt')}
            <!-- PPT: 按分隔符分页展示 -->
            {@const slides = textContent.split(/\n-{3,}\n|\n={3,}\n/)}
            <div class="rp-slides">
              {#each slides as slide, i}
                {#if slide.trim()}
                  <div class="rp-slide">
                    <div class="rp-slide-number">幻灯片 {i + 1}</div>
                    <div class="rp-slide-content">{slide.trim()}</div>
                  </div>
                {/if}
              {/each}
            </div>
          {:else}
            <!-- Word/其他: 格式化文本渲染 -->
            <div class="rp-office-text">{@html renderMd(textContent)}</div>
          {/if}
        </div>
      </div>
    {:else if previewType === 'text' && textContent}
      <pre class="rp-text">{textContent}</pre>
    {:else if previewType === 'code' && textContent}
      <div class="rp-code">
        <div class="rp-code-header">
          <KbIcon name="file" size={14} />
          <span class="rp-code-lang">{ft}</span>
          <Badge variant="outline" class="text-[10px]">代码</Badge>
        </div>
        <pre class="rp-code-content"><code>{textContent}</code></pre>
      </div>
    {:else}
      <Empty class="min-h-[300px]">
        <KbIcon name="file" size={32} color="var(--kb-text-3)" />
        <EmptyTitle class="text-sm">暂不支持预览此格式</EmptyTitle>
        <EmptyDescription>请下载后使用对应软件打开</EmptyDescription>
        {#if onDownload}
          <Button variant="outline" onclick={onDownload}><KbIcon name="download" size={14} />下载文件</Button>
        {/if}
      </Empty>
    {/if}
  </div>
</div>

<style>
  .resource-preview {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--app-bg-color);
  }
  .resource-preview.fullscreen {
    position: fixed;
    inset: 0;
    z-index: 9999;
    background: #000;
  }

  .rp-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    border-bottom: 1px solid var(--kb-border-subtle);
    flex-shrink: 0;
    background: var(--kb-surface);
  }
  .rp-title {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    flex: 1;
  }
  .rp-title-text {
    font-size: 13px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rp-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
  }
  .rp-zoom-label {
    font-size: 11.5px;
    color: var(--kb-text-3);
    min-width: 36px;
    text-align: center;
  }

  .rp-content {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  /* 图片预览 */
  .rp-image-wrap {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
    overflow: auto;
    cursor: grab;
  }
  .rp-image-wrap img {
    max-width: 90%;
    max-height: 85vh;
    object-fit: contain;
    transition: transform 0.2s ease;
    user-select: none;
  }

  /* PDF 预览 */
  .rp-pdf {
    width: 100%;
    height: 100%;
    border: none;
    display: block;
    background: #fff;
  }

  /* 音视频 */
  .rp-media-wrap {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
    padding: 20px;
  }
  .rp-audio-wrap {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 24px;
    width: 100%;
    height: 100%;
  }
  .rp-audio-icon {
    width: 80px;
    height: 80px;
    border-radius: 50%;
    background: var(--kb-surface-2);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  /* Markdown 预览 */
  .rp-markdown {
    padding: 20px 24px;
    max-width: 800px;
    width: 100%;
    font-size: 14px;
    line-height: 1.8;
    color: var(--kb-text);
  }

  /* 纯文本 */
  .rp-text {
    padding: 16px 20px;
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
    font-size: 13px;
    line-height: 1.7;
    color: var(--kb-text);
    font-family: 'Cascadia Code', 'Fira Code', monospace;
    width: 100%;
  }

  /* 加载动画 */
  .rp-loading {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    color: var(--kb-text-3);
    font-size: 13px;
  }
  .rp-spinner {
    width: 28px;
    height: 28px;
    border: 3px solid var(--kb-border);
    border-top-color: var(--kb-accent-bright);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  /* Office 文档预览 */
  .rp-office {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
  }
  .rp-office-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--kb-border-subtle);
    flex-shrink: 0;
    background: var(--kb-surface);
  }
  .rp-office-type {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--kb-text);
  }
  .rp-office-content {
    flex: 1;
    overflow: auto;
    padding: 16px 20px;
  }
  .rp-office-text {
    font-size: 13px;
    line-height: 1.8;
    color: var(--kb-text);
    white-space: pre-wrap;
    word-break: break-word;
    margin: 0;
    font-family: inherit;
  }

  /* Excel 表格 */
  .rp-table-wrap {
    overflow: auto;
    max-height: 100%;
  }
  .rp-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12.5px;
    line-height: 1.5;
  }
  .rp-table td {
    padding: 6px 10px;
    border: 1px solid var(--kb-border-subtle);
    color: var(--kb-text);
    white-space: nowrap;
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .rp-table-header td {
    background: var(--kb-surface-2);
    font-weight: 600;
    color: var(--kb-text);
    position: sticky;
    top: 0;
    z-index: 1;
  }
  .rp-table-more {
    padding: 12px;
    text-align: center;
    font-size: 12px;
    color: var(--kb-text-3);
  }

  /* PPT 幻灯片 */
  .rp-slides {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .rp-slide {
    border: 1px solid var(--kb-border);
    border-radius: 8px;
    overflow: hidden;
  }
  .rp-slide-number {
    padding: 6px 12px;
    background: var(--kb-surface-2);
    font-size: 11.5px;
    font-weight: 600;
    color: var(--kb-text-3);
    border-bottom: 1px solid var(--kb-border-subtle);
  }
  .rp-slide-content {
    padding: 12px 16px;
    font-size: 13px;
    line-height: 1.7;
    color: var(--kb-text);
    white-space: pre-wrap;
  }
</style>
