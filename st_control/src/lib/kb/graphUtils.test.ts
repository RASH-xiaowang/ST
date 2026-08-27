import { describe, it, expect } from 'vitest';
import { graphNeighborSet, nodeDegreeMap, edgeLinkTypes, visibleNodeIds } from './graphUtils';
import type { WikiGraph, WikiGraphEdge } from './kbTypes';

describe('graphUtils', () => {
  const edges: WikiGraphEdge[] = [
    { from: 1, to: 2, linkType: 'related', weight: 1 },
    { from: 2, to: 3, linkType: 'reference', weight: 1 },
    { from: 1, to: 3, linkType: 'related', weight: 1 },
  ];

  describe('graphNeighborSet', () => {
    it('includes self', () => {
      const set = graphNeighborSet(edges, 1);
      expect(set.has(1)).toBe(true);
    });
    it('includes direct neighbors (out)', () => {
      const set = graphNeighborSet(edges, 1);
      expect(set.has(2)).toBe(true);
      expect(set.has(3)).toBe(true);
    });
    it('includes direct neighbors (in)', () => {
      const set = graphNeighborSet(edges, 3);
      expect(set.has(1)).toBe(true);
      expect(set.has(2)).toBe(true);
    });
    it('handles isolated node', () => {
      const set = graphNeighborSet(edges, 99);
      expect(set.size).toBe(1);
    });
  });

  describe('nodeDegreeMap', () => {
    it('counts degrees correctly', () => {
      const map = nodeDegreeMap(edges);
      expect(map[1]).toBe(2); // connected to 2 and 3
      expect(map[2]).toBe(2); // connected to 1 and 3
      expect(map[3]).toBe(2); // connected to 2 and 1
    });
    it('handles empty edges', () => {
      expect(nodeDegreeMap([])).toEqual({});
    });
  });

  describe('edgeLinkTypes', () => {
    it('returns unique types', () => {
      const types = edgeLinkTypes({ nodes: [], edges });
      expect(types).toContain('related');
      expect(types).toContain('reference');
      expect(types).toHaveLength(2);
    });
    it('returns empty for null graph', () => {
      expect(edgeLinkTypes(null)).toEqual([]);
    });
  });

  describe('visibleNodeIds', () => {
    const graph: WikiGraph = {
      nodes: [
        { id: 1, pageId: 1, title: 'Page A', docId: null, docTitle: null, dirName: null, inDegree: 1, outDegree: 1, status: 'published' },
        { id: 2, pageId: 2, title: 'Page B', docId: null, docTitle: null, dirName: null, inDegree: 1, outDegree: 1, status: 'published' },
        { id: 3, pageId: 3, title: 'Missing', docId: null, docTitle: null, dirName: null, inDegree: 0, outDegree: 0, status: 'missing' },
      ],
      edges,
    };
    const nodeDegree = nodeDegreeMap(edges);

    it('filters by createdOnly', () => {
      const ids = visibleNodeIds(graph, {
        nodeDegree,
        ignorePatterns: [],
        createdOnly: true,
        showOrphans: true,
        query: '',
        localOnly: false,
        anchorId: null,
      });
      expect(ids.has(3)).toBe(false); // missing status
    });
    it('filters by showOrphans', () => {
      // Node 4 is isolated (not in any edge), so degree = 0
      const graphWithOrphan: WikiGraph = {
        nodes: [
          ...graph.nodes,
          { id: 4, pageId: 4, title: 'Orphan', docId: null, docTitle: null, dirName: null, inDegree: 0, outDegree: 0, status: 'published' },
        ],
        edges,
      };
      const deg = nodeDegreeMap(edges);
      const ids = visibleNodeIds(graphWithOrphan, {
        nodeDegree: deg,
        ignorePatterns: [],
        createdOnly: false,
        showOrphans: false,
        query: '',
        localOnly: false,
        anchorId: null,
      });
      expect(ids.has(4)).toBe(false); // orphan excluded
      expect(ids.has(1)).toBe(true);  // connected node kept
    });
    it('filters by query (case sensitive)', () => {
      // visibleNodeIds lowercases title but NOT the query, so use lowercase query
      const ids = visibleNodeIds(graph, {
        nodeDegree,
        ignorePatterns: [],
        createdOnly: false,
        showOrphans: true,
        query: 'page a',
        localOnly: false,
        anchorId: null,
      });
      expect(ids.has(1)).toBe(true);
      expect(ids.has(3)).toBe(false);
    });
    it('returns empty for null graph', () => {
      expect(visibleNodeIds(null, {
        nodeDegree: {},
        ignorePatterns: [],
        createdOnly: false,
        showOrphans: true,
        query: '',
        localOnly: false,
        anchorId: null,
      }).size).toBe(0);
    });
  });
});
