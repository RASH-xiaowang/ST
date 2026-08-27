// @vitest-environment jsdom
// KbChat（AI 问答中枢）组件测试：落地页推荐、检索结果渲染（mock IPC）。
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor, fireEvent } from '@testing-library/svelte';

const mocks = vi.hoisted(() => ({
  searchHistory: vi.fn(),
  recommendQuestions: vi.fn(),
  getDefaultChatModel: vi.fn(),
  search: vi.fn(),
  listSessions: vi.fn(),
  listMessages: vi.fn(),
  createSession: vi.fn(),
  ragStreamWithChannel: vi.fn(),
  getDefaultModel: vi.fn(),
  listModels: vi.fn(),
  track: vi.fn(),
}));

vi.mock('./services/ipc', () => ({
  kbApi: {
    searchHistory: mocks.searchHistory,
    recommendQuestions: mocks.recommendQuestions,
    getDefaultChatModel: mocks.getDefaultChatModel,
    search: mocks.search,
    listSessions: mocks.listSessions,
    listMessages: mocks.listMessages,
    createSession: mocks.createSession,
    ragStreamWithChannel: mocks.ragStreamWithChannel,
    getDefaultModel: mocks.getDefaultModel,
    listModels: mocks.listModels,
  },
}));

vi.mock('./analytics.svelte', () => ({ track: mocks.track }));
vi.mock('./KbConfirm.svelte', () => ({ kbConfirm: vi.fn(async () => true) }));

import KbChat from './KbChat.svelte';

const KBS = [{ id: 1, name: '产品知识库', description: null, owner_id: 1, pinned: false, isSystem: false, docCount: 3, created_at: '2026-08-01T00:00:00Z' }];
const MODELS = [{ providerId: 'demo', providerName: '演示服务', model: 'demo-chat', isDefault: true, modelType: '对话' }];
const RECOS = [
  { type: 'faq', question: '如何创建知识库？' },
  { type: 'query', question: '支持哪些文档格式？' },
];
const RESULTS = [
  { chunk_id: 1, doc_id: 101, kb_id: 1, content: '产品支持 Markdown 渲染。', page_no: 1, section: null, score: 0.92, source: 'upload', doc_title: '快速上手指南' },
];

beforeEach(() => {
  Object.values(mocks).forEach((f) => f.mockReset());
  mocks.searchHistory.mockResolvedValue([]);
  mocks.recommendQuestions.mockResolvedValue(RECOS);
  mocks.getDefaultChatModel.mockResolvedValue(['demo', 'demo-chat']);
  mocks.search.mockResolvedValue(RESULTS);
  mocks.listSessions.mockResolvedValue([]);
  mocks.listMessages.mockResolvedValue([]);
});

function props(overrides: Record<string, unknown> = {}) {
  return {
    selectedKb: 1,
    kbs: KBS,
    notify: vi.fn(),
    models: MODELS,
    openSession: null,
    onSessionsChanged: vi.fn(),
    onOpenDoc: vi.fn(),
    ...overrides,
  };
}

describe('KbChat', () => {
  it('renders landing with recommended questions', async () => {
    const { container } = render(KbChat, { props: props() });
    await waitFor(() => expect(mocks.recommendQuestions).toHaveBeenCalled());
    await waitFor(() => expect(container.textContent).toContain('如何创建知识库？'));
    expect(container.textContent).toContain('支持哪些文档格式？');
  });

  it('runs search from landing input and renders results', async () => {
    const { container } = render(KbChat, { props: props() });
    await waitFor(() => expect(mocks.recommendQuestions).toHaveBeenCalled());

    const textarea = container.querySelector('.kb-chat-landing textarea') as HTMLTextAreaElement;
    expect(textarea).not.toBeNull();
    await fireEvent.input(textarea!, { target: { value: 'Markdown 渲染' } });
    await waitFor(() => expect((container.querySelector('.kb-chat-send') as HTMLButtonElement).disabled).toBe(false));

    await fireEvent.click(container.querySelector('.kb-chat-send') as HTMLButtonElement);
    await waitFor(() => expect(mocks.search).toHaveBeenCalled());
    await waitFor(() => expect(container.textContent).toContain('产品支持 Markdown 渲染。'));
  });
});
