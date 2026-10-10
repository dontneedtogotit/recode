import {
  listAgentTaskRuns as listTypedAgentTaskRuns,
  saveAgentTaskRun as saveTypedAgentTaskRun,
  type AgentTaskRunRecord,
} from './ipc';

export type AgentTaskRecord = AgentTaskRunRecord;
export type { AgentTaskRunRecord } from './ipc';

export async function saveAgentTaskRun(record: AgentTaskRecord): Promise<void> {
  await saveTypedAgentTaskRun(record);
}

export async function listAgentTaskRuns(agentId: string): Promise<AgentTaskRecord[]> {
  return listTypedAgentTaskRuns(agentId);
}
