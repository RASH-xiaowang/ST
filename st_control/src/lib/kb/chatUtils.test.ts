import { describe, it, expect } from 'vitest';
import { highlightSegments, parseCitations, extractChineseTerms } from './chatUtils';

describe('chatUtils', () => {
  describe('highlightSegments', () => {
    it('returns single non-hit segment for empty query', () => {
      const result = highlightSegments('hello world', '');
      expect(result).toEqual([{ text: 'hello world', hit: false }]);
    });

    it('returns single non-hit segment for short query (< 2 chars)', () => {
      const result = highlightSegments('hello world', 'a');
      expect(result).toEqual([{ text: 'hello world', hit: false }]);
    });

    it('highlights matching terms', () => {
      const result = highlightSegments('hello world foo', 'world');
      const hits = result.filter((s) => s.hit);
      expect(hits).toHaveLength(1);
      expect(hits[0].text).toBe('world');
    });

    it('is case insensitive', () => {
      const result = highlightSegments('Hello World', 'hello');
      const hits = result.filter((s) => s.hit);
      expect(hits).toHaveLength(1);
      expect(hits[0].text).toBe('Hello');
    });

    it('handles multiple terms', () => {
      const result = highlightSegments('abc def ghi', 'abc ghi');
      const hits = result.filter((s) => s.hit);
      expect(hits).toHaveLength(2);
    });

    it('handles no match', () => {
      const result = highlightSegments('hello world', 'xyz');
      expect(result).toEqual([{ text: 'hello world', hit: false }]);
    });
  });


  describe('extractChineseTerms', () => {
    it('extracts 2-4 char Chinese terms', () => {
      const terms = extractChineseTerms('创建知识库');
      expect(terms).toContain('知识库');
      expect(terms).toContain('创建');
    });

    it('removes whole-term stop words', () => {
      const terms = extractChineseTerms('如何创建');
      expect(terms).not.toContain('如何');
      expect(terms).toContain('创建');
    });

    it('deduplicates terms', () => {
      const terms = extractChineseTerms('知识库知识库');
      const set = new Set(terms);
      expect(set.size).toBe(terms.length);
      expect(terms.filter((t) => t === '知识库').length).toBe(1);
    });

    it('keeps non-Chinese segments with length >= 2', () => {
      const terms = extractChineseTerms('RAG 检索原理');
      expect(terms).toContain('RAG');
      expect(terms).toContain('检索');
    });

    it('ignores punctuation and short segments', () => {
      const terms = extractChineseTerms('你好，世界！');
      expect(terms).toContain('你好');
      expect(terms).toContain('世界');
      expect(terms.some((t) => /[，！]/.test(t))).toBe(false);
    });

    it('returns empty for input under 2 chars', () => {
      expect(extractChineseTerms('一')).toEqual([]);
    });
  });

  describe('parseCitations', () => {
    it('returns empty for null', () => {
      expect(parseCitations(null)).toEqual([]);
    });

    it('returns empty for invalid JSON', () => {
      expect(parseCitations('not json')).toEqual([]);
    });

    it('returns empty for non-array JSON', () => {
      expect(parseCitations('{}')).toEqual([]);
    });

    it('parses valid citation array', () => {
      const c = JSON.stringify([{ doc_id: 1, doc_title: 'test' }]);
      const result = parseCitations(c);
      expect(result).toHaveLength(1);
      expect(result[0].doc_id).toBe(1);
    });

    it('parses empty array', () => {
      expect(parseCitations('[]')).toEqual([]);
    });
  });
});
