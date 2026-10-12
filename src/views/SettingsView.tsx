import { useState, useEffect, useRef } from 'react';
import { Card } from '../components/Card';
import { Field } from '../components/Field';
import { Row } from '../components/Row';
import { EmptyState } from '../components/EmptyState';
import { SectionHeader } from '../components/SectionHeader';
import type { RunState } from '../types/frontend';
import { loadAgentRuntimeSettings, saveAgentRuntimeSettings, DEFAULT_AGENT_RUNTIME_SETTINGS } from '../hooks/useAgentRuntimeSettings';
import type { AgentRuntimeSettings } from '../hooks/useAgentRuntimeSettings';
import { loadAgentTaskMetrics, type AgentTaskMetrics } from '../hooks/useAgentTaskMetrics';
import { executeAgentTask } from '../hooks/useAgentRuntimeExecution';
import { checkUpdates } from '../hooks/useUpdater';
import { runIpcBenchmark, type IpcBenchmarkResult } from '../hooks/useIpcBenchmark';
import { deleteProvider, listProviders, type ProviderSettings } from '../hooks/useProviderSettings';
import { ModelsPanel } from '../components/ModelsPanel';

type SettingsViewProps = {
  runState?: RunState;
  onRunAgentTask?: (agentId: string, input: string) => void;
};

type SettingsSection = 'runtime' | 'providers' | 'models' | 'maintenance';

const BENCHMARK_HISTORY_KEY = 'recode.benchmarkHistory';

function isBenchmarkResult(value: unknown): value is IpcBenchmarkResult {
  if (typeof value !== 'object' || value === null) return false;
  const entry = value as Record<string, unknown>;
  return ['iterations', 'total_ms', 'min_ms', 'max_ms', 'avg_ms', 'p95_ms'].every((key) =>
    typeof entry[key] === 'number' && Number.isFinite(entry[key]) && entry[key] >= 0
  ) && Number.isInteger(entry.iterations);
}

export default function SettingsView({ runState: _runState, onRunAgentTask: _onRunAgentTask }: SettingsViewProps) {
  const [settings, setSettings] = useState<AgentRuntimeSettings>(DEFAULT_AGENT_RUNTIME_SETTINGS);
  const [metrics, setMetrics] = useState<AgentTaskMetrics[]>([]);
  const [ipcResult, setIpcResult] = useState<IpcBenchmarkResult | null>(null);
  const [benchmarkRunning, setBenchmarkRunning] = useState(false);
  const [benchmarkError, setBenchmarkError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const savePending = useRef(false);
  const edited = useRef(false);
  const [benchmarkHistory, setBenchmarkHistory] = useState<IpcBenchmarkResult[]>(() => {
    try {
      const stored = window.localStorage.getItem(BENCHMARK_HISTORY_KEY);
      if (stored) {
        const parsed: unknown = JSON.parse(stored);
        return Array.isArray(parsed) ? parsed.filter(isBenchmarkResult).slice(0, 20) : [];
      }
    } catch { /* Local history is optional when browser storage is unavailable. */ }
    return [];
  });
  const [agentInput, setAgentInput] = useState('hello');
  const [providers, setProviders] = useState<ProviderSettings[]>([]);

  const [updateCheckResult, setUpdateCheckResult] = useState<string | null>(null);
  const [runtimeStatus, setRuntimeStatus] = useState<string | null>(null);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [settingsSection, setSettingsSection] = useState<SettingsSection>('runtime');

  useEffect(() => {
    let cancelled = false;
    const reportError = (error: unknown) => {
      if (!cancelled) setSettingsError(error instanceof Error ? error.message : 'Failed to load settings');
    };
    loadAgentRuntimeSettings().then((value) => { if (!cancelled && !edited.current && value) setSettings(value); }).catch(reportError);
    loadAgentTaskMetrics().then((value) => { if (!cancelled) setMetrics(value); }).catch(reportError);
    listProviders().then((value) => { if (!cancelled) setProviders(value); }).catch(reportError);
    return () => { cancelled = true; };
  }, []);

  const update = async (patch: Partial<AgentRuntimeSettings>) => {
    if (savePending.current) return;
    const next = { ...settings, ...patch };
    const values = [next.maxConcurrency, next.defaultTimeoutMs, next.retryLimit];
    if (values.some((value) => !Number.isFinite(value) || value < 0)) {
      setSettingsError('Runtime values must be finite, non-negative numbers.');
      return;
    }
    setSettingsError(null);
    edited.current = true;
    setSettings(next);
    savePending.current = true;
    setSaving(true);
    try {
      await saveAgentRuntimeSettings(next);
    } catch (error) {
      setSettingsError(`Settings not saved: ${error instanceof Error ? error.message : String(error)}`);
    } finally {
      savePending.current = false;
      setSaving(false);
    }
  };

  const updateNumber = (key: keyof AgentRuntimeSettings, raw: string) => {
    if (raw.trim() === '') {
      setSettingsError('Enter a value before saving runtime settings.');
      return;
    }
    const value = Number(raw);
    if (!Number.isFinite(value)) {
      setSettingsError('Enter a valid number.');
      return;
    }
    const minimums: Record<keyof AgentRuntimeSettings, number> = {
      maxConcurrency: 1,
      defaultTimeoutMs: 1,
      retryLimit: 0,
    };
    if (!Number.isInteger(value) || value < minimums[key]) {
      setSettingsError(`Runtime values must be whole numbers and at least ${minimums[key]}.`);
      return;
    }
    void update({ [key]: value });
  };

  const runTestTask = async () => {
    const actualAgentId = 'planner';
    setRuntimeStatus('Running agent task...');
    try {
      const result = await executeAgentTask({ id: actualAgentId, input: agentInput });
      setRuntimeStatus(result.status === 'success'
        ? `Task completed: ${result.taskId} in ${result.durationMs ?? 0}ms`
        : `Task failed: ${result.error ?? 'unknown error'}`);
      await loadAgentTaskMetrics().then(setMetrics);
    } catch (e) {
      setRuntimeStatus(`Task failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const checkForUpdates = async () => {
    setUpdateCheckResult('Checking for updates...');
    try {
      const result = await checkUpdates('https://example.com/updater', '0.1.0');
      setUpdateCheckResult(result.available
        ? `Update available: ${result.manifest?.version ?? 'unknown'}`
        : 'No updates available');
    } catch (e) {
      setUpdateCheckResult(`Update check failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const runBenchmark = async () => {
    if (benchmarkRunning) return;
    setBenchmarkRunning(true);
    setBenchmarkError(null);
    try {
      const result = await runIpcBenchmark(100);
      setIpcResult(result);
      const next = [result, ...benchmarkHistory].slice(0, 20);
      setBenchmarkHistory(next);
      try {
        window.localStorage.setItem(BENCHMARK_HISTORY_KEY, JSON.stringify(next));
      } catch {
        setBenchmarkError('Benchmark completed, but local history could not be saved.');
      }
    } catch (error) {
      setBenchmarkError(error instanceof Error ? error.message : 'Benchmark failed');
    } finally {
      setBenchmarkRunning(false);
    }
  };


  const handleDeleteProvider = async (id: string) => {
    if (!window.confirm(`Delete provider “${id}”?`)) return;
    try {
      await deleteProvider(id);
      setProviders((current) => current.filter((item) => item.id !== id));
    } catch (error) {
      setSettingsError(`Provider not deleted: ${error instanceof Error ? error.message : String(error)}`);
    }
  };

  return (
    <div className="view-enter stack-lg">
      <div className="page-header">
        <div>
          <h1 className="page-title" style={{ margin: 0 }}>Settings</h1>
          <p className="page-subtitle">Configure runtime behavior, model providers, and local maintenance tools.</p>
        </div>
        <div className="badge info">Configuration</div>
      </div>

      <div className="settings-tabs" role="tablist" aria-label="Settings sections">
        {(['runtime', 'providers', 'models', 'maintenance'] as const).map((section) => (
          <button
            key={section}
            type="button"
            role="tab"
            aria-selected={settingsSection === section}
            className={`settings-tab ${settingsSection === section ? 'active' : ''}`}
            onClick={() => setSettingsSection(section)}
          >
            {section === 'runtime' ? 'Runtime' : section === 'providers' ? 'Providers' : section === 'models' ? 'Models' : 'Maintenance'}
          </button>
        ))}
      </div>

      {settingsError && <div className="badge err" role="alert">{settingsError}</div>}
      {settingsSection === 'runtime' && (
        <Card className="settings-panel">
          <SectionHeader title="Agent runtime" description="Configure concurrency, timeouts, and retries." />
          <Row>
            <Field label="Max concurrency" htmlFor="setting-max-concurrency">
              <input id="setting-max-concurrency" type="number" min="1" step="1" disabled={saving} value={settings.maxConcurrency} onChange={(e) => updateNumber('maxConcurrency', e.target.value)} />
            </Field>
            <Field label="Default timeout (ms)" htmlFor="setting-default-timeout">
              <input id="setting-default-timeout" type="number" min="1" step="1" disabled={saving} value={settings.defaultTimeoutMs} onChange={(e) => updateNumber('defaultTimeoutMs', e.target.value)} />
            </Field>
            <Field label="Retry limit" htmlFor="setting-retry-limit">
              <input id="setting-retry-limit" type="number" min="0" step="1" disabled={saving} value={settings.retryLimit} onChange={(e) => updateNumber('retryLimit', e.target.value)} />
            </Field>
          </Row>
          {saving && <div role="status">Saving settings…</div>}
          <SectionHeader title="Runtime check" description="Send a small task through the local agent runtime." />
          <Row>
            <Field label="Agent input" htmlFor="settings-agent-input" style={{ flex: 1 }}>
              <input id="settings-agent-input" value={agentInput} onChange={(e) => setAgentInput(e.target.value)} />
            </Field>
            <button type="button" onClick={runTestTask} disabled={Boolean(settingsError) || !agentInput.trim()}>Run test agent task</button>
          </Row>
          {runtimeStatus && <div className="status-pill" role="status">{runtimeStatus}</div>}
          <SectionHeader title="Recent runtime metrics" />
          {metrics.length === 0 ? (
            <EmptyState title="No metrics yet" description="Run a test task to collect runtime metrics." />
          ) : (
            <div className="metric-list">
              {metrics.slice(0, 10).map((metric) => (
                <div key={metric.task_id} className="metric-row">
                  <div>
                    <strong>{metric.agent_id}</strong>
                    <span className="text-muted">{metric.duration_ms}ms</span>
                  </div>
                  <div className={`badge ${metric.success ? 'ok' : 'warn'}`}>{metric.success ? 'ok' : 'warn'}</div>
                </div>
              ))}
            </div>
          )}
        </Card>
      )}

      {settingsSection === 'providers' && (
        <Card className="settings-panel">
          <SectionHeader title="Providers" description="Legacy metadata only; these entries do not configure the live gateway." />
          <div className="settings-inline-note text-xs text-muted">The gateway reads RECODE_OPENAI_BASE_URL and RECODE_OPENAI_MODEL from the app process environment at startup. Set RECODE_OPENAI_API_KEY there for remote HTTPS providers. API keys can no longer be saved to SQLite; existing legacy records require an explicit secret migration.</div>
          <div className="provider-list">
            {providers.length === 0 && (
              <EmptyState title="No legacy provider metadata" description="Configure the gateway using the app process environment before launching recode." />
            )}
            {providers.map((provider) => (
              <div key={provider.id} className="provider-row">
                <div style={{ minWidth: 0 }}>
                  <div className="section-header-title">{provider.provider}</div>
                  <div className="text-sm text-muted">{provider.id}</div>
                </div>
                <div className="row">
                  <div className={`badge ${provider.enabled ? 'ok' : 'err'}`}>{provider.enabled ? 'enabled' : 'disabled'}</div>
                  <button type="button" onClick={() => handleDeleteProvider(provider.id)} className="ghost">Delete</button>
                </div>
              </div>
            ))}
          </div>

        </Card>
      )}

      {settingsSection === 'models' && <ModelsPanel />}

      {settingsSection === 'maintenance' && (
        <Card className="settings-panel">
          <SectionHeader title="Updates" description="Check the configured update channel." />
          <div className="settings-inline-note text-xs text-muted">The updater currently uses the local stub service. Configure a real release endpoint in the backend before shipping update checks.</div>
          <Row>
            <button type="button" onClick={checkForUpdates}>Check for updates</button>
            {updateCheckResult ? <div className="status-pill" role="status">{updateCheckResult}</div> : null}
          </Row>
          <div className="section-divider" />
          <SectionHeader title="Signature verification" description="Verify artifacts before applying them." />
          <div className="placeholder">
            <div className="placeholder-title">Verification paths are available in Updates</div>
            <div className="placeholder-body">Use the Updates workspace to enter artifact, signature, and trusted public-key paths.</div>
          </div>
          <div className="section-divider" />
          <SectionHeader title="IPC benchmark" description="Measure 100 local IPC calls and keep the latest results locally." />
          <button type="button" onClick={runBenchmark} disabled={benchmarkRunning}>{benchmarkRunning ? 'Running benchmark...' : 'Run IPC benchmark'}</button>
          {benchmarkError && <div className="badge err" role="alert">{benchmarkError}</div>}
          {ipcResult && (
            <div className="metric-grid">
              <div className="metric-card"><div className="metric-label">Iterations</div><div className="metric-value">{ipcResult.iterations}</div></div>
              <div className="metric-card"><div className="metric-label">Total</div><div className="metric-value">{ipcResult.total_ms.toFixed(2)}ms</div></div>
              <div className="metric-card"><div className="metric-label">Average</div><div className="metric-value">{ipcResult.avg_ms.toFixed(2)}ms</div></div>
              <div className="metric-card"><div className="metric-label">P95</div><div className="metric-value">{ipcResult.p95_ms.toFixed(2)}ms</div></div>
            </div>
          )}
          {benchmarkHistory.length > 0 && (
            <div className="metric-list">
              {benchmarkHistory.slice(0, 10).map((result, index) => (
                <div key={index} className="metric-row">
                  <div><strong>Avg:</strong> {result.avg_ms.toFixed(2)}ms</div>
                  <div><strong>P95:</strong> {result.p95_ms.toFixed(2)}ms</div>
                  <div className="text-muted">{result.iterations} iterations</div>
                </div>
              ))}
            </div>
          )}
        </Card>
      )}
    </div>
  );
}
