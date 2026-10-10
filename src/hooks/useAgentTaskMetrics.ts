import { listAgentTaskMetrics as listTypedAgentTaskMetrics, type AgentTaskMetrics } from '../lib/ipc';

export type { AgentTaskMetrics } from '../lib/ipc';

export async function loadAgentTaskMetrics(): Promise<AgentTaskMetrics[]> {
  try {
    return await listTypedAgentTaskMetrics();
  } catch {
    return [];
  }
}
