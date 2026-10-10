import { useMemo, useState } from 'react';
import { ArrowRight, ClipboardList, RefreshCw } from 'lucide-react';
import { Card } from '../components/Card';
import { Field } from '../components/Field';

import { EmptyState } from '../components/EmptyState';
import { SectionHeader } from '../components/SectionHeader';
import type { RunState } from '../types/frontend';
import { usePlans, type PlanRecord } from '../hooks/usePlans';
import '../styles/plans.css';

type PlansViewProps = {
  runState: RunState;
  onUsePlan?: (plan: PlanRecord) => void;
};

export default function PlansView({ runState, onUsePlan }: PlansViewProps) {
  const [goal, setGoal] = useState('');
  const [search, setSearch] = useState('');
  const [saved, setSaved] = useState(false);
  const { savePlan, plans, loading, saving, loadError, saveError, refreshPlans } = usePlans();
  const filteredPlans = useMemo(() => {
    const query = search.trim().toLowerCase();
    return plans.filter((plan) => `${plan.goal} ${plan.id}`.toLowerCase().includes(query));
  }, [plans, search]);

  const handleSave = async () => {
    const trimmed = goal.trim();
    if (!trimmed || saving) return;
    setSaved(false);
    if (await savePlan(trimmed)) {
      setGoal('');
      setSaved(true);
    }
  };

  return (
    <div className="view-enter stack-lg plans-view">
      <div className="page-header">
        <div><div className="eyebrow">Intent library</div><h1 className="page-title">Plans</h1><p className="page-subtitle">Save goals now. Review the brief and workspace before starting a run.</p></div>
        <button type="button" className="secondary icon-button" onClick={() => void refreshPlans()} disabled={loading || saving}><RefreshCw size={15} aria-hidden="true" />{loading ? 'Loading…' : 'Refresh plans'}</button>
      </div>

      <Card className="stack-md">
        <SectionHeader title="Capture a goal" description="A saved goal is a brief, not an approved execution plan." />
        <form className="stack-md" onSubmit={(event) => { event.preventDefault(); void handleSave(); }}>
          <Field label="Plan goal" htmlFor="saved-plan-goal"><textarea id="saved-plan-goal" rows={3} value={goal} disabled={saving} onChange={(event) => { setGoal(event.target.value); setSaved(false); }} placeholder="Build onboarding flow" /></Field>
          <div className="flex-between"><span className="text-xs text-muted">Saved in the local runtime. Saving does not execute work.</span><button type="submit" disabled={!goal.trim() || saving}>{saving ? 'Saving…' : 'Save plan'}</button></div>
          {saveError && <div role="alert" className="text-danger text-sm">{saveError} Your draft is retained; try Save plan again.</div>}
          {saved && <div role="status" className="text-success text-sm">Saved for review. No run started.</div>}
        </form>
      </Card>

      <Card className="stack-md">
        <SectionHeader title="Saved plans" right={<span className="badge">{filteredPlans.length} of {plans.length}</span>} />
        <input aria-label="Search saved plans" placeholder="Search goals or plan IDs…" value={search} onChange={(event) => setSearch(event.target.value)} />
        {loadError && <div role="alert" className="stack-sm"><span className="text-danger text-sm">{loadError}</span><span className="text-xs text-muted">Previously loaded plans may be out of date.</span><button type="button" className="secondary" onClick={() => void refreshPlans()} disabled={loading || saving}>Retry loading plans</button></div>}
        {loading && <div role="status" className="text-secondary text-sm">Loading saved plans…</div>}
        {!loading && !loadError && filteredPlans.length === 0 && <EmptyState title={search.trim() ? 'No matching plans' : 'No saved plans yet'} description={search.trim() ? 'Change the search to find another goal.' : 'Save a goal above to start your intent library.'} />}
        <div className="plan-library">
          {filteredPlans.map((plan) => <article key={plan.id} className="plan-library-row">
            <ClipboardList size={18} aria-hidden="true" />
            <div className="plan-library-copy"><p>{plan.goal}</p><code>{plan.id}</code></div>
            <div className="plan-library-actions"><span className="badge">Saved goal</span><button type="button" className="secondary icon-button" onClick={() => onUsePlan?.(plan)} disabled={!onUsePlan || runState.status === 'running'} aria-label={`Review plan ${plan.id}`}>Use as brief<ArrowRight size={14} aria-hidden="true" /></button></div>
          </article>)}
        </div>
        {runState.status === 'running' && <p className="text-xs text-muted">A run is active. You can save future goals; reviewing one for execution is available when the run finishes.</p>}
      </Card>
    </div>
  );
}
