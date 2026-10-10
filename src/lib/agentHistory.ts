export type AgentTaskRecord = {
  id: string;
  agentId: string;
  input: string;
  success: boolean;
  output?: string;
  error?: string;
  durationMs?: number;
  timestamp: number;
};

export type AgentHistoryState = {
  records: AgentTaskRecord[];
};

const MAX_HISTORY = 50;
const STORAGE_KEY = 'recode.agentHistory';

/** Storage is an adapter; history validation and retention do not depend on the browser. */
export interface AgentHistoryStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

function normalizeRecords(value: unknown): AgentTaskRecord[] {
  if (!Array.isArray(value)) return [];
  const records: AgentTaskRecord[] = [];
  const seen = new Set<string>();
  for (const item of value) {
    if (!item || typeof item !== 'object' || Array.isArray(item)) continue;
    const record = item as Record<string, unknown>;
    if (typeof record.id !== 'string' || !record.id.trim() || seen.has(record.id)
      || typeof record.agentId !== 'string' || !record.agentId.trim()
      || typeof record.input !== 'string' || typeof record.success !== 'boolean'
      || typeof record.timestamp !== 'number' || !Number.isFinite(record.timestamp) || record.timestamp < 0) continue;
    records.push({
      id: record.id,
      agentId: record.agentId,
      input: record.input,
      success: record.success,
      timestamp: record.timestamp,
      ...(typeof record.output === 'string' ? { output: record.output } : {}),
      ...(typeof record.error === 'string' ? { error: record.error } : {}),
      ...(typeof record.durationMs === 'number' && Number.isFinite(record.durationMs) && record.durationMs >= 0
        ? { durationMs: record.durationMs } : {}),
    });
    seen.add(record.id);
    if (records.length === MAX_HISTORY) break;
  }
  return records;
}

export function createAgentHistoryState(): AgentHistoryState {
  return { records: [] };
}

export function pushAgentTaskRecord(
  state: AgentHistoryState,
  record: AgentTaskRecord,
): AgentHistoryState {
  return { records: normalizeRecords([record, ...state.records]) };
}

export function createAgentHistoryRepository(storage: AgentHistoryStorage) {
  return {
    load(): AgentHistoryState {
      try {
        const raw = storage.getItem(STORAGE_KEY);
        const parsed: unknown = raw === null ? [] : JSON.parse(raw);
        return { records: normalizeRecords(parsed) };
      } catch {
        return createAgentHistoryState();
      }
    },
    save(state: AgentHistoryState): boolean {
      try {
        storage.setItem(STORAGE_KEY, JSON.stringify(normalizeRecords(state.records)));
        return true;
      } catch {
        return false;
      }
    },
  };
}

export function loadAgentHistory(): AgentHistoryState {
  try {
    return typeof window === 'undefined'
      ? createAgentHistoryState()
      : createAgentHistoryRepository(window.localStorage).load();
  } catch {
    return createAgentHistoryState();
  }
}

/** Best-effort cache persistence; false means the history was not saved. */
export function saveAgentHistory(state: AgentHistoryState): boolean {
  try {
    return typeof window !== 'undefined' && createAgentHistoryRepository(window.localStorage).save(state);
  } catch {
    return false;
  }
}
