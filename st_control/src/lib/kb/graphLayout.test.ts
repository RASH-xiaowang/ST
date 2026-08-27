import { describe, it, expect } from 'vitest';
import { radialTreeLayout, matchGlob, type RadialLayoutParams } from './graphLayout';
import type { WikiGraph } from './kbTypes';

const params: RadialLayoutParams = { forceRepulsion: 500, forceAttraction: 0.08, forceEdgeLength: 1, forceCentripetal: 0.1, nodeScale: 1 };

function node(id: number, title: string, inD: number, outD: number) {
  return { id, pageId: id, title, docId: null, docTitle: null, dirName: null, inDegree: inD, outDegree: outD, status: 'published' };
}

describe('graphLayout', () => {
  describe('radialTreeLayout', () => {
    it('centers a single node', () => {
      const g: WikiGraph = { nodes: [node(1, 'A', 0, 0)], edges: [] };
      const pos = radialTreeLayout(g, 800, 600, params);
      expect(pos[1]).toEqual({ x: 400, y: 300 });
    });

    it('positions all nodes and chooses highest-degree root', () => {
      const g: WikiGraph = {
        nodes: [node(1, 'A', 0, 0), node(2, 'B', 1, 0), node(3, 'C', 1, 0)],
        edges: [
          { from: 2, to: 1, linkType: 'wiki', weight: 1 },
          { from: 3, to: 1, linkType: 'wiki', weight: 1 },
        ],
      };
      const pos = radialTreeLayout(g, 800, 600, params);
      expect(Object.keys(pos).length).toBe(3);
      // 根节点（连接度最高者）应位于中心附近；两个高连接度节点中恰有一个居中
      const centered = Object.values(pos).filter((p) => Math.abs(p.x - 400) < 2 && Math.abs(p.y - 300) < 2);
      expect(centered.length).toBe(1);
      // 其他节点分布在画布内
      for (const nd of g.nodes) {
        expect(pos[nd.id].x).toBeGreaterThanOrEqual(0);
        expect(pos[nd.id].x).toBeLessThanOrEqual(800);
        expect(pos[nd.id].y).toBeGreaterThanOrEqual(0);
        expect(pos[nd.id].y).toBeLessThanOrEqual(600);
      }
    });

    it('handles empty edge list (isolated nodes hang under root)', () => {
      const g: WikiGraph = { nodes: [node(1, 'A', 0, 0), node(2, 'B', 0, 0)], edges: [] };
      const pos = radialTreeLayout(g, 400, 400, params);
      expect(Object.keys(pos).length).toBe(2);
    });
  });

  describe('matchGlob', () => {
    it('matches literal title case-insensitively', () => {
      expect(matchGlob('分片策略', '分片策略')).toBe(true);
      expect(matchGlob('Quick Start', 'quick start')).toBe(true);
    });
    it('matches star wildcard', () => {
      expect(matchGlob('快速上手指南', '*')).toBe(true);
      expect(matchGlob('快速上手指南', '快速*')).toBe(true);
      expect(matchGlob('快速上手指南', '指南*')).toBe(false);
    });
    it('escapes regex special characters in pattern', () => {
      expect(matchGlob('a[b]', 'a[b]')).toBe(true);
      expect(matchGlob('a.b', 'a.b')).toBe(true);
    });
  });
});
