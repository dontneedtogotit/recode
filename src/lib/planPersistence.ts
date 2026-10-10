import { listPlans as listTypedPlans, savePlan as saveTypedPlan, type PlanSummary } from './ipc';

export type PlanRecord = PlanSummary;

export async function savePlan(record: PlanRecord): Promise<void> {
  // The native SavePlanInput only stores a draft's id and goal. Never silently
  // discard execution state supplied through this compatibility adapter.
  if (record.steps.length > 0 || record.status.toLowerCase() !== 'pending') {
    throw new Error('Saving plan execution state is not supported by the native draft command');
  }
  await saveTypedPlan({ id: record.id, goal: record.goal });
}

export async function listPlans(): Promise<PlanRecord[]> {
  return listTypedPlans();
}
