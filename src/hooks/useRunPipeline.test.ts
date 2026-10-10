import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { cancelPipeline, runPipeline, subscribeToPipelineEvents, type PipelineEvent, type PipelineRun } from '../lib/ipc';
import { useRunPipeline } from './useRunPipeline';

vi.mock('../lib/ipc', () => ({
  cancelPipeline: vi.fn(),
  runPipeline: vi.fn(),
  subscribeToPipelineEvents: vi.fn(),
}));

const completedRun = {
  plan: {
    id: 'plan-1',
    goal: 'ship it',
    status: 'completed',
    steps: [],
  },
  repair_actions: [],
  events: [],
};

describe('useRunPipeline', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(subscribeToPipelineEvents).mockReturnValue(() => undefined);
    vi.mocked(runPipeline).mockResolvedValue(completedRun);
    vi.mocked(cancelPipeline).mockResolvedValue(true);
  });

  it('reports a failed native build outcome without discarding its run details', async () => {
    const failedRun: PipelineRun = {
      ...completedRun,
      failed_files: [{ path: 'src/main.ts', status: 'failed', error: 'write failed' }],
      outcome: {
        success: false,
        degraded: true,
        generated_file_count: 0,
        failed_file_count: 1,
        failed_files: [{ path: 'src/main.ts', status: 'failed', error: 'write failed' }],
        summary: 'generated 0 of 1 files',
      },
    };
    vi.mocked(runPipeline).mockResolvedValue(failedRun);
    const hook = renderHook(() => useRunPipeline());
    await act(async () => {
      await hook.result.current.execute({ goal: 'ship it', project_id: 'project-1', runId: 'run-1' });
    });
    expect(hook.result.current.result).toMatchObject({
      success: false,
      run: failedRun,
      error: failedRun.outcome?.summary,
    });
    expect(hook.result.current.lastFailedFiles).toEqual(failedRun.failed_files);
  });

  it('keeps plan-only simulation distinct from a failed registered build', async () => {
    vi.mocked(runPipeline).mockResolvedValue({
      ...completedRun,
      outcome: {
        success: false,
        degraded: false,
        generated_file_count: 0,
        failed_file_count: 0,
        failed_files: [],
        summary: 'no files generated',
      },
    });
    const hook = renderHook(() => useRunPipeline());
    await act(async () => {
      await hook.result.current.execute({ goal: 'ship it', runId: 'simulation-1' });
    });
    expect(hook.result.current.result?.success).toBe(true);
  });

  it('exposes the active run id and sends it to the cancel command', async () => {
    let resolveRun: (value: typeof completedRun) => void = () => undefined;
    vi.mocked(runPipeline).mockReturnValue(new Promise((resolve) => {
      resolveRun = resolve;
    }));
    const hook = renderHook(() => useRunPipeline());
    let executePromise: Promise<void> | undefined;

    act(() => {
      executePromise = hook.result.current.execute({ goal: 'ship it', runId: 'run-1' });
    });
    await waitFor(() => expect(hook.result.current.activeRunId).toBe('run-1'));

    await act(async () => {
      await hook.result.current.cancel();
    });
    expect(cancelPipeline).toHaveBeenCalledWith('run-1');

    await act(async () => {
      resolveRun(completedRun);
      await executePromise;
    });
    await waitFor(() => expect(hook.result.current.activeRunId).toBeNull());
  });

  it('keeps only events for the active run', async () => {
    let eventListener: ((event: PipelineEvent) => void) | undefined;
    vi.mocked(subscribeToPipelineEvents).mockImplementation((listener) => {
      eventListener = listener;
      return () => undefined;
    });
    vi.mocked(runPipeline).mockReturnValue(new Promise(() => undefined));
    const hook = renderHook(() => useRunPipeline());
    const event = {
      runId: 'run-1',
      topic: 'pipeline.plan_completed',
      severity: 'info' as const,
      message: 'done',
      ts: 123,
    };

    act(() => {
      void hook.result.current.execute({ goal: 'ship it', runId: 'run-1' });
    });
    await waitFor(() => expect(hook.result.current.activeRunId).toBe('run-1'));
    act(() => {
      eventListener?.(event);
    });
    expect(hook.result.current.events).toEqual([event]);
    hook.unmount();
  });
});
