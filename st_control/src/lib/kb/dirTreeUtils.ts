/* ============================================================
 * 知识库 — Wiki 目录树纯算法工具
 * 自 WikiPanel.svelte 下沉：子孙目录集合 / 有序树列表 / 按目录过滤。
 * ============================================================ */
import type { WikiDir, WikiDirTreeItem, WikiPageItem } from './kbTypes';

/** 每个目录的子孙目录 id 集合（含自身），用于「按目录筛选」与计数口径一致 */
export function buildDirSubtree(dirs: WikiDir[]): Map<number, Set<number>> {
  const byParent = new Map<number | null, number[]>();
  for (const d of dirs) {
    const k = d.parentId ?? null;
    const arr = byParent.get(k) ?? [];
    arr.push(d.id);
    byParent.set(k, arr);
  }
  const out = new Map<number, Set<number>>();
  const collect = (id: number): Set<number> => {
    const set = new Set<number>([id]);
    for (const c of byParent.get(id) ?? []) {
      for (const x of collect(c)) set.add(x);
    }
    return set;
  };
  for (const d of dirs) out.set(d.id, collect(d.id));
  return out;
}

/** 扁平目录 → 有序树列表（前序展开，同级保持输入顺序） */
export function buildDirTree(dirs: WikiDir[]): WikiDirTreeItem[] {
  const out: WikiDirTreeItem[] = [];
  const byParent = new Map<number | null, WikiDir[]>();
  for (const d of dirs) {
    const k = d.parentId ?? null;
    const arr = byParent.get(k) ?? [];
    arr.push(d);
    byParent.set(k, arr);
  }
  const walk = (parent: number | null, depth: number) => {
    for (const d of byParent.get(parent) ?? []) {
      out.push({ id: d.id, name: d.name, count: d.count, depth });
      walk(d.id, depth + 1);
    }
  };
  walk(null, 0);
  return out;
}

/** 按目录过滤页面：dirFilter 为 null 时不过滤；否则保留 dirId 属于该目录子树（含自身）的页面 */
export function filterPagesByDir(
  pages: WikiPageItem[],
  dirFilter: number | null,
  dirSubtree: Map<number, Set<number>>,
): WikiPageItem[] {
  if (dirFilter === null) return pages;
  return pages.filter(
    (p) => p.dirId !== null && (dirSubtree.get(dirFilter)?.has(p.dirId) ?? false),
  );
}
