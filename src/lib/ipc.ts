import type { PipelineRun, PlanStepState, StaticAnalysisResult, MentalSimulationResult, ProductionReadinessResult, AccessibilityAuditResult, UXConsistencyResult } from '../types/frontend';
import { normalizeVerificationResults } from './verification';
export type { ConsoleEntry, PipelineRun } from '../types/frontend';
export { loadAgentHistory, saveAgentHistory, pushAgentTaskRecord, createAgentHistoryState, type AgentTaskRecord } from './agentHistory';

function normalizeStaticAnalysis(raw: unknown): StaticAnalysisResult | undefined {
  if (!raw || typeof raw !== 'object') return undefined;
  const item = raw as Record<string, unknown>;
  if (typeof item.security_score !== 'number') return undefined;
  const findings = Array.isArray(item.findings) ? item.findings.flatMap((f) => {
    if (!f || typeof f !== 'object') return [];
    const finding = f as Record<string, unknown>;
    if (typeof finding.file !== 'string' || typeof finding.message !== 'string') return [];
    return [{
      file: finding.file,
      line: typeof finding.line === 'number' ? finding.line : undefined,
      category: typeof finding.category === 'string' ? finding.category : '',
      severity: (['critical', 'high', 'medium', 'low', 'info'].includes(finding.severity as string) ? finding.severity : 'info') as 'critical' | 'high' | 'medium' | 'low' | 'info',
      message: finding.message,
      suggestion: typeof finding.suggestion === 'string' ? finding.suggestion : '',
      tool: (finding.tool === 'security' || finding.tool === 'lint' ? finding.tool : 'lint') as 'security' | 'lint',
      rule_id: typeof finding.rule_id === 'string' ? finding.rule_id : undefined,
    }];
  }) : [];
  const summary = item.summary && typeof item.summary === 'object' ? item.summary as Record<string, unknown> : {};
  return {
    findings,
    security_score: item.security_score,
    files_scanned: typeof item.files_scanned === 'number' ? item.files_scanned : 0,
    summary: {
      critical: typeof summary.critical === 'number' ? summary.critical : 0,
      high: typeof summary.high === 'number' ? summary.high : 0,
      medium: typeof summary.medium === 'number' ? summary.medium : 0,
      low: typeof summary.low === 'number' ? summary.low : 0,
      info: typeof summary.info === 'number' ? summary.info : 0,
    },
    passed: item.passed === true,
  };
}

function normalizeMentalSimulation(raw: unknown): MentalSimulationResult | undefined {
  if (!raw || typeof raw !== 'object') return undefined;
  const item = raw as Record<string, unknown>;
  if (typeof item.is_valid !== 'boolean') return undefined;
  const metrics = item.metrics && typeof item.metrics === 'object' ? item.metrics as Record<string, unknown> : {};
  return {
    is_valid: item.is_valid,
    graph_valid: item.graph_valid === true,
    test_baseline_valid: item.test_baseline_valid === true,
    type_conformance_valid: item.type_conformance_valid === true,
    cycles: Array.isArray(item.cycles) ? item.cycles.flatMap((cycle) =>
      Array.isArray(cycle) ? [cycle.filter((step): step is string => typeof step === 'string')] : []
    ) : [],
    diagnostics: Array.isArray(item.diagnostics)
      ? item.diagnostics.filter((d): d is string => typeof d === 'string')
      : [],
    metrics: {
      total_modules_simulated: typeof metrics.total_modules_simulated === 'number' ? metrics.total_modules_simulated : 0,
      total_assertions_checked: typeof metrics.total_assertions_checked === 'number' ? metrics.total_assertions_checked : 0,
      cycles_detected: typeof metrics.cycles_detected === 'number' ? metrics.cycles_detected : 0,
    },
  };
}

function normalizeProductionReadiness(raw: unknown): ProductionReadinessResult | undefined {
  if (!raw || typeof raw !== 'object') return undefined;
  const item = raw as Record<string, unknown>;
  if (typeof item.passed !== 'boolean') return undefined;
  const findings = Array.isArray(item.findings)
    ? item.findings.flatMap((f) => {
        if (!f || typeof f !== 'object') return [];
        const r = f as Record<string, unknown>;
        if (typeof r.rule_id !== 'string' || typeof r.severity !== 'string' || typeof r.message !== 'string') return [];
        return [{
          rule_id: r.rule_id,
          severity: r.severity as string,
          message: r.message,
          file: typeof r.file === 'string' ? r.file : '',
          ...(typeof r.line === 'number' ? { line: r.line } : {}),
          ...(typeof r.suggestion === 'string' ? { suggestion: r.suggestion } : {}),
        }];
      })
    : [];
  return {
    passed: item.passed,
    findings,
    files_checked: typeof item.files_checked === 'number' ? item.files_checked : 0,
    error_count: typeof item.error_count === 'number' ? item.error_count : 0,
    warning_count: typeof item.warning_count === 'number' ? item.warning_count : 0,
    info_count: typeof item.info_count === 'number' ? item.info_count : 0,
  };
}

function normalizeAccessibility(raw: unknown): AccessibilityAuditResult | undefined {
  if (!raw || typeof raw !== 'object') return undefined;
  const item = raw as Record<string, unknown>;
  if (typeof item.passed !== 'boolean') return undefined;
  const findings = Array.isArray(item.findings)
    ? item.findings.flatMap((f) => {
        if (!f || typeof f !== 'object') return [];
        const r = f as Record<string, unknown>;
        if (typeof r.rule_id !== 'string' || !['error', 'warning', 'info'].includes(r.severity as string) || typeof r.message !== 'string') return [];
        return [{
          rule_id: r.rule_id,
          severity: r.severity as 'error' | 'warning' | 'info',
          message: r.message,
          file: typeof r.file === 'string' ? r.file : '',
          ...(typeof r.line === 'number' ? { line: r.line } : {}),
          ...(typeof r.column === 'number' ? { column: r.column } : {}),
          ...(typeof r.suggestion === 'string' ? { suggestion: r.suggestion } : {}),
        }];
      })
    : [];
  return {
    passed: item.passed,
    findings,
    files_checked: typeof item.files_checked === 'number' ? item.files_checked : 0,
    error_count: typeof item.error_count === 'number' ? item.error_count : 0,
    warning_count: typeof item.warning_count === 'number' ? item.warning_count : 0,
    info_count: typeof item.info_count === 'number' ? item.info_count : 0,
  };
}

function normalizeUXConsistency(raw: unknown): UXConsistencyResult | undefined {
  if (!raw || typeof raw !== 'object') return undefined;
  const item = raw as Record<string, unknown>;
  if (typeof item.passed !== 'boolean') return undefined;
  const findings = Array.isArray(item.findings)
    ? item.findings.flatMap((f) => {
        if (!f || typeof f !== 'object') return [];
        const r = f as Record<string, unknown>;
        if (typeof r.rule_id !== 'string' || !['error', 'warning', 'info'].includes(r.severity as string) || typeof r.message !== 'string') return [];
        return [{
          rule_id: r.rule_id,
          severity: r.severity as 'error' | 'warning' | 'info',
          message: r.message,
          file: typeof r.file === 'string' ? r.file : '',
          ...(typeof r.line === 'number' ? { line: r.line } : {}),
          ...(typeof r.suggestion === 'string' ? { suggestion: r.suggestion } : {}),
        }];
      })
    : [];
  return {
    passed: item.passed,
    findings,
    files_checked: typeof item.files_checked === 'number' ? item.files_checked : 0,
    error_count: typeof item.error_count === 'number' ? item.error_count : 0,
    warning_count: typeof item.warning_count === 'number' ? item.warning_count : 0,
    info_count: typeof item.info_count === 'number' ? item.info_count : 0,
  };
}

type TauriInvoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>;
type TauriEvent = {
  listen?: (name: string, callback: (event: { payload: unknown }) => void) => Promise<() => void>;
};

// Tauri v1 exposes `window.__TAURI__.invoke`; Tauri 2 moved it to
// `window.__TAURI__.core.invoke` (the v1 top-level property no longer exists).
// Resolve both so tests that mock the v1 shape keep working.
type TauriGlobal = {
  invoke?: TauriInvoke;
  core?: { invoke?: TauriInvoke };
  event?: TauriEvent;
};

function resolveTauriInvoke(): TauriInvoke | undefined {
  const tauri = typeof window !== 'undefined' ? window.__TAURI__ : undefined;
  return tauri?.invoke ?? tauri?.core?.invoke;
}

export type AgentTaskInput = { agentId: string } & (
  | { input: string; payload?: never }
  | { payload: Record<string, unknown>; input?: never }
);

export type AgentTaskResult = {
  taskId: string;
  agentId: string;
  success: boolean;
  output?: unknown;
  error: string | null;
};


export type PlanStep = {
  id: string;
  description: string;
  status: string;
};

export type PlanSummary = {
  id: string;
  goal: string;
  status: string;
  steps: PlanStep[];
};

export type PipelineInput = {
  goal: string;
  emit_events?: boolean;
  runId?: string;
  project_id?: string | null;
  /** Scoped consent for supervised verifier subprocesses; only 'granted' ever spawns. */
  execution_consent?: ExecutionConsent;
};

/** Mirrors src-tauri pipeline::supervisor::ExecutionConsent (kebab-case values). */
export type ExecutionConsent = 'not-supplied' | 'granted' | 'denied';

export type PipelineStatusResponse = {
  runId: string;
  active: boolean;
  state: 'running' | 'pausing' | 'paused' | 'cancelling' | 'unknown';
};

export type AgentRuntimeSettings = {
  maxConcurrency: number;
  defaultTimeoutMs: number;
  retryLimit: number;
};

export const DEFAULT_AGENT_RUNTIME_SETTINGS: AgentRuntimeSettings = {
  maxConcurrency: 2,
  defaultTimeoutMs: 60000,
  retryLimit: 3,
};

export type ProviderSettings = {
  id: string;
  provider: string;
  api_key: string;
  enabled: boolean;
};

export type ProviderRecord = ProviderSettings & {
  createdAt: number;
  updatedAt: number;
};

export type GatewayRequest = {
  provider: string;
  payload: unknown;
  task_type?: string;
};

export type GatewayResponse = {
  status: number;
  body: unknown;
  raw?: string;
};

export type PipelineRunHistoryRecord = {
  id: string;
  goal: string;
  status: string;
  step_count: number;
  repair_count: number;
  success: boolean;
  error?: string | null;
  finished_at?: number | null;
};

export type GatewayRunState = {
  status: 'idle' | 'running' | 'completed' | 'failed';
  provider?: string;
  result?: unknown;
  error?: string;
};

export type GatewayRunActions = {
  onRun: (request: GatewayRequest) => Promise<GatewayRunState>;
  onCancel: () => Promise<void>;
};

export type GatewayRunResult = GatewayRunState & GatewayRunActions;


export type LearningPattern = {
  id: string;
  kind: 'success' | 'anti';
  agent_role: string;
  task_type: string;
  content: string;
  score: number;
  sample_count: number;
  updated_at: number;
};

export type WorkspaceInfo = {
  id: string;
  root: string;
};

export type RepairAction = { description: string; applied: boolean };
export type StartupBenchmarkResult = { elapsed_ms: number; initialized: boolean };
export type UpdateCheckResult = { available: boolean; manifest?: { version: string } };
export type AgentTaskMetrics = { task_id: string; agent_id: string; duration_ms: number; success: boolean };
export type MonitorEvent = { ts: string; severity: string; message: string };
export type IpcBenchmarkResult = { iterations: number; total_ms: number; min_ms: number; max_ms: number; avg_ms: number; p95_ms: number };
export type ProjectFile = { path: string; content: string; language: string };
export type PathValidation = { valid: boolean; normalized: string; issues: string[] };
export type AgentDescriptor = { id: string; name: string; role: string; description: string; capabilities: string[] };
export type AgentTaskHistoryRecord = {
  id: string; agent_id: string; task_id: string; status: string;
  started_at: number; finished_at: number; output: string | null; error: string | null;
};
export type AgentTaskRunRecord = {
  id: string; agentId: string; input: string; success: boolean;
  output?: string; error?: string | null; durationMs: number; timestamp: number;
};
export type PipelineEvent = {
  runId: string; topic: string; severity: 'info' | 'warn' | 'error'; message: string; ts: number;
};

declare global {
  interface Window { __TAURI__?: TauriGlobal }
}

export function isTauriAvailable(): boolean {
  return typeof resolveTauriInvoke() === 'function';
}

export function normalizePlanStepState(value: unknown): PlanStepState {
  const normalized = typeof value === 'string' ? value.toLowerCase().replace(/[ _-]/g, '') : '';
  if (normalized === 'pending') return 'pending';
  if (normalized === 'inprogress') return 'in_progress';
  if (normalized === 'completed') return 'completed';
  if (normalized === 'failed') return 'failed';
  return 'pending';
}

export function normalizeLearningPatterns(value: unknown, kind: 'success' | 'anti'): LearningPattern[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item) => {
    if (!item || typeof item !== 'object') return [];
    const record = item as Record<string, unknown>;
    if (typeof record.id !== 'string' || typeof record.content !== 'string') return [];
    return [{ ...record, id: record.id, content: record.content, kind,
      agent_role: typeof record.agent_role === 'string' ? record.agent_role : '',
      task_type: typeof record.task_type === 'string' ? record.task_type : '',
      score: typeof record.score === 'number' ? record.score : 0,
      sample_count: typeof record.sample_count === 'number' ? record.sample_count : 0,
      updated_at: typeof record.updated_at === 'number' ? record.updated_at : 0,
    }];
  });
}

export function normalizeAgentRuntimeSettings(value: unknown): AgentRuntimeSettings {
  const record = value && typeof value === 'object' ? value as Record<string, unknown> : {};
  // The native Rust struct serializes camelCase; accept older snake_case records too.
  const maxConcurrency = record.maxConcurrency ?? record.max_concurrency;
  const defaultTimeoutMs = record.defaultTimeoutMs ?? record.default_timeout_ms;
  const retryLimit = record.retryLimit ?? record.retry_limit;
  return {
    maxConcurrency: typeof maxConcurrency === 'number' && Number.isInteger(maxConcurrency) && maxConcurrency > 0 ? maxConcurrency : DEFAULT_AGENT_RUNTIME_SETTINGS.maxConcurrency,
    defaultTimeoutMs: typeof defaultTimeoutMs === 'number' && Number.isInteger(defaultTimeoutMs) && defaultTimeoutMs > 0 ? defaultTimeoutMs : DEFAULT_AGENT_RUNTIME_SETTINGS.defaultTimeoutMs,
    retryLimit: typeof retryLimit === 'number' && Number.isInteger(retryLimit) && retryLimit >= 0 ? retryLimit : DEFAULT_AGENT_RUNTIME_SETTINGS.retryLimit,
  };
}

async function invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  // Tauri 2: `window.__TAURI__.core.invoke`; Tauri 1 / test mocks: `.invoke`.
  const invoke = resolveTauriInvoke();
  if (typeof invoke !== 'function') {
    // Browser preview (no Tauri shell): return a benign fallback instead of
    // throwing, so the frontend can be audited/developed without the backend.
    // Lists return empty arrays; scalar commands return null/0/false.
    if (command.startsWith('list_')) return [] as unknown as T;
    if (command.startsWith('get_')) return null as unknown as T;
    if (command.startsWith('is_') || command.startsWith('has_') || command.startsWith('can_')) return false as unknown as T;
    if (command.startsWith('save_') || command.startsWith('delete_') || command.startsWith('record_') || command.startsWith('register_') || command.startsWith('cancel_') || command.startsWith('abort_') || command.startsWith('pause_') || command.startsWith('resume_')) return undefined as unknown as T;
    return null as unknown as T;
  }
  return (args === undefined ? invoke(command) : invoke(command, args)) as Promise<T>;
}

export function normalizeAgentTaskResult(value: unknown, fallbackAgentId = ''): AgentTaskResult {
  const record = value && typeof value === 'object' ? value as Record<string, unknown> : {};
  return {
    taskId: typeof record.task_id === 'string' ? record.task_id : typeof record.taskId === 'string' ? record.taskId : '',
    agentId: typeof record.agent_id === 'string' ? record.agent_id : typeof record.agentId === 'string' ? record.agentId : fallbackAgentId,
    success: record.success === true,
    output: record.output,
    error: typeof record.error === 'string' ? record.error : null,
  };
}

export function normalizePipelineEvent(value: unknown): PipelineEvent | null {
  if (!value || typeof value !== 'object') return null;
  const event = value as Record<string, unknown>;
  const runId = event.run_id ?? event.runId;
  if (typeof runId !== 'string' || typeof event.topic !== 'string' || typeof event.message !== 'string') return null;
  return { runId, topic: event.topic, message: event.message,
    severity: event.severity === 'error' ? 'error' : event.severity === 'warn' ? 'warn' : 'info',
    ts: typeof event.ts === 'number' ? event.ts : Date.now() };
}

export function normalizePipelineRun(value: unknown): PipelineRun {
  const record = value && typeof value === 'object' ? value as Record<string, unknown> : {};
  const planValue = record.plan && typeof record.plan === 'object' ? record.plan as Record<string, unknown> : {};
  if (typeof planValue.id !== 'string' || typeof planValue.goal !== 'string') {
    console.warn('Invalid pipeline response:', value);
    throw new Error('Invalid pipeline response');
  }
  let executionContext: PipelineRun['execution_context'];
  if (record.execution_context != null) {
    const context = record.execution_context as Record<string, unknown>;
    if (typeof context !== 'object' || Array.isArray(context) || context.schema_version !== 1) {
      throw new Error('Unsupported pipeline execution context');
    }
    if (context.mode === 'simulation' && context.project_id == null) {
      executionContext = { schema_version: 1, mode: 'simulation' };
    } else if (context.mode === 'workspace' && typeof context.project_id === 'string'
      && context.project_id.length > 0 && !['.', '..', 'recode.db'].includes(context.project_id)
      && !/[\/\\\0]/.test(context.project_id)) {
      executionContext = { schema_version: 1, mode: 'workspace', project_id: context.project_id };
    } else {
      throw new Error('Invalid pipeline execution context');
    }
  }
  const planSteps = Array.isArray(planValue.steps) ? planValue.steps : [];
  const steps = planSteps.flatMap((step) => {
    if (!step || typeof step !== 'object') return [];
    const item = step as Record<string, unknown>;
    if (typeof item.id !== 'string' || typeof item.description !== 'string') return [];
    return [{ id: item.id, description: item.description, status: normalizePlanStepState(item.status) }];
  });
  const repairActions = Array.isArray(record.repair_actions) ? record.repair_actions : [];
  const events = Array.isArray(record.events) ? record.events : [];
  const normalizedGeneratedFiles = Array.isArray(record.generated_files) ? record.generated_files.flatMap((file) => {
    if (!file || typeof file !== 'object') return [];
    const item = file as Record<string, unknown>;
    if (typeof item.path !== 'string') return [];
    return [{
      path: item.path,
      content: typeof item.content === 'string' ? item.content : undefined,
      language: typeof item.language === 'string' ? item.language : undefined,
      status: typeof item.status === 'string' ? item.status : 'Pending',
      error: typeof item.error === 'string' ? item.error : undefined,
    }];
  }) : [];
  const normalizedFailedFiles = Array.isArray(record.failed_files) ? record.failed_files.flatMap((file) => {
    if (!file || typeof file !== 'object') return [];
    const item = file as Record<string, unknown>;
    if (typeof item.path !== 'string') return [];
    return [{
      path: item.path,
      content: typeof item.content === 'string' ? item.content : undefined,
      language: typeof item.language === 'string' ? item.language : undefined,
      status: typeof item.status === 'string' ? item.status : 'Failed',
      error: typeof item.error === 'string' ? item.error : undefined,
    }];
  }) : [];
  const outcome = record.outcome && typeof record.outcome === 'object' ? record.outcome as Record<string, unknown> : null;
  const normalizedOutcome = outcome ? {
    success: outcome.success === true,
    degraded: outcome.degraded === true,
    generated_file_count: typeof outcome.generated_file_count === 'number' ? outcome.generated_file_count : 0,
    failed_file_count: typeof outcome.failed_file_count === 'number' ? outcome.failed_file_count : 0,
    failed_files: Array.isArray(outcome.failed_files) ? outcome.failed_files.flatMap((file) => {
      if (!file || typeof file !== 'object') return [];
      const item = file as Record<string, unknown>;
      if (typeof item.path !== 'string') return [];
      return [{ path: item.path, status: typeof item.status === 'string' ? item.status : 'Failed', error: typeof item.error === 'string' ? item.error : undefined }];
    }) : [],
    summary: typeof outcome.summary === 'string' ? outcome.summary : '',
    verification_results: normalizeVerificationResults(outcome.verification_results ?? record.verification_results),
    repair_results: (Array.isArray(outcome.repair_results) ? outcome.repair_results : Array.isArray(record.repair_results) ? record.repair_results : []).flatMap((item) => {
      if (!item || typeof item !== 'object') return [];
      const record = item as Record<string, unknown>;
      return typeof record.description === 'string' ? [{ applied: record.applied === true, description: record.description, path: typeof record.path === 'string' ? record.path : undefined }] : [];
    }),
    verification_summary: typeof outcome.verification_summary === 'string' ? outcome.verification_summary : typeof record.verification_summary === 'string' ? record.verification_summary : '',
    static_analysis: normalizeStaticAnalysis(outcome.static_analysis),
    ...(normalizeMentalSimulation(outcome.mental_simulation) ? { mental_simulation: normalizeMentalSimulation(outcome.mental_simulation) } : {}),
    ...(normalizeProductionReadiness(outcome.production_readiness) ? { production_readiness: normalizeProductionReadiness(outcome.production_readiness) } : {}),
    ...(normalizeAccessibility(outcome.accessibility) ? { accessibility: normalizeAccessibility(outcome.accessibility) } : {}),
    ...(normalizeUXConsistency(outcome.ux_consistency) ? { ux_consistency: normalizeUXConsistency(outcome.ux_consistency) } : {}),
  } : undefined;
  return {
    ...(typeof record.run_id === 'string' ? { runId: record.run_id } : {}),
    ...(executionContext ? { execution_context: executionContext } : {}),
    plan: {
      id: typeof planValue.id === 'string' ? planValue.id : 'plan',
      goal: typeof planValue.goal === 'string' ? planValue.goal : '',
      status: typeof planValue.status === 'string' ? planValue.status : 'pending',
      steps,
    },
    repair_actions: repairActions.flatMap((action) => {
      if (!action || typeof action !== 'object') return [];
      const item = action as Record<string, unknown>;
      return typeof item.description === 'string' && typeof item.applied === 'boolean'
        ? [{ description: item.description, applied: item.applied }]
        : [];
    }),
    events: events.flatMap((event) => {
      if (!event || typeof event !== 'object') return [];
      const item = event as Record<string, unknown>;
      if (typeof item.message !== 'string') return [];
      return [{
        ts: typeof item.ts === 'string' ? item.ts : String(item.ts ?? Date.now()),
        severity: item.severity === 'error' ? 'error' : item.severity === 'warn' ? 'warn' : 'info',
        message: item.message,
      }];
    }),
    generated_files: normalizedGeneratedFiles,
    failed_files: normalizedFailedFiles,
    outcome: normalizedOutcome,
  };
}

export async function runPipeline(input: PipelineInput): Promise<PipelineRun> {
  const payload: Record<string, unknown> = {
    goal: input.goal,
    emit_events: input.emit_events ?? false,
    run_id: input.runId ?? '',
  };
  if (input.project_id !== undefined && input.project_id !== null) {
    payload.project_id = input.project_id;
  }
  if (input.execution_consent !== undefined) {
    payload.execution_consent = input.execution_consent;
  }
  return normalizePipelineRun(await invokeCommand<unknown>('run_pipeline', {
    input: payload,
  }));
}

export async function pausePipeline(runId: string): Promise<boolean> {
  return invokeCommand<boolean>('pause_pipeline', { run_id: runId });
}

export async function resumePipeline(runId: string): Promise<boolean> {
  return invokeCommand<boolean>('resume_pipeline', { run_id: runId });
}

export async function abortPipeline(runId: string): Promise<boolean> {
  return invokeCommand<boolean>('abort_pipeline', { run_id: runId });
}

export async function cancelPipeline(runId: string): Promise<boolean> {
  return invokeCommand<boolean>('cancel_pipeline', { run_id: runId });
}

/** One reconciler's outcome inside a workspace reconciliation report. */
export type ReconciliationResult = {
  reconciler: string;
  modified: boolean;
  summary: string;
  duration_ms: number;
  error?: string | null;
};

/** Report of the reconcile_workspace IPC command. */
export type ReconciliationReport = {
  results: ReconciliationResult[];
  modified_count: number;
  summary: string;
};

export async function reconcileWorkspace(projectId: string): Promise<ReconciliationReport> {
  const response = await invokeCommand<unknown>('reconcile_workspace', { project_id: projectId });
  if (!isRecord(response)) throw new Error('Invalid reconciliation report response');
  const results = Array.isArray(response.results) ? response.results : [];
  return {
    results: results.map((item) => {
      if (!isRecord(item)) throw new Error('Invalid reconciliation result');
      return {
        reconciler: typeof item.reconciler === 'string' ? item.reconciler : '',
        modified: item.modified === true,
        summary: typeof item.summary === 'string' ? item.summary : '',
        duration_ms: typeof item.duration_ms === 'number' ? item.duration_ms : 0,
        error: typeof item.error === 'string' ? item.error : null,
      };
    }),
    modified_count: typeof response.modified_count === 'number' ? response.modified_count : 0,
    summary: typeof response.summary === 'string' ? response.summary : '',
  };
}

/** Result of the consented install_dependencies IPC command. */
export type InstallResult = {
  note: string;
};

export async function installDependencies(
  projectId: string,
  consent: ExecutionConsent,
): Promise<InstallResult> {
  const response = await invokeCommand<unknown>('install_dependencies', {
    project_id: projectId,
    consent,
  });
  if (!isRecord(response)) throw new Error('Invalid install result response');
  return { note: typeof response.note === 'string' ? response.note : '' };
}

/** Per-gate result inside a build-verification report. */
export type BuildVerificationGate = {
  passed: boolean;
  degraded: boolean;
  gate: string;
  detail: string;
};

/** Report of the verify_build IPC command. */
export type BuildVerificationReport = {
  results: BuildVerificationGate[];
  summary: string;
};

export async function verifyBuild(projectId: string, consent: ExecutionConsent): Promise<BuildVerificationReport> {
  const response = await invokeCommand<unknown>('verify_build', {
    project_id: projectId,
    consent,
  });
  if (!isRecord(response)) throw new Error('Invalid build verification response');
  const results = Array.isArray(response.results) ? response.results : [];
  return {
    results: results.map((item) => {
      if (!isRecord(item)) throw new Error('Invalid verification result');
      return {
        passed: item.passed === true,
        degraded: item.degraded === true,
        gate: typeof item.gate === 'string' ? item.gate : '',
        detail: typeof item.detail === 'string' ? item.detail : '',
      };
    }),
    summary: typeof response.summary === 'string' ? response.summary : '',
  };
}

export async function getPipelineStatus(runId: string): Promise<PipelineStatusResponse> {
  const response = await invokeCommand<{ run_id: string; active: boolean; state: PipelineStatusResponse['state'] }>(
    'get_pipeline_status',
    { run_id: runId },
  );
  return { runId: response.run_id, active: response.active, state: response.state };
}

/** A single file entry in a generation checkpoint. */
export type GenerationCheckpointFile = {
  path: string;
  sha256: string;
  matchesDisk: boolean;
};

/** Inspectable partial-run state (generation checkpoint), or null when absent. */
export type GenerationCheckpoint = {
  attemptId: string;
  status: string;
  sequence: number;
  savedAt: number;
  files: GenerationCheckpointFile[];
};

export async function getGenerationCheckpoint(projectId: string): Promise<GenerationCheckpoint | null> {
  const response = await invokeCommand<unknown>('get_generation_checkpoint', {
    project_id: projectId,
  });
  if (!response || typeof response !== 'object') {
    return null;
  }
  const record = response as Record<string, unknown>;
  const files = Array.isArray(record.files) ? record.files : [];
  return {
    attemptId: typeof record.attempt_id === 'string' ? record.attempt_id : '',
    status: typeof record.status === 'string' ? record.status : '',
    sequence: typeof record.sequence === 'number' ? record.sequence : 0,
    savedAt: typeof record.saved_at === 'number' ? record.saved_at : 0,
    files: files.map((item) => {
      if (!isRecord(item)) throw new Error('Invalid checkpoint file entry');
      return {
        path: typeof item.path === 'string' ? item.path : '',
        sha256: typeof item.sha256 === 'string' ? item.sha256 : '',
        matchesDisk: item.matches_disk === true,
      };
    }),
  };
}

export async function getPipelineRunResult(runId: string): Promise<PipelineRun | null> {
  const response = await invokeCommand<unknown>('get_pipeline_result', { run_id: runId });
  if (!response) {
    return null;
  }
  if (typeof response !== 'object') {
    console.warn('Invalid pipeline result response:', response);
    return null;
  }
  try {
    return normalizePipelineRun(response);
  } catch (error) {
    console.warn('Failed to normalize pipeline result:', error);
    return null;
  }
}

export async function listPipelineRuns(limit: number): Promise<PipelineRunHistoryRecord[]> {
  const response = await invokeCommand<unknown>('list_pipeline_runs', { limit });
  if (!Array.isArray(response)) throw new Error('Invalid pipeline history response');
  return response.map((item) => {
    if (!item || typeof item !== 'object') throw new Error('Invalid pipeline history record');
    const record = item as Record<string, unknown>;
    return {
      id: typeof record.id === 'string' ? record.id : '',
      goal: typeof record.goal === 'string' ? record.goal : '',
      status: typeof record.status === 'string' ? record.status : 'running',
      step_count: typeof record.step_count === 'number' ? record.step_count : 0,
      repair_count: typeof record.repair_count === 'number' ? record.repair_count : 0,
      success: record.success === true,
      error: typeof record.error === 'string' ? record.error : null,
      finished_at: typeof record.finished_at === 'number' ? record.finished_at : null,
    };
  });
}

export async function createAndExecutePlan(input: { id?: string; goal: string }): Promise<PlanSummary> {
  const response = await invokeCommand<unknown>('create_and_execute_plan', {
    input: { id: input.id ?? crypto.randomUUID(), goal: input.goal },
  });
  return normalizePlanSummary(response);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

export function normalizePlanSummary(value: unknown): PlanSummary {
  if (!isRecord(value) || typeof value.id !== 'string' || !value.id.trim()
    || typeof value.goal !== 'string') throw new Error('Invalid plan response');
  const record = value;
  if (record.steps !== undefined && !Array.isArray(record.steps)) throw new Error('Invalid plan steps');
  const planSteps = Array.isArray(record.steps) ? record.steps : [];
  const steps = planSteps.map((step: unknown) => {
    if (!isRecord(step) || typeof step.id !== 'string' || !step.id.trim()
      || typeof step.description !== 'string') throw new Error('Invalid plan step');
    return { id: step.id, description: step.description, status: normalizePlanStepState(step.status) };
  });
  return {
    id: value.id,
    goal: value.goal,
    status: typeof record.status === 'string' ? record.status : 'pending',
    steps,
  };
}

export async function savePlan(input: { id?: string; goal: string }): Promise<void> {
  if (!input.id?.trim()) throw new Error('Plan ID is required');
  await invokeCommand<void>('save_plan', { input: { id: input.id, goal: input.goal } });
}

export async function listPlans(): Promise<PlanSummary[]> {
  const response = await invokeCommand<unknown>('list_plans');
  if (!Array.isArray(response)) throw new Error('Invalid plans response');
  return response.map(normalizePlanSummary);
}

export async function recordPlanMetrics(input: { projectId: string; status: string; phases: string[] }): Promise<void> {
  await invokeCommand<void>('record_plan_metrics', { project_id: input.projectId, status: input.status, phases: input.phases });
}

export async function attemptSelfHeal(): Promise<Array<{ description: string; applied: boolean }>> {
  const response = await invokeCommand<{ actions: RepairAction[] }>('attempt_self_heal');
  return response.actions;
}

export async function attemptStartupSelfHeal(): Promise<RepairAction[]> {
  const response = await invokeCommand<{ actions: RepairAction[] }>('attempt_startup_self_heal');
  return response.actions;
}

export async function listMonitorEvents(): Promise<Array<{ ts: string; severity: string; message: string }>> {
  return invokeCommand<Array<{ ts: string; severity: string; message: string }>>('list_monitor_events');
}

export async function captureDiagnostics(input: { kind: string; message: string; detail?: string }): Promise<void> {
  await invokeCommand<void>('capture_diagnostics', { input });
}

export async function checkUpdates(endpoint: string, currentVersion: string): Promise<UpdateCheckResult> {
  const response = await invokeCommand<unknown>('check_updates', { request: { endpoint, current_version: currentVersion } });
  if (!response || typeof response !== 'object') throw new Error('Invalid update response');
  const record = response as Record<string, unknown>;
  return {
    available: record.available === true,
    ...(record.manifest && typeof record.manifest === 'object' && typeof (record.manifest as Record<string, unknown>).version === 'string'
      ? { manifest: record.manifest as { version: string } } : {}),
  };
}

export async function verifyUpdateSignature(artifactPath: string, signaturePath: string, publicKeyPath: string): Promise<boolean> {
  return invokeCommand<boolean>('verify_signature', {
    request: { artifact_path: artifactPath, signature_path: signaturePath, public_key_path: publicKeyPath },
  });
}

export async function runGatewayTask(input: GatewayRequest): Promise<GatewayResponse> {
  const response = await invokeCommand<unknown>('run_gateway_task', { input });
  if (!response || typeof response !== 'object') throw new Error('Invalid gateway response');
  return response as GatewayResponse;
}

export async function getGatewayStatus(): Promise<{ state: string }> {
  const state = await invokeCommand<unknown>('get_gateway_status');
  if (typeof state !== 'string' || !state.trim()) throw new Error('Invalid gateway configuration status');
  return { state };
}

export async function runStartupBenchmark(): Promise<StartupBenchmarkResult> {
  const response = await invokeCommand<unknown>('run_startup_benchmark_command', {});
  if (!response || typeof response !== 'object') throw new Error('Invalid benchmark response');
  return response as StartupBenchmarkResult;
}

export async function runIpcBenchmark(iterations: number): Promise<{ iterations: number; total_ms: number; min_ms: number; max_ms: number; avg_ms: number; p95_ms: number }> {
  return invokeCommand<IpcBenchmarkResult>('run_ipc_benchmark_command', { request: { iterations } });
}

export async function loadAgentRuntimeSettings(): Promise<AgentRuntimeSettings> {
  return normalizeAgentRuntimeSettings(await invokeCommand<unknown>('load_agent_runtime_settings'));
}

export async function saveAgentRuntimeSettings(input: AgentRuntimeSettings): Promise<AgentRuntimeSettings> {
  return normalizeAgentRuntimeSettings(await invokeCommand<unknown>('save_agent_runtime_settings', {
    settings: input,
  }));
}

export async function listProviderSettings(): Promise<ProviderRecord[]> {
  return invokeCommand<ProviderRecord[]>('list_providers');
}

export async function saveProviderSettings(input: ProviderSettings): Promise<ProviderRecord> {
  return invokeCommand<ProviderRecord>('save_provider', { provider: input });
}

export async function deleteProviderSettings(id: string): Promise<void> {
  await invokeCommand<void>('delete_provider', { id });
}

export async function createWorkspace(input: { id?: string }): Promise<WorkspaceInfo> {
  return invokeCommand<WorkspaceInfo>('create_workspace', { input });
}

export async function listWorkspaces(): Promise<WorkspaceInfo[]> {
  return invokeCommand<WorkspaceInfo[]>('list_workspaces');
}

export async function writeProjectFile(workspaceId: string, file: ProjectFile): Promise<void> {
  await invokeCommand<void>('write_project_file', { workspace_id: workspaceId, file });
}

export async function readProjectFile(workspaceId: string, path: string): Promise<ProjectFile | null> {
  return invokeCommand<ProjectFile | null>('read_project_file', { workspace_id: workspaceId, path });
}

export async function listProjectFiles(workspaceId: string): Promise<ProjectFile[]> {
  return invokeCommand<ProjectFile[]>('list_project_files', { workspace_id: workspaceId });
}

export async function validateWorkspacePath(workspaceId: string, path: string): Promise<PathValidation> {
  return invokeCommand<PathValidation>('validate_workspace_path', { workspace_id: workspaceId, path });
}

export function subscribeToPipelineEvents(handler: (event: PipelineEvent) => void): () => void {
  const listen = typeof window !== 'undefined' ? window.__TAURI__?.event?.listen : undefined;
  if (!listen) return () => {};
  let disposed = false;
  let unlisten: (() => void) | undefined;
  void listen('pipeline-event', (event: any) => {
    const normalized = normalizePipelineEvent(event.payload);
    if (!disposed && normalized) handler(normalized);
  }).then((cleanup: any) => {
    if (disposed) cleanup(); else unlisten = cleanup;
  });
  return () => { disposed = true; unlisten?.(); };
}

export async function recordAgentTaskHistory(input: AgentTaskHistoryRecord): Promise<void> {
  await invokeCommand<void>('record_agent_task_history', { input });
}

export async function listAgentTaskHistory(agent_id: string): Promise<AgentTaskHistoryRecord[]> {
  return invokeCommand<AgentTaskHistoryRecord[]>('list_agent_task_history', { agent_id });
}

export async function listAgentTaskMetrics(): Promise<Array<{ task_id: string; agent_id: string; duration_ms: number; success: boolean }>> {
  const response = await invokeCommand<unknown>('list_agent_task_metrics');
  if (!Array.isArray(response)) throw new Error('Invalid metrics response');
  return response as AgentTaskMetrics[];
}

export async function listAgentTaskRuns(agent_id: string): Promise<AgentTaskRunRecord[]> {
  const response = await invokeCommand<unknown>('list_agent_task_runs', { agent_id });
  if (!Array.isArray(response)) throw new Error('Invalid task runs response');
  return response.map((record: unknown) => {
    if (!isRecord(record) || typeof record.id !== 'string' || !record.id.trim()
      || typeof record.agent_id !== 'string' || !record.agent_id.trim()
      || typeof record.input !== 'string' || typeof record.success !== 'boolean'
      || !isNonNegativeInteger(record.duration_ms) || !isNonNegativeInteger(record.timestamp)
      || (record.output != null && typeof record.output !== 'string')
      || (record.error != null && typeof record.error !== 'string')) throw new Error('Invalid task run');
    return {
      id: record.id,
      agentId: record.agent_id,
      input: record.input,
      success: record.success,
      output: typeof record.output === 'string' ? record.output : undefined,
      error: typeof record.error === 'string' ? record.error : undefined,
      durationMs: record.duration_ms,
      timestamp: record.timestamp,
    };
  });
}

export async function listAgents(): Promise<AgentDescriptor[]> {
  const response = await invokeCommand<unknown>('list_agents');
  if (!Array.isArray(response)) throw new Error('Invalid agents response');
  return response.map((record: unknown) => {
    if (!isRecord(record) || typeof record.id !== 'string' || !record.id.trim()
      || typeof record.name !== 'string' || typeof record.role !== 'string'
      || typeof record.description !== 'string' || !Array.isArray(record.capabilities)
      || !record.capabilities.every((item: unknown) => typeof item === 'string')) throw new Error('Invalid agent descriptor');
    return { id: record.id, name: record.name, role: record.role, description: record.description,
      capabilities: record.capabilities.map((item: string) => item) };
  });
}

export async function registerAgent(descriptor: AgentDescriptor): Promise<void> {
  await invokeCommand<void>('register_agent', { descriptor });
}

export async function saveAgentTaskRun(input: AgentTaskRunRecord): Promise<void> {
  if (!input.id.trim() || !input.agentId.trim() || !isNonNegativeInteger(input.durationMs)
    || !isNonNegativeInteger(input.timestamp)) throw new Error('Invalid task run');
  await invokeCommand<void>('save_agent_task_run', { input: {
    id: input.id,
    agent_id: input.agentId,
    input: input.input,
    success: input.success,
    output: input.output,
    error: input.error ?? null,
    duration_ms: input.durationMs,
    timestamp: input.timestamp,
  } });
}

export async function listSuccessPatterns(agentRole: string, taskType: string): Promise<LearningPattern[]> {
  const response = await invokeCommand<unknown>('list_success_patterns', { agent_role: agentRole, task_type: taskType });
  if (!Array.isArray(response)) {
    return [];
  }
  return normalizeLearningPatterns(response, 'success');
}

export async function listAntiPatterns(agentRole: string, taskType: string): Promise<LearningPattern[]> {
  const response = await invokeCommand<unknown>('list_anti_patterns', { agent_role: agentRole, task_type: taskType });
  if (!Array.isArray(response)) {
    return [];
  }
  return normalizeLearningPatterns(response, 'anti');
}

export async function recordSuccessPattern(input: { agentRole: string; taskType: string; summary: string }): Promise<void> {
  await invokeCommand<void>('record_success_pattern', { input });
}

export async function recordAntiPattern(input: { agentRole: string; taskType: string; summary: string }): Promise<void> {
  await invokeCommand<void>('record_anti_pattern', { input });
}

export async function executeAgentTask(input: AgentTaskInput): Promise<AgentTaskResult> {
  const response = await invokeCommand<unknown>('execute_agent_task', {
    input: { agent_id: input.agentId, payload: input.payload ?? { input: input.input } },
  });
  if (!isRecord(response) || typeof response.success !== 'boolean'
    || typeof (response.task_id ?? response.taskId) !== 'string'
    || !String(response.task_id ?? response.taskId).trim()
    || typeof (response.agent_id ?? response.agentId) !== 'string'
    || !String(response.agent_id ?? response.agentId).trim()
    || (response.error != null && typeof response.error !== 'string')) throw new Error('Invalid agent task response');
  return normalizeAgentTaskResult(response, input.agentId);
}

function isNonNegativeInteger(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
}
// ---------------------------------------------------------------------------
// Dynamic provider / model management: catalog, discovery, routing,
// credentials, and the dynamic orchestrator. Mirrors the Tauri commands in
// src-tauri/src/tauri_commands.rs.
// ---------------------------------------------------------------------------

export type Modality = 'text' | 'image' | 'audio' | 'video';
export type CostTier = 'low' | 'medium' | 'high';

export type ModelInfo = {
  id: string;
  provider: string;
  modelId: string;
  name: string;
  family: string;
  contextWindow: number;
  maxOutputTokens: number;
  inputModalities: Modality[];
  supportsTools: boolean;
  supportsStructuredOutput: boolean;
  supportsReasoning: boolean;
  costTier: CostTier;
  description: string;
  strengths: string[];
  taskAffinity: Record<string, number>;
  promptPricePerMtok: number | null;
  completionPricePerMtok: number | null;
  discovered: boolean;
};

export type UpsertModelInput = {
  provider: string;
  modelId: string;
  name?: string;
  family?: string;
  contextWindow?: number;
  maxOutputTokens?: number;
  inputModalities?: Modality[];
  supportsTools?: boolean;
  supportsStructuredOutput?: boolean;
  supportsReasoning?: boolean;
  costTier?: CostTier;
  description?: string;
  strengths?: string[];
  taskAffinity?: Record<string, number>;
  promptPricePerMtok?: number | null;
  completionPricePerMtok?: number | null;
};

export type DiscoveryResult = {
  provider: string;
  ok: boolean;
  count: number;
  modelIds: string[];
  error: string | null;
};

export type ProviderCredentialInfo = {
  provider: string;
  hasApiKey: boolean;
  baseUrl: string | null;
  defaultModel: string | null;
  keyHint: string | null;
};

export type RoutingRequest = {
  taskKind: string;
  minContextWindow: number;
  requiredModalities: Modality[];
  requiresTools: boolean;
  requiresStructuredOutput: boolean;
  prefersReasoning: boolean;
  preferredModel?: string | null;
};

export type CandidateScore = {
  modelId: string;
  provider: string;
  model: string;
  eligible: boolean;
  disqualification: string | null;
  score: number;
  affinity: number;
  costPenalty: number;
  reliability: number;
  speed: number;
};

export type RoutingDecision = {
  chosen: CandidateScore | null;
  ranked: CandidateScore[];
  considered: number;
};

export type OrchestratorDecision = {
  agentId: string;
  modelId: string | null;
  taskKind: string;
  taskDescription: string;
  rationale: string;
  request: unknown | null;
};

export type OrchestratorStepResult = {
  decision: OrchestratorDecision;
  success: boolean;
  output: unknown;
  error: string | null;
};

export type ConcurrentLimitRecord = {
  modelId: string;
  limit: number;
};

export type KeyStatusRecord = {
  provider: string;
  status: string;
  message: string;
  checkedAtMs: number;
};

export type SelectionState = {
  selectedModels: string[];
  disabledProviders: string[];
  concurrencyLimits: ConcurrentLimitRecord[];
  providerVerification: KeyStatusRecord[];
};

export type KeyVerificationResult = {
  provider: string;
  status: string;
  message: string;
  modelCount: number;
  checkedAtMs: number;
};

export type ConcurrencyCalibrationResult = {
  provider: string;
  modelId: string;
  ok: boolean;
  limit: number;
  levelsTested: number;
  stoppedReason: string;
  checkedAtMs: number;
};

export async function listModels(provider?: string): Promise<ModelInfo[]> {
  return invokeCommand<ModelInfo[]>('list_models', { provider: provider ?? null });
}

export async function upsertModel(input: UpsertModelInput): Promise<ModelInfo> {
  return invokeCommand<ModelInfo>('upsert_model', { input });
}

export async function removeModel(id: string): Promise<boolean> {
  return invokeCommand<boolean>('remove_model', { id });
}

export async function discoverProviderModels(provider: string): Promise<DiscoveryResult> {
  return invokeCommand<DiscoveryResult>('discover_provider_models', { provider });
}

export async function importEnvApiKey(provider: string): Promise<ProviderCredentialInfo> {
  return invokeCommand<ProviderCredentialInfo>('import_env_api_key', { provider });
}

export async function listProviderCredentials(): Promise<Record<string, ProviderCredentialInfo>> {
  return invokeCommand<Record<string, ProviderCredentialInfo>>('list_provider_credentials');
}

export async function saveProviderConfig(
  provider: string,
  baseUrl: string | null,
  defaultModel: string | null,
): Promise<void> {
  await invokeCommand<void>('save_provider_config', { provider, baseUrl, defaultModel });
}

export async function saveProviderApiKey(
  provider: string,
  apiKey: string,
): Promise<ProviderCredentialInfo> {
  return invokeCommand<ProviderCredentialInfo>('save_provider_api_key', {
    provider,
    apiKey,
  });
}

export async function deleteProviderApiKey(provider: string): Promise<boolean> {
  return invokeCommand<boolean>('delete_provider_api_key', { provider });
}

export async function verifyProviderKey(provider: string): Promise<KeyVerificationResult> {
  return invokeCommand<KeyVerificationResult>('verify_provider_key', { provider });
}

export async function calibrateModelConcurrency(
  provider: string,
  modelId: string,
): Promise<ConcurrencyCalibrationResult> {
  return invokeCommand<ConcurrencyCalibrationResult>('calibrate_model_concurrency', {
    provider,
    modelId,
  });
}

export async function getSelectionState(): Promise<SelectionState> {
  return invokeCommand<SelectionState>('get_selection_state');
}

export async function setModelSelected(modelId: string, selected: boolean): Promise<SelectionState> {
  return invokeCommand<SelectionState>('set_model_selected', { modelId, selected });
}

export async function setProviderEnabled(provider: string, enabled: boolean): Promise<SelectionState> {
  return invokeCommand<SelectionState>('set_provider_enabled', { provider, enabled });
}

export async function routeModels(input: {
  task?: string;
  request?: RoutingRequest;
}): Promise<RoutingDecision> {
  return invokeCommand<RoutingDecision>('route_models', { input });
}

export async function inferRoutingRequest(task: string): Promise<RoutingRequest> {
  return invokeCommand<RoutingRequest>('infer_routing_request', { task });
}

export async function runOrchestratorStep(input: {
  goal: string;
  step: string;
}): Promise<OrchestratorStepResult> {
  const response = await invokeCommand<OrchestratorStepResult>('run_orchestrator_step', { input });
  return response;
}

export type OrchestratorAutonomousResult = {
  completed: boolean;
  aborted: boolean;
  steps: string[];
};

export async function runOrchestratorAutonomous(input: {
  goal: string;
  initialStep?: string;
  maxSteps?: number;
}): Promise<OrchestratorAutonomousResult> {
  return invokeCommand<OrchestratorAutonomousResult>('run_orchestrator_autonomous', { input });
}

