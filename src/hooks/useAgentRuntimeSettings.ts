import {
  DEFAULT_AGENT_RUNTIME_SETTINGS as IPC_DEFAULT_AGENT_RUNTIME_SETTINGS,
  loadAgentRuntimeSettings as loadTypedAgentRuntimeSettings,
  saveAgentRuntimeSettings as saveTypedAgentRuntimeSettings,
  type AgentRuntimeSettings,
} from '../lib/ipc';

export type { AgentRuntimeSettings } from '../lib/ipc';
export { DEFAULT_AGENT_RUNTIME_SETTINGS } from '../lib/ipc';

export async function loadAgentRuntimeSettings(): Promise<AgentRuntimeSettings> {
  try {
    return (await loadTypedAgentRuntimeSettings()) ?? { ...IPC_DEFAULT_AGENT_RUNTIME_SETTINGS };
  } catch {
    // Return a fresh copy so callers can mutate their result without
    // corrupting the shared default (test contract: independent identities).
    return { ...IPC_DEFAULT_AGENT_RUNTIME_SETTINGS };
  }
}

export async function saveAgentRuntimeSettings(
  settings: AgentRuntimeSettings
): Promise<AgentRuntimeSettings> {
  try {
    return await saveTypedAgentRuntimeSettings(settings);
  } catch {
    // ignore IPC failure in dev fallback mode
    return settings;
  }
}
