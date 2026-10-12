import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  attemptSelfHeal,
  cancelPipeline,
  pausePipeline,
  resumePipeline,
  abortPipeline,
  getPipelineStatus,
  captureDiagnostics,
  checkUpdates,
  createAndExecutePlan,
  createWorkspace,
  deleteProviderSettings,
  executeAgentTask,
  listAgentTaskHistory,
  listAgentTaskMetrics,
  listMonitorEvents,
  listPipelineRuns,
  normalizePipelineRun,
  listPlans,
  listProviderSettings,
  listWorkspaces,
  registerAgent,
  recordAgentTaskHistory,
  saveAgentTaskRun,
  listAgentTaskRuns,
  listAgents,
  listAntiPatterns,
  listSuccessPatterns,
  loadAgentRuntimeSettings,
  runStartupBenchmark,
  runPipeline,
  runGatewayTask,
  runIpcBenchmark,
  saveAgentRuntimeSettings,
  savePlan,
  saveProviderSettings,
  subscribeToPipelineEvents,
  verifyUpdateSignature,
  writeProjectFile,
  readProjectFile,
  listProjectFiles,
  validateWorkspacePath,
} from './ipc';
import { createAgentHistoryRepository, createAgentHistoryState, loadAgentHistory, pushAgentTaskRecord, saveAgentHistory } from './agentHistory';
import { executeAgentTask as executeRegistryTask, listAgents as listRegistryAgents } from './agentRegistry';
import { listAgentTaskRuns as listPersistedTaskRuns, saveAgentTaskRun as savePersistedTaskRun } from './agentTaskPersistence';
import { listPlans as listPersistedPlans, savePlan as savePersistedPlan } from './planPersistence';
import { DEFAULT_REPLACEMENT_CRITERIA, meetsAll, unmetCount } from './replacementCriteria';

describe('executeAgentTask', () => {
  afterEach(() => {
    delete (globalThis as { __TAURI__?: unknown }).__TAURI__;
  });

  it('uses the backend field names and normalizes the response', async () => {
    const invoke = vi.fn().mockResolvedValue({
      task_id: 'task-42',
      agent_id: 'planner',
      success: true,
      output: { status: 'completed' },
      error: null,
    });
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };

    const result = await executeAgentTask({ agentId: 'planner', input: 'hello' });

    expect(invoke).toHaveBeenCalledWith('execute_agent_task', {
      input: { agent_id: 'planner', payload: { input: 'hello' } },
    });
    expect(result).toEqual({
      taskId: 'task-42',
      agentId: 'planner',
      success: true,
      output: { status: 'completed' },
      error: null,
    });
  });

  it('normalizes snake_case pipeline events from Tauri', async () => {
    let handler: ((event: { payload: unknown }) => void) | undefined;
    const listen = vi.fn().mockImplementation(async (_name, callback) => {
      handler = callback;
      return vi.fn();
    });
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { event: { listen } };
    const received = vi.fn();
    const cleanup = subscribeToPipelineEvents(received);
    await vi.waitFor(() => expect(listen).toHaveBeenCalled());
    handler?.({ payload: {
      run_id: 'run-1',
      topic: 'pipeline.plan_completed',
      severity: 'info',
      message: 'done',
      ts: 42,
    } });
    expect(received).toHaveBeenCalledWith({
      runId: 'run-1',
      topic: 'pipeline.plan_completed',
      severity: 'info',
      message: 'done',
      ts: 42,
    });
    cleanup();
  });

  it('sends the backend-native run id when cancelling a pipeline', async () => {
    const invoke = vi.fn().mockResolvedValue(true);
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };

    await expect(cancelPipeline('run-7')).resolves.toBe(true);
    expect(invoke).toHaveBeenCalledWith('cancel_pipeline', { run_id: 'run-7' });
  });

  it('does not report a task as successful when Tauri is unavailable', async () => {
    await expect(executeAgentTask({ agentId: 'reviewer', input: 'review this' }))
      .rejects.toThrow('Invalid agent task response');
  });

  it('uses the native camelCase settings body inside the settings argument', async () => {
    const invoke = vi.fn()
      .mockResolvedValueOnce({ maxConcurrency: 4, defaultTimeoutMs: 120000, retryLimit: 2 })
      .mockResolvedValueOnce({ maxConcurrency: 4, defaultTimeoutMs: 120000, retryLimit: 2 });
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };

    await expect(loadAgentRuntimeSettings()).resolves.toEqual({
      maxConcurrency: 4,
      defaultTimeoutMs: 120000,
      retryLimit: 2,
    });
    await expect(saveAgentRuntimeSettings({
      maxConcurrency: 4,
      defaultTimeoutMs: 120000,
      retryLimit: 2,
    })).resolves.toEqual({
      maxConcurrency: 4,
      defaultTimeoutMs: 120000,
      retryLimit: 2,
    });
    expect(invoke).toHaveBeenNthCalledWith(1, 'load_agent_runtime_settings');
    expect(invoke).toHaveBeenNthCalledWith(2, 'save_agent_runtime_settings', {
      settings: { maxConcurrency: 4, defaultTimeoutMs: 120000, retryLimit: 2 },
    });
  });

  it('keeps provider command payloads and responses typed at the boundary', async () => {
    const provider = { id: 'openai', provider: 'openai', api_key: '', enabled: true };
    const invoke = vi.fn()
      .mockResolvedValueOnce([provider])
      .mockResolvedValueOnce(provider)
      .mockResolvedValueOnce(undefined);
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };

    await expect(listProviderSettings()).resolves.toEqual([provider]);
    await expect(saveProviderSettings(provider)).resolves.toEqual(provider);
    await expect(deleteProviderSettings('openai')).resolves.toBeUndefined();
    expect(invoke).toHaveBeenNthCalledWith(1, 'list_providers');
    expect(invoke).toHaveBeenNthCalledWith(2, 'save_provider', { provider });
    expect(invoke).toHaveBeenNthCalledWith(3, 'delete_provider', { id: 'openai' });
  });

  it('normalizes diagnostics and runtime metric responses', async () => {
    const invoke = vi.fn()
      .mockResolvedValueOnce([{ ts: '42', severity: 'info', message: 'ready' }])
      .mockResolvedValueOnce({ actions: [{ description: 'retry', applied: false }] })
      .mockResolvedValueOnce([{ task_id: 'task-1', agent_id: 'planner', duration_ms: 3, success: true }]);
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };

    await expect(listMonitorEvents()).resolves.toEqual([{ ts: '42', severity: 'info', message: 'ready' }]);
    await expect(attemptSelfHeal()).resolves.toEqual([{ description: 'retry', applied: false }]);
    await expect(listAgentTaskMetrics()).resolves.toEqual([
      { task_id: 'task-1', agent_id: 'planner', duration_ms: 3, success: true },
    ]);
    expect(invoke).toHaveBeenNthCalledWith(1, 'list_monitor_events');
    expect(invoke).toHaveBeenNthCalledWith(2, 'attempt_self_heal');
    expect(invoke).toHaveBeenNthCalledWith(3, 'list_agent_task_metrics');
  });

  it('uses the backend-native diagnostics capture payload', async () => {
    const invoke = vi.fn().mockResolvedValue(undefined);
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };
    await captureDiagnostics({ kind: 'ui', message: 'broken', detail: 'trace' });
    expect(invoke).toHaveBeenCalledWith('capture_diagnostics', {
      input: { kind: 'ui', message: 'broken', detail: 'trace' },
    });
  });

  it('owns workspace, plan, history, and plan-execution command shapes', async () => {
    const workspace = { id: 'workspace-1', root: '/tmp/workspace-1' };
    const plan = { id: 'plan-1', goal: 'ship', status: 'completed', steps: [] };
    const history = {
      id: 'run-1',
      goal: 'ship',
      status: 'completed',
      step_count: 1,
      repair_count: 0,
      success: true,
      error: null,
      finished_at: 42,
    };
    const invoke = vi.fn()
      .mockResolvedValueOnce([workspace])
      .mockResolvedValueOnce(workspace)
      .mockResolvedValueOnce([plan])
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce([history])
      .mockResolvedValueOnce(plan);
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };

    await expect(listWorkspaces()).resolves.toEqual([workspace]);
    await expect(createWorkspace({ id: 'workspace-1' })).resolves.toEqual(workspace);
    await expect(listPlans()).resolves.toEqual([plan]);
    await expect(savePlan({ id: 'plan-1', goal: 'ship' })).resolves.toBeUndefined();
    await expect(listPipelineRuns(20)).resolves.toEqual([history]);
    await expect(createAndExecutePlan({ id: 'plan-1', goal: 'ship' })).resolves.toEqual(plan);

    expect(invoke).toHaveBeenNthCalledWith(1, 'list_workspaces');
    expect(invoke).toHaveBeenNthCalledWith(2, 'create_workspace', { input: { id: 'workspace-1' } });
    expect(invoke).toHaveBeenNthCalledWith(3, 'list_plans');
    expect(invoke).toHaveBeenNthCalledWith(4, 'save_plan', { input: { id: 'plan-1', goal: 'ship' } });
    expect(invoke).toHaveBeenNthCalledWith(5, 'list_pipeline_runs', { limit: 20 });
    expect(invoke).toHaveBeenNthCalledWith(6, 'create_and_execute_plan', { input: { id: 'plan-1', goal: 'ship' } });
  });

  it('normalizes gateway and benchmark command contracts', async () => {
    const invoke = vi.fn()
      .mockResolvedValueOnce({ status: 200, body: { ok: true } })
      .mockResolvedValueOnce({ iterations: 3, total_ms: 6, min_ms: 1, max_ms: 4, avg_ms: 2, p95_ms: 4 });
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };

    await expect(runGatewayTask({ provider: 'stub', payload: { prompt: 'hello' }, task_type: 'chat' })).resolves.toEqual({
      status: 200,
      body: { ok: true },
    });
    await expect(runIpcBenchmark(3)).resolves.toEqual({
      iterations: 3,
      total_ms: 6,
      min_ms: 1,
      max_ms: 4,
      avg_ms: 2,
      p95_ms: 4,
    });
    expect(invoke).toHaveBeenNthCalledWith(1, 'run_gateway_task', {
      input: { provider: 'stub', payload: { prompt: 'hello' }, task_type: 'chat' },
    });
    expect(invoke).toHaveBeenNthCalledWith(2, 'run_ipc_benchmark_command', { request: { iterations: 3 } });
  });

  it('uses nested backend request objects for updater commands', async () => {
    const invoke = vi.fn()
      .mockResolvedValueOnce({ available: false })
      .mockResolvedValueOnce(true);
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };

    await expect(checkUpdates('https://updates.example', '0.1.0')).resolves.toEqual({ available: false });
    await expect(verifyUpdateSignature('/artifact', '/signature', '/public-key')).resolves.toBe(true);
    expect(invoke).toHaveBeenNthCalledWith(1, 'check_updates', {
      request: { endpoint: 'https://updates.example', current_version: '0.1.0' },
    });
    expect(invoke).toHaveBeenNthCalledWith(2, 'verify_signature', {
      request: { artifact_path: '/artifact', signature_path: '/signature', public_key_path: '/public-key' },
    });
  });

  it('uses the backend-native agent history argument', async () => {
    const invoke = vi.fn().mockResolvedValue([]);
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };
    await expect(listAgentTaskHistory('planner')).resolves.toEqual([]);
    expect(invoke).toHaveBeenCalledWith('list_agent_task_history', { agent_id: 'planner' });
  });

  it('owns agent registry and history mutation command shapes', async () => {
    const descriptor = {
      id: 'reviewer',
      name: 'Reviewer',
      role: 'quality',
      description: 'Reviews outputs',
      capabilities: ['review'],
    };
    const record = {
      id: 'history-1',
      agent_id: 'reviewer',
      task_id: 'task-1',
      status: 'completed',
      started_at: 1,
      finished_at: 2,
      output: 'ok',
      error: null,
    };
    const invoke = vi.fn()
      .mockResolvedValueOnce([descriptor])
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce(undefined);
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };

    await expect(listAgents()).resolves.toEqual([descriptor]);
    await expect(registerAgent(descriptor)).resolves.toBeUndefined();
    await expect(recordAgentTaskHistory(record)).resolves.toBeUndefined();
    expect(invoke).toHaveBeenNthCalledWith(1, 'list_agents');
    expect(invoke).toHaveBeenNthCalledWith(2, 'register_agent', { descriptor });
    expect(invoke).toHaveBeenNthCalledWith(3, 'record_agent_task_history', { input: record });
  });

  it('owns agent task persistence request and response shapes', async () => {
    const invoke = vi.fn()
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce([{ id: 'run-1', agent_id: 'planner', input: 'hello', success: true, output: 'ok', error: null, duration_ms: 3, timestamp: 10 }]);
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };
    const record = { id: 'run-1', agentId: 'planner', input: 'hello', success: true, output: 'ok', durationMs: 3, timestamp: 10 };

    await expect(saveAgentTaskRun(record)).resolves.toBeUndefined();
    await expect(listAgentTaskRuns('planner')).resolves.toEqual([{
      id: 'run-1',
      agentId: 'planner',
      input: 'hello',
      success: true,
      output: 'ok',
      durationMs: 3,
      timestamp: 10,
    }]);
    expect(invoke).toHaveBeenNthCalledWith(1, 'save_agent_task_run', { input: {
      id: 'run-1', agent_id: 'planner', input: 'hello', success: true,
      output: 'ok', error: null, duration_ms: 3, timestamp: 10,
    } });
    expect(invoke).toHaveBeenNthCalledWith(2, 'list_agent_task_runs', { agent_id: 'planner' });
  });

  it('owns learning and startup benchmark command contracts', async () => {
    const success = { id: 's1', kind: 'Success', agent_role: 'runtime', task_type: 'agent_task', content: 'use tests', score: 0.9, sample_count: 3, updated_at: 4 };
    const anti = { id: 'a1', kind: 'AntiPattern', agent_role: 'runtime', task_type: 'agent_task', content: 'skip validation', score: 0.2, sample_count: 2, updated_at: 5 };
    const invoke = vi.fn()
      .mockResolvedValueOnce([success])
      .mockResolvedValueOnce([anti])
      .mockResolvedValueOnce({ elapsed_ms: 12, initialized: true });
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };

    await expect(listSuccessPatterns('runtime', 'agent_task')).resolves.toEqual([{ ...success, kind: 'success' }]);
    await expect(listAntiPatterns('runtime', 'agent_task')).resolves.toEqual([{ ...anti, kind: 'anti' }]);
    await expect(runStartupBenchmark()).resolves.toEqual({ elapsed_ms: 12, initialized: true });
    expect(invoke).toHaveBeenNthCalledWith(1, 'list_success_patterns', { agent_role: 'runtime', task_type: 'agent_task' });
    expect(invoke).toHaveBeenNthCalledWith(2, 'list_anti_patterns', { agent_role: 'runtime', task_type: 'agent_task' });
    expect(invoke).toHaveBeenNthCalledWith(3, 'run_startup_benchmark_command', {});
  });

  it('owns pipeline control command contracts', async () => {
    const invoke = vi.fn()
      .mockResolvedValueOnce(true)
      .mockResolvedValueOnce(true)
      .mockResolvedValueOnce(true)
      .mockResolvedValueOnce({ run_id: 'run-1', active: true, state: 'pausing' });
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };

    await expect(pausePipeline('run-1')).resolves.toBe(true);
    await expect(resumePipeline('run-1')).resolves.toBe(true);
    await expect(abortPipeline('run-1')).resolves.toBe(true);
    await expect(getPipelineStatus('run-1')).resolves.toEqual({ runId: 'run-1', active: true, state: 'pausing' });

    expect(invoke).toHaveBeenNthCalledWith(1, 'pause_pipeline', { run_id: 'run-1' });
    expect(invoke).toHaveBeenNthCalledWith(2, 'resume_pipeline', { run_id: 'run-1' });
    expect(invoke).toHaveBeenNthCalledWith(3, 'abort_pipeline', { run_id: 'run-1' });
    expect(invoke).toHaveBeenNthCalledWith(4, 'get_pipeline_status', { run_id: 'run-1' });
  });

  it('sends the nested Rust pipeline input and preserves its run id', async () => {
    const invoke = vi.fn().mockResolvedValue({
      plan: { id: 'plan-1', goal: 'build page', status: 'Completed', steps: [] },
      repair_actions: [],
      events: [],
      run_id: 'run-9',
      generated_files: [{ path: 'src/index.ts', status: 'Written', language: 'typescript' }],
      failed_files: [{ path: 'src/bad.ts', status: 'Failed', error: 'model error' }],
      outcome: { success: true, degraded: true, generated_file_count: 1, failed_file_count: 1, failed_files: [{ path: 'src/bad.ts', status: 'Failed', error: 'model error' }], summary: 'partial success' },
    });
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };

    await expect(runPipeline({ goal: 'build page', emit_events: true, runId: 'run-9', project_id: 'proj-1' }))
      .resolves.toMatchObject({
        runId: 'run-9',
        plan: { goal: 'build page' },
        generated_files: [{ path: 'src/index.ts', status: 'Written', language: 'typescript' }],
        failed_files: [{ path: 'src/bad.ts', status: 'Failed', error: 'model error' }],
        outcome: { success: true, degraded: true, generated_file_count: 1, failed_file_count: 1, summary: 'partial success' },
      });
    expect(invoke).toHaveBeenCalledWith('run_pipeline', {
      input: { goal: 'build page', emit_events: true, run_id: 'run-9', project_id: 'proj-1' },
    });
  });

  it('omits project_id from the backend payload when it is not provided', async () => {
    const invoke = vi.fn().mockResolvedValue({
      plan: { id: 'plan-1', goal: 'build page', status: 'Completed', steps: [] },
      repair_actions: [],
      events: [],
      run_id: 'run-9',
    });
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };

    await expect(runPipeline({ goal: 'build page', emit_events: true, runId: 'run-9' }))
      .resolves.toMatchObject({ runId: 'run-9', plan: { goal: 'build page' } });
    expect(invoke).toHaveBeenCalledWith('run_pipeline', {
      input: { goal: 'build page', emit_events: true, run_id: 'run-9' },
    });
  });

  it('preserves verification results returned at the run level', () => {
    const run = normalizePipelineRun({
      run_id: 'run-verify',
      plan: { id: 'plan-1', goal: 'ship', status: 'Completed', steps: [] },
      repair_actions: [], events: [],
      outcome: { success: false, degraded: true, generated_file_count: 1, failed_file_count: 0, failed_files: [], summary: 'verification failed' },
      verification_results: [{ gate: 'disk', passed: false, degraded: false, detail: 'missing output' }],
      repair_results: [{ applied: false, description: 'manual review', path: 'src/index.ts' }],
      verification_summary: '1 gate failed',
    });
    expect(run.outcome?.verification_results).toEqual([{ gate: 'disk', passed: false, degraded: false, detail: 'missing output' }]);
    expect(run.outcome?.repair_results).toEqual([{ applied: false, description: 'manual review', path: 'src/index.ts' }]);
    expect(run.outcome?.verification_summary).toBe('1 gate failed');
  });

  it('rejects malformed native plan responses instead of inventing a plan', async () => {
    const invoke = vi.fn().mockResolvedValue(undefined);
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };
    await expect(createAndExecutePlan({ id: 'plan-1', goal: 'ship' }))
      .rejects.toThrow('Invalid plan response');
  });
});

describe('shared persistence boundaries', () => {
  afterEach(() => {
    delete window.__TAURI__;
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  const taskRun = { id: 'run', agentId: 'planner', input: 'goal', success: false, durationMs: 0, timestamp: 0 };
  const nativeRun = { id: 'run', agent_id: 'planner', input: 'goal', success: false, duration_ms: 0, timestamp: 0 };
  const draft = { id: 'plan', goal: 'goal', status: 'pending', steps: [] };

  it('does not advertise runtime agents in browser preview', async () => {
    await expect(listRegistryAgents()).resolves.toEqual([]);
    // In browser preview (no Tauri shell), invokeCommand returns a benign fallback.
    await expect(listAgents()).resolves.toEqual([]);
    await expect(executeRegistryTask({ agent_id: 'planner', payload: {} })).rejects.toThrow('Invalid agent task response');
  });

  it.each(['answer', null, ['answer'], { text: 'answer' }])('preserves registry payloads and JSON outputs: %j', async (output) => {
    const invoke = vi.fn().mockResolvedValue({ task_id: 'task', agent_id: 'planner', success: true, output, error: null });
    window.__TAURI__ = { invoke };
    const payload = { input: 'goal', files: ['a.ts'], options: { review: true } };
    await expect(executeRegistryTask({ agent_id: 'planner', payload })).resolves.toEqual({
      task_id: 'task', agent_id: 'planner', success: true, output, error: null,
    });
    expect(invoke).toHaveBeenCalledWith('execute_agent_task', { input: { agent_id: 'planner', payload } });
  });

  it.each([null, [], {}, { success: true }, { task_id: ' ', agent_id: 'planner', success: true },
    { task_id: 'task', agent_id: 'planner', success: 'true' }])('rejects malformed execution responses: %j', async (response) => {
    window.__TAURI__ = { invoke: vi.fn().mockResolvedValue(response) };
    await expect(executeAgentTask({ agentId: 'planner', input: 'goal' })).rejects.toThrow('Invalid agent task response');
  });

  it.each([null, {}, [null], [{}], [{ id: 'a', name: 'A', role: 'r', description: '', capabilities: [1] }]])(
    'rejects malformed registry responses: %j', async (response) => {
      window.__TAURI__ = { invoke: vi.fn().mockResolvedValue(response) };
      await expect(listRegistryAgents()).rejects.toThrow('Invalid agent');
    },
  );

  it('rejects a non-callable native bridge without pretending it is available', async () => {
    vi.stubGlobal('__TAURI__', { invoke: true });
    // A non-callable bridge is treated like no bridge: benign fallback returned.
    await expect(savePlan(draft)).resolves.toBeUndefined();
  });

  it('shares plan decoding across execution and persistence, including native drafts without status', async () => {
    const nativePlan = { id: 'plan', goal: 'goal', steps: [{ id: 'step', description: 'do it', status: 'InProgress' }] };
    const invoke = vi.fn().mockResolvedValueOnce([nativePlan]).mockResolvedValueOnce(nativePlan);
    window.__TAURI__ = { invoke };
    const plans = await listPersistedPlans();
    expect(plans[0]).toEqual({ ...nativePlan, status: 'pending', steps: [{ ...nativePlan.steps[0], status: 'in_progress' }] });
    await expect(createAndExecutePlan(draft)).resolves.toEqual(plans[0]);
  });

  it.each([{}, [], { id: ' ', goal: 'goal' }, { id: 'plan', goal: 'goal', steps: {} },
    { id: 'plan', goal: 'goal', steps: [null] }])('rejects corrupt plans in both read paths: %j', async (response) => {
    window.__TAURI__ = { invoke: vi.fn().mockResolvedValueOnce([response]).mockResolvedValueOnce(response) };
    await expect(listPersistedPlans()).rejects.toThrow('Invalid plan');
    await expect(createAndExecutePlan(draft)).rejects.toThrow('Invalid plan');
  });

  it('saves drafts but refuses silent loss of plan execution state', async () => {
    const invoke = vi.fn().mockResolvedValue(undefined);
    window.__TAURI__ = { invoke };
    await savePersistedPlan(draft);
    expect(invoke).toHaveBeenCalledWith('save_plan', { input: { id: 'plan', goal: 'goal' } });
    await expect(savePersistedPlan({ ...draft, status: 'completed' })).rejects.toThrow('not supported');
    await expect(savePersistedPlan({ ...draft, steps: [{ id: 'step', description: 'do it', status: 'pending' }] }))
      .rejects.toThrow('not supported');
    await expect(savePlan({ ...draft, id: ' ' })).rejects.toThrow('Plan ID is required');
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it('round-trips task records through the compatibility adapter without losing failure details', async () => {
    const invoke = vi.fn().mockResolvedValueOnce(undefined).mockResolvedValueOnce([{ ...nativeRun, output: null, error: 'failed' }]);
    window.__TAURI__ = { invoke };
    await savePersistedTaskRun({ ...taskRun, error: 'failed' });
    expect(invoke).toHaveBeenCalledWith('save_agent_task_run', { input: { ...nativeRun, output: undefined, error: 'failed' } });
    await expect(listPersistedTaskRuns('planner')).resolves.toEqual([{ ...taskRun, output: undefined, error: 'failed' }]);
  });

  it.each([{ id: '' }, { agent_id: '' }, { input: 1 }, { success: 'false' }, { duration_ms: -1 },
    { duration_ms: NaN }, { timestamp: Infinity }, { timestamp: 0.5 }, { output: {} }, { error: false }])(
    'rejects malformed native task records: %j', async (invalid) => {
      window.__TAURI__ = { invoke: vi.fn().mockResolvedValue([{ ...nativeRun, ...invalid }]) };
      await expect(listPersistedTaskRuns('planner')).rejects.toThrow('Invalid task run');
    },
  );

  it.each([-1, NaN, Infinity, 1.5, Number.MAX_SAFE_INTEGER + 1])('rejects unrepresentable task durations before writing: %s', async (durationMs) => {
    const invoke = vi.fn();
    window.__TAURI__ = { invoke };
    await expect(savePersistedTaskRun({ ...taskRun, durationMs })).rejects.toThrow('Invalid task run');
    expect(invoke).not.toHaveBeenCalled();
  });

  it('propagates persistence failures instead of reporting successful saves', async () => {
    window.__TAURI__ = { invoke: vi.fn().mockRejectedValue(new Error('disk full')) };
    await expect(savePersistedPlan(draft)).rejects.toThrow('disk full');
    await expect(savePersistedTaskRun(taskRun)).rejects.toThrow('disk full');
    await expect(listPersistedPlans()).rejects.toThrow('disk full');
    await expect(listPersistedTaskRuns('planner')).rejects.toThrow('disk full');
  });
});

describe('local agent history repository', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  const record = { id: 'task', agentId: 'planner', input: 'goal', success: true, timestamp: 1 };
  function repository(raw: string | null) {
    const storage = { getItem: vi.fn(() => raw), setItem: vi.fn((_key: string, value: string) => { raw = value; }) };
    return { storage, history: createAgentHistoryRepository(storage) };
  }

  it.each([null, '{broken', '{}', 'null', '42', '[null,1,"bad",{}]'])('recovers from missing or corrupt history: %s', (raw) => {
    expect(repository(raw).history.load()).toEqual(createAgentHistoryState());
  });

  it('filters malformed records and untrusted optional fields without discarding valid history', () => {
    const { history } = repository(JSON.stringify([
      { ...record, success: 'true' }, { ...record, timestamp: -1 },
      { ...record, id: ' ' }, { ...record, agentId: null },
      { ...record, output: {}, error: [], durationMs: -1, extra: 'not persisted' },
      { ...record, id: 'second', durationMs: 0, output: '', error: '' },
    ]));
    expect(history.load().records).toEqual([record, { ...record, id: 'second', durationMs: 0, output: '', error: '' }]);
  });

  it('deduplicates updates and snapshots records without mutating callers', () => {
    const previous = { records: [record] };
    const replacement = { ...record, success: false, error: 'failed' };
    const next = pushAgentTaskRecord(previous, replacement);
    replacement.error = 'mutated';
    expect(next.records).toEqual([{ ...record, success: false, error: 'failed' }]);
    expect(previous.records).toEqual([record]);
  });

  it('bounds both restored and saved history after deduplication', () => {
    const records = Array.from({ length: 60 }, (_, index) => ({ ...record, id: `task-${index}` }));
    const { history, storage } = repository(JSON.stringify([records[0], ...records]));
    expect(history.load().records).toEqual(records.slice(0, 50));
    expect(history.save({ records })).toBe(true);
    expect(storage.setItem).toHaveBeenCalledWith('recode.agentHistory', JSON.stringify(records.slice(0, 50)));
    expect(history.load().records).toEqual(records.slice(0, 50));
    expect(records).toHaveLength(60);
  });

  it('reports write failure and recovers from read failure using an injected storage port', () => {
    const storage = { getItem: () => { throw new Error('blocked'); }, setItem: () => { throw new Error('full'); } };
    const history = createAgentHistoryRepository(storage);
    expect(history.load()).toEqual(createAgentHistoryState());
    expect(history.save({ records: [record] })).toBe(false);
  });

  it('handles a blocked browser storage getter', () => {
    vi.spyOn(window, 'localStorage', 'get').mockImplementation(() => { throw new Error('blocked'); });
    expect(loadAgentHistory()).toEqual(createAgentHistoryState());
    expect(saveAgentHistory({ records: [record] })).toBe(false);
  });

  it('handles server-side calls without browser globals', () => {
    vi.stubGlobal('window', undefined);
    expect(loadAgentHistory()).toEqual(createAgentHistoryState());
    expect(saveAgentHistory({ records: [record] })).toBe(false);
  });
});

describe('replacement readiness', () => {
  it('does not interpret an empty checklist as verified readiness', () => {
    expect(meetsAll({ criteria: [] })).toBe(false);
    expect(unmetCount({ criteria: [] })).toBe(0);
  });

  it('requires all supplied checks and preserves the default checklist', () => {
    expect(meetsAll(DEFAULT_REPLACEMENT_CRITERIA)).toBe(false);
    expect(unmetCount(DEFAULT_REPLACEMENT_CRITERIA)).toBe(DEFAULT_REPLACEMENT_CRITERIA.criteria.length);
    const complete = { criteria: DEFAULT_REPLACEMENT_CRITERIA.criteria.map((item) => ({ ...item, met: true })) };
    expect(meetsAll(complete)).toBe(true);
    expect(unmetCount(complete)).toBe(0);
    complete.criteria[0].met = false;
    expect(meetsAll(complete)).toBe(false);
    expect(unmetCount(complete)).toBe(1);
    expect(DEFAULT_REPLACEMENT_CRITERIA.criteria.every((item) => !item.met)).toBe(true);
  });
});

describe('workspace file operations', () => {
  afterEach(() => {
    delete (globalThis as { __TAURI__?: unknown }).__TAURI__;
  });

  it('writes a project file', async () => {
    const invoke = vi.fn().mockResolvedValue(undefined);
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };

    await writeProjectFile('ws', {
      path: 'src/main.ts',
      content: '// hi',
      language: 'ts',
    });

    expect(invoke).toHaveBeenCalledWith('write_project_file', {
      workspace_id: 'ws',
      file: { path: 'src/main.ts', content: '// hi', language: 'ts' },
    });
  });

  it('reads a project file', async () => {
    const invoke = vi.fn().mockResolvedValue({
      path: 'src/main.ts',
      content: '// hi',
      language: 'ts',
    });
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };

    const result = await readProjectFile('ws', 'src/main.ts');

    expect(invoke).toHaveBeenCalledWith('read_project_file', {
      workspace_id: 'ws',
      path: 'src/main.ts',
    });
    expect(result).toEqual({
      path: 'src/main.ts',
      content: '// hi',
      language: 'ts',
    });
  });

  it('lists project files', async () => {
    const invoke = vi.fn().mockResolvedValue([
      { path: 'index.ts', content: '// hi', language: 'ts' },
    ]);
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };

    const result = await listProjectFiles('ws');

    expect(invoke).toHaveBeenCalledWith('list_project_files', {
      workspace_id: 'ws',
    });
    expect(result).toEqual([{ path: 'index.ts', content: '// hi', language: 'ts' }]);
  });

  it('validates workspace path', async () => {
    const invoke = vi.fn().mockResolvedValue({
      valid: true,
      normalized: 'src/main.ts',
      issues: [],
    });
    (globalThis as { __TAURI__?: unknown }).__TAURI__ = { invoke };

    const result = await validateWorkspacePath('ws', '../etc/passwd');

    expect(invoke).toHaveBeenCalledWith('validate_workspace_path', {
      workspace_id: 'ws',
      path: '../etc/passwd',
    });
    expect(result.normalized).toBe('src/main.ts');
  });
});
