// @vitest-environment jsdom
// KbDocs（文档管理中心）组件测试：文档列表渲染、空态。
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, waitFor } from '@testing-library/svelte';

const mocks = vi.hoisted(() => ({
  listDocuments: vi.fn(),
  listTags: vi.fn(),
  getDocument: vi.fn(),
  listVersions: vi.fn(),
  listDirs: vi.fn(),
}));

vi.mock('./services/ipc', () => ({
  kbApi: {
    listDocuments: mocks.listDocuments,
    listTags: mocks.listTags,
    getDocument: mocks.getDocument,
    listVersions: mocks.listVersions,
    listDirs: mocks.listDirs,
  },
}));

vi.mock('./KbConfirm.svelte', () => ({ kbConfirm: vi.fn(async () => true) }));

import KbDocs from './KbDocs.svelte';

const DOCS = [
  { id: 101, title: '快速上手指南', fileType: 'md', status: 'ready', processStatus: 'done', createdAt: '2026-08-01T00:00:00Z', updatedAt: '2026-08-02T00:00:00Z', fileSize: 24576, source: 'upload', tags: ['入门'], snippet: '本指南……' },
  { id: 102, title: 'API 参考手册', fileType: 'pdf', status: 'ready', processStatus: 'done', createdAt: '2026-08-01T00:00:00Z', fileSize: 1048576, source: 'upload', tags: ['API'], snippet: 'REST API……' },
];

let unmounts: Array<() => void> = [];

beforeEach(() => {
  Object.values(mocks).forEach((f) => f.mockReset());
  mocks.listDocuments.mockResolvedValue({ items: DOCS, total: 2 });
  mocks.listTags.mockResolvedValue(['入门', 'API']);
  mocks.getDocument.mockResolvedValue({ meta: DOCS[0], content: '', chunks: [] });
  mocks.listVersions.mockResolvedValue([]);
  mocks.listDirs.mockResolvedValue([]);
  unmounts = [];
});

afterEach(() => {
  unmounts.forEach((u) => u());
});

function props(overrides: Record<string, unknown> = {}) {
  return {
    selectedKb: 1,
    notify: vi.fn(),
    refreshKbs: vi.fn(async () => {}),
    selProvider: '',
    selModel: '',
    onTotalDocs: vi.fn(),
    openDocId: null,
    searchInit: null,
    ...overrides,
  };
}

describe('KbDocs', () => {
  it('renders document list from kbApi', async () => {
    const { container, unmount } = render(KbDocs, { props: props() });
    unmounts.push(unmount);
    await waitFor(() => expect(mocks.listDocuments).toHaveBeenCalled());
    await waitFor(() => {
      expect(container.textContent).toContain('快速上手指南');
      expect(container.textContent).toContain('API 参考手册');
    });
  });

  it('shows empty state when no documents', async () => {
    mocks.listDocuments.mockResolvedValue({ items: [], total: 0 });
    const { container, unmount } = render(KbDocs, { props: props() });
    unmounts.push(unmount);
    await waitFor(() => expect(mocks.listDocuments).toHaveBeenCalled());
    await waitFor(() => {
      const text = container.textContent ?? '';
      expect(text).toMatch(/暂无文档|还没有文档|拖拽文件/);
    });
  });
});
