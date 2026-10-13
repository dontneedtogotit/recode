import { useCallback, useEffect, useState } from 'react';
import { Card } from './Card';
import { Field } from './Field';
import { Row } from './Row';
import { EmptyState } from './EmptyState';
import { SectionHeader } from './SectionHeader';
import {
  discoverProviderModels,
  deleteProviderApiKey,
  importEnvApiKey,
  inferRoutingRequest,
  listModels,
  listProviderCredentials,
  removeModel,
  routeModels,
  saveProviderApiKey,
  runOrchestratorStep,
  runOrchestratorAutonomous,
  upsertModel,
  type CandidateScore,
  type CostTier,
  type Modality,
  type ModelInfo,
  type ProviderCredentialInfo,
  type RoutingDecision,
  type RoutingRequest,
  type OrchestratorStepResult,
  type OrchestratorAutonomousResult,
  type UpsertModelInput,
} from '../lib/ipc';

const KNOWN_PROVIDERS = ['openai', 'openrouter', 'deepseek', 'grok', 'mistral', 'anthropic'] as const;
const TASK_KINDS = [
  'planning', 'coding', 'refactoring', 'testing', 'review', 'debugging',
  'documentation', 'architecture', 'reasoning', 'generation',
] as const;
const MODALITIES: Modality[] = ['text', 'image', 'audio', 'video'];

const EMPTY_FORM: UpsertModelInput = {
  provider: 'openai',
  modelId: '',
  name: '',
  contextWindow: 128000,
  maxOutputTokens: 8192,
  inputModalities: ['text'],
  supportsTools: true,
  supportsStructuredOutput: true,
  supportsReasoning: false,
  costTier: 'medium',
  description: '',
  strengths: [],
  taskAffinity: {},
};

function formatTokens(value: number): string {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(value % 1_000_000 === 0 ? 0 : 1)}M`;
  if (value >= 1000) return `${Math.round(value / 1000)}k`;
  return String(value);
}

function capabilityBadges(model: ModelInfo): Array<{ label: string; tone: 'ok' | 'info' | 'warn' }> {
  const badges: Array<{ label: string; tone: 'ok' | 'info' | 'warn' }> = [];
  if (model.supportsTools) badges.push({ label: 'tools', tone: 'ok' });
  if (model.supportsStructuredOutput) badges.push({ label: 'structured', tone: 'ok' });
  if (model.supportsReasoning) badges.push({ label: 'reasoning', tone: 'info' });
  if (model.inputModalities.includes('image')) badges.push({ label: 'vision', tone: 'info' });
  if (model.discovered) badges.push({ label: 'discovered', tone: 'warn' });
  return badges;
}

export function ModelsPanel() {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [creds, setCreds] = useState<Record<string, ProviderCredentialInfo>>({});
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [filterProvider, setFilterProvider] = useState('');
  const [busyProvider, setBusyProvider] = useState<string | null>(null);

  // Model editor form.
  const [showForm, setShowForm] = useState(false);
  const [form, setForm] = useState<UpsertModelInput>(EMPTY_FORM);
  const [formAffinity, setFormAffinity] = useState<Record<string, string>>({});
  const [formStrengths, setFormStrengths] = useState('');
  const [editingId, setEditingId] = useState<string | null>(null);
  const [savingModel, setSavingModel] = useState(false);

  // Routing preview.
  const [taskText, setTaskText] = useState('');
  const [inferredRequest, setInferredRequest] = useState<RoutingRequest | null>(null);
  const [routing, setRouting] = useState(false);
  const [decision, setDecision] = useState<RoutingDecision | null>(null);

  // Orchestrator.
  const [goal, setGoal] = useState('');
  const [step, setStep] = useState('');
  const [orchestrating, setOrchestrating] = useState(false);
  const [stepResult, setStepResult] = useState<OrchestratorStepResult | null>(null);

  // Autonomous orchestrator.
  const [autoRunning, setAutoRunning] = useState(false);
  const [autoMaxSteps, setAutoMaxSteps] = useState(5);
  const [autoResult, setAutoResult] = useState<OrchestratorAutonomousResult | null>(null);

  const refresh = useCallback(async () => {
    const [nextModels, nextCreds] = await Promise.all([listModels(), listProviderCredentials()]);
    setModels(nextModels);
    setCreds(nextCreds);
  }, []);

  useEffect(() => {
    refresh().catch((e) => setError(e instanceof Error ? e.message : 'Failed to load models'));
  }, [refresh]);

  const flash = (message: string) => {
    setNotice(message);
    setError(null);
    window.setTimeout(() => setNotice(null), 6000);
  };

  const runProviderAction = async (provider: string, action: () => Promise<unknown>, done: string) => {
    setBusyProvider(provider);
    setError(null);
    try {
      await action();
      await refresh();
      flash(done);
    } catch (e) {
      setError(`${done.replace(/\.$/, '')} failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setBusyProvider(null);
    }
  };

  const discover = (provider: string) =>
    runProviderAction(
      provider,
      async () => {
        const result = await discoverProviderModels(provider);
        if (!result.ok) throw new Error(result.error ?? 'discovery failed');
        setNotice(`Discovered ${result.count} models from ${provider}.`);
      },
      `Discovery from ${provider}.`,
    );

  const importKey = (provider: string) =>
    runProviderAction(provider, () => importEnvApiKey(provider), `API key imported into the local credential store for ${provider}.`);

  const deleteKey = (provider: string) => {
    if (!window.confirm(`Remove the stored API key for ${provider}?`)) return;
    void runProviderAction(provider, () => deleteProviderApiKey(provider), `API key removed for ${provider}.`);
  };

  // Paste a key directly from the GUI into the local credential store.
  const [keyEntry, setKeyEntry] = useState<Record<string, string>>({});
  const [keyEntryOpen, setKeyEntryOpen] = useState<string | null>(null);
  const addKey = (provider: string) => {
    const raw = keyEntry[provider]?.trim();
    if (!raw) {
      setError(`Enter an API key for ${provider} first.`);
      return;
    }
    void runProviderAction(
      provider,
      async () => {
        const info = await saveProviderApiKey(provider, raw);
        setKeyEntry((prev) => {
          const next = { ...prev };
          delete next[provider];
          return next;
        });
        if (!info.baseUrl) {
          setNotice(`Key stored for ${provider}. Set a base URL (or leave it — known providers use a built-in default) before discovering.`);
        }
      },
      `API key stored locally for ${provider}.`,
    );
  };

  const openAddForm = () => {
    setForm({ ...EMPTY_FORM, provider: filterProvider || 'openai' });
    setFormAffinity({});
    setFormStrengths('');
    setEditingId(null);
    setShowForm(true);
  };

  const openEditForm = (model: ModelInfo) => {
    setForm({
      provider: model.provider,
      modelId: model.modelId,
      name: model.name,
      contextWindow: model.contextWindow,
      maxOutputTokens: model.maxOutputTokens,
      inputModalities: model.inputModalities,
      supportsTools: model.supportsTools,
      supportsStructuredOutput: model.supportsStructuredOutput,
      supportsReasoning: model.supportsReasoning,
      costTier: model.costTier,
      description: model.description,
      strengths: model.strengths,
      taskAffinity: model.taskAffinity,
    });
    const affinity: Record<string, string> = {};
    for (const [kind, value] of Object.entries(model.taskAffinity ?? {})) {
      if (value > 0) affinity[kind] = String(value);
    }
    setFormAffinity(affinity);
    setFormStrengths(model.strengths.join(', '));
    setEditingId(model.id);
    setShowForm(true);
  };

  const submitModel = async () => {
    if (savingModel) return;
    if (!form.modelId.trim()) {
      setError('Model id is required.');
      return;
    }
    setSavingModel(true);
    setError(null);
    try {
      const affinity: Record<string, number> = {};
      for (const [kind, raw] of Object.entries(formAffinity)) {
        const value = Number(raw);
        if (raw.trim() !== '' && Number.isFinite(value) && value > 0) affinity[kind] = value;
      }
      const payload: UpsertModelInput = {
        ...form,
        modelId: form.modelId.trim(),
        description: form.description?.trim() ?? '',
        strengths: formStrengths
          .split(',')
          .map((s) => s.trim())
          .filter(Boolean),
        taskAffinity: affinity,
      };
      const saved = await upsertModel(payload);
      await refresh();
      setShowForm(false);
      flash(`${editingId ? 'Updated' : 'Added'} ${saved.id} to the catalog.`);
    } catch (e) {
      setError(`Model save failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSavingModel(false);
    }
  };

  const remove = (id: string) => {
    if (!window.confirm(`Remove ${id} from the model catalog?`)) return;
    void (async () => {
      try {
        await removeModel(id);
        await refresh();
        flash(`Removed ${id}.`);
      } catch (e) {
        setError(`Remove failed: ${e instanceof Error ? e.message : String(e)}`);
      }
    })();
  };

  const routeTask = async () => {
    if (routing || !taskText.trim()) return;
    setRouting(true);
    setError(null);
    setDecision(null);
    try {
      const request = await inferRoutingRequest(taskText);
      setInferredRequest(request);
      setDecision(await routeModels({ request }));
    } catch (e) {
      setError(`Routing failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setRouting(false);
    }
  };

  const runStep = async () => {
    if (orchestrating || !goal.trim() || !step.trim()) return;
    setOrchestrating(true);
    setError(null);
    setStepResult(null);
    try {
      setStepResult(await runOrchestratorStep({ goal: goal.trim(), step: step.trim() }));
    } catch (e) {
      setError(`Orchestrator step failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setOrchestrating(false);
    }
  };

  const runAutonomous = async () => {
    if (autoRunning || !goal.trim()) return;
    setAutoRunning(true);
    setError(null);
    setAutoResult(null);
    try {
      const result = await runOrchestratorAutonomous({
        goal: goal.trim(),
        initialStep: step.trim() || undefined,
        maxSteps: autoMaxSteps,
      });
      setAutoResult(result);
      flash(
        result.completed
          ? `Goal completed in ${result.steps.length} steps.`
          : `Autonomous run stopped after ${result.steps.length} steps${result.aborted ? ' (aborted or budget)' : ''}.`,
      );
    } catch (e) {
      setError(`Autonomous run failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setAutoRunning(false);
    }
  };

  const visibleModels = models.filter((m) => !filterProvider || m.provider === filterProvider);

  const candidateRow = (candidate: CandidateScore) => (
    <div key={candidate.modelId} className="metric-row">
      <div>
        <strong>{candidate.model}</strong>
        <span className="text-muted"> · {candidate.provider}</span>
      </div>
      <div>
        {candidate.eligible ? (
          <span className="text-muted">
            affinity {candidate.affinity.toFixed(2)} · reliability {candidate.reliability.toFixed(2)} · speed{' '}
            {candidate.speed.toFixed(2)}
          </span>
        ) : (
          <span className="text-muted">{candidate.disqualification ?? 'ineligible'}</span>
        )}
      </div>
      <div className={`badge ${candidate.eligible ? 'ok' : 'err'}`}>
        {candidate.eligible ? candidate.score.toFixed(3) : 'out'}
      </div>
    </div>
  );

  return (
    <div className="stack-lg">
      {error && <div className="badge err" role="alert">{error}</div>}
      {notice && <div className="status-pill" role="status">{notice}</div>}

      <Card className="settings-panel">
        <SectionHeader
          title="Provider credentials & discovery"
          description="Keys live in the local credential store only. Import from the process environment, then discover the provider's live model list."
        />
        <div className="provider-list">
          {KNOWN_PROVIDERS.map((provider) => {
            const cred = creds[provider];
            const busy = busyProvider === provider;
            return (
              <div key={provider} className="provider-row">
                <div style={{ minWidth: 0, flex: 1 }}>
                  <div className="section-header-title">{provider}</div>
                  <div className="text-sm text-muted">
                    {cred?.hasApiKey ? `key ${cred.keyHint ?? ''}` : 'no key stored'}
                    {cred?.baseUrl ? ` · ${cred.baseUrl}` : ''}
                    {cred?.defaultModel ? ` · default ${cred.defaultModel}` : ''}
                  </div>
                  {keyEntryOpen === provider && (
                    <div style={{ display: 'flex', gap: 8, marginTop: 8, alignItems: 'center' }}>
                      <input
                        type="password"
                        autoFocus
                        style={{ flex: 1 }}
                        placeholder={`API key for ${provider}`}
                        value={keyEntry[provider] ?? ''}
                        onChange={(e) =>
                          setKeyEntry((prev) => ({ ...prev, [provider]: e.target.value }))
                        }
                        onKeyDown={(e) => {
                          if (e.key === 'Enter') addKey(provider);
                          if (e.key === 'Escape') setKeyEntryOpen(null);
                        }}
                      />
                      <button type="button" disabled={busy} onClick={() => addKey(provider)}>
                        {busy ? 'Saving…' : 'Save key'}
                      </button>
                      <button type="button" className="ghost" disabled={busy} onClick={() => setKeyEntryOpen(null)}>
                        Cancel
                      </button>
                    </div>
                  )}
                </div>
                <div className="row">
                  <button type="button" className="ghost" disabled={busy || Boolean(cred?.hasApiKey)} onClick={() => importKey(provider)}>
                    Import env key
                  </button>
                  <button type="button" className="ghost" disabled={busy || Boolean(cred?.hasApiKey)} onClick={() => setKeyEntryOpen(keyEntryOpen === provider ? null : provider)}>
                    {cred?.hasApiKey ? 'Replace key' : 'Add key'}
                  </button>
                  <button type="button" className="ghost" disabled={busy || !cred?.hasApiKey} onClick={() => deleteKey(provider)}>
                    Remove key
                  </button>
                  <button type="button" disabled={busy} onClick={() => discover(provider)}>
                    {busy ? 'Discovering…' : 'Discover models'}
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      </Card>

      <Card className="settings-panel">
        <SectionHeader
          title="Model catalog"
          description="Every model the router and gateway may select. Discover from a provider or add any model manually — arbitrary numbers are supported."
        />
        <Row>
          <Field label="Filter by provider" htmlFor="models-filter" style={{ flex: 1 }}>
            <input
              id="models-filter"
              list="models-provider-options"
              value={filterProvider}
              onChange={(e) => setFilterProvider(e.target.value)}
              placeholder="all providers"
            />
            <datalist id="models-provider-options">
              {[...new Set(models.map((m) => m.provider))].map((p) => (
                <option key={p} value={p} />
              ))}
            </datalist>
          </Field>
          <button type="button" onClick={openAddForm}>Add model</button>
        </Row>

        {showForm && (
          <div className="stack-md">
            <Row>
              <Field label="Provider" htmlFor="model-provider">
                <input
                  id="model-provider"
                  list="model-provider-options"
                  value={form.provider}
                  onChange={(e) => setForm({ ...form, provider: e.target.value })}
                />
                <datalist id="model-provider-options">
                  {[...KNOWN_PROVIDERS, ...models.map((m) => m.provider)].map((p, i) => (
                    <option key={`${p}-${i}`} value={p} />
                  ))}
                </datalist>
              </Field>
              <Field label="Model id" htmlFor="model-id">
                <input
                  id="model-id"
                  value={form.modelId}
                  onChange={(e) => setForm({ ...form, modelId: e.target.value })}
                  placeholder="gpt-4o"
                />
              </Field>
              <Field label="Display name" htmlFor="model-name">
                <input
                  id="model-name"
                  value={form.name ?? ''}
                  onChange={(e) => setForm({ ...form, name: e.target.value })}
                  placeholder="GPT-4o"
                />
              </Field>
            </Row>
            <Row>
              <Field label="Context window (tokens)" htmlFor="model-ctx">
                <input
                  id="model-ctx"
                  type="number"
                  min="1"
                  value={form.contextWindow ?? 128000}
                  onChange={(e) => setForm({ ...form, contextWindow: Number(e.target.value) || 0 })}
                />
              </Field>
              <Field label="Max output (tokens)" htmlFor="model-maxout">
                <input
                  id="model-maxout"
                  type="number"
                  min="1"
                  value={form.maxOutputTokens ?? 8192}
                  onChange={(e) => setForm({ ...form, maxOutputTokens: Number(e.target.value) || 0 })}
                />
              </Field>
              <Field label="Cost tier" htmlFor="model-tier">
                <select
                  id="model-tier"
                  value={form.costTier}
                  onChange={(e) => setForm({ ...form, costTier: e.target.value as CostTier })}
                >
                  <option value="low">low</option>
                  <option value="medium">medium</option>
                  <option value="high">high</option>
                </select>
              </Field>
              <div className="field">
                <label>Modalities</label>
                <Row>
                  {MODALITIES.map((modality) => (
                    <label key={modality} className="text-sm">
                      <input
                        type="checkbox"
                        checked={(form.inputModalities ?? []).includes(modality)}
                        onChange={(e) => {
                          const current = form.inputModalities ?? [];
                          const next = e.target.checked
                            ? [...current, modality]
                            : current.filter((m) => m !== modality);
                          setForm({ ...form, inputModalities: next.length ? next : ['text'] });
                        }}
                      />{' '}
                      {modality}
                    </label>
                  ))}
                </Row>
              </div>
            </Row>
            <Row>
              <label className="text-sm">
                <input
                  type="checkbox"
                  checked={form.supportsTools ?? false}
                  onChange={(e) => setForm({ ...form, supportsTools: e.target.checked })}
                />{' '}
                tool calling
              </label>
              <label className="text-sm">
                <input
                  type="checkbox"
                  checked={form.supportsStructuredOutput ?? false}
                  onChange={(e) => setForm({ ...form, supportsStructuredOutput: e.target.checked })}
                />{' '}
                structured output
              </label>
              <label className="text-sm">
                <input
                  type="checkbox"
                  checked={form.supportsReasoning ?? false}
                  onChange={(e) => setForm({ ...form, supportsReasoning: e.target.checked })}
                />{' '}
                reasoning
              </label>
            </Row>
            <Field label="What it is good for (one line)" htmlFor="model-desc">
              <input
                id="model-desc"
                value={form.description ?? ''}
                onChange={(e) => setForm({ ...form, description: e.target.value })}
                placeholder="Strong general-purpose coding model"
              />
            </Field>
            <Field label="Strengths (comma separated)" htmlFor="model-strengths">
              <input
                id="model-strengths"
                value={formStrengths}
                onChange={(e) => setFormStrengths(e.target.value)}
                placeholder="refactoring, long context, multi-file edits"
              />
            </Field>
            <div className="field">
              <label>Task affinity (0–1, blank = none)</label>
              <div className="metric-grid">
                {TASK_KINDS.map((kind) => (
                  <div key={kind} className="metric-card">
                    <div className="metric-label">{kind}</div>
                    <input
                      type="number"
                      min="0"
                      max="1"
                      step="0.05"
                      value={formAffinity[kind] ?? ''}
                      onChange={(e) => setFormAffinity({ ...formAffinity, [kind]: e.target.value })}
                      aria-label={`affinity for ${kind}`}
                    />
                  </div>
                ))}
              </div>
            </div>
            <Row>
              <button type="button" onClick={submitModel} disabled={savingModel}>
                {savingModel ? 'Saving…' : editingId ? 'Save changes' : 'Add to catalog'}
              </button>
              <button type="button" className="ghost" onClick={() => setShowForm(false)}>
                Cancel
              </button>
            </Row>
          </div>
        )}

        <div className="section-divider" />
        {visibleModels.length === 0 ? (
          <EmptyState
            title="No models in the catalog"
            description="Discover models from a provider above, or add one manually. The router selects among everything here."
          />
        ) : (
          <div className="provider-list">
            {visibleModels.map((model) => (
              <div key={model.id} className="provider-row">
                <div style={{ minWidth: 0 }}>
                  <div className="section-header-title">
                    {model.name} <span className="text-muted text-sm">· {model.id}</span>
                  </div>
                  <div className="text-sm text-muted">
                    {formatTokens(model.contextWindow)} ctx · max out {formatTokens(model.maxOutputTokens)} · {model.costTier}
                    {model.promptPricePerMtok != null
                      ? ` · $${model.promptPricePerMtok}/M in, $${model.completionPricePerMtok ?? '?'}/M out`
                      : ''}
                  </div>
                  {model.description && <div className="text-sm">{model.description}</div>}
                  {model.strengths.length > 0 && (
                    <div className="text-sm text-muted">Good for: {model.strengths.join(', ')}</div>
                  )}
                  <div className="row" style={{ gap: 6 }}>
                    {capabilityBadges(model).map((badge) => (
                      <span key={badge.label} className={`badge ${badge.tone}`}>{badge.label}</span>
                    ))}
                    {Object.entries(model.taskAffinity ?? {})
                      .filter(([, value]) => value > 0)
                      .slice(0, 5)
                      .map(([kind, value]) => (
                        <span key={kind} className="badge info">
                          {kind} {value.toFixed(2)}
                        </span>
                      ))}
                  </div>
                </div>
                <div className="row">
                  <button type="button" className="ghost" onClick={() => openEditForm(model)}>Edit</button>
                  <button type="button" className="ghost" onClick={() => remove(model.id)}>Remove</button>
                </div>
              </div>
            ))}
          </div>
        )}
      </Card>

      <Card className="settings-panel">
        <SectionHeader
          title="Routing preview"
          description="Describe a task and watch the router infer requirements, then rank every catalog model by affinity, cost, and learned latency/reliability."
        />
        <Field label="Task description" htmlFor="route-task">
          <textarea
            id="route-task"
            rows={2}
            value={taskText}
            onChange={(e) => setTaskText(e.target.value)}
            placeholder="Refactor the payments module and update its tests"
          />
        </Field>
        <Row>
          <button type="button" onClick={routeTask} disabled={routing || !taskText.trim()}>
            {routing ? 'Routing…' : 'Route this task'}
          </button>
        </Row>
        {inferredRequest && (
          <div className="text-sm text-muted">
            Inferred: kind <strong>{inferredRequest.taskKind}</strong> · min ctx {formatTokens(inferredRequest.minContextWindow)}
            {inferredRequest.requiresTools ? ' · tools' : ''}
            {inferredRequest.requiresStructuredOutput ? ' · structured' : ''}
            {inferredRequest.prefersReasoning ? ' · reasoning' : ''}
            {inferredRequest.requiredModalities.length > 0 ? ` · ${inferredRequest.requiredModalities.join('+')} input` : ''}
          </div>
        )}
        {decision && (
          <div className="stack-sm">
            {decision.chosen ? (
              <div className="status-pill" role="status">
                Chosen: <strong>{decision.chosen.model}</strong> ({decision.chosen.provider}) — score {decision.chosen.score.toFixed(3)}
              </div>
            ) : (
              <div className="badge err" role="alert">No eligible model — none of the {decision.considered} candidates met the requirements.</div>
            )}
            <div className="metric-list">
              {decision.ranked.slice(0, 8).map(candidateRow)}
            </div>
          </div>
        )}
      </Card>

      <Card className="settings-panel">
        <SectionHeader
          title="Dynamic orchestrator"
          description="Runs one autonomous step: the orchestrator decides the next action, routes it to an agent and a model, executes it, and feeds latency/reliability back into routing."
        />
        <Field label="Goal" htmlFor="orch-goal">
          <input
            id="orch-goal"
            value={goal}
            onChange={(e) => setGoal(e.target.value)}
            placeholder="Implement a CSV import feature"
          />
        </Field>
        <Field label="Current situation / next step hint" htmlFor="orch-step">
          <textarea
            id="orch-step"
            rows={2}
            value={step}
            onChange={(e) => setStep(e.target.value)}
            placeholder="The schema is defined; the parser needs to be implemented"
          />
        </Field>
        <Row>
          <button type="button" onClick={runStep} disabled={orchestrating || !goal.trim() || !step.trim()}>
            {orchestrating ? 'Deciding & dispatching…' : 'Run orchestrator step'}
          </button>
        </Row>
        {stepResult && (
          <div className="stack-sm">
            <div className={`badge ${stepResult.success ? 'ok' : 'err'}`}>
              {stepResult.success ? 'step succeeded' : 'step failed'}
            </div>
            <div className="text-sm">
              <strong>{stepResult.decision.agentId}</strong>
              {stepResult.decision.modelId ? ` → ${stepResult.decision.modelId}` : ''} · {stepResult.decision.taskKind}
            </div>
            <div className="text-sm text-muted">{stepResult.decision.rationale}</div>
            {stepResult.decision.taskDescription && (
              <div className="text-sm">Steered task: {stepResult.decision.taskDescription}</div>
            )}
            {stepResult.error && <div className="badge err" role="alert">{stepResult.error}</div>}
            {stepResult.output != null && stepResult.output !== '' && (
              <pre className="placeholder-body" style={{ margin: 0 }}>
                {typeof stepResult.output === 'string'
                  ? stepResult.output
                  : JSON.stringify(stepResult.output, null, 2)}
              </pre>
            )}
          </div>
        )}

        <div className="section-divider" />
        <div className="field">
          <label>Autonomous run — let the orchestrator steer step after step until the goal is judged done</label>
          <Row>
            <Field label="Max steps (1–10)" htmlFor="auto-max-steps" style={{ width: 140 }}>
              <input
                id="auto-max-steps"
                type="number"
                min="1"
                max="10"
                value={autoMaxSteps}
                onChange={(e) => setAutoMaxSteps(Math.max(1, Math.min(10, Number(e.target.value) || 5)))}
              />
            </Field>
            <button
              type="button"
              onClick={runAutonomous}
              disabled={autoRunning || !goal.trim()}
              style={{ alignSelf: 'flex-end' }}
            >
              {autoRunning ? 'Steering…' : 'Run autonomously'}
            </button>
          </Row>
        </div>
        {autoResult && (
          <div className="stack-sm">
            <div className={`badge ${autoResult.completed ? 'ok' : 'warn'}`}>
              {autoResult.completed
                ? 'goal completed'
                : autoResult.aborted
                  ? 'stopped early'
                  : 'stopped'}
            </div>
            {autoResult.steps.length === 0 ? (
              <div className="text-sm text-muted">No steps executed (empty goal or zero budget).</div>
            ) : (
              <ol className="metric-list" style={{ margin: 0 }}>
                {autoResult.steps.map((s, i) => (
                  <li key={i} className="metric-row">
                    <div className="text-sm" style={{ minWidth: 0, overflowWrap: 'anywhere' }}>{s}</div>
                  </li>
                ))}
              </ol>
            )}
          </div>
        )}
      </Card>
    </div>
  );
}
