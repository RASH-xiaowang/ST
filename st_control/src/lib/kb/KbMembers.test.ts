// @vitest-environment jsdom
// KbMembers 组件测试：成员列表渲染、空态、非管理员无添加按钮。
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor } from '@testing-library/svelte';

const mocks = vi.hoisted(() => ({
  listMembers: vi.fn(),
  listUsers: vi.fn(),
  addMember: vi.fn(),
  removeMember: vi.fn(),
  updateMemberRole: vi.fn(),
}));

vi.mock('./services/ipc', () => ({
  kbApi: { listMembers: mocks.listMembers, listUsers: mocks.listUsers, addMember: mocks.addMember, removeMember: mocks.removeMember, updateMemberRole: mocks.updateMemberRole },
}));

vi.mock('./KbConfirm.svelte', () => ({ kbConfirm: vi.fn(async () => true) }));

import KbMembers from './KbMembers.svelte';

const MEMBERS = [
  { userId: 1, username: 'admin', displayName: '管理员', role: 'owner' },
  { userId: 2, username: 'zhangsan', displayName: '张三', role: 'editor' },
];

beforeEach(() => {
  mocks.listMembers.mockReset();
  mocks.listUsers.mockReset();
  mocks.addMember.mockReset();
  mocks.removeMember.mockReset();
  mocks.updateMemberRole.mockReset();
  mocks.listMembers.mockResolvedValue(MEMBERS);
  mocks.listUsers.mockResolvedValue([{ id: 3, username: 'lisi', displayName: '李四', isAdmin: false }]);
});

describe('KbMembers', () => {
  it('renders member list from kbApi', async () => {
    const { container } = render(KbMembers, { props: { kbId: 1, isAdmin: true, notify: vi.fn() } });
    await waitFor(() => expect(mocks.listMembers).toHaveBeenCalledWith(1));
    await waitFor(() => {
      expect(container.textContent).toContain('管理员');
      expect(container.textContent).toContain('张三');
    });
  });

  it('shows empty state when no members', async () => {
    mocks.listMembers.mockResolvedValue([]);
    const { container } = render(KbMembers, { props: { kbId: 1, isAdmin: true, notify: vi.fn() } });
    await waitFor(() => expect(container.textContent).toContain('暂无成员'));
  });

  it('does not show add button for non-admin', async () => {
    const { container } = render(KbMembers, { props: { kbId: 1, isAdmin: false, notify: vi.fn() } });
    await waitFor(() => expect(mocks.listMembers).toHaveBeenCalled());
    expect([...container.querySelectorAll('button')].some((b) => b.textContent?.includes('添加成员'))).toBe(false);
  });
});
