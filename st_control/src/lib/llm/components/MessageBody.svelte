<script lang="ts">
  import { errText } from '../../format';
  // 单条会话消息渲染：
  // - 用户消息：渲染文本 + 多模态附件（图片预览 / 文件卡片）
  // - 助手消息：分段解析 → 文本(markdown) / 代码 / 图表(chart) / 媒体(图片·视频·文件)
  import type { ChatMessage } from "../types";
  import ChartView from "./ChartView.svelte";
  import { llmApi } from "../services/ipc";
  import { parseBlocks } from "../messageRender";
  import { RippleButton } from "fancy-ui-svelte";

  let { msg }: { msg: ChatMessage } = $props();

  // ─── 资源查看 / 保存（生图结果、聊天内图片）───
  let zoomSrc = $state<string | null>(null);
  let zoomName = $state("");
  let zoomKind = $state<"image" | "video" | "audio">("image");
  let saving = $state(false);
  let saveMsg = $state<string | null>(null);

  function openZoom(src: string, name: string, kind: "image" | "video" | "audio" = "image") {
    zoomSrc = src;
    zoomName = name || "image";
    zoomKind = kind;
    saveMsg = null;
  }
  function closeZoom() {
    zoomSrc = null;
  }

  // 点击助手消息中的图片/视频/音频（markdown 渲染出的 <img>/<video>/<audio>）打开查看器
  // 事件委托容器：点击与键盘 Enter 共用（仅使用 e.target 定位链接）
  function onProseClick(e: Event) {
    const t = e.target as HTMLElement | null;
    if (!t || !t.classList.contains("llm-md-img")) return;
    if (t.tagName === "IMG") {
      openZoom((t as HTMLImageElement).src, t.getAttribute("alt") || zoomName || "image", "image");
    } else if (t.tagName === "VIDEO") {
      openZoom((t as HTMLVideoElement).src, t.getAttribute("title") || "video", "video");
    } else if (t.tagName === "AUDIO") {
      const a = t as HTMLAudioElement;
      openZoom(a.src || a.currentSrc || "", t.getAttribute("title") || "audio", "audio");
    }
  }

  // 将图片地址转成字节，并推导出保存文件名
  async function srcToBytes(src: string): Promise<{ data: Uint8Array; name: string }> {
    let name = "image.png";
    try {
      const u = new URL(src);
      const base = decodeURIComponent(u.pathname.split("/").pop() || "");
      if (base) name = base;
    } catch {
      /* data: 或相对地址，使用默认名 */
    }
    const resp = await fetch(src);
    if (!resp.ok) throw new Error("下载失败 " + resp.status);
    const buf = await resp.arrayBuffer();
    return { data: new Uint8Array(buf), name };
  }

  // 保存资源：优先由后端下载（绕过浏览器跨域限制），失败则前端兜底
  async function saveImage() {
    if (!zoomSrc) return;
    saving = true;
    saveMsg = null;
    try {
      let name: string | undefined;
      try {
        const u = new URL(zoomSrc);
        const base = decodeURIComponent(u.pathname.split("/").pop() || "");
        if (base) name = base;
      } catch {
        /* data: 地址，由后端推断扩展名 */
      }
      const path = await llmApi.saveResourceFromUrl(zoomSrc, name);
      saveMsg = "已保存到：" + path;
    } catch (e: unknown) {
      // 后端下载失败（如本地 data URL 未走网络），退回前端 fetch + 写入
      try {
        const { data, name } = await srcToBytes(zoomSrc);
        const path = await llmApi.saveUploadedFile(name, data);
        saveMsg = "已保存到：" + path;
      } catch (e: unknown) {
        // 仍失败，则在外部程序中打开
        try {
          const { open } = await import("@tauri-apps/plugin-shell");
          await open(zoomSrc);
          saveMsg = "已尝试在外部程序中打开";
        } catch (e: unknown) {
          saveMsg = "保存失败：" + (errText(e));
        }
      }
    } finally {
      saving = false;
    }
  }

  const blocks = $derived(
    msg.role === "assistant" && msg.content ? parseBlocks(msg.content) : [],
  );

  const isUser = $derived(msg.role === "user");
  const hasParts = $derived(!!msg.parts && msg.parts.length > 0);
</script>

{#if isUser && hasParts}
  <div class="llm-attachments">
    {#each msg.parts as p (p.name ?? p.text ?? p.image_url?.url ?? Math.random())}
      {#if p.type === "image_url" && p.image_url}
        <button type="button"
          style="padding:0;border:none;background:none;cursor:zoom-in;display:inline-flex"
          title="点击查看 / 保存"
          onclick={() => openZoom(p.image_url!.url, p.name ?? "image")}
          onkeydown={(e) => e.key === 'Enter' && openZoom(p.image_url!.url, p.name ?? "image")}>
          <img class="llm-att-img" src={p.image_url.url} alt={p.name ?? "image"} />
        </button>
      {:else if p.type === "file"}
        <div class="llm-att-file">
          <span class="llm-att-icon">📎</span>
          <span class="llm-att-name">{p.name ?? "文件"}</span>
          {#if p.mime}<span class="llm-att-mime">{p.mime}</span>{/if}
        </div>
      {:else if p.type === "text" && p.text}
        <div class="llm-att-file llm-att-text">
          <span class="llm-att-name">{p.name ?? "文本片段"}</span>
        </div>
      {/if}
    {/each}
  </div>
{/if}

{#if isUser}
  {#if msg.content}<div class="llm-md-user">{msg.content}</div>{/if}
{:else}
  {#each blocks as b, bi (bi)}
    {#if b.type === "prose"}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions —— 消息内容链接委托容器，链接本身可聚焦 -->
      <div class="llm-md" role="application" aria-label="消息内容" tabindex="-1" onclick={onProseClick} onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); onProseClick(e); } }}>{@html b.html}</div>
    {:else if b.type === "code"}
      <pre class="llm-pre"><code>{b.code}</code></pre>
    {:else if b.type === "chart"}
      <ChartView spec={b.spec} />
    {/if}
  {/each}
{/if}

{#if zoomSrc}
  <div class="llm-zoom" onclick={closeZoom} role="presentation">
    <div class="llm-zoom-inner" onclick={(e) => e.stopPropagation()} role="presentation">
      {#if zoomKind === "video"}
        <video controls autoplay src={zoomSrc}><track kind="captions" /></video>
      {:else if zoomKind === "audio"}
        <audio controls autoplay src={zoomSrc}></audio>
      {:else}
        <img src={zoomSrc} alt={zoomName} />
      {/if}
      <div class="llm-zoom-bar">
        <span class="llm-zoom-name" title={zoomName}>{zoomName}</span>
        <RippleButton onclick={saveImage} disabled={saving} rippleColor="#a5f3fc"
          class="h-8 rounded-md border-0 bg-[var(--primary)] px-3 text-xs font-medium text-[var(--primary-foreground)] hover:opacity-90">
          {saving ? "保存中…" : "保存资源"}
        </RippleButton>
        <button class="llm-zoom-btn" onclick={closeZoom}>关闭</button>
      </div>
      {#if saveMsg}
        <div class="llm-zoom-msg">{saveMsg}</div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .llm-attachments { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 6px; }
  .llm-att-img {
    max-width: 120px; max-height: 120px; border-radius: 6px;
    border: 1px solid var(--app-color-border); object-fit: cover;
  }
  .llm-att-file {
    display: inline-flex; align-items: center; gap: 6px;
    padding: 4px 8px; border-radius: 6px;
    background: var(--app-color-surface-alt); border: 1px solid var(--app-color-border);
    font-size: 11.5px; color: var(--app-color-text); max-width: 200px;
  }
  .llm-att-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .llm-att-mime { color: var(--app-color-muted); font-size: 11.5px; }
  .llm-att-text { font-style: italic; color: var(--app-color-muted); }
  .llm-md-user { white-space: pre-wrap; word-break: break-word; margin: 0; }
  .llm-pre {
    background: var(--app-color-surface-alt); border: 1px solid var(--app-color-border);
    border-radius: 6px; padding: 8px; overflow-x: auto; font-size: 12px; margin: 6px 0;
  }
  .llm-pre code { font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; white-space: pre; }
  :global(.llm-md) { line-height: 1.4; }
  :global(.llm-md p) { margin: 4px 0; }
  :global(.llm-md h1), :global(.llm-md h2), :global(.llm-md h3), :global(.llm-md h4) { margin: 8px 0 4px; font-size: 14px; font-weight: 600; }
  :global(.llm-md ul), :global(.llm-md ol) { margin: 4px 0; padding-left: 20px; }
  :global(.llm-md img.llm-md-img),
  :global(.llm-md video.llm-md-img) {
    max-width: 360px; max-height: 360px; width: auto; height: auto;
    object-fit: contain; cursor: pointer;
    border-radius: 8px; border: 1px solid var(--app-color-border); margin: 6px 0; display: block;
  }
  :global(.llm-md code) {
    background: var(--app-color-surface-alt); border: 1px solid var(--app-color-border);
    border-radius: 4px; padding: 0 4px; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-size: 12px;
  }
  :global(.llm-md video) {
    max-width: 360px; max-height: 360px; border-radius: 8px;
    border: 1px solid var(--app-color-border); margin: 6px 0; display: block;
  }
  :global(.llm-md audio.llm-md-img) {
    max-width: 360px; width: 100%; border-radius: 8px;
    border: 1px solid var(--app-color-border); margin: 6px 0; display: block;
  }
  :global(.llm-md .llm-file-link),
  :global(.llm-md .llm-ext-link) {
    color: var(--app-color-accent); text-decoration: underline; word-break: break-all;
  }
  :global(.llm-md .llm-file-link) { display: inline-block; margin: 4px 0; }
  /* 引用块 */
  :global(.llm-md blockquote) {
    margin: 6px 0; padding: 5px 12px;
    border-left: 3px solid color-mix(in srgb, var(--app-color-accent) 60%, transparent);
    background: color-mix(in srgb, var(--app-color-accent) 6%, transparent);
    color: var(--app-color-muted);
    border-radius: 0 8px 8px 0;
  }
  /* 分割线 */
  :global(.llm-md hr) {
    border: none; border-top: 1px solid var(--app-color-border);
    margin: 10px 0;
  }
  /* 表格：外层可横向滚动，避免宽表把消息栏撑变形 */
  :global(.llm-md .llm-md-table) {
    overflow-x: auto; margin: 6px 0;
    border: 1px solid var(--app-color-border); border-radius: 8px;
  }
  :global(.llm-md table) {
    border-collapse: collapse; width: 100%; font-size: 12px;
    min-width: 320px;
  }
  :global(.llm-md th),
  :global(.llm-md td) {
    border: 1px solid var(--app-color-border);
    padding: 5px 9px; text-align: left; vertical-align: top;
  }
  :global(.llm-md th) {
    background: var(--app-color-surface-alt); font-weight: 600;
    white-space: nowrap;
  }
  :global(.llm-md td) { word-break: break-word; }

  /* 图片查看 / 保存浮层 */
  .llm-zoom {
    position: fixed; inset: 0; z-index: 9999;
    background: rgba(0, 0, 0, 0.74);
    display: flex; align-items: center; justify-content: center; padding: 24px;
  }
  .llm-zoom-inner {
    display: flex; flex-direction: column; gap: 10px;
    max-width: 92vw; max-height: 92vh;
  }
  .llm-zoom-inner img,
  .llm-zoom-inner video,
  .llm-zoom-inner audio {
    max-width: 92vw; max-height: 72vh; width: auto; height: auto;
    object-fit: contain; border-radius: 8px; background: #000;
  }
  .llm-zoom-inner audio { width: 70vw; }
  .llm-zoom-bar { display: flex; align-items: center; gap: 8px; justify-content: center; }
  .llm-zoom-name {
    color: #fff; font-size: 12px; max-width: 38vw;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .llm-zoom-btn {
    background: var(--app-color-surface-alt); color: var(--app-color-text);
    border: 1px solid var(--app-color-border); border-radius: 7px;
    padding: 7px 14px; font-size: 13px; cursor: pointer;
  }
  .llm-zoom-btn:disabled { opacity: 0.6; cursor: not-allowed; }
  .llm-zoom-msg { color: #cbd5e1; font-size: 12px; text-align: center; word-break: break-all; }
</style>

