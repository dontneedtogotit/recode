import { useState, useEffect, useCallback, useRef } from 'react';
import { listPlans as listTypedPlans, savePlan as saveTypedPlan } from '../lib/ipc';

export type PlanRecord = {
  id: string;
  goal: string;
};

type UsePlansOptions = {
  autoRefresh?: boolean;
};

export function usePlans(options: UsePlansOptions = {}) {
  const { autoRefresh = true } = options;
  const [plans, setPlans] = useState<PlanRecord[]>([]);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const savingRef = useRef(false);
  const requestRef = useRef(0);
  const mountedRef = useRef(true);
  const pendingSaveRef = useRef<PlanRecord | null>(null);

  const refreshPlans = useCallback(async () => {
    if (savingRef.current) return;
    const request = ++requestRef.current;
    setLoading(true);
    setLoadError(null);
    try {
      const rows = await listTypedPlans();
      if (mountedRef.current && request === requestRef.current) {
        setPlans(rows.map((row) => ({ id: row.id, goal: row.goal })));
      }
    } catch (error) {
      if (mountedRef.current && request === requestRef.current) {
        setLoadError(error instanceof Error ? error.message : 'Could not load saved plans');
      }
    } finally {
      if (mountedRef.current && request === requestRef.current) setLoading(false);
    }
  }, []);

  const savePlan = useCallback(async (goal: string): Promise<boolean> => {
    const trimmed = goal.trim();
    if (!trimmed || savingRef.current) return false;
    savingRef.current = true;
    setSaving(true);
    setSaveError(null);
    try {
      // Reuse the ID after an uncertain response: native save_plan upserts by ID.
      const record = pendingSaveRef.current?.goal === trimmed
        ? pendingSaveRef.current
        : { id: `plan-${crypto.randomUUID()}`, goal: trimmed };
      pendingSaveRef.current = record;
      await saveTypedPlan(record);
      if (!mountedRef.current) return false;
      requestRef.current += 1; // A stale refresh must not erase this confirmed save.
      setLoading(false);
      setPlans((current) => [record, ...current.filter((plan) => plan.id !== record.id)]);
      pendingSaveRef.current = null;
      return true;
    } catch (error) {
      if (mountedRef.current) setSaveError(error instanceof Error ? error.message : 'Could not save the plan');
      return false;
    } finally {
      savingRef.current = false;
      if (mountedRef.current) setSaving(false);
    }
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    if (autoRefresh) void refreshPlans();
    return () => { mountedRef.current = false; requestRef.current += 1; };
  }, [autoRefresh, refreshPlans]);

  return { plans, loading, saving, loadError, saveError, refreshPlans, savePlan };
}
