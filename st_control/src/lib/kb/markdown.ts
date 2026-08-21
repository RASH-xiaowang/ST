/* ============================================================
 * 知识库 — Wiki Markdown 渲染（纯函数）
 * 自 WikiPanel.svelte 下沉：行内语法 + 块级结构，输出安全 HTML。
 * ============================================================ */

/** HTML 转义（防注入） */
function esc(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

/** 行内 Markdown：代码 / 图片 / 链接 / [[Wiki 链接]] / 粗体 / 斜体 */
function inlineMd(s: string): string {
  // 先整体转义原始文本，再解析行内语法，防止 HTML 注入（XSS）
  let x = esc(s);
  x = x.replace(/`([^`]+)`/g, (_m, c) => '<code>' + c + '</code>');
  x = x.replace(/!\[([^\]]*)\]\(([^)]+)\)/g, (_m, alt, url) => `<img class="wiki-md-img" src="${url}" alt="${alt}">`);
  x = x.replace(/\[([^\]]+)\]\(([^)]+)\)/g, (_m, t, url) => `<a class="wiki-md-a" href="${url}" target="_blank" rel="noreferrer">${t}</a>`);
  // [[页面]] 或 [[页面|别名]] → 可点击的 Wiki 链接。
  // key 在整体 esc 阶段已转义（引号/尖括号成实体），属性注入安全。
  x = x.replace(/\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_m, t, label) => {
    const key = (t || '').trim();
    const shown = (label || t || '').trim();
    return `<button type="button" class="wiki-md-wl" data-wiki-page="${key}">${shown}</button>`;
  });
  x = x.replace(/\*\*([^*]+)\*\*/g, '<b>$1</b>');
  x = x.replace(/(^|[^*])\*([^*\n]+)\*/g, '$1<i>$2</i>');
  return x;
}

/** 块级 Markdown → HTML（标题 / 分隔线 / 列表 / 代码块 / 引用 / 段落） */
export function renderMd(src: string): string {
  const lines = (src || '').split('\n');
  let html = '';
  let listType: 'ul' | 'ol' | null = null;
  let listBuf: string[] = [];
  let codeBuf: string[] | null = null;
  const flushList = () => {
    if (listBuf.length) {
      html += `<${listType}>${listBuf.map((t) => `<li>${inlineMd(t)}</li>`).join('')}</${listType}>`;
      listBuf = [];
      listType = null;
    }
  };
  const flushAll = () => {
    flushList();
    if (codeBuf !== null) {
      html += `<pre class="wiki-md-code"><code>${esc(codeBuf.join('\n'))}</code></pre>`;
      codeBuf = null;
    }
  };
  for (const line of lines) {
    const raw = line.trim();
    if (codeBuf !== null) {
      if (/^```/.test(raw)) { flushAll(); continue; }
      codeBuf.push(line);
      continue;
    }
    if (/^```/.test(raw)) { flushAll(); codeBuf = []; continue; }
    if (raw === '') { flushList(); continue; }
    let m: RegExpMatchArray | null;
    if ((m = /^(#{1,6})\s+(.*)$/.exec(raw))) {
      flushAll();
      const lvl = m[1].length;
      html += `<h${lvl} class="wiki-md-h">${inlineMd(m[2])}</h${lvl}>`;
      continue;
    }
    if (/^(?:---+|\*\*\*+)$/.test(raw)) {
      flushAll();
      html += '<hr class="wiki-md-hr">';
      continue;
    }
    if (/^\s*[-*]\s+/.test(raw)) {
      if (listType !== 'ul') { flushList(); listType = 'ul'; }
      listBuf.push(raw.replace(/^\s*[-*]\s+/, ''));
      continue;
    }
    if (/^\d+\.\s+/.test(raw)) {
      if (listType !== 'ol') { flushList(); listType = 'ol'; }
      listBuf.push(raw.replace(/^\d+\.\s+/, ''));
      continue;
    }
    if (/^>\s?/.test(raw)) {
      flushList();
      html += `<blockquote class="wiki-md-quote"><p>${inlineMd(raw.replace(/^>\s?/, ''))}</p></blockquote>`;
      continue;
    }
    flushList();
    html += `<p>${inlineMd(line)}</p>`;
  }
  flushAll();
  return html;
}
