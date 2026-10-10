import { useState, useEffect, useCallback, useRef } from 'react';
import { ArrowRight, Bot, ClipboardList, Code2, FolderOpen, Globe, History, Layers, ShieldCheck, Terminal } from 'lucide-react';
import { createWorkspace, listWorkspaces, type WorkspaceInfo } from '../lib/ipc';
import { Card } from '../components/Card';
import { Field } from '../components/Field';
import { GatewayConfigurationCheck } from '../components/GatewayConfigurationCheck';
import type { View } from '../components/AppShell';
import '../styles/workbench.css';
import type { RunState, BriefReviewRequest } from '../types/frontend';

type ProjectViewProps = {
  onOpenRunner: () => void;
  onWorkspaceSelected: (id: string | null) => void;
  runState: RunState;
  onRunGoal: (goal: string) => void;
  loading: boolean;
  onNavigate?: (view: View) => void;
  briefReview?: BriefReviewRequest | null;
  onBriefReviewed?: () => void;
};

const RECENT_GOALS_KEY = 'recode.recentGoals';
const SELECTED_WORKSPACE_KEY = 'recode.selectedWorkspace';
const BRIEF_KEY = 'recode.projectBrief';
function readStoredValue(key: string): string | null {
  try { return window.localStorage.getItem(key); } catch { return null; }
}
const STARTERS = [
  { title: 'Web application', description: 'Responsive UI, routes & forms', icon: Globe, goal: 'Build a responsive web application for [audience]. Include [key screens], accessible forms, loading and error states, and tests for the main user journey.' },
  { title: 'API service', description: 'Endpoints, validation & tests', icon: Terminal, goal: 'Build an API service for [domain]. Include [endpoints], input validation, structured errors, and tests. Document how to run it locally.' },
  { title: 'Internal tool', description: 'Data tables, filters & workflows', icon: Layers, goal: 'Build an internal tool for [team] to manage [records]. Include a searchable table, detail view, create/edit forms, and empty states. Add tests for filtering and validation.' },
] as const;

export default function ProjectView({ onOpenRunner, onWorkspaceSelected, runState, onRunGoal, loading, onNavigate, briefReview, onBriefReviewed }: ProjectViewProps) {
  const goalRef = useRef<HTMLTextAreaElement>(null);
  const reviewRef = useRef<HTMLElement>(null);
  const [creating, setCreating] = useState(false);
  const [workspaceLoading, setWorkspaceLoading] = useState(true);
  const [workspaceId, setWorkspaceId] = useState('');
  const [workspaces, setWorkspaces] = useState<WorkspaceInfo[]>([]);
  const [selectedWorkspace, setSelectedWorkspace] = useState<WorkspaceInfo | null>(null);
  const [goal, setGoal] = useState(() => readStoredValue(BRIEF_KEY) ?? '');
  const [storageError, setStorageError] = useState(false);
  const persist = useCallback((key: string, value: string | null) => {
    try {
      if (value === null) window.localStorage.removeItem(key);
      else window.localStorage.setItem(key, value);
    } catch { setStorageError(true); }
  }, []);
  const [workspaceError, setWorkspaceError] = useState<string | null>(null);
  const [recentGoals, setRecentGoals] = useState<string[]>(() => {
    if (typeof window !== 'undefined') {
      const stored = readStoredValue(RECENT_GOALS_KEY);
      if (stored) {
        try {
          const parsed: unknown = JSON.parse(stored);
          return Array.isArray(parsed) ? parsed.filter((item): item is string => typeof item === 'string').slice(0, 10) : [];
        } catch {
          return [];
        }
      }
    }
    return [];
  });
  const loadWorkspaces = useCallback(async (preferredId?: string) => {
    setWorkspaceLoading(true);
    try {
      const available = await listWorkspaces();
      setWorkspaces(available);
      const stored = preferredId ?? readStoredValue(SELECTED_WORKSPACE_KEY);
      // Old releases stored a JSON object; trust only the ID after resolving it through Rust.
      let id = stored;
      if (stored?.startsWith('{')) {
        try { id = (JSON.parse(stored) as WorkspaceInfo).id; } catch { id = null; }
      }
      const selected = available.find((item) => item.id === id) ?? null;
      setSelectedWorkspace(selected);
      onWorkspaceSelected(selected?.id ?? null);
      if (!selected) persist(SELECTED_WORKSPACE_KEY, null);
      setWorkspaceError(null);
    } catch (error) {
      setWorkspaces([]);
      setSelectedWorkspace(null);
      onWorkspaceSelected(null);
      setWorkspaceError(error instanceof Error ? error.message : 'Failed to load workspaces');
    } finally {
      setWorkspaceLoading(false);
    }
  }, [onWorkspaceSelected, persist]);

  useEffect(() => {
    void loadWorkspaces();
  }, [loadWorkspaces]);

  useEffect(() => {
    persist(BRIEF_KEY, goal);
  }, [goal, persist]);

  useEffect(() => {
    if (briefReview) reviewRef.current?.focus();
  }, [briefReview]);

  const handleCreateWorkspace = async () => {
    const id = workspaceId.trim();
    if (!id || creating || loading || runState.status === 'running') return;
    setCreating(true);
    setWorkspaceError(null);
    try {
      const info = await createWorkspace({ id });
      if (info.id && info.root) {
        const wsInfo = { id: info.id, root: info.root };
        persist(SELECTED_WORKSPACE_KEY, wsInfo.id);
        await loadWorkspaces(wsInfo.id);
        setWorkspaceId('');
      }
    } catch (error) {
      setWorkspaceError(error instanceof Error ? error.message : 'Workspace creation failed');
    } finally {
      setCreating(false);
    }
  };

  const handleRun = useCallback((nextGoal?: string) => {
    const trimmed = (nextGoal ?? goal).trim();
    if (!trimmed || loading || runState.status === 'running' || workspaceLoading || creating || briefReview) return;
    onRunGoal(trimmed);
    const next = [trimmed, ...recentGoals.filter((g) => g !== trimmed)].slice(0, 10);
    setRecentGoals(next);
    persist(RECENT_GOALS_KEY, JSON.stringify(next));
  }, [goal, onRunGoal, loading, runState.status, workspaceLoading, creating, briefReview, recentGoals, persist]);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    const isMeta = e.ctrlKey || e.metaKey;
    if (isMeta && e.key === 'Enter' && !e.nativeEvent.isComposing) {
      e.preventDefault();
      handleRun();
    }
  };

  const handleWorkspaceSelect = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const next = workspaces.find((item) => item.id === e.target.value) ?? null;
    setSelectedWorkspace(next);
    onWorkspaceSelected(next?.id ?? null);
    if (next) {
      persist(SELECTED_WORKSPACE_KEY, next.id);
    } else {
      persist(SELECTED_WORKSPACE_KEY, null);
    }
  };

  const statusLabel =
    runState.status === 'idle' ? 'Ready' :
    runState.status === 'running' ? 'Running' :
    runState.status === 'completed' ? 'Done' : 'Failed';

  const busy = loading || runState.status === 'running';
  const loadBrief = (value: string) => {
    setGoal(value);
    goalRef.current?.focus();
  };

  return (
    <div className="view-enter project-workbench">
      <div className="page-header">
        <div>
          <div className="eyebrow">From intent to implementation</div>
          <h1 className="page-title">Project</h1>
          <p className="page-subtitle">Define the work. Generate files. Inspect the evidence.</p>
        </div>
        <button type="button" onClick={onOpenRunner} className="secondary icon-button"><Terminal size={16} aria-hidden="true" />Open runner<ArrowRight size={14} aria-hidden="true" /></button>
      </div>

      {briefReview && <section ref={reviewRef} tabIndex={-1} aria-label="Review incoming brief" className="brief-review stack-md">
        <div className="section-label-row"><h2>Review before reusing this goal</h2><span>{briefReview.source === 'saved-plan' ? 'Saved plan' : briefReview.source === 'run-history' ? 'Run history' : 'Last run'} · {briefReview.sourceId}</span></div>
        <p className="text-sm text-secondary">Your current draft is unchanged. Accepting replaces the draft only; it does not start a run. Choose the destination workspace below—the original workspace and runtime are not restored automatically.</p>
        <pre>{briefReview.goal}</pre>
        <div className="flex-start"><button type="button" disabled={busy || !onBriefReviewed} onClick={() => { loadBrief(briefReview.goal); onBriefReviewed?.(); }}>Use this brief</button><button type="button" className="secondary" disabled={!onBriefReviewed} onClick={() => { onBriefReviewed?.(); goalRef.current?.focus(); }}>Keep current brief</button></div>
      </section>}

      <div className="project-grid">
        <section className="project-primary" aria-label="Project brief">
          <Card className="brief-card">
            <div className="brief-heading">
              <span className="surface-icon"><Code2 size={20} aria-hidden="true" /></span>
              <div><h2>What do you want to build?</h2><p>Describe the outcome, constraints, and what good looks like.</p></div>
            </div>
            <Field label="Pipeline goal" htmlFor="project-goal-input">
              <textarea ref={goalRef} id="project-goal-input" value={goal} onChange={(e) => setGoal(e.target.value)} onKeyDown={handleKeyDown} placeholder="Build onboarding flow" rows={7} aria-describedby="brief-help" />
            </Field>
            <div className="brief-footer">
              <span id="brief-help" className="text-xs text-muted">{storageError ? 'Local storage unavailable; changes are kept for this session only' : 'Brief saved locally'} · Ctrl/⌘ + Enter to run</span>
              <button type="button" className="icon-button" onClick={() => handleRun()} disabled={busy || !goal.trim() || workspaceLoading || creating || Boolean(briefReview)}>
                {busy ? (selectedWorkspace ? 'Generating...' : 'Simulating...') : (selectedWorkspace ? 'Generate files' : 'Simulate plan')}<ArrowRight size={15} aria-hidden="true" />
              </button>
            </div>
            <div className="execution-notice">
              <ShieldCheck size={16} aria-hidden="true" />
              <div><strong>{selectedWorkspace ? 'File generation' : 'Plan-only simulation'}</strong><span>{selectedWorkspace ? 'Writes to your managed workspace. Review file and verification results in Runner.' : 'No project files will be written. Select a workspace to enable generation.'}</span></div>
            </div>
          </Card>

          <section aria-label="Starter briefs" className="stack-md">
            <div className="section-label-row"><h2>Start with a brief</h2><span>Editable prompts, not prebuilt templates</span></div>
            <div className="starter-grid">
              {STARTERS.map(({ title, description, icon: Icon, goal: starterGoal }) => (
                <button key={title} type="button" className="starter-card" onClick={() => loadBrief(starterGoal)} disabled={busy}>
                  <Icon size={19} aria-hidden="true" /><strong>{title}</strong><span>{description}</span><ArrowRight size={14} className="starter-arrow" aria-hidden="true" />
                </button>
              ))}
            </div>
          </section>

          {recentGoals.length > 0 && <section className="recent-briefs" aria-label="Recent goals"><div className="section-label-row"><h2>Recent goals</h2><span>Select to review and edit</span></div>{recentGoals.map((g) => <button key={g} type="button" className="ghost recent-brief" title={g} onClick={() => loadBrief(g)} disabled={busy}><History size={15} aria-hidden="true" /><span>{g}</span><ArrowRight size={14} aria-hidden="true" /></button>)}</section>}
        </section>

        <aside className="project-context" aria-label="Execution context">
          <Card className="workspace-card stack-md">
            <div className="context-heading"><FolderOpen size={17} aria-hidden="true" /><h2>Workspace</h2><span className="badge">{workspaces.length}</span></div>
            <Field label="Select workspace" htmlFor="project-workspace-select">
              <select id="project-workspace-select" value={selectedWorkspace?.id ?? ''} onChange={handleWorkspaceSelect} disabled={busy || creating || workspaceLoading}>
                <option value="">No workspace · simulate only</option>
                {workspaces.map((item) => <option key={item.id} value={item.id}>{item.id}</option>)}
              </select>
            </Field>
            {workspaceLoading && <span className="text-xs text-secondary" role="status">Loading workspaces…</span>}
            {selectedWorkspace && <div className="workspace-root"><span className="metric-label">Managed project root</span><code>{selectedWorkspace.root}</code></div>}
            <form className="stack-sm workspace-create" onSubmit={(event) => { event.preventDefault(); void handleCreateWorkspace(); }}>
              <Field label="New workspace id" htmlFor="new-workspace-id"><input id="new-workspace-id" value={workspaceId} onChange={(e) => setWorkspaceId(e.target.value)} placeholder="my-project" disabled={busy || creating} /></Field>
              <button type="submit" className="secondary" disabled={!workspaceId.trim() || busy || creating || workspaceLoading}>{creating ? 'Creating…' : 'Create workspace'}</button>
            </form>
            {workspaceError && <div className="workspace-error" role="alert"><strong>Workspace unavailable</strong><span>{workspaceError}</span><button type="button" className="secondary compact-button" onClick={() => void loadWorkspaces()} disabled={workspaceLoading || busy}>Retry connection</button></div>}
            <p className="context-note">Projects stay in recode’s app data directory. Importing an external folder is not supported yet.</p>
          </Card>

          <Card className="stack-md context-card">
            <div className="context-heading"><ShieldCheck size={17} aria-hidden="true" /><h2>Before you run</h2></div>
            <dl className="readiness-list"><div><dt>Execution mode</dt><dd>{selectedWorkspace ? 'Generate files' : 'Simulation'}</dd></div><div><dt>Current run</dt><dd>{runState.status === 'idle' ? 'Not started' : statusLabel}</dd></div><div><dt>Verification</dt><dd>Reported per run</dd></div></dl>
            <details className="runtime-requirements"><summary>Generation requirements</summary><p>File generation requires RECODE_OPENAI_BASE_URL and RECODE_OPENAI_MODEL in the app environment; saved provider settings are not yet used.</p></details>
            <GatewayConfigurationCheck />
            {onNavigate && <button type="button" className="ghost context-link" onClick={() => onNavigate('ai')}>Inspect gateway<ArrowRight size={14} aria-hidden="true" /></button>}
          </Card>
        </aside>
      </div>

      {runState.status !== 'idle' && <div className={`project-run-notice ${runState.status}`} role="status"><div><span className={`run-status-dot ${runState.status}`} /><strong>{runState.status === 'running' ? `Running: ${runState.goal}` : runState.status === 'failed' ? `Failed: ${runState.error}` : runState.outcome?.success ? `Generated ${runState.outcome.generated_file_count} files for plan ${runState.planId}` : runState.outcome ? `Build failed for plan ${runState.planId}: ${runState.outcome.summary}` : `Simulated plan ${runState.planId}; no project was generated`}</strong></div><button type="button" className="secondary compact-button" onClick={onOpenRunner}>Inspect runner</button></div>}

      {onNavigate && <section className="workflow-links" aria-label="Development tools">
        <button type="button" className="ghost workflow-link" onClick={() => onNavigate('plans')}><ClipboardList size={19} aria-hidden="true" /><span><strong>Organize your plans</strong><small>Keep goals ready for the next session</small></span><ArrowRight size={15} aria-hidden="true" /></button>
        <button type="button" className="ghost workflow-link" onClick={() => onNavigate('agent')}><Bot size={19} aria-hidden="true" /><span><strong>Delegate a task</strong><small>Work with a specialized runtime agent</small></span><ArrowRight size={15} aria-hidden="true" /></button>
        <button type="button" className="ghost workflow-link" onClick={() => onNavigate('history')}><History size={19} aria-hidden="true" /><span><strong>Review past runs</strong><small>Inspect outcomes, files and failures</small></span><ArrowRight size={15} aria-hidden="true" /></button>
      </section>}
    </div>
  );
}