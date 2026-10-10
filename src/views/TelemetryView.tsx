import { Card } from '../components/Card';
import { Row } from '../components/Row';
import { EmptyState } from '../components/EmptyState';
import { loadAgentTaskMetrics } from '../hooks/useAgentTaskMetrics';
import { runIpcBenchmark } from '../hooks/useIpcBenchmark';
import type { AgentTaskMetrics } from '../hooks/useAgentTaskMetrics';
import type { IpcBenchmarkResult } from '../hooks/useIpcBenchmark';
import { useEffect, useState } from 'react';

type TelemetryViewProps = {};

export default function TelemetryView({}: TelemetryViewProps) {
  const [metrics, setMetrics] = useState<AgentTaskMetrics[]>([]);
  const [benchmark, setBenchmark] = useState<IpcBenchmarkResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [benchmarkError, setBenchmarkError] = useState<string | null>(null);
  const [metricsError, setMetricsError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    loadAgentTaskMetrics()
      .then((value) => { if (!cancelled) setMetrics(value); })
      .catch((error: unknown) => { if (!cancelled) setMetricsError(error instanceof Error ? error.message : 'Metrics unavailable'); });
    return () => { cancelled = true; };
  }, []);

  const handleBenchmark = async () => {
    if (loading) return;
    setLoading(true);
    setBenchmarkError(null);
    try {
      const result = await runIpcBenchmark(100);
      setBenchmark(result);
    } catch (error) {
      setBenchmarkError(error instanceof Error ? error.message : 'Benchmark failed');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="view-enter" style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <div className="page-header">
        <h1 className="page-title" style={{ margin: 0 }}>Telemetry</h1>
        <div className="badge info">Performance</div>
      </div>
      <p className="page-subtitle">Benchmark history and runtime metrics.</p>

      <Card style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
        <Row>
          <div className="metric-card" style={{ flex: 1 }}>
            <div className="metric-label">IPC latency</div>
            <div className="metric-value">
              {benchmark ? `${benchmark.avg_ms.toFixed(2)}ms avg` : 'Not run'}
            </div>
            <div style={{ color: 'var(--text-secondary)', fontSize: 13, marginTop: 6 }}>
              {benchmark ? `P95 ${benchmark.p95_ms.toFixed(2)}ms across ${benchmark.iterations} iterations` : 'Run the benchmark to see latency.'}
            </div>
          </div>
            <div className="metric-card" style={{ flex: 1 }}>
              <div className="metric-label">App start</div>
              <div className="metric-value">Not measured</div>
              <div style={{ color: 'var(--text-secondary)', fontSize: 13, marginTop: 6 }}>
                Startup timing is recorded by the runtime when available.
              </div>
            </div>
        </Row>

        <Row>
          <button type="button" onClick={handleBenchmark} disabled={loading}>
            {loading ? 'Running benchmark...' : 'Run IPC benchmark'}
          </button>
        </Row>

        {benchmarkError && <div role="alert" className="badge err">{benchmarkError}</div>}
        {benchmark && (
          <div className="metric-grid">
            <div className="metric-card">
              <div className="metric-label">Iterations</div>
              <div className="metric-value">{benchmark.iterations}</div>
            </div>
            <div className="metric-card">
              <div className="metric-label">Total</div>
              <div className="metric-value">{benchmark.total_ms.toFixed(2)}ms</div>
            </div>
            <div className="metric-card">
              <div className="metric-label">Average</div>
              <div className="metric-value">{benchmark.avg_ms.toFixed(2)}ms</div>
            </div>
            <div className="metric-card">
              <div className="metric-label">P95</div>
              <div className="metric-value">{benchmark.p95_ms.toFixed(2)}ms</div>
            </div>
          </div>
        )}
      </Card>

      <Card style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
        <div style={{ fontWeight: 700 }}>Runtime metrics</div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {metricsError ? <div role="alert" className="badge err">{metricsError}</div> : metrics.length === 0 ? (
            <EmptyState
              title="No metrics yet"
              description="Run an IPC benchmark or agent task to populate local performance data."
            />
          ) : (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
              {metrics.map((metric) => (
                <div key={metric.task_id} className="card card-flat" style={{ padding: 12 }}>
                  <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                    <div style={{ fontWeight: 700 }}>{metric.agent_id}</div>
                    <div className={`badge ${metric.success ? 'ok' : 'warn'}`}>{metric.success ? 'ok' : 'warn'}</div>
                  </div>
                  <div style={{ color: 'var(--text-secondary)', fontSize: 13, marginTop: 4 }}>{metric.duration_ms}ms</div>
                </div>
              ))}
            </div>
          )}
        </div>
      </Card>
    </div>
  );
}
