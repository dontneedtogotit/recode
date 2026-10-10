import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import ProjectView from './ProjectView';

const workspace = { id: 'demo', root: '/app-data/projects/demo' };

afterEach(() => {
  delete (globalThis as { __TAURI__?: unknown }).__TAURI__;
  window.localStorage.clear();
});

describe('managed workspace selection', () => {
  it('keeps the brief usable when local storage is blocked', async () => {
    vi.mocked(window.localStorage.getItem).mockImplementation(() => { throw new Error('Storage blocked'); });
    vi.mocked(window.localStorage.setItem).mockImplementation(() => { throw new Error('Storage blocked'); });
    const onRunGoal = vi.fn();
    render(<ProjectView onOpenRunner={vi.fn()} onWorkspaceSelected={vi.fn()} runState={{ status: 'idle' }} onRunGoal={onRunGoal} loading={false} />);
    fireEvent.change(screen.getByLabelText('Pipeline goal'), { target: { value: 'build offline' } });
    await waitFor(() => expect(screen.getByRole('button', { name: 'Simulate plan' })).toBeEnabled());
    fireEvent.click(screen.getByRole('button', { name: 'Simulate plan' }));
    expect(onRunGoal).toHaveBeenCalledWith('build offline');
    expect(screen.getByText(/Local storage unavailable/)).toBeInTheDocument();
  });

  it('reports an unsuccessful build outcome rather than calling it a simulation', async () => {
    render(<ProjectView onOpenRunner={vi.fn()} onWorkspaceSelected={vi.fn()} runState={{
      status: 'completed', goal: 'Build app', planId: 'blocked',
      outcome: { success: false, degraded: true, generated_file_count: 2, failed_file_count: 0, summary: 'Verification missing' },
    }} onRunGoal={vi.fn()} loading={false} />);
    await act(async () => {});
    expect(screen.getByText(/Build failed for plan blocked/)).toBeInTheDocument();
    expect(screen.queryByText(/no project was generated/)).not.toBeInTheDocument();
  });

  it('labels the current pipeline as simulation, not generated-project completion', async () => {
    render(<ProjectView onOpenRunner={vi.fn()} onWorkspaceSelected={vi.fn()} runState={{ status: 'completed', goal: 'Build app', planId: 'plan-1' }} onRunGoal={vi.fn()} loading={false} />);
    await act(async () => { await Promise.resolve(); });
    expect(screen.getByRole('button', { name: 'Simulate plan' })).toBeInTheDocument();
    expect(screen.getByText(/no project was generated/)).toBeInTheDocument();
  });

  it('labels a verified file-generation outcome as generation rather than simulation', async () => {
    render(<ProjectView onOpenRunner={vi.fn()} onWorkspaceSelected={vi.fn()} runState={{
      status: 'completed', goal: 'Build app', planId: 'plan-2',
      outcome: { success: true, degraded: false, generated_file_count: 2, failed_file_count: 0, summary: 'generated 2 of 2 files' },
    }} onRunGoal={vi.fn()} loading={false} />);
    await act(async () => {});
    expect(screen.getByText('Generated 2 files for plan plan-2')).toBeInTheDocument();
    expect(screen.queryByText(/no project was generated/)).not.toBeInTheDocument();
  });

  it('resolves a legacy saved selection by ID instead of trusting its stored root', async () => {
    window.localStorage.setItem('recode.selectedWorkspace', JSON.stringify({ id: 'demo', root: '/etc' }));
    const invoke = vi.fn().mockResolvedValue([workspace]);
    const onWorkspaceSelected = vi.fn();
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };

    render(<ProjectView onOpenRunner={vi.fn()} onWorkspaceSelected={onWorkspaceSelected} runState={{ status: 'idle' }} onRunGoal={vi.fn()} loading={false} />);

    await waitFor(() => expect(screen.getAllByText('/app-data/projects/demo').length).toBeGreaterThan(0));
    expect(screen.queryByText('/etc')).not.toBeInTheDocument();
    expect(screen.getByText(/requires RECODE_OPENAI_BASE_URL and RECODE_OPENAI_MODEL/)).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith('list_workspaces');
    expect(onWorkspaceSelected).toHaveBeenCalledWith('demo');
    fireEvent.change(screen.getByLabelText('Select workspace'), { target: { value: '' } });
    expect(onWorkspaceSelected).toHaveBeenLastCalledWith(null);
  });

  it('creates workspaces using only their ID and reloads the authoritative list', async () => {
    const invoke = vi.fn()
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce(workspace)
      .mockResolvedValueOnce([workspace]);
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };

    render(<ProjectView onOpenRunner={vi.fn()} onWorkspaceSelected={vi.fn()} runState={{ status: 'idle' }} onRunGoal={vi.fn()} loading={false} />);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('list_workspaces'));
    fireEvent.change(screen.getByPlaceholderText('my-project'), { target: { value: 'demo' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create workspace' }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith('create_workspace', { input: { id: 'demo' } }));
    await waitFor(() => expect(screen.getAllByText('/app-data/projects/demo').length).toBeGreaterThan(0));
    expect(window.localStorage.getItem('recode.selectedWorkspace')).toBe('demo');
  });

  it('loads a recent goal for review without starting an unexpected run', async () => {
    window.localStorage.setItem('recode.recentGoals', JSON.stringify(['build a new dashboard']));
    const onRunGoal = vi.fn();
    render(<ProjectView onOpenRunner={vi.fn()} onWorkspaceSelected={vi.fn()} runState={{ status: 'idle' }} onRunGoal={onRunGoal} loading={false} />);
    fireEvent.change(screen.getByPlaceholderText('Build onboarding flow'), { target: { value: 'different goal' } });
    fireEvent.click(screen.getByRole('button', { name: 'build a new dashboard' }));
    expect(onRunGoal).not.toHaveBeenCalled();
    expect(screen.getByLabelText('Pipeline goal')).toHaveValue('build a new dashboard');
    await waitFor(() => expect(screen.getByRole('button', { name: 'Simulate plan' })).toBeEnabled());
    fireEvent.click(screen.getByRole('button', { name: 'Simulate plan' }));
    expect(onRunGoal).toHaveBeenCalledWith('build a new dashboard');
  });

  it('inserts a starter brief without executing it', async () => {
    const onRunGoal = vi.fn();
    render(<ProjectView onOpenRunner={vi.fn()} onWorkspaceSelected={vi.fn()} runState={{ status: 'idle' }} onRunGoal={onRunGoal} loading={false} />);
    await act(async () => {});
    fireEvent.click(screen.getByRole('button', { name: /Web application/ }));
    expect((screen.getByLabelText('Pipeline goal') as HTMLTextAreaElement).value).toContain('responsive web application');
    expect(onRunGoal).not.toHaveBeenCalled();
  });

  it('does not launch a second run through keyboard submission', async () => {
    const onRunGoal = vi.fn();
    render(<ProjectView onOpenRunner={vi.fn()} onWorkspaceSelected={vi.fn()} runState={{ status: 'running', goal: 'active' }} onRunGoal={onRunGoal} loading={true} />);
    await act(async () => {});
    const input = screen.getByLabelText('Pipeline goal');
    fireEvent.change(input, { target: { value: 'new work' } });
    fireEvent.keyDown(input, { key: 'Enter', ctrlKey: true });
    expect(onRunGoal).not.toHaveBeenCalled();
  });

  it('keeps plain Enter available for a multiline brief', async () => {
    const onRunGoal = vi.fn();
    render(<ProjectView onOpenRunner={vi.fn()} onWorkspaceSelected={vi.fn()} runState={{ status: 'idle' }} onRunGoal={onRunGoal} loading={false} />);
    const input = screen.getByLabelText('Pipeline goal');
    fireEvent.change(input, { target: { value: 'first requirement' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    await act(async () => {});
    expect(onRunGoal).not.toHaveBeenCalled();
  });

  it('ignores malformed recent-goal data', async () => {
    window.localStorage.setItem('recode.recentGoals', '{"invalid":true}');
    expect(() => render(<ProjectView onOpenRunner={vi.fn()} onWorkspaceSelected={vi.fn()} runState={{ status: 'idle' }} onRunGoal={vi.fn()} loading={false} />)).not.toThrow();
    await act(async () => {});
  });
});
