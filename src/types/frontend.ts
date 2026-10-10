export type PlanStepState = 'pending' | 'in_progress' | 'completed' | 'failed';

export type PlanStep = {
  id: string;
  description: string;
  status: PlanStepState;
};

export type RightPanelTab = 'details' | 'chat' | 'history' | 'telemetry';

/** Navigation intent only. Accepting a brief never grants execution permission. */
export type BriefReviewRequest = {
  goal: string;
  source: 'saved-plan' | 'run-history' | 'last-run';
  sourceId: string;
};

export type ConsoleEntry = {
  id: string;
  source: 'pipeline' | 'agent' | 'self_heal' | 'runtime' | 'system';
  level: 'info' | 'warn' | 'error';
  message: string;
  ts: number;
};

export type GeneratedFile = {
  path: string;
  content?: string;
  language?: string;
  status: string;
  error?: string;
};

export type VerificationEvidenceState = 'passed' | 'failed' | 'limited' | 'unknown' | 'skipped';

/** Versioned native report; a receipt describes execution, not authority to run. */
export type GateEvidence = {
  schema_version: 1;
  status: 'passed' | 'failed' | 'skipped' | 'unknown';
  required: boolean;
  applicable: boolean | null;
  execution: {
    command: string[];
    exit_code: number;
    duration_ms: number;
    tool_version: string;
  } | null;
};

export type VerificationGateResult = {
  gate: string;
  passed: boolean;
  degraded: boolean;
  detail: string;
  evidence?: GateEvidence;
  /** Preserve a known failure even when its descriptive metadata is malformed. */
  evidenceState?: 'unknown' | 'failed';
};

export type StaticAnalysisFinding = {
  file: string;
  line?: number;
  category: string;
  severity: 'critical' | 'high' | 'medium' | 'low' | 'info';
  message: string;
  suggestion: string;
  tool: 'security' | 'lint';
  rule_id?: string;
};

export type StaticAnalysisResult = {
  findings: StaticAnalysisFinding[];
  security_score: number;
  files_scanned: number;
  summary: { critical: number; high: number; medium: number; low: number; info: number };
  passed: boolean;
};

/** Mental execution simulation result (backend pipeline/mental_simulator.rs). */
export type MentalSimulationResult = {
  is_valid: boolean;
  graph_valid: boolean;
  test_baseline_valid: boolean;
  type_conformance_valid: boolean;
  cycles: string[][];
  diagnostics: string[];
  metrics: { total_modules_simulated: number; total_assertions_checked: number; cycles_detected: number };
};

export type RunOutcome = {
  success: boolean;
  degraded: boolean;
  generated_file_count: number;
  failed_file_count: number;
  summary: string;
  verification_summary?: string;
  verification_results?: VerificationGateResult[];
  static_analysis?: StaticAnalysisResult;
  mental_simulation?: MentalSimulationResult;
  production_readiness?: ProductionReadinessResult;
  accessibility?: AccessibilityAuditResult;
  ux_consistency?: UXConsistencyResult;
};

export type AccessibilityAuditResult = {
  passed: boolean;
  findings: AccessibilityFinding[];
  files_checked: number;
  error_count: number;
  warning_count: number;
  info_count: number;
};

export type AccessibilityFinding = {
  rule_id: string;
  severity: 'error' | 'warning' | 'info';
  message: string;
  file: string;
  line?: number;
  column?: number;
  suggestion?: string;
};

export type UXConsistencyResult = {
  passed: boolean;
  findings: UXFinding[];
  files_checked: number;
  error_count: number;
  warning_count: number;
  info_count: number;
};

export type UXFinding = {
  rule_id: string;
  severity: 'error' | 'warning' | 'info';
  message: string;
  file: string;
  line?: number;
  suggestion?: string;
};

/** Shared data contracts are independent of the native transport adapter. */
export type RunExecutionContext = { schema_version: 1 } & (
  | { mode: 'simulation' }
  | { mode: 'workspace'; project_id: string }
);

export type PipelineRun = {
  runId?: string;
  execution_context?: RunExecutionContext;
  plan: { id: string; goal: string; status: string; steps: PlanStep[] };
  repair_actions: Array<{ description: string; applied: boolean }>;
  events: Array<{ ts: string; severity: string; message: string }>;
  generated_files?: GeneratedFile[];
  failed_files?: GeneratedFile[];
  outcome?: RunOutcome & {
    failed_files: Array<{ path: string; status: string; error?: string }>;
    repair_results?: Array<{ applied: boolean; description: string; path?: string }>;
  };
};

export type RunState =
  | { status: 'idle'; goal?: string; outcome?: never }
  | { status: 'running'; goal: string; steps?: PlanStep[]; outcome?: never }
  | {
      status: 'completed';
      goal: string;
      planId: string;
      steps?: PlanStep[];
      repairActions?: Array<{ description: string; applied: boolean }>;
      generatedFiles?: GeneratedFile[];
      failedFiles?: GeneratedFile[];
      outcome?: RunOutcome;
      durationMs?: number;
    }
  | { status: 'failed'; goal: string; error: string; durationMs?: number; outcome?: never; run?: PipelineRun };

