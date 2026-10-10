import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useGatewayRun } from './useGatewayRun';
import * as useGateway from './useGateway';
import { loadAgentRuntimeSettings, saveAgentRuntimeSettings, DEFAULT_AGENT_RUNTIME_SETTINGS } from './useAgentRuntimeSettings';
import { executeAgentTask } from './useAgentRuntimeExecution';
import { runIpcBenchmark } from './useIpcBenchmark';
import { runStartupBenchmark } from './useStartupBenchmark';
import { checkUpdates, verifyUpdateSignature } from './useUpdater';
import { listProviders, saveProvider, deleteProvider } from './useProviderSettings';
import * as diagnostics from './useDiagnostics';
import { listMonitorEvents } from './useMonitorEvents';
import { attemptSelfHeal } from './useSelfHeal';
import { usePipelineRunHistory } from './usePipelineRunHistory';

vi.mock('./useGateway', () => ({
  getGatewayStatus: vi.fn(),
  runGatewayTask: vi.fn(),
}));

describe('useGatewayRun', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(useGateway.getGatewayStatus).mockResolvedValue({ state: 'Open' });
  });

  it('starts with default state', async () => {
    vi.mocked(useGateway.getGatewayStatus).mockResolvedValue({ state: 'Open' });
    const { result } = renderHook(() => useGatewayRun());
    expect(result.current.provider).toBe('openai-compatible');
    expect(result.current.taskType).toBe('chat');
    expect(result.current.prompt).toBe('');
    expect(result.current.response).toBeNull();
    expect(result.current.status).toBeNull();
    expect(result.current.loading).toBe(false);
    expect(result.current.rawResponse).toBe('');
    await waitFor(() => expect(result.current.status).toBe('Open'));
  });

  it('updates fields and runs a task', async () => {
    vi.mocked(useGateway.getGatewayStatus).mockResolvedValue({ state: 'Open' });
    vi.mocked(useGateway.runGatewayTask).mockResolvedValue({
      status: 200,
      body: { hello: 'world' },
      raw: 'raw-output',
    });

    const { result } = renderHook(() => useGatewayRun());

    await waitFor(() => expect(result.current.status).toBe('Open'));
    await act(() => result.current.setProvider('test-provider'));
    await act(() => result.current.setTaskType('code'));
    await act(() => result.current.setPrompt('Say hi'));
    expect(result.current.provider).toBe('test-provider');
    expect(result.current.taskType).toBe('code');
    expect(result.current.prompt).toBe('Say hi');

    await act(() => result.current.run());
    expect(useGateway.runGatewayTask).toHaveBeenCalledWith({
      provider: 'test-provider',
      payload: { messages: [{ role: 'user', content: 'Say hi' }] },
      task_type: 'code',
    });
    expect(result.current.response).toEqual({ hello: 'world' });
    expect(result.current.rawResponse).toBe('raw-output');
    expect(result.current.loading).toBe(false);
  });

  it('reports a rejected configuration check without an unhandled rejection', async () => {
    vi.mocked(useGateway.getGatewayStatus).mockRejectedValue(new Error('offline'));
    const { result } = renderHook(() => useGatewayRun());
    await waitFor(() => expect(result.current.status).toBe('Gateway status unavailable: offline'));
  });

  it('ignores blank prompts and duplicate submissions while pending', async () => {
    let resolve!: (value: useGateway.GatewayResponse) => void;
    vi.mocked(useGateway.runGatewayTask).mockReturnValue(new Promise((done) => { resolve = done; }));
    const { result } = renderHook(() => useGatewayRun());
    await act(async () => { await result.current.run(); });
    expect(useGateway.runGatewayTask).not.toHaveBeenCalled();
    act(() => result.current.setPrompt(' hello '));
    let first!: Promise<void>;
    await act(async () => {
      first = result.current.run();
      void result.current.run();
    });
    expect(useGateway.runGatewayTask).toHaveBeenCalledTimes(1);
    expect(result.current.loading).toBe(true);
    await act(async () => { resolve({ status: 200, body: { ok: true } }); await first; });
    expect(result.current.response).toEqual({ ok: true });
  });

  it.each([null, 'plain text', [1, 2], 7])('keeps body %j within the declared response contract', async (body) => {
    vi.mocked(useGateway.runGatewayTask).mockResolvedValue({ status: 200, body });
    const { result } = renderHook(() => useGatewayRun());
    await act(async () => { result.current.setPrompt('hello'); });
    await act(async () => { await result.current.run(); });
    expect(result.current.response).toEqual(body === null ? { error: 'empty response' } : { body });
  });

  it('settles errors and allows a subsequent retry', async () => {
    vi.mocked(useGateway.runGatewayTask)
      .mockRejectedValueOnce(new Error('request failed'))
      .mockResolvedValueOnce({ status: 200, body: { ok: true }, raw: 'ok' });
    const { result } = renderHook(() => useGatewayRun());
    await act(async () => { result.current.setPrompt('hello'); });
    await act(async () => { await result.current.run(); });
    expect(result.current.response).toEqual({ error: 'Error: request failed' });
    expect(result.current.loading).toBe(false);
    await act(async () => { await result.current.run(); });
    expect(result.current.response).toEqual({ ok: true });
  });

  it('does not execute a retained action after unmount', async () => {
    const hook = renderHook(() => useGatewayRun());
    await act(async () => { hook.result.current.setPrompt('hello'); });
    const run = hook.result.current.run;
    hook.unmount();
    await run();
    expect(useGateway.runGatewayTask).not.toHaveBeenCalled();
  });
});

// Exercise real application adapters through the native IPC seam.
describe('application command contracts', () => {
  const invoke = vi.fn<(command: string, args?: Record<string, unknown>) => Promise<unknown>>();
  beforeEach(() => { invoke.mockReset(); window.__TAURI__ = { invoke }; });
  afterEach(() => { delete window.__TAURI__; });

  it('preserves native runtime task identity and failure details', async () => {
    invoke.mockResolvedValue({ task_id: 'native-task', agent_id: 'planner', success: false, output: 'partial', error: 'failed' });
    const result = await executeAgentTask({ id: 'planner', input: 'goal' });
    expect(invoke).toHaveBeenCalledWith('execute_agent_task', { input: { agent_id: 'planner', payload: { input: 'goal' } } });
    expect(result).toMatchObject({ taskId: 'native-task', status: 'error', output: 'partial', error: 'failed' });
    expect(result.durationMs).toBeGreaterThanOrEqual(0);
  });

  it('falls back gracefully on transport failures when saving runtime settings', async () => {
    invoke.mockRejectedValue(new Error('disk full'));
    expect(await saveAgentRuntimeSettings(DEFAULT_AGENT_RUNTIME_SETTINGS)).toEqual(DEFAULT_AGENT_RUNTIME_SETTINGS);
  });

  it('returns independent optional runtime defaults when offline', async () => {
    invoke.mockRejectedValue(new Error('offline'));
    const first = await loadAgentRuntimeSettings();
    const second = await loadAgentRuntimeSettings();
    expect(first).toEqual(DEFAULT_AGENT_RUNTIME_SETTINGS);
    expect(first).not.toBe(DEFAULT_AGENT_RUNTIME_SETTINGS);
    expect(first).not.toBe(second);
  });

  it('forwards runtime settings without changing their values', async () => {
    const settings = { maxConcurrency: 4, defaultTimeoutMs: 500, retryLimit: 0 };
    invoke.mockResolvedValueOnce(settings).mockResolvedValueOnce(settings);
    expect(await loadAgentRuntimeSettings()).toEqual(settings);
    expect(await saveAgentRuntimeSettings(settings)).toEqual(settings);
    expect(invoke).toHaveBeenLastCalledWith('save_agent_runtime_settings', { settings });
  });

  it.each([
    ['benchmark', () => runIpcBenchmark(100)],
    ['update check', () => checkUpdates('https://updates.test', '1.0.0')],
    ['signature verification', () => verifyUpdateSignature('artifact', 'signature', 'key')],
  ])('falls back gracefully on failed %s instead of crashing', async (_name, call) => {
    invoke.mockRejectedValue(new Error('unavailable'));
    const result = await call();
    expect(result).toBeDefined();
  });

  it('forwards benchmark and updater requests to the native contract', async () => {
    const benchmark = { iterations: 100, total_ms: 100, min_ms: 1, max_ms: 1, avg_ms: 1, p95_ms: 1 };
    invoke.mockResolvedValueOnce(benchmark);
    expect(await runIpcBenchmark()).toEqual(benchmark);
    expect(invoke).toHaveBeenLastCalledWith('run_ipc_benchmark_command', { request: { iterations: 100 } });
    invoke.mockResolvedValueOnce({ elapsed_ms: 20, initialized: true });
    expect(await runStartupBenchmark()).toEqual({ elapsed_ms: 20, initialized: true });
    expect(invoke).toHaveBeenLastCalledWith('run_startup_benchmark_command', {});
    invoke.mockResolvedValueOnce({ available: true, manifest: { version: '2.0.0' } });
    expect(await checkUpdates('https://updates.test', '1.0.0')).toEqual({ available: true, manifest: { version: '2.0.0' } });
    expect(invoke).toHaveBeenLastCalledWith('check_updates', { request: { endpoint: 'https://updates.test', current_version: '1.0.0' } });
    invoke.mockResolvedValueOnce(false);
    expect(await verifyUpdateSignature('artifact', 'signature', 'key')).toBe(false);
    expect(invoke).toHaveBeenLastCalledWith('verify_signature', { request: { artifact_path: 'artifact', signature_path: 'signature', public_key_path: 'key' } });
  });

  it('preserves provider arguments and propagates failed writes', async () => {
    const provider = { id: 'local', provider: 'local', api_key: '', enabled: true };
    invoke.mockResolvedValueOnce([provider]);
    expect(await listProviders()).toEqual([provider]);
    expect(invoke).toHaveBeenLastCalledWith('list_providers');
    invoke.mockResolvedValueOnce(provider);
    await saveProvider(provider);
    expect(invoke).toHaveBeenLastCalledWith('save_provider', { provider });
    invoke.mockResolvedValueOnce(undefined);
    await deleteProvider('local');
    expect(invoke).toHaveBeenLastCalledWith('delete_provider', { id: 'local' });
    invoke.mockRejectedValueOnce(new Error('write failed'));
    await expect(saveProvider(provider)).rejects.toThrow('write failed');
  });

  it('keeps diagnostics, monitor and self-heal entry points consistent', async () => {
    const events = [{ ts: '1', severity: 'warn', message: 'warning' }];
    const actions = [{ description: 'repair', applied: false }];
    invoke.mockResolvedValueOnce(events).mockResolvedValueOnce(events);
    expect(await diagnostics.listMonitorEvents()).toEqual(events);
    expect(await listMonitorEvents()).toEqual(events);
    expect(invoke).toHaveBeenLastCalledWith('list_monitor_events');
    invoke.mockResolvedValueOnce({ actions }).mockResolvedValueOnce({ actions });
    expect(await diagnostics.attemptSelfHeal()).toEqual(actions);
    expect(await attemptSelfHeal()).toEqual(actions);
    expect(invoke).toHaveBeenLastCalledWith('attempt_self_heal');
    const input = { kind: 'test', message: 'message', detail: 'detail' };
    invoke.mockResolvedValueOnce(undefined);
    await diagnostics.captureDiagnostics(input);
    expect(invoke).toHaveBeenLastCalledWith('capture_diagnostics', { input });
  });

  it('loads history with a stable callback and preserves load errors', async () => {
    const hook = renderHook(() => usePipelineRunHistory());
    const load = hook.result.current.load;
    hook.rerender();
    expect(hook.result.current.load).toBe(load);
    invoke.mockResolvedValueOnce([]);
    expect(await load()).toEqual([]);
    expect(invoke).toHaveBeenLastCalledWith('list_pipeline_runs', { limit: 20 });
    invoke.mockRejectedValueOnce(new Error('history unavailable'));
    await expect(load(5)).rejects.toThrow('history unavailable');
    expect(invoke).toHaveBeenLastCalledWith('list_pipeline_runs', { limit: 5 });
  });
});
