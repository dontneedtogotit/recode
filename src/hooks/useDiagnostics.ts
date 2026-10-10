import {
  attemptSelfHeal as attemptTypedSelfHeal,
  captureDiagnostics as captureTypedDiagnostics,
  listMonitorEvents as listTypedMonitorEvents,
  type MonitorEvent,
  type RepairAction,
} from '../lib/ipc';

export type { MonitorEvent, RepairAction } from '../lib/ipc';

export async function listMonitorEvents(): Promise<MonitorEvent[]> {
  return listTypedMonitorEvents();
}

export async function attemptSelfHeal(): Promise<RepairAction[]> {
  return attemptTypedSelfHeal();
}

export async function captureDiagnostics(input: { kind: string; message: string; detail?: string }): Promise<void> {
  await captureTypedDiagnostics(input);
}
