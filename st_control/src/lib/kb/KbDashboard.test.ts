// @vitest-environment jsdom
// KbDashboard 组件回归测试：props 异步就绪（首帧后 kbs 从 [] 变为数据）时，
// 卡片网格必须正常渲染——曾因 $derived 捕获初始空数组导致「有数据却显示空态」。
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor } from '@testing-library/svelte';
import KbDashboard from './KbDashboard.svelte';
import type { KbSummary } from './kbTypes';

// jsdom 缺少这些浏览器 API，KbDashboard 依赖的 UI 组件会用到
beforeEach(() => {
  (globalThis as unknown as Record<string, unknown>).ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
  (globalThis as unknown as Record<string, unknown>).IntersectionObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
  (globalThis as unknown as Record<string, unknown>).matchMedia =
    (globalThis as unknown as Record<string, unknown>).matchMedia ??
    (() => ({ matches: false, addEventListener() {}, removeEventListener() {} }));
});

type DashboardProps = {
  kbs: KbSummary[];
  selectedKb: number | null;
  refreshKbs: () => Promise<void>;
  onOpenKb: (id: number) => void;
  onNewKb?: () => void;
  onImportKb?: (e: Event) => void;
  onEditKb: (kb: KbSummary) => void;
  onDeleteKb: (kb: KbSummary) => void;
  onTogglePin: (kb: KbSummary) => void;
  onExportKb?: (kb: KbSummary) => void;
  notify: (msg: string, type?: 'success' | 'error' | 'warn') => void;
  mode?: 'full' | 'kbs';
  isAdmin?: boolean;
};

function makeProps(overrides: Partial<DashboardProps> = {}): DashboardProps {
  const base: DashboardProps = {
    kbs: [],
    selectedKb: null,
    refreshKbs: vi.fn(async () => {}),
    onOpenKb: vi.fn(),
    onNewKb: vi.fn(),
    onImportKb: vi.fn(),
    onEditKb: vi.fn(),
    onDeleteKb: vi.fn(),
    onTogglePin: vi.fn(),
    onExportKb: vi.fn(),
    notify: vi.fn(),
    mode: 'kbs',
    isAdmin: false,
  };
  return { ...base, ...overrides };
}

const KB: KbSummary = {
  id: 1,
  name: '产品知识库',
  description: null,
  owner_id: 1,
  pinned: false,
  isSystem: false,
  docCount: 3,
  created_at: '2026-08-01T00:00:00Z',
};

describe('KbDashboard', () => {
  it('renders empty state when no kbs', () => {
    const { container } = render(KbDashboard, { props: makeProps() });
    expect(container.querySelector('.kb-kb-card-full')).toBeNull();
  });

  it('renders KB cards when kbs prop arrives after mount (async first-load regression)', async () => {
    const { container, rerender } = render(KbDashboard, { props: makeProps() });
    // 首帧：无数据 → 空态
    expect(container.querySelector('.kb-kb-card-full')).toBeNull();

    // props 异步就绪后更新（模拟首帧后数据到达）
    await rerender({ kbs: [KB] });

    await waitFor(() => {
      expect(container.querySelectorAll('.kb-kb-card-full').length).toBe(1);
    });
    expect(container.textContent).toContain('产品知识库');
  });

  it('filters cards by keyword', async () => {
    const { container } = render(KbDashboard, {
      props: makeProps({ kbs: [KB, { ...KB, id: 2, name: '技术 Wiki' }] }),
    });
    await waitFor(() => expect(container.querySelectorAll('.kb-kb-card-full').length).toBe(2));

    const input = container.querySelector('input[placeholder*="搜索知识库"]') as HTMLInputElement;
    await input!;
    // 触发 Svelte bind:value
    input.value = '技术';
    input.dispatchEvent(new (globalThis as any).Event('input'));
    await waitFor(() => expect(container.querySelectorAll('.kb-kb-card-full').length).toBe(1));
  });
});
