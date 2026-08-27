// @vitest-environment jsdom
// KbSettings（设置页）组件测试：模型配置渲染（推理/Embeddings 模型选择）。
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor } from '@testing-library/svelte';

const mocks = vi.hoisted(() => ({
  getModelSettings: vi.fn(),
  getAnalyticsSettings: vi.fn(),
  getRagSystemPrompt: vi.fn(),
  getChunkSettings: vi.fn(),
  listUsers: vi.fn(),
  listAuditLogs: vi.fn(),
  testModel: vi.fn(),
  setModelSettings: vi.fn(),
  setRagSystemPrompt: vi.fn(),
  setChunkSettings: vi.fn(),
  setAnalyticsSettings: vi.fn(),
}));

vi.mock('./services/ipc', () => ({
  kbApi: {
    getModelSettings: mocks.getModelSettings,
    getAnalyticsSettings: mocks.getAnalyticsSettings,
    getRagSystemPrompt: mocks.getRagSystemPrompt,
    getChunkSettings: mocks.getChunkSettings,
    listUsers: mocks.listUsers,
    listAuditLogs: mocks.listAuditLogs,
    testModel: mocks.testModel,
    setModelSettings: mocks.setModelSettings,
    setRagSystemPrompt: mocks.setRagSystemPrompt,
    setChunkSettings: mocks.setChunkSettings,
    setAnalyticsSettings: mocks.setAnalyticsSettings,
  },
}));

vi.mock('./KbConfirm.svelte', () => ({ kbConfirm: vi.fn(async () => true) }));

import KbSettings from './KbSettings.svelte';

const MODELS = [
  { providerId: 'demo', providerName: '演示服务', model: 'demo-chat', isDefault: true, modelType: '对话' },
  { providerId: 'demo', providerName: '演示服务', model: 'demo-embed', isDefault: false, modelType: '嵌入' },
];

beforeEach(() => {
  Object.values(mocks).forEach((f) => f.mockReset());
  mocks.getModelSettings.mockResolvedValue({ inference: { providerId: 'demo', model: 'demo-chat' }, embedding: { providerId: 'demo', model: 'demo-embed' }, parsing: null, rerank: null });
  mocks.getAnalyticsSettings.mockResolvedValue([]);
  mocks.getRagSystemPrompt.mockResolvedValue('默认提示词');
  mocks.getChunkSettings.mockResolvedValue({ strategy: 'recursive', size: 800, overlap: 128, vectorScanCap: 500 });
  mocks.listUsers.mockResolvedValue([]);
  mocks.listAuditLogs.mockResolvedValue([]);
});

function props(overrides: Record<string, unknown> = {}) {
  return { models: MODELS, setModel: vi.fn(), notify: vi.fn(), isAdmin: true, ...overrides };
}

describe('KbSettings', () => {
  it('renders model config with inference and embedding selections', async () => {
    const { container } = render(KbSettings, { props: props() });
    await waitFor(() => expect(mocks.getModelSettings).toHaveBeenCalled());
    await waitFor(() => {
      const text = container.textContent ?? '';
      expect(text).toContain('推理模型');
      expect(text).toContain('Embeddings');
      expect(text).toContain('demo-chat');
      expect(text).toContain('demo-embed');
    });
  });

  it('renders chunk settings tab', async () => {
    const { container } = render(KbSettings, { props: props() });
    await waitFor(() => expect(mocks.getModelSettings).toHaveBeenCalled());
    // 切换到「分块设置」标签（分块参数来自共享 kbChunkCfg 存储）
    const tab = [...container.querySelectorAll('button')].find((b) => b.textContent?.includes('分块设置'));
    expect(tab).toBeTruthy();
    tab!.click();
    await waitFor(() => {
      const text = container.textContent ?? '';
      expect(text).toContain('分块策略');
    });
  });
});
