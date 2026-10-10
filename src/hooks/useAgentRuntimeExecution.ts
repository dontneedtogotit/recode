import { executeAgentTask as executeTypedAgentTask } from '../lib/ipc';

export type AgentRuntimeExecutionResult = {
  taskId: string;
  status: 'success' | 'error';
  output?: unknown;
  error?: string;
  durationMs?: number;
};

export async function executeAgentTask(
  task: { id: string; input: string },
): Promise<AgentRuntimeExecutionResult> {
  const start = Date.now();
  const result = await executeTypedAgentTask({ agentId: task.id, input: task.input });
  const durationMs = Date.now() - start;

  if (!result.taskId) {
    return {
      taskId: task.id,
      status: 'error',
      error: 'missing task id in response',
      durationMs,
    };
  }

  return {
    taskId: result.taskId,
    status: result.success && !result.error ? 'success' : 'error',
    output: result.output,
    error: result.error ?? undefined,
    durationMs,
  };
}
