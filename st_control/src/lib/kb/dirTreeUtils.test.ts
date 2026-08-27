import { describe, it, expect } from 'vitest';
import { buildDirSubtree, buildDirTree, filterPagesByDir } from './dirTreeUtils';
import type { WikiDir, WikiPageItem } from './kbTypes';

describe('dirTreeUtils', () => {
  const dirs: WikiDir[] = [
    { id: 1, parentId: null, name: 'Root', count: 5 },
    { id: 2, parentId: 1, name: 'Child A', count: 3 },
    { id: 3, parentId: 1, name: 'Child B', count: 2 },
    { id: 4, parentId: 2, name: 'Grandchild', count: 1 },
  ];

  describe('buildDirSubtree', () => {
    it('includes self in subtree', () => {
      const subtree = buildDirSubtree(dirs);
      expect(subtree.get(1)?.has(1)).toBe(true);
    });
    it('includes children', () => {
      const subtree = buildDirSubtree(dirs);
      expect(subtree.get(1)?.has(2)).toBe(true);
      expect(subtree.get(1)?.has(3)).toBe(true);
    });
    it('includes grandchildren', () => {
      const subtree = buildDirSubtree(dirs);
      expect(subtree.get(1)?.has(4)).toBe(true);
    });
    it('leaf node only has self', () => {
      const subtree = buildDirSubtree(dirs);
      expect(subtree.get(4)?.size).toBe(1);
    });
  });

  describe('buildDirTree', () => {
    it('returns pre-order traversal', () => {
      const tree = buildDirTree(dirs);
      expect(tree.map((d) => d.id)).toEqual([1, 2, 4, 3]);
    });
    it('sets correct depth', () => {
      const tree = buildDirTree(dirs);
      expect(tree.find((d) => d.id === 1)?.depth).toBe(0);
      expect(tree.find((d) => d.id === 2)?.depth).toBe(1);
      expect(tree.find((d) => d.id === 4)?.depth).toBe(2);
    });
    it('handles empty', () => {
      expect(buildDirTree([])).toEqual([]);
    });
  });

  describe('filterPagesByDir', () => {
    const pages: WikiPageItem[] = [
      { id: 1, kbId: 1, dirId: 1, docId: null, docTitle: null, title: 'P1', slug: 'p1', summary: '', status: 'published', outLinks: 0, inLinks: 0, entityCount: 0, createdAt: '', updatedAt: '' },
      { id: 2, kbId: 1, dirId: 2, docId: null, docTitle: null, title: 'P2', slug: 'p2', summary: '', status: 'published', outLinks: 0, inLinks: 0, entityCount: 0, createdAt: '', updatedAt: '' },
      { id: 3, kbId: 1, dirId: 3, docId: null, docTitle: null, title: 'P3', slug: 'p3', summary: '', status: 'published', outLinks: 0, inLinks: 0, entityCount: 0, createdAt: '', updatedAt: '' },
      { id: 4, kbId: 1, dirId: null, docId: null, docTitle: null, title: 'P4', slug: 'p4', summary: '', status: 'published', outLinks: 0, inLinks: 0, entityCount: 0, createdAt: '', updatedAt: '' },
    ];
    const subtree = buildDirSubtree(dirs);

    it('returns all when no filter', () => {
      expect(filterPagesByDir(pages, null, subtree)).toHaveLength(4);
    });
    it('filters by dir subtree', () => {
      const filtered = filterPagesByDir(pages, 1, subtree);
      // dirId 1, 2, 3 are under dir 1 (via subtree), dirId null is excluded
      expect(filtered).toHaveLength(3);
    });
    it('filters leaf dir', () => {
      const filtered = filterPagesByDir(pages, 2, subtree);
      // dirId 2 and 4 are under dir 2
      expect(filtered.map((p) => p.id)).toEqual([2]);
    });
    it('excludes pages with null dirId', () => {
      const filtered = filterPagesByDir(pages, 1, subtree);
      expect(filtered.some((p) => p.dirId === null)).toBe(false);
    });
  });
});
