import { describe, expect, it, vi, beforeEach } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import AgentChatView from './AgentChatView';
import { executeAgentTask } from '../lib/ipc';
import { listAgents } from '../lib/agentRegistry';

vi.mock('../lib/agentRegistry', () => ({
  listAgents: vi.fn().mockResolvedValue([
    { id: 'planner', name: 'Planner', role: 'planning', description: 'Plans', capabilities: ['planning'] },
    { id: 'reviewer', name: 'Reviewer', role: 'quality', description: 'Reviews', capabilities: ['review'] },
  ]),
}));

vi.mock('../lib/ipc', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../lib/ipc')>();
  return {
    ...actual,
    executeAgentTask: vi.fn().mockImplementation(({ agentId, input }: { agentId: string; input: string }) => Promise.resolve({
      taskId: `${agentId}-task`,
      agentId,
      success: true,
      output: `${agentId}:${input}`,
      error: null,
    })),
  };
});

describe('AgentChatView', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listAgents).mockResolvedValue([
      { id: 'planner', name: 'Planner', role: 'planning', description: 'Plans', capabilities: ['planning'] },
      { id: 'reviewer', name: 'Reviewer', role: 'quality', description: 'Reviews', capabilities: ['review'] },
    ]);
    vi.mocked(executeAgentTask).mockImplementation(async ({ agentId, input }) => ({
      taskId: `${agentId}-task`, agentId, success: true, output: `${agentId}:${input}`, error: null,
    }));
  });

  it('does not send while an IME is composing a message', async () => {
    render(<AgentChatView onOpenConsole={vi.fn()} />);
    await screen.findByDisplayValue('Planner');
    const input = screen.getByLabelText('Message');
    fireEvent.change(input, { target: { value: 'draft' } });
    fireEvent.keyDown(input, { key: 'Enter', isComposing: true });
    expect(executeAgentTask).not.toHaveBeenCalled();
    expect(input).toHaveValue('draft');
  });


  it('keeps the agent label attached to an assistant message after switching agents', async () => {
    render(<AgentChatView onOpenConsole={vi.fn()} />);
    await waitFor(() => expect(screen.getByDisplayValue('Planner')).toBeInTheDocument());
    const input = screen.getByPlaceholderText('Ask the agent to do something... (Shift+Enter for newline)');
    fireEvent.change(input, { target: { value: 'first request' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    await screen.findByText('Agent planner');
    fireEvent.change(screen.getByLabelText('Agent'), { target: { value: 'reviewer' } });
    expect(screen.getByText('Agent planner')).toBeInTheDocument();
  });
});
