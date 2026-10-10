import {
  executeAgentTask as executeTypedAgentTask,
  isTauriAvailable,
  listAgents as listTypedAgents,
  listAgentTaskHistory as listTypedAgentTaskHistory,
  recordAgentTaskHistory as recordTypedAgentTaskHistory,
  registerAgent as registerTypedAgent,
  type AgentDescriptor,
  type AgentTaskHistoryRecord,
} from './ipc';

export type { AgentDescriptor, AgentTaskHistoryRecord } from './ipc';

export type AgentTaskResult = {
  task_id: string;
  agent_id: string;
  success: boolean;
  output: unknown;
  error: string | null;
};

export async function listAgents(): Promise<AgentDescriptor[]> {
  if (!isTauriAvailable()) {
    // Browser preview has no registered runtime agents.
    return [];
  }

  return listTypedAgents();
}

export async function registerAgent(descriptor: AgentDescriptor): Promise<void> {
  await registerTypedAgent(descriptor);
}

export async function executeAgentTask(input: {
  agent_id: string;
  payload: Record<string, unknown>;
}): Promise<AgentTaskResult> {
  const result = await executeTypedAgentTask({
    agentId: input.agent_id,
    payload: input.payload,
  });
  return {
    task_id: result.taskId,
    agent_id: result.agentId,
    success: result.success,
    output: result.output,
    error: result.error,
  };
}

export async function recordAgentTaskHistory(input: AgentTaskHistoryRecord): Promise<void> {
  await recordTypedAgentTaskHistory(input);
}

export async function listAgentTaskHistory(agentId: string): Promise<AgentTaskHistoryRecord[]> {
  return listTypedAgentTaskHistory(agentId);
}
