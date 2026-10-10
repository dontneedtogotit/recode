import { useState } from 'react';
import { createAndExecutePlan as createAndExecutePlanCommand } from '../lib/ipc';

export type CreateAndExecutePlanInput = {
  goal: string;
};

export type PlanStepDto = {
  id: string;
  description: string;
  status: string;
};

export type PlanDto = {
  id: string;
  goal: string;
  status: string;
  steps: PlanStepDto[];
};

export type CreateAndExecutePlanResult =
  | { success: true; plan: PlanDto }
  | { success: false; error: string };

export function useCreateAndExecutePlan() {
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<CreateAndExecutePlanResult | null>(null);

  const execute = async (input: CreateAndExecutePlanInput) => {
    setLoading(true);
    setResult(null);
    try {
      const plan = await createAndExecutePlanCommand(input);
      setResult({ success: true, plan });
    } catch (error) {
      const message = error instanceof Error ? error.message : 'unknown error';
      setResult({ success: false, error: message });
    } finally {
      setLoading(false);
    }
    return result;
  };

  return { loading, result, execute };
}
