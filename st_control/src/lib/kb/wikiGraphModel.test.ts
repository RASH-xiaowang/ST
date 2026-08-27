import { describe, it, expect } from 'vitest';
import { buildWikiGraph, communityColor, COMMUNITY_COLORS, type WikiGraphBuildParams } from './wikiGraphModel';
import type { WikiGraph } from './kbTypes';

const params: WikiGraphBuildParams = { nodeScale: 1, forceEdgeLength: 1, showImplicit: true };

const graph: WikiGraph = {
  nodes: [
    { id: 11, pageId: 11, title: '快速上手指南', docId: 101, docTitle: '快速上手指南', dirName: null, inDegree: 1, outDegree: 1, status: 'published' },
    { id: 12, pageId: 12, title: '常见问题', docId: 103, docTitle: '常见问题 FAQ', dirName: null, inDegree: 1, outDegree: 0, status: 'published' },
    { id: 99, pageId: -1, title: '缺失页面', docId: null, docTitle: null, dirName: null, inDegree: 0, outDegree: 1, status: 'missing' },
  ],
  edges: [
    { from: 11, to: 12, linkType: 'wiki', weight: 1 },
    { from: 11, to: -1, linkType: 'entity', weight: 2 },
  ],
};

describe('wikiGraphModel', () => {
  describe('communityColor', () => {
    it('returns neutral gray for ungrouped communities', () => {
      expect(communityColor(-1)).toBe('#9aa0a6');
    });
    it('cycles through the palette', () => {
      expect(communityColor(0)).toBe(COMMUNITY_COLORS[0]);
      expect(communityColor(COMMUNITY_COLORS.length)).toBe(COMMUNITY_COLORS[0]);
    });
  });

  describe('buildWikiGraph', () => {
    it('returns empty result for null or empty graph', () => {
      expect(buildWikiGraph(null, new Set(), params)).toEqual({ nodes: [], edges: [], communityCount: 0 });
      expect(buildWikiGraph({ nodes: [], edges: [] }, new Set(), params).nodes).toHaveLength(0);
    });

    it('adds fixed center node and keeps visible pages + ghosts', () => {
      const r = buildWikiGraph(graph, new Set([11, 12, -1]), params, '产品知识库');
      const center = r.nodes.find((n) => n.pageId === 0);
      expect(center?.label).toBe('产品知识库');
      expect(center?.fx).toBe(0);
      expect(center?.fy).toBe(0);
      expect(r.nodes).toHaveLength(4); // center + 3 visible pages
      expect(r.nodes.some((n) => n.pageId === -1)).toBe(true); // 幽灵节点参与布局
    });

    it('filters nodes by visibleIds and drops edges with invisible ends', () => {
      const r = buildWikiGraph(graph, new Set([11]), params);
      const ids = r.nodes.map((n) => n.pageId);
      expect(ids).toContain(0);
      expect(ids).toContain(11);
      expect(ids).not.toContain(12);
      expect(ids).not.toContain(-1);
      // 页面间边两端都必须可见：11->12 与 11->-1 均被丢弃
      expect(r.edges.filter((e) => e.linkType !== 'center')).toHaveLength(0);
    });

    it('connects every page to the center node', () => {
      const r = buildWikiGraph(graph, new Set([11, 12]), params);
      const centerEdges = r.edges.filter((e) => e.linkType === 'center');
      expect(centerEdges).toHaveLength(2);
    });

    it('drops implicit entity edges when showImplicit=false', () => {
      const r = buildWikiGraph(graph, new Set([11, -1]), { ...params, showImplicit: false });
      expect(r.edges.some((e) => e.linkType === 'entity')).toBe(false);
      expect(r.edges.some((e) => e.linkType === 'center')).toBe(true);
    });

    it('keeps entity edges when showImplicit=true', () => {
      const r = buildWikiGraph(graph, new Set([11, -1]), params);
      expect(r.edges.some((e) => e.linkType === 'entity')).toBe(true);
    });
  });
});
