import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Card } from '../components/Card';
import { EmptyState } from '../components/EmptyState';
import { SectionHeader } from '../components/SectionHeader';
import { listAntiPatterns, listSuccessPatterns, type LearningPattern } from '../lib/ipc';

type PatternEntry = LearningPattern;

export default function LearningView() {
  const [agentRole, setAgentRole] = useState('runtime');
  const [taskType, setTaskType] = useState('agent_task');
  const [successPatterns, setSuccessPatterns] = useState<PatternEntry[]>([]);
  const [antiPatterns, setAntiPatterns] = useState<PatternEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const requestRef = useRef(0);

  const load = useCallback(async () => {
    const request = ++requestRef.current;
    setLoading(true);
    setError(null);
    setSuccessPatterns([]);
    setAntiPatterns([]);
    try {
      const [success, anti] = await Promise.all([
        listSuccessPatterns(agentRole, taskType),
        listAntiPatterns(agentRole, taskType),
      ]);
      if (request !== requestRef.current) return;
      setSuccessPatterns(success);
      setAntiPatterns(anti);
    } catch (err) {
      if (request === requestRef.current) setError(err instanceof Error ? err.message : 'Failed to load learning patterns');
    } finally {
      if (request === requestRef.current) setLoading(false);
    }
  }, [agentRole, taskType]);

  useEffect(() => {
    void load();
    return () => { requestRef.current += 1; };
  }, [load]);

  const scoreFor = (item: PatternEntry) => `${Math.max(0, Math.min(1, item.score)) * 100}%`;
  const countFor = (item: PatternEntry) => `${item.sample_count} ${item.sample_count === 1 ? 'sample' : 'samples'}`;

  const successAverage = useMemo(() => successPatterns.length ? successPatterns.reduce((sum, item) => sum + item.score, 0) / successPatterns.length : 0, [successPatterns]);
  const antiAverage = useMemo(() => antiPatterns.length ? antiPatterns.reduce((sum, item) => sum + item.score, 0) / antiPatterns.length : 0, [antiPatterns]);

  return (
    <div className="view-enter stack-lg">
      <div className="page-header">
        <div>
          <h1 className="page-title" style={{ margin: 0 }}>Learning patterns</h1>
          <p className="page-subtitle">Review the approaches the runtime has learned from successful and unsuccessful runs.</p>
        </div>
        <button type="button" onClick={load} className="secondary" disabled={loading}>{loading ? 'Refreshing...' : 'Refresh'}</button>
      </div>

      <Card className="learning-filter-card">
        <div className="learning-filter-row">
          <label className="field"><span>Agent role</span><input value={agentRole} onChange={(e) => setAgentRole(e.target.value)} /></label>
          <label className="field"><span>Task type</span><input value={taskType} onChange={(e) => setTaskType(e.target.value)} /></label>
          <div className="learning-filter-summary">
            <span className="badge ok">{successPatterns.length} success</span>
            <span className="badge warn">{antiPatterns.length} anti-patterns</span>
          </div>
        </div>
        {error && <div className="badge err" role="alert">{error}</div>}
        {loading && <div className="badge info" role="status">Loading patterns...</div>}
      </Card>

      <div className="learning-columns">
        <Card className="learning-column">
          <SectionHeader title="Success patterns" description={successPatterns.length ? `Average confidence ${(successAverage * 100).toFixed(0)}%` : 'Approaches worth repeating'} />
          {loading || error ? null : successPatterns.length === 0 ? (
            <EmptyState title="No success patterns yet" description="Patterns appear after the runtime records successful approaches for this role and task type." />
          ) : (
            <div className="pattern-list">
              {successPatterns.map((item) => (
                <div key={item.id} className="pattern-row success">
                  <div className="pattern-row-header"><span className="section-header-title">{item.id}</span><span className="badge ok">{scoreFor(item)}</span></div>
                  <div className="pattern-content">{item.content}</div>
                  <div className="pattern-bar"><div style={{ width: scoreFor(item) }} /></div>
                  <div className="text-xs text-muted">{countFor(item)} · Updated {new Date(item.updated_at).toLocaleDateString()}</div>
                </div>
              ))}
            </div>
          )}
        </Card>
        <Card className="learning-column">
          <SectionHeader title="Anti-patterns" description={antiPatterns.length ? `Average confidence ${(antiAverage * 100).toFixed(0)}%` : 'Approaches to avoid'} />
          {loading || error ? null : antiPatterns.length === 0 ? (
            <EmptyState title="No anti patterns yet" description="Patterns appear after the runtime records approaches that need improvement." />
          ) : (
            <div className="pattern-list">
              {antiPatterns.map((item) => (
                <div key={item.id} className="pattern-row anti">
                  <div className="pattern-row-header"><span className="section-header-title">{item.id}</span><span className="badge warn">{scoreFor(item)}</span></div>
                  <div className="pattern-content">{item.content}</div>
                  <div className="pattern-bar"><div style={{ width: scoreFor(item) }} /></div>
                  <div className="text-xs text-muted">{countFor(item)} · Updated {new Date(item.updated_at).toLocaleDateString()}</div>
                </div>
              ))}
            </div>
          )}
        </Card>
      </div>
    </div>
  );
}
