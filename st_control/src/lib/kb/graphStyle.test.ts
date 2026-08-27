import { describe, it, expect } from 'vitest';
import { edgeColor, colorSlug, nodeMatches, nodeTypeName, nodeColor, EDGE_COLOR_FALLBACK, EDGE_COLORS } from './graphStyle';
import type { WikiGraphNode } from './kbTypes';

function nd(over: Partial<WikiGraphNode> = {}): WikiGraphNode {
  return { id: 1, pageId: 1, title: '页面', docId: null, docTitle: null, dirName: null, inDegree: 0, outDegree: 0, status: 'published', ...over };
}

describe('graphStyle', () => {
  describe('edgeColor', () => {
    it('maps known types and falls back to gray', () => {
      expect(edgeColor('related')).toBe(EDGE_COLORS.related);
      expect(edgeColor('unknown-type')).toBe(EDGE_COLOR_FALLBACK);
    });
  });

  describe('colorSlug', () => {
    it('strips # and lowercases', () => {
      expect(colorSlug('#5B8FF9')).toBe('5b8ff9');
    });
  });

  describe('nodeMatches', () => {
    it('matches title/docTitle case-insensitively; empty query is false', () => {
      expect(nodeMatches(nd({ title: '架构设计' }), '架构')).toBe(true);
      expect(nodeMatches(nd({ title: '架构设计' }), '架构设计')).toBe(true);
      expect(nodeMatches(nd({ title: 'Quick Start' }), 'quick')).toBe(true);
      expect(nodeMatches(nd({ title: '架构设计', docTitle: 'FAQ 文档' }), 'faq')).toBe(true);
      expect(nodeMatches(nd({ title: '架构设计' }), '')).toBe(false);
      expect(nodeMatches(nd({ title: '架构设计' }), '   ')).toBe(false);
    });
  });

  describe('nodeTypeName', () => {
    it('classifies entity dirs, concept, and falls back to 页面', () => {
      expect(nodeTypeName(nd({ dirName: '实体' }))).toBe('实体');
      expect(nodeTypeName(nd({ dirName: '人物' }))).toBe('实体');
      expect(nodeTypeName(nd({ dirName: '概念' }))).toBe('概念');
      expect(nodeTypeName(nd({ dirName: '摘要' }))).toBe('摘要');
      expect(nodeTypeName(nd({ dirName: null }))).toBe('页面');
      expect(nodeTypeName(nd({ dirName: '' }))).toBe('页面');
    });
  });

  describe('nodeColor', () => {
    it('color groups take priority', () => {
      expect(nodeColor('published', nd({ title: '架构设计' }), [{ query: '架构', color: '#123456' }])).toBe('#123456');
    });
    it('uses type color first, then draft/missing/default for unknown dirs', () => {
      expect(nodeColor('published', nd({ dirName: '概念' }))).toBe('#7cc0ff');
      expect(nodeColor('published', nd({ dirName: null }))).toBe('#8d99ae'); // 页面类型色
      // 未知目录：类型色缺失时按状态回退
      expect(nodeColor('draft', nd({ dirName: '自定义' }))).toBe('#f6bd16');
      expect(nodeColor('missing', nd({ dirName: '自定义' }))).toBe('#8d99ae');
      expect(nodeColor('published', nd({ dirName: '自定义' }))).toBe('#5b8ff9');
    });
  });
});
