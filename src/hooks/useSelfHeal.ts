import { attemptSelfHeal as attemptTypedSelfHeal, type RepairAction } from '../lib/ipc';

export type { RepairAction } from '../lib/ipc';

export async function attemptSelfHeal(): Promise<RepairAction[]> {
  return attemptTypedSelfHeal();
}
