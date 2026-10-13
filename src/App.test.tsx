import { describe, expect, it, vi, beforeEach } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import App from './App';
import { useRunPipeline } from './hooks/useRunPipeline';
import type { PipelineRun } from './lib/ipc';

vi.mock('./hooks/useRunPipeline', () => ({
  useRunPipeline: vi.fn(),
}));

vi.mock('./views/AgentChatView', () => ({ default: () => <div>agent</div> }));
vi.mock('./views/RunnerView', () => ({ default: () => <div>runner</div> }));
vi.mock('./views/DiagnosticsView', () => ({ default: () => <div>diagnostics</div> }));
vi.mock('./views/PlansView', () => ({ default: ({ onUsePlan }: { onUsePlan?: (plan: { id: string; goal: string }) => void }) => <div>plans<button onClick={() => onUsePlan?.({ id: 'p-review', goal: 'review this goal' })}>Pick saved goal</button></div> }));
vi.mock('./views/AIView', () => ({ default: () => <div>ai</div> }));
vi.mock('./views/LearningView', () => ({ default: () => <div>learning</div> }));
vi.mock('./views/RunHistoryView', () => ({ default: ({ onReviewGoal }: { onReviewGoal?: (goal: string, runId: string) => void }) => <div>history<button onClick={() => onReviewGoal?.('past goal', 'run-old')}>Pick past run</button></div> }));
vi.mock('./views/TelemetryView', () => ({ default: () => <div>telemetry</div> }));
vi.mock('./views/SettingsView', () => ({ default: () => <div>settings</div> }));
vi.mock('./views/UpdatesView', () => ({ default: () => <div>updates</div> }));

describe('App palette integration', () => {
  const execute = vi.fn().mockResolvedValue(undefined);
  const cancel = vi.fn().mockResolvedValue(true);

  beforeEach(() => {
    vi.clearAllMocks();
    window.localStorage.clear();
    delete (globalThis as { __TAURI__?: unknown }).__TAURI__;
    vi.mocked(useRunPipeline).mockReturnValue({
      execute,
      cancel,
      loading: false,
      result: null,
      events: [],
      activeRunId: null,
      lastOutcome: null,
      lastFailedFiles: [],
    });
  });

  it('renders the primary shell and navigates between views', () => {
    render(<App />);
    expect(screen.getByRole('heading', { name: 'Project' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /Agent/ }));
    expect(screen.getByText('agent')).toBeInTheDocument();
  });

  it('reviews a saved goal without overwriting the current brief or starting a run', async () => {
    window.localStorage.setItem('recode.projectBrief', 'existing draft');
    render(<App />);
    fireEvent.click(screen.getByRole('button', { name: /^Plans/ }));
    fireEvent.click(screen.getByRole('button', { name: 'Pick saved goal' }));
    expect(screen.getByRole('region', { name: 'Review incoming brief' })).toBeInTheDocument();
    expect(screen.getByLabelText('Pipeline goal')).toHaveValue('existing draft');
    expect(screen.getByRole('button', { name: 'Simulate plan' })).toBeDisabled();
    expect(execute).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'Use this brief' }));
    expect(screen.getByLabelText('Pipeline goal')).toHaveValue('review this goal');
    expect(execute).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.getByRole('button', { name: 'Simulate plan' })).toBeEnabled());
    fireEvent.click(screen.getByRole('button', { name: 'Simulate plan' }));
    expect(execute).toHaveBeenCalledWith({ goal: 'review this goal', emit_events: true, execution_consent: 'not-supplied' });
  });

  it('lets a history review be dismissed without changing the current draft', () => {
    window.localStorage.setItem('recode.projectBrief', 'keep this');
    render(<App />);
    fireEvent.click(screen.getByRole('button', { name: /^History/ }));
    fireEvent.click(screen.getByRole('button', { name: 'Pick past run' }));
    expect(screen.getByText(/Run history · run-old/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Keep current brief' }));
    expect(screen.getByLabelText('Pipeline goal')).toHaveValue('keep this');
    expect(execute).not.toHaveBeenCalled();
  });

  it('does not execute the retired history browser event', () => {
    render(<App />);
    fireEvent(window, new CustomEvent('rerun-history-goal', { detail: { goal: 'unexpected execution' } }));
    expect(execute).not.toHaveBeenCalled();
  });

  it('passes palette run actions into the active pipeline hook', () => {
    render(<App />);
    fireEvent.keyDown(window, { key: 'k', ctrlKey: true });
    const input = screen.getByPlaceholderText('Type a view, action, or run command…');
    fireEvent.change(input, { target: { value: 'run ship the feature' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    expect(execute).toHaveBeenCalledWith({ goal: 'ship the feature', emit_events: true, execution_consent: 'not-supplied' });
    expect(screen.getAllByText('Running').length).toBeGreaterThan(0);
  });

  it('does not celebrate a failed build returned with inspectable run details', () => {
    const run: PipelineRun = {
      plan: { id: 'plan-1', goal: 'build', status: 'completed', steps: [] },
      repair_actions: [], events: [],
      failed_files: [{ path: 'src/main.ts', status: 'failed', error: 'write failed' }],
      outcome: {
        success: false, degraded: true, generated_file_count: 0, failed_file_count: 1,
        failed_files: [{ path: 'src/main.ts', status: 'failed', error: 'write failed' }],
        summary: 'generated 0 of 1 files',
      },
    };
    vi.mocked(useRunPipeline).mockReturnValue({
      execute, cancel, loading: false,
      result: { success: false, run, runId: 'run-1', error: 'generated 0 of 1 files' },
      events: [], activeRunId: null, lastOutcome: run.outcome ?? null, lastFailedFiles: run.failed_files ?? [],
    });
    render(<App />);
    expect(screen.getByText('Pipeline failed')).toBeInTheDocument();
    expect(screen.queryByText('Pipeline completed')).not.toBeInTheDocument();
  });

  it('sends only the resolved managed workspace ID with a project run', async () => {
    window.localStorage.setItem('recode.selectedWorkspace', JSON.stringify({ id: 'demo', root: '/etc' }));
    const invoke = vi.fn().mockResolvedValue([{ id: 'demo', root: '/app-data/projects/demo' }]);
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };
    render(<App />);
    await waitFor(() => expect(screen.getAllByText('/app-data/projects/demo').length).toBeGreaterThan(0));
    fireEvent.change(screen.getByPlaceholderText('Build onboarding flow'), { target: { value: 'build a page' } });
    fireEvent.click(screen.getByRole('button', { name: 'Generate files' }));
    expect(execute).toHaveBeenCalledWith({ goal: 'build a page', emit_events: true, execution_consent: 'not-supplied', project_id: 'demo' });
    expect(screen.queryByText('/etc')).not.toBeInTheDocument();
  });

  it('does not send a stale stored workspace ID that the native list cannot resolve', async () => {
    window.localStorage.setItem('recode.selectedWorkspace', 'unregistered');
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke: vi.fn().mockResolvedValue([]) };
    render(<App />);
    await waitFor(() => expect(window.localStorage.getItem('recode.selectedWorkspace')).toBeNull());
    fireEvent.change(screen.getByPlaceholderText('Build onboarding flow'), { target: { value: 'simulate only' } });
    fireEvent.click(screen.getByRole('button', { name: 'Simulate plan' }));
    expect(execute).toHaveBeenCalledWith({ goal: 'simulate only', emit_events: true, execution_consent: 'not-supplied' });
  });
});
