import { useState, useEffect, useCallback, useRef, type ReactNode } from 'react';
import AppShell, { type View, NAV_ITEMS } from './components/AppShell';
import CommandPalette, { type PaletteAction } from './components/CommandPalette';
import { ToastProvider, useToast } from './components/Toast';
import { ErrorBoundary } from './components/ErrorBoundary';
import ProjectView from './views/ProjectView';
import AgentChatView from './views/AgentChatView';
import RunnerView from './views/RunnerView';
import DiagnosticsView from './views/DiagnosticsView';
import PlansView from './views/PlansView';
import AIView from './views/AIView';
import LearningView from './views/LearningView';
import TelemetryView from './views/TelemetryView';
import SettingsView from './views/SettingsView';
import UpdatesView from './views/UpdatesView';
import RunHistoryView from './views/RunHistoryView';
import { useRunPipeline } from './hooks/useRunPipeline';
import type { RunState, RightPanelTab, ConsoleEntry, BriefReviewRequest } from './types/frontend';
import { ThemeProvider, useTheme, type Accent } from './lib/theme';
import { executeAgentTask } from './lib/ipc';
import { attemptSelfHeal } from './hooks/useSelfHeal';

const STORAGE_KEYS = {
  lastView: 'recode.lastView',
  sidebarOpen: 'recode.sidebarOpen',
  rightPanelOpen: 'recode.rightPanelOpen',
  rightPanelTab: 'recode.rightPanelTab',
} as const;

const ViewWithErrorBoundary = ({ children }: { children: ReactNode }) => (
  <ErrorBoundary
    fallback={
      <div className="view-enter" style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', minHeight: 300 }}>
        <div style={{ fontSize: 48, marginBottom: 16 }}>⚠</div>
        <h2 style={{ margin: '0 0 12', fontSize: 20 }}>View failed to load</h2>
        <p style={{ color: 'var(--text-secondary)', margin: '0 0 16' }}>This view encountered an error. Other views should still work.</p>
        <button onClick={() => window.location.reload()} className="secondary">Reload app</button>
      </div>
    }
  >
    {children}
  </ErrorBoundary>
);

function AppContent() {
  const { showToast } = useToast();
  const [view, setView] = useState<View>(() => {
    if (typeof window !== 'undefined') {
      const stored = window.localStorage.getItem(STORAGE_KEYS.lastView) as View | null;
      if (stored && NAV_ITEMS.some((item) => item.id === stored)) return stored;
    }
    return 'project';
  });
  const [rightPanelOpen, setRightPanelOpen] = useState(() => {
    if (typeof window !== 'undefined') {
      return window.localStorage.getItem(STORAGE_KEYS.rightPanelOpen) === 'true';
    }
    return false;
  });
  const [rightPanelTab, setRightPanelTab] = useState<RightPanelTab>(() => {
    if (typeof window !== 'undefined') {
      const stored = window.localStorage.getItem(STORAGE_KEYS.rightPanelTab) as RightPanelTab | null;
      if (stored) return stored;
    }
    return 'details';
  });
  const [consoleOpen, setConsoleOpen] = useState(false);
  const [runState, setRunState] = useState<RunState>({ status: 'idle' });
  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState<string | null>(null);
  const [verifierConsent, setVerifierConsent] = useState<'not-supplied' | 'granted' | 'denied'>('not-supplied');
  const [briefReview, setBriefReview] = useState<BriefReviewRequest | null>(null);
  const [consoleEntries, setConsoleEntries] = useState<ConsoleEntry[]>([]);
  const { execute, cancel, loading, result, events, activeRunId } = useRunPipeline();
  const [paletteOpen, setPaletteOpen] = useState(false);
  const { accent, setAccent, themeMode, setThemeMode, density, setDensity } = useTheme();
  const cancelledRunRef = useRef<string | null>(null);
  const activeGoalRef = useRef('');
  const runStartedAtRef = useRef<number | null>(null);
  const processedPipelineEventsRef = useRef(0);
  const consoleOpenRef = useRef(false);


  useEffect(() => {
    consoleOpenRef.current = consoleOpen;
  }, [consoleOpen]);

  useEffect(() => {
    const handleGlobalKeys = (event: KeyboardEvent) => {
      if (event.defaultPrevented || (event.target instanceof HTMLElement && event.target.closest('input, textarea, select, [contenteditable="true"], [role="dialog"]'))) {
        return;
      }
      // Backtick (`) toggles console
      if (event.key === '`') {
        event.preventDefault();
        const next = !consoleOpenRef.current;
        setConsoleOpen(next);
        return;
      }
    };
    window.addEventListener('keydown', handleGlobalKeys);
    return () => window.removeEventListener('keydown', handleGlobalKeys);
  }, [setView]);

  useEffect(() => {
    window.localStorage.setItem(STORAGE_KEYS.lastView, view);
  }, [view]);

  useEffect(() => {
    window.localStorage.setItem(STORAGE_KEYS.rightPanelOpen, String(rightPanelOpen));
  }, [rightPanelOpen]);

  useEffect(() => {
    window.localStorage.setItem(STORAGE_KEYS.rightPanelTab, rightPanelTab);
  }, [rightPanelTab]);

  useEffect(() => {
    if (events.length === 0) return;
    const newEvents = events.slice(processedPipelineEventsRef.current);
    processedPipelineEventsRef.current = events.length;
    if (newEvents.length === 0) return;
    setConsoleEntries((current) => {
      const existing = new Set(current.map((entry) => `${entry.source}:${entry.ts}:${entry.message}`));
      return [
        ...current,
        ...newEvents.flatMap((event, index) => {
          const key = `pipeline:${event.ts}:${event.message}`;
          if (existing.has(key)) return [];
          existing.add(key);
          return [{
            id: `pipeline-event-${event.runId}-${event.ts}-${index}`,
            source: 'pipeline' as const,
            level: event.severity,
            message: event.message,
            ts: event.ts,
          }];
        }),
      ];
    });
  }, [events]);

  useEffect(() => {
    if (!result || cancelledRunRef.current === result.runId || cancelledRunRef.current === '*') return;
    if (result.success) {
      if (!result.run) return;
      const run = result.run;
      setRunState({ 
        status: 'completed', 
        goal: run.plan.goal, 
        planId: run.plan.id,
        steps: run.plan.steps,
        repairActions: run.repair_actions,
        generatedFiles: run.generated_files,
        failedFiles: run.failed_files,
        outcome: run.outcome,
        durationMs: runStartedAtRef.current ? Date.now() - runStartedAtRef.current : undefined,
      });
      const resultEntries: ConsoleEntry[] = (run.events ?? []).map((event, index) => ({
        id: `${result.runId}-result-${index}`,
        source: 'pipeline',
        level: event.severity === 'error' ? 'error' : event.severity === 'warn' ? 'warn' : 'info',
        message: event.message,
        ts: Number.parseInt(event.ts, 10) || Date.now(),
      }));
      setConsoleEntries((current) => {
        const existing = new Set(current.map((entry) => `${entry.source}:${entry.ts}:${entry.message}`));
        return [
          ...current,
          ...resultEntries.filter((entry) => !existing.has(`${entry.source}:${entry.ts}:${entry.message}`)),
        ];
      });
      showToast({
        type: 'success',
        title: run.outcome?.success ? 'Files generated' : 'Plan simulated',
        message: run.outcome?.success
          ? `Generated ${run.outcome.generated_file_count} files for plan ${run.plan.id}`
          : `Plan ${run.plan.id} finished; no project files generated`,
      });
    } else {
      const message = result.error ?? 'Pipeline failed';
      setRunState({ status: 'failed', goal: activeGoalRef.current, error: message, run: result.run, durationMs: runStartedAtRef.current ? Date.now() - runStartedAtRef.current : undefined });
      setConsoleEntries((current) => [
        ...current,
        { id: `run-${Date.now()}`, source: 'pipeline', level: 'error', message, ts: Date.now() },
      ]);
      showToast({
        type: 'error',
        title: 'Pipeline failed',
        message: message,
      });
    }
  }, [result, showToast]);


  useEffect(() => {
    const titleParts = ['recode'];
    if (view !== 'project') titleParts.push(view);
    if (runState.status === 'running') titleParts.push(`Running: ${runState.goal.slice(0, 40)}`);
    else if (runState.status === 'completed') titleParts.push(`Done: ${runState.planId}`);
    else if (runState.status === 'failed') titleParts.push('Failed');
    document.title = titleParts.join(' — ');
  }, [view, runState]);

  const handleRunAgentTask = useCallback(async (agentId: string, input: string) => {
    setConsoleEntries((current) => [
      ...current,
      { id: `run-${Date.now()}`, source: 'agent', level: 'info', message: `Agent task started: ${agentId}`, ts: Date.now() },
    ]);
    try {
      const taskResult = await executeAgentTask({ agentId, input });
      setConsoleEntries((current) => [
        ...current,
        { id: `run-${Date.now()}`, source: 'agent', level: 'info', message: `Agent task completed: ${taskResult.taskId}`, ts: Date.now() },
      ]);
    } catch (error) {
      setConsoleEntries((current) => [
        ...current,
        { id: `run-${Date.now()}`, source: 'agent', level: 'error', message: error instanceof Error ? error.message : 'agent task failed', ts: Date.now() },
      ]);
    }
  }, []);

  const toggleRightPanel = useCallback(() => setRightPanelOpen((current) => !current), []);
  const lastGoal = runState.status !== 'idle' ? runState.goal : '';
  const handleRunGoal = useCallback((goal: string) => {
    const trimmedGoal = goal.trim();
    if (!trimmedGoal || loading || runState.status === 'running' || briefReview) return;
    cancelledRunRef.current = null;
    activeGoalRef.current = trimmedGoal;
    runStartedAtRef.current = Date.now();
    processedPipelineEventsRef.current = 0;
    setRunState({ status: 'running', goal: trimmedGoal });
    setConsoleEntries([{ id: `run-start-${Date.now()}`, source: 'pipeline', level: 'info', message: `Run started: ${trimmedGoal}`, ts: Date.now() }]);
    void execute({
      goal: trimmedGoal,
      emit_events: true,
      execution_consent: verifierConsent,
      ...(selectedWorkspaceId ? { project_id: selectedWorkspaceId } : {}),
    });
  }, [execute, selectedWorkspaceId, verifierConsent, loading, runState.status, briefReview]);

  const handleReviewBrief = useCallback((request: BriefReviewRequest) => {
    if (!request.goal.trim() || loading || runState.status === 'running') return;
    setBriefReview({ ...request, goal: request.goal.trim() });
    setView('project');
  }, [loading, runState.status]);

  const handleRerun = useCallback((goal?: string) => {
    const nextGoal = (goal ?? lastGoal).trim();
    if (nextGoal) handleReviewBrief({ goal: nextGoal, source: 'last-run', sourceId: 'Last run' });
  }, [handleReviewBrief, lastGoal]);

  const handleCancel = useCallback(() => {
    if (runState.status !== 'running') return;
    cancelledRunRef.current = activeRunId ?? '*';
    void cancel();
    setConsoleEntries((current) => [
      ...current,
      { id: `run-cancelled-${Date.now()}`, source: 'pipeline', level: 'warn', message: 'Run cancelled by user.', ts: Date.now() },
    ]);
    setRunState({ status: 'failed', goal: activeGoalRef.current, error: 'Run cancelled by user.', durationMs: runStartedAtRef.current ? Date.now() - runStartedAtRef.current : undefined });
    runStartedAtRef.current = null;
  }, [activeRunId, cancel, runState.status]);

  const handlePaletteAction = useCallback((action: Exclude<PaletteAction, 'run'>) => {
    if (action === 'clear-console') {
      setConsoleEntries([]);
      return;
    }
    if (action === 'toggle-inspector') {
      toggleRightPanel();
      return;
    }
    if (action === 'copy-console') {
      const text = consoleEntries.map((entry) => `[${entry.source}] ${entry.message}`).join('\n');
      if (navigator.clipboard) {
        void navigator.clipboard.writeText(text);
      }
      return;
    }
    void attemptSelfHeal()
      .then((actions) => {
        setConsoleEntries((current) => [
          ...current,
          ...actions.map((actionItem, index) => ({
            id: `self-heal-${Date.now()}-${index}`,
            source: 'self_heal' as const,
            level: actionItem.applied ? 'info' as const : 'warn' as const,
            message: actionItem.description,
            ts: Date.now(),
          })),
        ]);
      })
      .catch((error: unknown) => {
        setConsoleEntries((current) => [
          ...current,
          { id: `self-heal-error-${Date.now()}`, source: 'self_heal', level: 'error', message: error instanceof Error ? error.message : 'Self-heal failed', ts: Date.now() },
        ]);
      });
  }, [consoleEntries, toggleRightPanel]);

  const accentColors: Record<Accent, string> = {
  violet: '#8b5cf6',
  emerald: '#10b981',
  sky: '#38bdf8',
  amber: '#f59e0b',
  rose: '#f43f5e',
};

const accents: { value: Accent; label: string }[] = [
  { value: 'violet', label: 'Violet' },
  { value: 'emerald', label: 'Emerald' },
  { value: 'sky', label: 'Sky' },
  { value: 'amber', label: 'Amber' },
  { value: 'rose', label: 'Rose' },
];

const themeModes: { value: 'light' | 'dark' | 'system'; label: string }[] = [
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
  { value: 'system', label: 'System' },
];

const densities: { value: 'compact' | 'comfortable'; label: string }[] = [
  { value: 'compact', label: 'Compact' },
  { value: 'comfortable', label: 'Comfortable' },
];

  return (
    <>
      <CommandPalette
        view={view}
        onNavigate={setView}
        onRunGoal={handleRunGoal}
        onAction={handlePaletteAction}
        open={paletteOpen}
        onOpenChange={setPaletteOpen}
      />
      <AppShell
        view={view}
        onNavigate={setView}
        onOpenCommandPalette={() => setPaletteOpen(true)}
        onToggleInspector={toggleRightPanel}
        onToggleConsole={() => setConsoleOpen((current) => !current)}
        runState={runState}
        rightPanel={
          rightPanelOpen
            ? {
                tab: rightPanelTab,
                onChangeTab: setRightPanelTab,
                onClose: toggleRightPanel,
                runState,
                consoleEntries,
                onRunAgentTask: handleRunAgentTask,
                onToggleConsole: () => setConsoleOpen((current) => !current),
              }
            : null
        }
        console={
          consoleOpen
            ? { entries: consoleEntries, runState, onRunAgentTask: handleRunAgentTask, onClose: () => setConsoleOpen(false) }
            : null
        }
        accentPicker={
          <div className="stack-sm">
            <div className="text-xs text-secondary font-semibold">Accent</div>
            <div className="flex-start" style={{ gap: 8, flexWrap: 'wrap' }}>
              {accents.map((item) => (
                <button
                  key={item.value}
                  type="button"
                  onClick={() => setAccent(item.value)}
                  className={`ghost ${accent === item.value ? 'active-accent' : ''}`}
                  style={{
                    width: 28,
                    height: 28,
                    borderRadius: '50%',
                    border: '2px solid',
                    borderColor: accent === item.value ? 'var(--accent)' : 'var(--border-subtle)',
                    backgroundColor: accentColors[item.value],
                    padding: 0,
                    boxShadow: accent === item.value ? '0 0 0 2px var(--accent-soft)' : 'none',
                  }}
                  aria-label={item.label}
                  title={item.label}
                />
              ))}
            </div>
            <div className="text-xs text-secondary font-semibold" style={{ marginTop: 12 }}>Theme</div>
            <div className="flex-start" style={{ gap: 6, flexWrap: 'wrap' }}>
              {themeModes.map((item) => (
                <button
                  key={item.value}
                  type="button"
                  onClick={() => setThemeMode(item.value as 'light' | 'dark' | 'system')}
                  className={`ghost ${themeMode === item.value ? 'active-accent' : ''}`}
                  style={{
                    padding: '6px 10px',
                    borderRadius: 'var(--radius-pill)',
                    border: '1px solid',
                    borderColor: themeMode === item.value ? 'var(--accent)' : 'var(--border-subtle)',
                    color: themeMode === item.value ? 'var(--text-primary)' : 'var(--text-secondary)',
                    background: themeMode === item.value ? 'var(--accent-soft)' : 'transparent',
                    fontSize: 12,
                  }}
                >
                  {item.label}
                </button>
              ))}
            </div>
            <div className="text-xs text-secondary font-semibold" style={{ marginTop: 12 }}>Density</div>
            <div className="flex-start" style={{ gap: 6, flexWrap: 'wrap' }}>
              {densities.map((item) => (
                <button
                  key={item.value}
                  type="button"
                  onClick={() => setDensity(item.value as 'compact' | 'comfortable')}
                  className={`ghost ${density === item.value ? 'active-accent' : ''}`}
                  style={{
                    padding: '6px 10px',
                    borderRadius: 'var(--radius-pill)',
                    border: '1px solid',
                    borderColor: density === item.value ? 'var(--accent)' : 'var(--border-subtle)',
                    color: density === item.value ? 'var(--text-primary)' : 'var(--text-secondary)',
                    background: density === item.value ? 'var(--accent-soft)' : 'transparent',
                    fontSize: 12,
                  }}
                >
                  {item.label}
                </button>
              ))}
            </div>
          </div>
        }
      >
        <ViewWithErrorBoundary>
          {view === 'project' && (
            <ProjectView
              briefReview={briefReview}
              onBriefReviewed={() => setBriefReview(null)}
              onNavigate={setView}
              onWorkspaceSelected={setSelectedWorkspaceId}
              verifierConsent={verifierConsent}
              onVerifierConsentChange={setVerifierConsent}
              onOpenRunner={() => {
                setView('runner');
              }}
              runState={runState}
              onRunGoal={handleRunGoal}
              loading={loading}
            />
          )}
          {view === 'agent' && (
            <AgentChatView onOpenConsole={() => { setView('runner'); setConsoleOpen(true); }} />
          )}
          {view === 'runner' && (
            <RunnerView
              startedAt={runStartedAtRef.current}
              runState={runState}
              consoleEntries={consoleEntries}
              onToggleRightPanel={toggleRightPanel}
              onCancel={handleCancel}
              onRerun={handleRerun}
              onClearConsole={() => setConsoleEntries([])}
            />
          )}
          {view === 'diagnostics' && <DiagnosticsView runState={runState} onRunAgentTask={handleRunAgentTask} />}
          {view === 'plans' && <PlansView runState={runState} onUsePlan={(plan) => handleReviewBrief({ goal: plan.goal, source: 'saved-plan', sourceId: plan.id })} />}
          {view === 'ai' && <AIView />}
          {view === 'learning' && <LearningView />}
          {view === 'history' && <RunHistoryView reviewDisabled={loading || runState.status === 'running'} onReviewGoal={(goal, runId) => handleReviewBrief({ goal, source: 'run-history', sourceId: runId })} />}
          {view === 'telemetry' && <TelemetryView />}
          {view === 'settings' && <SettingsView runState={runState} onRunAgentTask={handleRunAgentTask} />}
          {view === 'updates' && <UpdatesView />}
        </ViewWithErrorBoundary>
      </AppShell>
    </>
  );
}

export default function App() {
  return (
    <ThemeProvider>
      <ToastProvider>
        <AppContent />
      </ToastProvider>
    </ThemeProvider>
  );
}