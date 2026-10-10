import { useMemo, useRef, useEffect, useState, useCallback } from 'react';
import { Card } from '../components/Card';
import { EmptyState } from '../components/EmptyState';
import { SectionHeader } from '../components/SectionHeader';
import RunCompletionSummary from '../components/RunCompletionSummary';
import { VerificationEvidence } from '../components/VerificationEvidence';
import type { RunState, ConsoleEntry } from '../types/frontend';
import { Circle, CircleCheck, CircleX, LoaderCircle } from 'lucide-react';
import '../styles/runner.css';

type RunnerViewProps = {
  runState: RunState;
  consoleEntries: ConsoleEntry[];
  onToggleRightPanel: () => void;
  onCancel?: () => void;
  onRerun?: (goal: string) => void;
  onClearConsole?: () => void;
  startedAt?: number | null;
};

const STEP_STATUS_ICON = {
  pending: Circle,
  in_progress: LoaderCircle,
  completed: CircleCheck,
  failed: CircleX,
} as const;

const STEP_STATUS_LABEL = {
  pending: 'Pending',
  in_progress: 'Running',
  completed: 'Done',
  failed: 'Failed',
} as const;

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  const seconds = Math.floor(ms / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  return `${minutes}m ${remainingSeconds}s`;
}

export default function RunnerView({ runState, consoleEntries, onToggleRightPanel, onCancel, onRerun, onClearConsole, startedAt }: RunnerViewProps) {
  const visibleEntries = useMemo(() => consoleEntries.slice(-300), [consoleEntries]);
  const latest = visibleEntries[visibleEntries.length - 1];
  const lastGoal = runState.status !== 'idle' ? runState.goal : latest?.message ?? '';

  const consoleRef = useRef<HTMLDivElement>(null);
  const [autoScroll, setAutoScroll] = useState(true);
  const [showScrollIndicator, setShowScrollIndicator] = useState(false);
  const [elapsedMs, setElapsedMs] = useState(0);
  const [levelFilter, setLevelFilter] = useState<'all' | 'info' | 'warn' | 'error'>('all');
  const [sourceFilter, setSourceFilter] = useState<string>('all');
  const [search, setSearch] = useState('');
  const [copyStatus, setCopyStatus] = useState('');

  const filteredEntries = useMemo(() => {
    return visibleEntries.filter((entry) => {
      if (levelFilter !== 'all' && entry.level !== levelFilter) return false;
      if (sourceFilter !== 'all' && entry.source !== sourceFilter) return false;
      return `${entry.message} ${entry.source}`.toLowerCase().includes(search.trim().toLowerCase());
    });
  }, [visibleEntries, levelFilter, sourceFilter, search]);

  const sources = useMemo(() => [...new Set(visibleEntries.map((e) => e.source))], [visibleEntries]);

  const copyAllEntries = useCallback(async () => {
    const text = filteredEntries.map((entry) => `[${entry.source}] ${entry.message}`).join('\n');
    try {
      if (!navigator.clipboard) throw new Error('Clipboard unavailable');
      await navigator.clipboard.writeText(text);
      setCopyStatus('Copied filtered output');
    } catch {
      setCopyStatus('Copy unavailable. Export JSON instead.');
    }
  }, [filteredEntries]);

  const exportJson = useCallback(() => {
    const data = JSON.stringify(filteredEntries, null, 2);
    const blob = new Blob([data], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `console-export-${Date.now()}.json`;
    a.click();
    URL.revokeObjectURL(url);
  }, [filteredEntries]);

  const clearEntries = useCallback(() => {
    onClearConsole?.();
  }, [onClearConsole]);

  const scrollToBottom = useCallback(() => {
    if (consoleRef.current) {
      consoleRef.current.scrollTop = consoleRef.current.scrollHeight;
    }
  }, []);

  useEffect(() => {
    if (runState.status !== 'running' || startedAt == null) {
      setElapsedMs(0);
      return;
    }
    const update = () => setElapsedMs(Math.max(0, Date.now() - startedAt));
    update();
    const interval = setInterval(update, 1000);
    return () => clearInterval(interval);
  }, [runState.status, startedAt]);

  useEffect(() => {
    if (autoScroll) {
      scrollToBottom();
    }
  }, [filteredEntries, autoScroll, scrollToBottom]);

  const handleScroll = useCallback(() => {
    if (!consoleRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = consoleRef.current;
    const isAtBottom = scrollHeight - scrollTop - clientHeight < 50;
    setAutoScroll(isAtBottom);
    setShowScrollIndicator(!isAtBottom);
  }, []);

  const handleScrollIndicatorClick = () => {
    setAutoScroll(true);
    setShowScrollIndicator(false);
    scrollToBottom();
  };

  const statusLabel =
    runState.status === 'idle' ? 'No active run' :
    runState.status === 'running' ? `Running: ${runState.goal}${startedAt == null ? '' : ` — ${formatDuration(elapsedMs)}`}` :
    runState.status === 'completed' ? (runState.outcome?.success
      ? `Generated ${runState.outcome.generated_file_count} files for plan ${runState.planId}${runState.durationMs == null ? '' : ` in ${formatDuration(runState.durationMs)}`}`
      : runState.outcome ? `Build failed for plan ${runState.planId}: ${runState.outcome.summary}`
      : `Simulated plan ${runState.planId}; no files generated`) :
    `Failed: ${runState.error}`;

  const steps = runState.status === 'running' || runState.status === 'completed' ? (runState.steps ?? []) : runState.status === 'failed' ? (runState.run?.plan.steps ?? []) : [];
  const displayStatus = runState.status === 'completed' && runState.outcome?.success === false ? 'failed' : runState.status;
  const repairActions = runState.status === 'completed' && runState.repairActions ? runState.repairActions : [];
  const completedStepCount = steps.filter((step) => step.status === 'completed').length;
  const activeStep = steps.find((step) => step.status === 'in_progress');
  const progressPercent = steps.length > 0
    ? Math.round((completedStepCount / steps.length) * 100)
    : null;
  const progressLabel = runState.status === 'failed' ? 'Run stopped — review failure details' : activeStep?.description ?? (steps.length > 0 ? `${completedStepCount} of ${steps.length} steps completed` : runState.status === 'running' ? 'Working — step progress not reported yet' : runState.status === 'completed' ? 'Run finished — review the outcome below' : 'Ready for your next run');

  return (
    <div className="view-enter stack-lg runner-view">
      <div className="page-header">
        <div><div className="eyebrow">Execution workspace</div><h1 className="page-title">Runner</h1><p className="page-subtitle">Follow execution, inspect generated files, and review verification evidence.</p></div>
        <div className="flex-start">
          <div className={`badge ${displayStatus === 'running' ? 'warn' : displayStatus === 'failed' ? 'err' : displayStatus === 'completed' ? 'ok' : ''}`}>
            {displayStatus}
          </div>
          {runState.status === 'running' && onCancel && (
            <button type="button" onClick={onCancel}>Cancel</button>
          )}
          {(runState.status === 'completed' || runState.status === 'failed') && onRerun && (
            <button type="button" onClick={() => onRerun(lastGoal)}>Review rerun</button>
          )}
          <button type="button" onClick={onToggleRightPanel} className="secondary">Inspector</button>
        </div>
      </div>

      <Card style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column', gap: 12 }}>
        <div className="run-summary" aria-label="Run summary">
          <div className="run-summary-copy">
            <span className="run-summary-label">Progress</span>
            <strong>{progressLabel}</strong>
          </div>
          {progressPercent !== null ? <div className="run-progress-wrap" role="progressbar" aria-label="Plan step progress" aria-valuemin={0} aria-valuemax={100} aria-valuenow={progressPercent}>
            <div className="run-progress-track"><div className="run-progress-value" style={{ width: `${progressPercent}%` }} /></div>
            <span className="run-progress-percent">{progressPercent}%</span>
          </div> : <span className="badge">No step measurements</span>}
        </div>

        {runState.status === 'failed' && runState.run && (
          <RunCompletionSummary run={runState.run} />
        )}

        {steps.length > 0 && (
          <div className="stack-sm" style={{ maxHeight: 200, overflow: 'auto' }}>
            <SectionHeader title="Plan steps" />
            <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
              {steps.map((step, index) => {
                const isCurrent = runState.status === 'running' && index === steps.findIndex(s => s.status === 'in_progress');
                const isCompleted = step.status === 'completed';
                const isFailed = step.status === 'failed';
                const isRunning = step.status === 'in_progress';
                const StepIcon = STEP_STATUS_ICON[step.status as keyof typeof STEP_STATUS_ICON] ?? Circle;

                return (
                  <div
                    key={step.id}
                    className="step-item"
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: 10,
                      padding: '8px 10px',
                      background: 'var(--bg-inset)',
                      border: '1px solid var(--border-subtle)',
                      borderRadius: 'var(--radius-sm)',
                      transition: 'background var(--transition-fast), border-color var(--transition-fast)',
                    }}
                  >
                    <span
                      style={{
                        fontSize: 16,
                        color: isFailed ? 'var(--danger)' : isCompleted ? 'var(--success)' : isRunning ? 'var(--warning)' : 'var(--text-muted)',
                        minWidth: 24,
                        textAlign: 'center',
                      }}
                      aria-label={STEP_STATUS_LABEL[step.status as keyof typeof STEP_STATUS_LABEL] || step.status}
                    >
                      <StepIcon size={16} aria-hidden="true" />
                    </span>
                    <span
                      style={{
                        flex: 1,
                        fontSize: 13,
                        color: isCurrent ? 'var(--text-primary)' : 'var(--text-secondary)',
                        fontWeight: isCurrent ? 600 : 400,
                      }}
                    >
                      {step.description}
                    </span>
                    {isRunning && (
                      <span className="badge warn" style={{ fontSize: 11, padding: '2px 6px' }}>
                        Running
                      </span>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        )}

      {repairActions.length > 0 && (
        <div className="stack-sm" style={{ maxHeight: 150, overflow: 'auto' }}>
          <SectionHeader title="Repair actions" />
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            {repairActions.map((action, index) => (
              <div
                key={index}
                className="repair-action-item"
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 10,
                  padding: '8px 10px',
                  background: action.applied ? 'var(--success-soft)' : 'var(--warning-soft)',
                  border: `1px solid ${action.applied ? 'rgba(34, 197, 94, 0.32)' : 'rgba(245, 158, 11, 0.32)'}`,
                  borderRadius: 'var(--radius-sm)',
                  color: action.applied ? 'var(--success)' : 'var(--warning)',
                }}
              >
                <span style={{ fontSize: 14, minWidth: 24, textAlign: 'center' }}>
                  {action.applied ? '✓' : '⟳'}
                </span>
                <span style={{ flex: 1, fontSize: 13 }}>{action.description}</span>
                <span
                  className={`badge ${action.applied ? 'ok' : 'warn'}`}
                  style={{ fontSize: 11, padding: '2px 6px', textTransform: 'capitalize' }}
                >
                  {action.applied ? 'Applied' : 'Pending'}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}

        {runState.status === 'completed' && (runState.outcome || (runState.generatedFiles && runState.generatedFiles.length > 0) || (runState.failedFiles && runState.failedFiles.length > 0)) && (
          <div className="stack-sm">
            <SectionHeader title="Generated files" />
            <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
              {(runState.generatedFiles ?? []).map((file: { path: string; status: string; error?: string }) => (
                <div
                  key={file.path}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 10,
                    padding: '8px 10px',
                    background: 'var(--bg-inset)',
                    border: '1px solid var(--border-subtle)',
                    borderRadius: 'var(--radius-sm)',
                  }}
                >
                  <span style={{ fontSize: 14, minWidth: 24, textAlign: 'center' }}>✓</span>
                  <span style={{ flex: 1, fontSize: 13 }}>{file.path}</span>
                  <span className="badge ok" style={{ fontSize: 11, padding: '2px 6px' }}>
                    {file.status}
                  </span>
                </div>
              ))}
            </div>
          </div>
        )}

        {runState.status === 'completed' && runState.failedFiles && runState.failedFiles.length > 0 && (
          <div className="stack-sm">
            <SectionHeader title="Failed files" />
            <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
              {(runState.failedFiles ?? []).map((file: { path: string; status: string; error?: string }) => (
                <div
                  key={file.path}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 10,
                    padding: '8px 10px',
                    background: 'var(--bg-inset)',
                    border: '1px solid var(--border-subtle)',
                    borderRadius: 'var(--radius-sm)',
                  }}
                >
                  <span style={{ fontSize: 14, minWidth: 24, textAlign: 'center' }}>✗</span>
                  <span style={{ flex: 1, fontSize: 13 }}>{file.path}</span>
                  <span className="badge err" style={{ fontSize: 11, padding: '2px 6px' }}>
                    {file.status}
                  </span>
                  {file.error && (
                    <span className="text-xs text-muted" style={{ maxWidth: 260, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                      {file.error}
                    </span>
                  )}
                </div>
              ))}
            </div>
          </div>
        )}

        {runState.status === 'completed' && runState.outcome && (
          <div className="stack-sm">
            <SectionHeader title="Build outcome" />
            <div
              style={{
                display: 'flex',
                gap: 10,
                flexWrap: 'wrap',
                padding: '10px 12px',
                background: 'var(--bg-inset)',
                border: '1px solid var(--border-subtle)',
                borderRadius: 'var(--radius-sm)',
              }}
            >
              <div className={`badge ${runState.outcome.success ? 'ok' : 'err'}`}>
                {runState.outcome.success ? 'success' : 'failed'}
              </div>
              {runState.outcome.degraded && <div className="badge warn">degraded</div>}
              <div className="badge info">{runState.outcome.generated_file_count} generated</div>
              <div className="badge err">{runState.outcome.failed_file_count} failed</div>
              <span className="text-xs text-muted">{runState.outcome.summary}</span>
            </div>
          </div>
        )}

        {runState.status === 'completed' && <VerificationEvidence results={runState.outcome?.verification_results} runtimeSummary={runState.outcome?.verification_summary} reportedSuccess={runState.outcome?.success} />}
        {runState.status === 'failed' && !runState.run && <VerificationEvidence />}

        <div className="stack-sm">
          <SectionHeader
            title="Live output"
            right={
              <div className="flex-start runner-toolbar" style={{ gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
                <div className="text-secondary text-sm">{statusLabel}</div>
                <input aria-label="Search output" placeholder="Search output…" value={search} onChange={(event) => setSearch(event.target.value)} />
                <select
                  aria-label="Filter by level"
                  value={levelFilter}
                  onChange={(e) => setLevelFilter(e.target.value as 'all' | 'info' | 'warn' | 'error')}
                  style={{
                    padding: '4px 8px',
                    fontSize: 12,
                    borderRadius: 'var(--radius-sm)',
                    border: '1px solid var(--border-subtle)',
                    background: 'var(--bg-elevated)',
                    color: 'var(--text-primary)',
                  }}
                >
                  <option value="all">All levels</option>
                  <option value="info">Info</option>
                  <option value="warn">Warning</option>
                  <option value="error">Error</option>
                </select>
                <select
                  aria-label="Filter by source"
                  value={sourceFilter}
                  onChange={(e) => setSourceFilter(e.target.value)}
                  style={{
                    padding: '4px 8px',
                    fontSize: 12,
                    borderRadius: 'var(--radius-sm)',
                    border: '1px solid var(--border-subtle)',
                    background: 'var(--bg-elevated)',
                    color: 'var(--text-primary)',
                  }}
                >
                  <option value="all">All sources</option>
                  {sources.map((s) => (
                    <option key={s} value={s}>{s}</option>
                  ))}
                </select>
                <label style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 12, color: 'var(--text-secondary)', cursor: 'pointer' }}>
                  <input
                    type="checkbox"
                    checked={autoScroll}
                    onChange={(e) => setAutoScroll(e.target.checked)}
                  />
                  Auto-scroll
                </label>
                <button type="button" onClick={copyAllEntries} disabled={filteredEntries.length === 0} className="ghost" style={{ fontSize: 11, padding: '4px 8px' }}>
                  Copy all
                </button>
                <button type="button" onClick={exportJson} disabled={filteredEntries.length === 0} className="ghost" style={{ fontSize: 11, padding: '4px 8px' }}>
                  Export JSON
                </button>
                <button type="button" onClick={clearEntries} disabled={!onClearConsole || visibleEntries.length === 0} className="ghost" style={{ fontSize: 11, padding: '4px 8px', color: 'var(--danger)' }}>
                  Clear
                </button>
              </div>
            }
          />
        </div>

        <div className="runner-log-meta"><span>Showing {filteredEntries.length} of {consoleEntries.length} events{consoleEntries.length > 300 ? ' · newest 300 available in this view' : ''}</span><span role="status">{copyStatus}</span></div>
        <div
          ref={consoleRef}
          className="console-preview scroll-y"
          role="log"
          aria-live="polite"
          aria-label="Console output"
          style={{
            flex: 1,
            background: 'var(--bg-inset)',
            border: '1px solid var(--border-subtle)',
            borderRadius: 'var(--radius-md)',
            padding: '10px 12px',
            position: 'relative',
          }}
          onScroll={handleScroll}
        >
          {filteredEntries.length === 0 && visibleEntries.length > 0 && (
            <EmptyState
              title="No matching entries"
              description="Adjust filters to see entries."
            />
          )}
          {visibleEntries.length === 0 && (
            <EmptyState
              title="No output yet"
              description="Start a run to stream pipeline and agent events here."
            />
          )}
          {filteredEntries.map((entry) => (
            <div key={entry.id} className={`console-entry console-entry-${entry.level}`}>
              <span className="console-entry-time">{new Date(entry.ts).toLocaleTimeString()}</span>
              <span className="console-entry-source">[{entry.source}]</span>
              <span>{entry.message}</span>
            </div>
          ))}
        </div>

        {showScrollIndicator && (
          <button type="button"
            className="scroll-indicator"
            onClick={handleScrollIndicatorClick}
            style={{
              position: 'sticky',
              bottom: 12,
              right: 12,
              padding: '6px 10px',
              background: 'var(--accent-soft)',
              border: '1px solid var(--accent)',
              borderRadius: 'var(--radius-pill)',
              fontSize: 12,
              color: 'var(--accent)',
              cursor: 'pointer',
              zIndex: 10,
              boxShadow: 'var(--shadow-soft)',
            }}
            aria-label="Scroll to bottom"
          >
            ↓ New output
          </button>
        )}

        {runState.status === 'idle' && steps.length === 0 && repairActions.length === 0 && (
          <div className="runner-hint">
            <div className="placeholder-title">Your next run will appear here</div>
            <div className="placeholder-body">Start a pipeline from Project to see step progress, live events, and repair results.</div>
          </div>
        )}
      </Card>
    </div>
  );
}