/* ============================================================
 * 大模型对话 — 助手消息渲染纯函数
 * 自 MessageBody.svelte 下沉：零依赖轻量 markdown 渲染
 * （段落/标题/列表/行内样式/裸媒体 URL/代码块/图表块）。
 * ============================================================ */

import type { ChartSpec } from './types';

/** 消息内容块 */
export type Block =
  | { type: "prose"; html: string }
  | { type: "code"; lang: string; code: string }
  | { type: "chart"; spec: ChartSpec };

const MEDIA_RE = /^https?:\/\/\S+$/;
const IMG_EXT = /\.(png|jpe?g|gif|webp|svg|bmp)$/i;
const VID_EXT = /\.(mp4|webm|ogg|mov|m4v)(\?.*)?$/i;
const AUDIO_EXT = /\.(mp3|wav|m4a|aac|flac|opus|oga)(\?.*)?$/i;
const FILE_EXT = /\.(pdf|docx?|xlsx?|pptx?|csv|zip|txt|json|md)$/i;

/** 音频 URL 识别（扩展名或 data:audio） */
export function isAudioUrl(u: string): boolean {
  return AUDIO_EXT.test(u) || /^data:audio\//i.test(u);
}

/** 安全 JSON 解析（非法返回 null） */
export function safeJson(s: string): ChartSpec | null {
  try {
    return JSON.parse(s);
  } catch {
    return null;
  }
}

/** 最小 HTML 转义（& < >） */
function esc(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/** 行内 markdown：行内代码 / 图片 / 链接 / 粗体 / 斜体。
 *  行内代码先提取保护，避免 `**` 等规则误伤代码内容。 */
export function inlineMd(s: string): string {
  let x = s;
  // 1) 行内代码优先提取保护（占位符不含 markdown 符号）
  const codeSpans: string[] = [];
  x = x.replace(/`([^`]+)`/g, (_, c) => {
    codeSpans.push(`<code>${esc(c)}</code>`);
    return `\u0000${codeSpans.length - 1}\u0000`;
  });
  x = x.replace(
    /!\[([^\]]*)\]\(([^)]+)\)/g,
    (_, alt, url) => {
      const a = String(alt || "");
      // 根据「媒体类型标记」或扩展名决定渲染方式（视频/音频链接常无扩展名）
      if (VID_EXT.test(url) || a.startsWith("🎬") || a.startsWith("📹")) {
        return `<video class="llm-md-img" controls preload="metadata" src="${url}" title="${alt}"></video>`;
      }
      if (isAudioUrl(url) || a.startsWith("🎙") || a.startsWith("🔊")) {
        return `<audio class="llm-md-img" controls preload="metadata" src="${url}" title="${alt}"></audio>`;
      }
      return `<img class="llm-md-img" src="${url}" alt="${alt}">`;
    },
  );
  x = x.replace(
    /\[([^\]]+)\]\(([^)]+)\)/g,
    (_, t, url) => `<a class="llm-ext-link" href="${url}" target="_blank" rel="noreferrer">${t}</a>`,
  );
  x = x.replace(/\*\*([^*]+)\*\*/g, "<b>$1</b>");
  x = x.replace(/(^|[^*])\*([^*\n]+)\*/g, "$1<i>$2</i>");
  // 2) 还原行内代码
  x = x.replace(/\u0000(\d+)\u0000/g, (_, i) => codeSpans[Number(i)] ?? "");
  return x;
}

/** 拆表格行：去掉首尾 | 后按 | 切分单元格 */
function splitTableRow(raw: string): string[] {
  const trimmed = raw.trim();
  const inner = trimmed.replace(/^\|/, "").replace(/\|$/, "");
  return inner.split("|");
}

/** 零依赖轻量 markdown：段落 / 标题 / 列表 / 引用 / 表格 / 分割线 / 裸媒体 URL 行 */
export function miniMarkdown(src: string): string {
  const lines = src.split("\n");
  let html = "";
  let listType: "ul" | "ol" | null = null;
  let listBuf: string[] = [];
  let quoteBuf: string[] = [];
  const flush = () => {
    if (quoteBuf.length) {
      html += `<blockquote>${quoteBuf.map((t) => inlineMd(t)).join("<br>")}</blockquote>`;
      quoteBuf = [];
    }
    if (listBuf.length) {
      html += `<${listType}>${listBuf.map((t) => `<li>${inlineMd(t)}</li>`).join("")}</${listType}>`;
      listBuf = [];
      listType = null;
    }
  };
  const isQuote = (raw: string) => /^>\s?/.test(raw);
  const isHr = (raw: string) => /^(-{3,}|\*{3,}|_{3,})\s*$/.test(raw);
  const isTableRow = (raw: string) => raw.startsWith("|") && raw.trim().endsWith("|");
  const isTableSep = (raw: string) => /^\|[\s:|-]+\|?\s*$/.test(raw) && raw.includes("-");

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const raw = line.trim();
    if (raw === "") {
      flush();
      continue;
    }
    // 表格：一行 | 单元格 + 下一行 |---| 分隔行
    if (isTableRow(raw)) {
      const next = (lines[i + 1] ?? "").trim();
      if (isTableSep(next)) {
        flush();
        const rows: string[][] = [splitTableRow(raw)];
        let j = i + 2;
        while (j < lines.length && isTableRow(lines[j].trim())) {
          rows.push(splitTableRow(lines[j].trim()));
          j++;
        }
        i = j - 1;
        const head = rows[0];
        const body = rows.slice(1);
        const cell = (c: string, tag: string) => `<${tag}>${inlineMd(esc(c.trim()))}</${tag}>`;
        html += `<div class="llm-md-table"><table><thead><tr>${head.map((c) => cell(c, "th")).join("")}</tr></thead><tbody>${body.map((r) => `<tr>${r.map((c) => cell(c, "td")).join("")}</tr>`).join("")}</tbody></table></div>`;
        continue;
      }
    }
    // 引用块：连续的 > 行合并为一个 blockquote
    if (isQuote(raw)) {
      if (listType !== null) flush();
      quoteBuf.push(raw.replace(/^>\s?/, ""));
      continue;
    }
    // 分割线
    if (isHr(raw)) {
      flush();
      html += "<hr>";
      continue;
    }
    // 裸媒体 URL 行：直接渲染为对应元素
    if (MEDIA_RE.test(raw)) {
      flush();
      if (IMG_EXT.test(raw)) {
        html += `<img class="llm-md-img" src="${raw}" alt="">`;
      } else if (VID_EXT.test(raw)) {
        html += `<video class="llm-md-img" controls preload="metadata" src="${raw}" title=""></video>`;
      } else if (isAudioUrl(raw)) {
        html += `<audio class="llm-md-img" controls preload="metadata" src="${raw}" title=""></audio>`;
      } else if (FILE_EXT.test(raw)) {
        const name = decodeURIComponent(raw.split("/").pop() || raw);
        html += `<a class="llm-file-link" href="${raw}" target="_blank" rel="noreferrer" download>📎 ${name}</a>`;
      } else {
        html += `<a class="llm-ext-link" href="${raw}" target="_blank" rel="noreferrer">${raw}</a>`;
      }
      continue;
    }
    let m: RegExpMatchArray | null;
    if ((m = /^(#{1,6})\s+(.*)$/.exec(raw))) {
      flush();
      const lvl = m[1].length;
      html += `<h${lvl}>${inlineMd(m[2])}</h${lvl}>`;
      continue;
    }
    if (/^[-*]\s+/.test(raw)) {
      if (listType !== "ul") {
        flush();
        listType = "ul";
      }
      listBuf.push(raw.replace(/^[-*]\s+/, ""));
      continue;
    }
    if (/^\d+\.\s+/.test(raw)) {
      if (listType !== "ol") {
        flush();
        listType = "ol";
      }
      listBuf.push(raw.replace(/^\d+\.\s+/, ""));
      continue;
    }
    flush();
    html += `<p>${inlineMd(esc(line))}</p>`;
  }
  flush();
  return html;
}

/** 解析消息为块：代码块（lang=chart 走图表）、其余为 prose */
export function parseBlocks(raw: string): Block[] {
  const lines = raw.split("\n");
  const blocks: Block[] = [];
  let prose: string[] = [];
  const flush = () => {
    if (prose.length) {
      blocks.push({ type: "prose", html: miniMarkdown(prose.join("\n")) });
      prose = [];
    }
  };
  let i = 0;
  while (i < lines.length) {
    const fm = /^```(\w*)\s*$/.exec(lines[i]);
    if (fm) {
      const lang = (fm[1] || "").toLowerCase();
      const body: string[] = [];
      i++;
      while (i < lines.length && !/^```\s*$/.test(lines[i])) {
        body.push(lines[i]);
        i++;
      }
      i++; // 跳过结束的 ```
      if (lang === "chart") {
        flush();
        const spec = safeJson(body.join("\n"));
        if (spec) blocks.push({ type: "chart", spec });
      } else {
        flush();
        blocks.push({ type: "code", lang, code: body.join("\n") });
      }
    } else {
      prose.push(lines[i]);
      i++;
    }
  }
  flush();
  return blocks;
}
