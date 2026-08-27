/* ============================================================
 * 知识库 — Wiki Markdown 渲染（纯函数）
 * 自 WikiPanel.svelte 下沉：行内语法 + 块级结构，输出安全 HTML。
 * ============================================================ */

/** HTML 转义（含引号，防属性注入） */
function esc(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

/** URL 协议白名单校验（仅允许 http/https/data:image/相对路径/锚点） */
function safeUrl(raw: string): string {
  // 已转义文本中 &amp; 还原为 & 以便 URL 解析
  const decoded = raw.replace(/&amp;/g, '&');
  try {
    const u = new URL(decoded, 'http://localhost');
    if (u.protocol === 'http:' || u.protocol === 'https:' || u.protocol === 'mailto:') return raw;
    if (u.protocol === 'data:' && /^data:image\/(png|jpe?g|gif|webp|bmp|svg\+xml);/i.test(decoded)) return raw;
    if (decoded.startsWith('/') || decoded.startsWith('#')) return raw; // 相对路径/锚点
    return ''; // 拒绝 javascript: 等
  } catch {
    // 相对路径解析失败时允许（可能是片段锚点）
    if (decoded.startsWith('#') || decoded.startsWith('/')) return raw;
    return '';
  }
}

/** 行内 Markdown：代码 / 图片 / 链接 / [[Wiki 链接]] / 粗体 / 斜体 */
function inlineMd(s: string): string {
  // 先整体转义原始文本，再解析行内语法，防止 HTML 注入（XSS）
  let x = esc(s);
  x = x.replace(/`([^`]+)`/g, (_m, c) => '<code>' + c + '</code>');
  x = x.replace(/!\[([^\]]*)\]\(([^)]+)\)/g, (_m, alt, url) => {
    const su = safeUrl(url);
    return su ? `<img class="wiki-md-img" src="${su}" alt="${alt}">` : esc(`![${alt}](${url})`);
  });
  x = x.replace(/\[([^\]]+)\]\(([^)]+)\)/g, (_m, t, url) => {
    const su = safeUrl(url);
    return su ? `<a class="wiki-md-a" href="${su}" target="_blank" rel="noreferrer">${t}</a>` : esc(`[${t}](${url})`);
  });
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
  let tableBuf: string[][] = [];
  let tableHeader: string[] | null = null;
  const flushTable = () => {
    if (tableBuf.length && tableHeader) {
      html += '<div class="wiki-md-table-wrap"><table class="wiki-md-table"><thead><tr>';
      for (const cell of tableHeader) {
        html += '<th>' + inlineMd(cell) + '</th>';
      }
      html += '</tr></thead><tbody>';
      for (const row of tableBuf) {
        html += '<tr>';
        for (const cell of row) {
          html += '<td>' + inlineMd(cell) + '</td>';
        }
        html += '</tr>';
      }
      html += '</tbody></table></div>';
    }
    tableBuf = [];
    tableHeader = null;
  };
  const flushAll = () => {
    flushList();
    flushTable();
    if (codeBuf !== null) {
      html += `<pre class="wiki-md-code"><code>${esc(codeBuf.join('\n'))}</code></pre>`;
      codeBuf = null;
    }
  };
  // 按 | 切分单元格（过滤空单元格）
  const splitCells = (line: string): string[] => line.split('|').filter((c) => c.trim() !== '');
  // 是否为表格分隔行（|---| 或 |:--:|）
  const isDelimiterRow = (cells: string[]): boolean =>
    cells.length >= 1 && cells.every((c) => /^\s*[-:]+\s*$/.test(c));
  // 是否为块级结构开头（引用/标题/列表/代码围栏）：此类行即使含 | 也不是表格行，
  // 保证表格后的引用/标题/列表能正确结束表格并按块级渲染。
  const isBlockStart = (line: string): boolean =>
    /^>|^#{1,6}\s|^\s*[-*]\s+|^\d+\.\s|^```/.test(line);
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
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
    // 检测表格：仅当「表头行 + 下一行分隔行」同时满足才按表格解析，
    // 避免把含 | 的普通文本（如 [[页面|别名]] Wiki 链接）误判为表格导致整行丢失。
    if (raw.includes('|')) {
      const cells = splitCells(raw);
      // 表内分隔行：跳过（表头后的 |---| 已在表头启动时消费，此处处理表中间出现的分隔行）
      if (isDelimiterRow(cells)) {
        flushList();
        continue;
      }
      if (cells.length >= 2 && !isBlockStart(raw)) {
        if (tableHeader !== null) {
          tableBuf.push(cells.map((c) => c.trim()));
          continue;
        }
        // 候选表头：必须紧跟分隔行才成立
        const nextCells = splitCells(lines[i + 1]?.trim() ?? '');
        if (isDelimiterRow(nextCells)) {
          flushList();
          tableHeader = cells.map((c) => c.trim());
          i++; // 消费分隔行
          continue;
        }
        // 否则不是表格，落到普通段落处理
      }
    }
    flushTable();
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
