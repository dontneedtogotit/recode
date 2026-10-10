import { listMonitorEvents as listTypedMonitorEvents, type MonitorEvent } from '../lib/ipc';

export type { MonitorEvent } from '../lib/ipc';

export async function listMonitorEvents(): Promise<MonitorEvent[]> {
  return listTypedMonitorEvents();
}
