import { describe, it, expect } from 'vitest';
import { statusLabel, wikiNodeColor, wikiNodeTooltip } from './wikiPanelUtils';

describe('wikiPanelUtils', () => {
  describe('statusLabel', () => {
    it('maps known statuses to Chinese labels and passes through unknown', () => {
      expect(statusLabel('draft')).toBe('草稿');
      expect(statusLabel('published')).toBe('已发布');
      expect(statusLabel('archived')).toBe('已归档');
      expect(statusLabel('ready')).toBe('就绪');
      expect(statusLabel('custom')).toBe('custom');
    });
  });

  describe('wikiNodeColor', () => {
    it('uses community color when colorByCommunity', () => {
      const c = wikiNodeColor({ status: 'published', community: 2, colorByCommunity: true, colorGroups: [], label: '页', docTitle: null, dirName: null });
      expect(c).toMatch(/^#/);
    });
    it('uses type color when not by community', () => {
      const c = wikiNodeColor({ status: 'published', community: 0, colorByCommunity: false, colorGroups: [], label: '概念页', docTitle: null, dirName: '概念' });
      expect(c).toBe('#7cc0ff'); // 概念类型色
    });
    it('color groups take priority', () => {
      const c = wikiNodeColor({ status: 'published', community: 0, colorByCommunity: false, colorGroups: [{ query: '架构', color: '#123456' }], label: '架构设计', docTitle: null, dirName: null });
      expect(c).toBe('#123456');
    });
  });

  describe('wikiNodeTooltip', () => {
    it('builds type · status · source · degree tooltip', () => {
      const t = wikiNodeTooltip({ label: '架构设计', status: 'published', docTitle: '架构文档.md', dirName: null, inDegree: 3, outDegree: 2 });
      expect(t).toContain('页面');      // 类型
      expect(t).toContain('已创建');     // 状态
      expect(t).toContain('来源：架构文档.md');
      expect(t).toContain('入链 3 · 出链 2');
    });
    it('shows missing/draft statuses', () => {
      expect(wikiNodeTooltip({ label: '幽灵', status: 'missing', docTitle: null, dirName: null, inDegree: 0, outDegree: 0 })).toContain('尚未创建');
      expect(wikiNodeTooltip({ label: '草稿页', status: 'draft', docTitle: null, dirName: '实体', inDegree: 1, outDegree: 0 })).toContain('草稿');
    });
  });
});
