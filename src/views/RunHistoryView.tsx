import { useEffect, useMemo, useRef, useState } from 'react';
import { Card } from '../components/Card';
import { usePipelineRunHistory } from '../hooks/usePipelineRunHistory';
import { EmptyState } from '../components/EmptyState';
import { getPipelineRunResult } from '../lib/ipc';
import RunCompletionSummary from '../components/RunCompletionSummary';
import type { PipelineRun } from '../lib/ipc';

type RunHistoryEntry = {
  id: string;
  goal: string;
  status: string;
  step_count: number;
  repair_count: number;
  success: boolean;
  error?: string | null;
  finished_at?: number | null;
};

function downloadFile(content: string, filename: string, type: string) {
  const blob = new Blob([content], { type });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

const exportJson = (runs: RunHistoryEntry[]) => {
  downloadFile(JSON.stringify(runs, null, 2), `run-history-${Date.now()}.json`, 'application/json');
};

const exportCsv = (runs: RunHistoryEntry[]) => {
  const headers = ['id', 'goal', 'status', 'step_count', 'repair_count', 'success', 'error', 'finished_at'];
  const rows = runs.map((run) => [
    run.id,
    `"${run.goal.replace(/"/g, '""')}"`,
    run.status,
    run.step_count.toString(),
    run.repair_count.toString(),
    run.success.toString(),
    run.error ? `"${run.error.replace(/"/g, '""')}"` : '',
    run.finished_at ? new Date(run.finished_at).toISOString() : '',
  ]);
  downloadFile([headers.join(','), ...rows.map((row) => row.join(','))].join('\n'), `run-history-${Date.now()}.csv`, 'text/csv');
};

type RunHistoryViewProps = {
  onReviewGoal?: (goal: string, runId: string) => void;
  reviewDisabled?: boolean;
};

export default function RunHistoryView({ onReviewGoal, reviewDisabled = false }: RunHistoryViewProps = {}) {
  const { load } = usePipelineRunHistory();
  const [runs, setRuns] = useState<RunHistoryEntry[]>([]);
  const [search, setSearch] = useState('');
  const [loading, setLoading] = useState(true);
  const [selectedRun, setSelectedRun] = useState<PipelineRun | null>(null);
  const [selectedLoading, setSelectedLoading] = useState(false);
  const [selectedError, setSelectedError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const requestRef = useRef(0);
  const [listError, setListError] = useState<string | null>(null);
  const [refreshVersion, setRefreshVersion] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setListError(null);
    load(20)
      .then((items) => { if (!cancelled) setRuns(items); })
      .catch((error: unknown) => { if (!cancelled) { setRuns([]); setListError(error instanceof Error ? error.message : 'Failed to load run history'); } })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [load, refreshVersion]);

  const filteredRuns = useMemo(() => {
    const query = search.trim().toLowerCase();
    if (!query) return runs;
    return runs.filter((run) => `${run.goal} ${run.status} ${run.id}`.toLowerCase().includes(query));
  }, [runs, search]);
  const successRate = runs.length > 0 ? Math.round((runs.filter((run) => run.success).length / runs.length) * 100) : null;

  const statusVariant = (run: RunHistoryEntry) => run.success ? 'ok' : /cancel/i.test(run.status) ? 'warn' : 'err';

  const handleInspectRun = async (runId: string) => {
    const request = ++requestRef.current;
    setSelectedId(runId);
    setSelectedRun(null);
    setSelectedError(null);
    setSelectedLoading(true);
    try {
      const result = await getPipelineRunResult(runId);
      if (request !== requestRef.current) return;
      setSelectedRun(result);
      if (!result) setSelectedError('This run result is no longer available.');
    } catch (error) {
      if (request === requestRef.current) setSelectedError(error instanceof Error ? error.message : 'Failed to load run result');
    } finally {
      if (request === requestRef.current) setSelectedLoading(false);
    }
  };


  return (
    <div className="view-enter stack-lg">
      <div className="page-header">
        <div>
          <h1 className="page-title" style={{ margin: 0 }}>Run history</h1>
          <p className="page-subtitle">Search recent executions and inspect their file and verification results.</p>
        </div>
        <div className="flex-start" style={{ gap: 8 }}>
          <button type="button" className="secondary compact-button" onClick={() => setRefreshVersion((current) => current + 1)} disabled={loading}>Refresh history</button>
          {successRate !== null && <div className="badge ok">{successRate}% success</div>}
          <div className="badge info">{runs.length} runs</div>
          <button type="button" onClick={() => exportJson(runs)} className="ghost compact-button">Export JSON</button>
          <button type="button" onClick={() => exportCsv(runs)} className="ghost compact-button">Export CSV</button>
        </div>
      </div>

      <Card className="history-panel">
        <div className="history-toolbar">
          <input value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Search goal, status, or run id…" aria-label="Search run history" />
          <div className="text-xs text-muted">Showing {filteredRuns.length} of {runs.length}</div>
        </div>
        {loading ? (
          <div className="status-pill info" role="status">Loading run history…</div>
        ) : listError ? (
          <div role="alert" className="stack-sm"><span className="text-danger">{listError}</span><button type="button" className="secondary" onClick={() => setRefreshVersion((current) => current + 1)}>Retry history</button></div>
        ) : filteredRuns.length === 0 ? (
          <EmptyState title={runs.length === 0 ? 'No pipeline runs recorded yet' : 'No matching runs'} description={runs.length === 0 ? 'Run a pipeline to populate execution history.' : 'Adjust the search to see more results.'} />
        ) : (
          <div className="history-list">
            {filteredRuns.map((run) => (
              <div key={run.id} className="history-row">
                <div className="history-row-main">
                  <div className="section-header-title">{run.goal || 'Untitled run'}</div>
                  <div className="history-meta">
                    <span className={`badge ${statusVariant(run)}`}>{run.status}</span>
                    <span>{run.step_count} steps</span>
                    <span>{run.repair_count} repairs</span>
                    {run.finished_at && <span>{new Date(run.finished_at).toLocaleString()}</span>}
                  </div>
                </div>
                <div className="history-row-actions">
                  <span className="text-xs text-muted">{run.id}</span>
                  <button type="button" className="ghost compact-button" onClick={() => handleInspectRun(run.id)}>Inspect</button>
                  <button type="button" className="ghost compact-button" disabled={!onReviewGoal || reviewDisabled || !run.goal.trim()} onClick={() => onReviewGoal?.(run.goal, run.id)}>Review rerun</button>
                </div>
                {run.error && <div className="console-entry console-entry-error">{run.error}</div>}
              </div>
            ))}
          </div>
        )}
      </Card>
      {selectedId && (
        <Card className="history-detail">
          <div className="history-detail-header">
            <div>
              <div className="section-header-title">Run detail</div>
              <div className="history-meta">
                <span>{selectedId}</span>
                {selectedRun && <span>{selectedRun.plan.goal}</span>}
              </div>
            </div>
            <button type="button" className="ghost compact-button" onClick={() => { requestRef.current += 1; setSelectedId(null); setSelectedRun(null); }}>Close</button>
          </div>
          {selectedLoading ? (
            <div className="status-pill info" role="status">Loading run result…</div>
          ) : selectedError ? (
            <div className="stack-sm" role="alert"><span className="text-danger">{selectedError}</span><button type="button" className="secondary" onClick={() => void handleInspectRun(selectedId)}>Retry loading result</button></div>
          ) : selectedRun ? (
            <RunCompletionSummary run={selectedRun} />
          ) : null}
        </Card>
      )}
    </div>
  );
}
