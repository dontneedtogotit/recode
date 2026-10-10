import { useEffect, useState, useCallback, useRef } from 'react';
import { Card } from '../components/Card';
import { Row } from '../components/Row';
import { SectionHeader } from '../components/SectionHeader';
import { EmptyState } from '../components/EmptyState';
import type { RunState } from '../types/frontend';
import { attemptSelfHeal, listMonitorEvents, type MonitorEvent, type RepairAction } from '../hooks/useDiagnostics';

type DiagnosticsViewProps = {
  runState: RunState;
  onRunAgentTask: (agentId: string, input: string) => void;
};

export default function DiagnosticsView({ runState, onRunAgentTask }: DiagnosticsViewProps) {
  const [events, setEvents] = useState<MonitorEvent[]>([]);
  const [actions, setActions] = useState<RepairAction[]>([]);
  const [healing, setHealing] = useState(false);
  const [severityFilter, setSeverityFilter] = useState<'all' | 'info' | 'warn' | 'error'>('all');
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [eventsError, setEventsError] = useState<string | null>(null);
  const [healError, setHealError] = useState<string | null>(null);
  const requestRef = useRef(0);
  const healingRef = useRef(false);
  const mounted = useRef(false);

  const fetchEvents = useCallback(async () => {
    if (!mounted.current) return;
    const request = ++requestRef.current;
    try {
      const next = await listMonitorEvents();
      if (!mounted.current || request !== requestRef.current) return;
      setEvents(next);
      setEventsError(null);
    } catch (error) {
      if (mounted.current && request === requestRef.current) {
        setEventsError(error instanceof Error ? error.message : 'Monitor events unavailable');
      }
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; requestRef.current += 1; };
  }, []);

  useEffect(() => {
    void fetchEvents();
    const interval = autoRefresh ? setInterval(() => void fetchEvents(), 5000) : undefined;
    return () => { clearInterval(interval); requestRef.current += 1; };
  }, [autoRefresh, fetchEvents]);

  const handleSelfHeal = async () => {
    if (healingRef.current) return;
    healingRef.current = true;
    setHealing(true);
    setHealError(null);
    try {
      const next = await attemptSelfHeal();
      if (!mounted.current) return;
      setActions(next);
      await fetchEvents();
    } catch (error) {
      if (mounted.current) setHealError(error instanceof Error ? error.message : 'Self-heal failed');
    } finally {
      healingRef.current = false;
      if (mounted.current) setHealing(false);
    }
  };

  const handleClearEvents = () => {
    requestRef.current += 1;
    setEvents([]);
  };

  const filteredEvents = events.filter((event) =>
    severityFilter === 'all' || event.severity === severityFilter
  );

  return (
    <div className="view-enter stack-lg">
      <div className="page-header">
        <h1 className="page-title" style={{ margin: 0 }}>Diagnostics</h1>
        <div className={`badge ${runState.status === 'failed' ? 'err' : 'ok'}`}>Run state: {runState.status}</div>
      </div>
      <p className="page-subtitle">Monitor runtime events and self-heal suggestions.</p>

      <Card style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
        <Row>
          <button type="button" onClick={() => onRunAgentTask('diagnostics', 'capture sample diagnostics')}>
            Capture sample diagnostics
          </button>
          <button type="button" onClick={handleSelfHeal} disabled={healing}>
            {healing ? 'Healing...' : 'Attempt self-heal'}
          </button>
        </Row>
        {healError && <div role="alert" className="badge err">{healError}</div>}

        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          <SectionHeader
            title="Recent monitor events"
            right={
              <div className="flex-start" style={{ gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
                <select
                  aria-label="Filter events by severity"
                  value={severityFilter}
                  onChange={(e) => setSeverityFilter(e.target.value as 'all' | 'info' | 'warn' | 'error')}
                  style={{
                    padding: '4px 8px',
                    fontSize: 12,
                    borderRadius: 'var(--radius-sm)',
                    border: '1px solid var(--border-subtle)',
                    background: 'var(--bg-elevated)',
                    color: 'var(--text-primary)',
                  }}
                >
                  <option value="all">All severities</option>
                  <option value="info">Info</option>
                  <option value="warn">Warning</option>
                  <option value="error">Error</option>
                </select>
                <label style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 12, color: 'var(--text-secondary)', cursor: 'pointer' }}>
                  <input
                    type="checkbox"
                    checked={autoRefresh}
                    onChange={(e) => setAutoRefresh(e.target.checked)}
                  />
                  Auto-refresh (5s)
                </label>
                <button type="button" onClick={handleClearEvents} className="ghost" style={{ fontSize: 11, padding: '4px 8px', color: 'var(--danger)' }}>
                  Clear events
                </button>
              </div>
            }
          />
          {eventsError && <div role="alert" className="badge err">{eventsError}</div>}
          {filteredEvents.length === 0 && events.length > 0 && (
            <EmptyState
              title="No matching events"
              description="Adjust severity filter to see events."
            />
          )}
          {!eventsError && events.length === 0 && (
            <EmptyState
              title="No events recorded"
              description="The monitor will show runtime events after the next pipeline or agent task."
            />
          )}
          {filteredEvents.slice(-20).reverse().map((event, index) => (
            <div key={`${event.ts}-${index}`} className={`console-entry console-entry-${event.severity}`} role="log" aria-live="polite">
              <span className="console-entry-time">{new Date(Number(event.ts)).toLocaleTimeString()}</span>
              <span>{event.message}</span>
            </div>
          ))}
        </div>

        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          <SectionHeader title="Self-heal actions" />
          {actions.length === 0 && (
            <EmptyState
              title="No repairs yet"
              description="Run self-heal to generate suggested or applied repairs."
            />
          )}
          {actions.map((action, index) => (
            <div key={`${action.description}-${index}`} className={`console-entry ${action.applied ? 'console-entry-warn' : 'console-entry-info'}`}>
              <span>{action.description}</span>
              <span style={{ fontSize: 12, opacity: 0.8 }}>{action.applied ? 'applied' : 'suggested'}</span>
            </div>
          ))}
        </div>

        {events.length > 0 && (
          <div className="text-xs text-muted">Monitoring refreshes every 5 seconds while enabled. Use Clear events to reset the local view.</div>
        )}
      </Card>
    </div>
  );
}
