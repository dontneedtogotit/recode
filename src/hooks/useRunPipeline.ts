import { useCallback, useEffect, useRef, useState } from 'react';
import {
  cancelPipeline as cancelPipelineCommand,
  runPipeline,
  subscribeToPipelineEvents,
  type PipelineEvent,
  type PipelineInput,
  type PipelineRun,
} from '../lib/ipc';

export type { PipelineInput, PipelineRun };
export type PipelineRunResult =
  | { success: true; run: PipelineRun; runId: string }
  | { success: false; error: string; runId: string; run?: PipelineRun };

function createRunId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

export function useRunPipeline() {
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<PipelineRunResult | null>(null);
  const [events, setEvents] = useState<PipelineEvent[]>([]);
  const [activeRunId, setActiveRunId] = useState<string | null>(null);
  const runIdRef = useRef<string | null>(null);

  useEffect(() => {
    return subscribeToPipelineEvents((event) => {
      if (event.runId === runIdRef.current) {
        setEvents((current) => [...current, event]);
      }
    });
  }, []);

  const execute = useCallback(async (input: PipelineInput) => {
    const runId = input.runId ?? createRunId();
    runIdRef.current = runId;
    setActiveRunId(runId);
    setEvents([]);
    setResult(null);
    setLoading(true);
    try {
      const response = await runPipeline({ ...input, runId });
      if (runIdRef.current === runId) {
        setResult(input.project_id && response.outcome?.success === false
          ? { success: false, run: response, runId, error: response.outcome.summary }
          : { success: true, run: response, runId });
      }
    } catch (error) {
      if (runIdRef.current === runId) {
        setResult({
          success: false,
          error: error instanceof Error ? error.message : 'pipeline run failed',
          runId,
        });
      }
    } finally {
      if (runIdRef.current === runId) {
        setLoading(false);
        setActiveRunId(null);
        runIdRef.current = null;
      }
    }
  }, []);

  const cancel = useCallback(async () => {
    const runId = runIdRef.current;
    if (!runId) return false;
    try {
      return await cancelPipelineCommand(runId);
    } catch {
      return false;
    }
  }, []);

  const lastOutcome = result?.run?.outcome ?? null;
  const lastFailedFiles = result?.run?.failed_files ?? [];

  return { loading, result, events, activeRunId, execute, cancel, lastOutcome, lastFailedFiles };
}
