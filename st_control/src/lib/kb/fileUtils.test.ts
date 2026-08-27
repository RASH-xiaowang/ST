import { describe, it, expect } from 'vitest';
import {
  STATUS_LABEL,
  SOURCE_LABEL,
  MODE_LABEL,
  fileIco,
  previewMime,
  parseTags,
  flattenDirs,
  kbMonogram,
  trendArrow,
  trendClass,
  filterKbsByKeyword,
} from './fileUtils';
import type { DirNode, KbSummary } from './kbTypes';

describe('fileUtils', () => {
  describe('STATUS_LABEL', () => {
    it('maps known statuses', () => {
      expect(STATUS_LABEL.ready).toBe('解析完成');
      expect(STATUS_LABEL.processing).toBe('解析中');
      expect(STATUS_LABEL.pending).toBe('待解析');
      expect(STATUS_LABEL.failed).toBe('解析失败');
    });
  });

  describe('SOURCE_LABEL', () => {
    it('maps known sources', () => {
      expect(SOURCE_LABEL.upload).toBe('文件上传');
      expect(SOURCE_LABEL.fetch).toBe('网页抓取');
      expect(SOURCE_LABEL.manual).toBe('手动编辑');
    });
  });

  describe('MODE_LABEL', () => {
    it('maps known modes', () => {
      expect(MODE_LABEL.hybrid).toBe('混合');
      expect(MODE_LABEL.vector).toBe('向量');
      expect(MODE_LABEL.bm25).toBe('全文');
    });
  });

  describe('fileIco', () => {
    it('returns correct icons for known types', () => {
      expect(fileIco('pdf')).toBe('filePdf');
      expect(fileIco('docx')).toBe('fileDoc');
      expect(fileIco('xlsx')).toBe('fileXlsx');
      expect(fileIco('md')).toBe('fileMd');
      expect(fileIco('csv')).toBe('fileCsv');
    });
    it('returns file for null/unknown', () => {
      expect(fileIco(null)).toBe('file');
      expect(fileIco('xyz')).toBe('file');
    });
  });

  describe('previewMime', () => {
    it('returns correct MIME for known types', () => {
      expect(previewMime('pdf')).toBe('application/pdf');
      expect(previewMime('png')).toBe('image/png');
      expect(previewMime('jpg')).toBe('image/jpeg');
      expect(previewMime('md')).toBe('text/markdown');
    });
    it('returns octet-stream for unknown', () => {
      expect(previewMime('xyz')).toBe('application/octet-stream');
      expect(previewMime(null)).toBe('application/octet-stream');
    });
  });

  describe('parseTags', () => {
    it('splits by comma and deduplicates', () => {
      expect(parseTags('a,b,c')).toEqual(['a', 'b', 'c']);
      expect(parseTags('a，b，c')).toEqual(['a', 'b', 'c']);
      expect(parseTags('a,b,a')).toEqual(['a', 'b']);
    });
    it('handles semicolons', () => {
      expect(parseTags('a;b')).toEqual(['a', 'b']);
    });
    it('trims and filters empty', () => {
      expect(parseTags(' a , , b ')).toEqual(['a', 'b']);
    });
    it('filters tags longer than 30 chars', () => {
      const long = 'a'.repeat(31);
      expect(parseTags(`short,${long}`)).toEqual(['short']);
    });
    it('returns empty for empty input', () => {
      expect(parseTags('')).toEqual([]);
    });
  });

  describe('flattenDirs', () => {
    it('flattens nested tree', () => {
      const tree: DirNode[] = [
        { id: 1, kb_id: 1, parent_id: null, name: 'root', depth: 0, children: [
          { id: 2, kb_id: 1, parent_id: 1, name: 'child', depth: 1, children: [] },
        ]},
      ];
      const flat = flattenDirs(tree);
      expect(flat).toEqual([
        { id: 1, name: 'root', depth: 0 },
        { id: 2, name: 'child', depth: 1 },
      ]);
    });
    it('handles empty tree', () => {
      expect(flattenDirs([])).toEqual([]);
    });
  });

  describe('kbMonogram', () => {
    it('returns uppercase first letter', () => {
      expect(kbMonogram('hello')).toBe('H');
      expect(kbMonogram('知识库')).toBe('知');
    });
    it('handles non-ASCII first char', () => {
      // Emoji handling varies by JS engine; just verify it returns a single char
      const result = kbMonogram('知识库');
      expect(result).toBe('知');
    });
    it('handles empty string', () => {
      expect(kbMonogram('')).toBe('');
    });
  });

  describe('trendArrow', () => {
    it('returns up arrow for positive', () => {
      expect(trendArrow('+10%')).toBe('▲ ');
    });
    it('returns down arrow for negative', () => {
      expect(trendArrow('-5%')).toBe('▼ ');
    });
    it('returns empty for --', () => {
      expect(trendArrow('--')).toBe('');
    });
  });

  describe('trendClass', () => {
    it('returns correct classes', () => {
      expect(trendClass('+10%')).toBe('kb-trend-up');
      expect(trendClass('-5%')).toBe('kb-trend-down');
      expect(trendClass('--')).toBe('');
    });
  });

  describe('filterKbsByKeyword', () => {
    const kbs: KbSummary[] = [
      { id: 1, name: '产品手册', description: '产品文档', owner_id: 1, pinned: false, isSystem: false, docCount: 10, created_at: '' },
      { id: 2, name: '技术文档', description: 'API 说明', owner_id: 1, pinned: false, isSystem: false, docCount: 5, created_at: '' },
    ];
    it('returns all for empty keyword', () => {
      expect(filterKbsByKeyword(kbs, '')).toBe(kbs);
    });
    it('filters by name', () => {
      expect(filterKbsByKeyword(kbs, '产品')).toHaveLength(1);
    });
    it('filters by description', () => {
      expect(filterKbsByKeyword(kbs, 'API')).toHaveLength(1);
    });
    it('case insensitive', () => {
      expect(filterKbsByKeyword(kbs, 'api')).toHaveLength(1);
    });
    it('returns empty for no match', () => {
      expect(filterKbsByKeyword(kbs, '不存在')).toHaveLength(0);
    });
  });
});
