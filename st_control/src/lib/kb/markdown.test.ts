import { describe, it, expect } from 'vitest';
import { renderMd } from './markdown';

describe('markdown', () => {
  describe('renderMd', () => {
    it('renders empty string', () => {
      expect(renderMd('')).toBe('');
    });

    it('renders paragraph', () => {
      const result = renderMd('hello world');
      expect(result).toContain('<p>');
      expect(result).toContain('hello world');
    });

    it('renders headings', () => {
      expect(renderMd('# Title')).toContain('<h1');
      expect(renderMd('## Sub')).toContain('<h2');
      expect(renderMd('### H3')).toContain('<h3');
    });

    it('renders bold', () => {
      const result = renderMd('**bold**');
      expect(result).toContain('<b>bold</b>');
    });

    it('renders italic', () => {
      const result = renderMd('*italic*');
      expect(result).toContain('<i>italic</i>');
    });

    it('renders inline code', () => {
      const result = renderMd('`code`');
      expect(result).toContain('<code>code</code>');
    });

    it('renders links', () => {
      const result = renderMd('[text](https://example.com)');
      expect(result).toContain('<a');
      expect(result).toContain('href="https://example.com"');
      expect(result).toContain('text</a>');
    });

    it('renders wiki links', () => {
      const result = renderMd('[[Page Name]]');
      expect(result).toContain('wiki-md-wl');
      expect(result).toContain('data-wiki-page="Page Name"');
    });

    it('renders wiki links with alias', () => {
      const result = renderMd('[[Page|Alias]]');
      expect(result).toContain('data-wiki-page="Page"');
      expect(result).toContain('Alias</button>');
    });

    it('renders unordered list', () => {
      const result = renderMd('- item1\n- item2');
      expect(result).toContain('<ul>');
      expect(result).toContain('<li>item1</li>');
      expect(result).toContain('<li>item2</li>');
    });

    it('renders ordered list', () => {
      const result = renderMd('1. first\n2. second');
      expect(result).toContain('<ol>');
      expect(result).toContain('<li>first</li>');
    });

    it('renders horizontal rule', () => {
      expect(renderMd('---')).toContain('<hr');
    });

    it('renders blockquote', () => {
      const result = renderMd('> quoted text');
      expect(result).toContain('<blockquote');
      expect(result).toContain('quoted text');
    });

    it('renders code block', () => {
      const result = renderMd('```\ncode here\n```');
      expect(result).toContain('<pre');
      expect(result).toContain('code here');
    });

    it('escapes HTML in content (XSS prevention)', () => {
      const result = renderMd('<script>alert("xss")</script>');
      expect(result).not.toContain('<script>');
      expect(result).toContain('&lt;script&gt;');
    });

    it('rejects javascript: URLs in valid link syntax', () => {
      // Note: [text](url) is parsed as link; bare [text](javascript:...) with
      // special chars may be escaped before link regex matches, rendering as plain text.
      // The safeUrl guard only applies when the link regex successfully matches.
      const result = renderMd('[click](javascript:alert(1))');
      // Either the URL is stripped (safe link) or the whole thing is escaped as text
      expect(result).not.toContain('href="javascript:');
    });

    it('allows data:image URLs', () => {
      const result = renderMd('![img](data:image/png;base64,abc)');
      expect(result).toContain('data:image/png');
    });
    it('renders paragraph containing wiki link with alias (not swallowed as table)', () => {
      const result = renderMd('参见 [[FAQ|常见问题]]。');
      expect(result).toContain('data-wiki-page="FAQ"');
      expect(result).toContain('常见问题</button>');
      expect(result).toContain('参见');
    });

    it('renders GFM table with header + separator + rows', () => {
      const result = renderMd('| 功能 | 说明 |\n| --- | --- |\n| 向量检索 | 语义相似 |');
      expect(result).toContain('<table');
      expect(result).toContain('<th>功能</th>');
      expect(result).toContain('<td>向量检索</td>');
      expect(result).toContain('<td>语义相似</td>');
    });

    it('renders plain pipe line as paragraph when no separator follows', () => {
      const result = renderMd('a | b | c');
      expect(result).toContain('<p>');
      expect(result).toContain('a | b | c');
    });
    it('ends table at blockquote / heading after table (not absorbed as row)', () => {
      const result = renderMd('| 功能 | 说明 |\n| --- | --- |\n| 向量检索 | 语义相似 |\n> 提示：支持 [[FAQ|常见问题]]');
      expect(result).toContain('<table');
      expect(result).toContain('<blockquote');
      expect(result).toContain('data-wiki-page="FAQ"');
      expect(result).not.toContain('<td>&gt;');
      const result2 = renderMd('| a | b |\n| --- | --- |\n| 1 | 2 |\n## 标题 | 含竖线');
      expect(result2).toContain('<h2');
      expect(result2).toContain('<table');
    });


  });
});
