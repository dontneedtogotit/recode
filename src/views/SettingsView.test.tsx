import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import SettingsView from './SettingsView';
import TelemetryView from './TelemetryView';
import DiagnosticsView from './DiagnosticsView';
import LearningView from './LearningView';
import UpdatesView from './UpdatesView';
import * as runtime from '../hooks/useAgentRuntimeSettings';
import * as metrics from '../hooks/useAgentTaskMetrics';
import * as providers from '../hooks/useProviderSettings';
import * as benchmark from '../hooks/useIpcBenchmark';
import * as diagnostics from '../hooks/useDiagnostics';
import * as updater from '../hooks/useUpdater';
import * as ipc from '../lib/ipc';

// These views share the same maintenance command contracts. Keep their tests
// here because unit-005 deliberately limits the presentation test file scope.
describe('Maintenance presentation contracts', () => {
  beforeEach(() => {
    vi.spyOn(runtime, 'loadAgentRuntimeSettings').mockResolvedValue({ ...runtime.DEFAULT_AGENT_RUNTIME_SETTINGS });
    vi.spyOn(metrics, 'loadAgentTaskMetrics').mockResolvedValue([]);
    vi.spyOn(providers, 'listProviders').mockResolvedValue([]);
  });

  it.each(['null', '{}', '[null]', '[{"avg_ms":"bad"}]'])('ignores invalid benchmark history %s', async (stored) => {
    window.localStorage.setItem('recode.benchmarkHistory', stored);
    render(<SettingsView />);
    await act(async () => {});
    fireEvent.click(screen.getByRole('tab', { name: 'Maintenance' }));
    expect(screen.getByRole('button', { name: 'Run IPC benchmark' })).toBeEnabled();
  });

  it('renders settings when access to local storage is denied', async () => {
    vi.mocked(window.localStorage.getItem).mockImplementation(() => { throw new Error('denied'); });
    render(<SettingsView />);
    await act(async () => {});
    expect(screen.getByLabelText('Max concurrency')).toHaveValue(2);
  });

  it('reports rejected runtime saves without losing the edited value', async () => {
    vi.spyOn(runtime, 'saveAgentRuntimeSettings').mockRejectedValue(new Error('disk full'));
    render(<SettingsView />);
    await act(async () => {});
    fireEvent.change(screen.getByLabelText('Max concurrency'), { target: { value: '4' } });
    expect(await screen.findByRole('alert')).toHaveTextContent('disk full');
    expect(screen.getByLabelText('Max concurrency')).toHaveValue(4);
  });

  it('reports failed benchmark commands and allows retry in settings', async () => {
    vi.spyOn(benchmark, 'runIpcBenchmark').mockRejectedValue(new Error('benchmark unavailable'));
    render(<SettingsView />);
    await act(async () => {});
    fireEvent.click(screen.getByRole('tab', { name: 'Maintenance' }));
    fireEvent.click(screen.getByRole('button', { name: 'Run IPC benchmark' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('benchmark unavailable');
    expect(screen.getByRole('button', { name: 'Run IPC benchmark' })).toBeEnabled();
  });

  it('reports failed benchmark commands and allows retry in telemetry', async () => {
    vi.spyOn(benchmark, 'runIpcBenchmark').mockRejectedValue(new Error('benchmark unavailable'));
    render(<TelemetryView />);
    fireEvent.click(screen.getByRole('button', { name: 'Run IPC benchmark' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('benchmark unavailable');
    expect(screen.getByRole('button', { name: 'Run IPC benchmark' })).toBeEnabled();
    expect(screen.getByText('Not run')).toBeInTheDocument();
  });

  it('reports failed self-heal and re-enables the action', async () => {
    vi.spyOn(diagnostics, 'listMonitorEvents').mockResolvedValue([]);
    vi.spyOn(diagnostics, 'attemptSelfHeal').mockRejectedValue(new Error('repair unavailable'));
    render(<DiagnosticsView runState={{ status: 'idle' }} onRunAgentTask={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Attempt self-heal' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('repair unavailable');
    expect(screen.getByRole('button', { name: 'Attempt self-heal' })).toBeEnabled();
  });

  it('reports failed monitor reads rather than claiming no recorded events', async () => {
    vi.spyOn(diagnostics, 'listMonitorEvents').mockRejectedValue(new Error('monitor unavailable'));
    render(<DiagnosticsView runState={{ status: 'idle' }} onRunAgentTask={vi.fn()} />);
    expect(await screen.findByRole('alert')).toHaveTextContent('monitor unavailable');
    expect(screen.queryByText('No events recorded')).not.toBeInTheDocument();
  });

  it('reports unavailable learning data rather than treating failures as empty results', async () => {
    vi.spyOn(ipc, 'listSuccessPatterns').mockRejectedValue(new Error('learning unavailable'));
    vi.spyOn(ipc, 'listAntiPatterns').mockResolvedValue([]);
    render(<LearningView />);
    expect(await screen.findByRole('alert')).toHaveTextContent('learning unavailable');
    expect(screen.queryByText('No success patterns yet')).not.toBeInTheDocument();
  });

  it('ignores learning responses for a previous filter', async () => {
    let resolveOld!: (value: ipc.LearningPattern[]) => void;
    vi.spyOn(ipc, 'listSuccessPatterns')
      .mockImplementationOnce(() => new Promise((resolve) => { resolveOld = resolve; }))
      .mockResolvedValue([{ id: 'new', kind: 'success', agent_role: 'reviewer', task_type: 'agent_task', content: 'current pattern', score: 1, sample_count: 1, updated_at: 0 }]);
    vi.spyOn(ipc, 'listAntiPatterns').mockResolvedValue([]);
    render(<LearningView />);
    fireEvent.change(screen.getByLabelText('Agent role'), { target: { value: 'reviewer' } });
    await screen.findByText('current pattern');
    await act(async () => { resolveOld([{ id: 'old', kind: 'success', agent_role: 'runtime', task_type: 'agent_task', content: 'stale pattern', score: 1, sample_count: 1, updated_at: 0 }]); });
    expect(screen.queryByText('stale pattern')).not.toBeInTheDocument();
    expect(screen.getByText('current pattern')).toBeInTheDocument();
  });

  it('requires all signature paths before verification', () => {
    const verify = vi.spyOn(updater, 'verifyUpdateSignature').mockResolvedValue(true);
    render(<UpdatesView />);
    expect(screen.getByRole('button', { name: 'Verify signature' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: 'Verify signature' }));
    expect(verify).not.toHaveBeenCalled();
  });
});

describe('SettingsView runtime values', () => {
  it('does not persist an empty numeric value and reports validation', async () => {
    const invoke = vi.fn().mockResolvedValue(undefined);
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };
    render(<SettingsView />);
    const input = screen.getByLabelText('Max concurrency');
    fireEvent.change(input, { target: { value: '' } });
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Enter a value'));
    expect(invoke).not.toHaveBeenCalledWith('save_agent_runtime_settings', expect.objectContaining({ maxConcurrency: Number.NaN }));
  });

  it('rejects values below the field minimum', async () => {
    const invoke = vi.fn().mockResolvedValue(undefined);
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };
    render(<SettingsView />);
    fireEvent.change(screen.getByLabelText('Max concurrency'), { target: { value: '0' } });
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('at least 1'));
    expect(invoke).not.toHaveBeenCalledWith('save_agent_runtime_settings', expect.objectContaining({ maxConcurrency: 0 }));
  });
});
