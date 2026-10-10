import type { GateEvidence, VerificationGateResult, VerificationEvidenceState } from '../types/frontend';

function normalizeGateEvidence(value: unknown): GateEvidence | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return undefined;
  const evidence = value as Record<string, unknown>;
  if (evidence.schema_version !== 1 || typeof evidence.required !== 'boolean'
    || (evidence.applicable !== null && typeof evidence.applicable !== 'boolean')
    || !['passed', 'failed', 'skipped', 'unknown'].includes(String(evidence.status))) return undefined;
  let execution: GateEvidence['execution'] = null;
  if (evidence.execution !== null) {
    if (!evidence.execution || typeof evidence.execution !== 'object' || Array.isArray(evidence.execution)) return undefined;
    const receipt = evidence.execution as Record<string, unknown>;
    if (!Array.isArray(receipt.command) || receipt.command.length === 0
      || !receipt.command.every((arg): arg is string => typeof arg === 'string')
      || !receipt.command[0].trim() || !Number.isSafeInteger(receipt.exit_code)
      || !Number.isSafeInteger(receipt.duration_ms) || (receipt.duration_ms as number) < 0
      || typeof receipt.tool_version !== 'string' || !receipt.tool_version.trim()) return undefined;
    execution = { command: receipt.command, exit_code: receipt.exit_code as number,
      duration_ms: receipt.duration_ms as number, tool_version: receipt.tool_version };
  }
  return { schema_version: 1, status: evidence.status as GateEvidence['status'],
    required: evidence.required, applicable: evidence.applicable as boolean | null, execution };
}

/** Legacy booleans cannot express missing evidence; retain that distinction explicitly. */
export function normalizeVerificationResults(value: unknown): VerificationGateResult[] {
  if (value == null) return [];
  const validCollection = Array.isArray(value);
  const rows = validCollection ? value : [value];
  return rows.map((value) => {
    const record = value && typeof value === 'object' && !Array.isArray(value)
      ? value as Record<string, unknown> : {};
    const evidence = normalizeGateEvidence(record.evidence);
    const invalidEvidence = record.evidence != null && !evidence;
    const rawEvidence = record.evidence && typeof record.evidence === 'object'
      ? record.evidence as Record<string, unknown> : undefined;
    const rawExecution = rawEvidence?.execution && typeof rawEvidence.execution === 'object'
      ? rawEvidence.execution as Record<string, unknown> : undefined;
    const knownFailure = validCollection && typeof record.gate === 'string' && Boolean(record.gate.trim())
      && (record.evidenceState === 'failed' || (rawEvidence?.schema_version === 1
        && (rawEvidence.status === 'failed'
          || (rawEvidence.status === 'passed' && record.passed === false)
          || (Number.isSafeInteger(rawExecution?.exit_code) && rawExecution?.exit_code !== 0))));
    const valid = validCollection && record.evidenceState !== 'unknown'
      && !invalidEvidence
      && typeof record.gate === 'string' && Boolean(record.gate.trim())
      && typeof record.passed === 'boolean'
      && (record.passed === false || (typeof record.degraded === 'boolean' && typeof record.detail === 'string'));
    return {
      gate: typeof record.gate === 'string' && record.gate.trim() ? record.gate : 'Unknown gate',
      passed: record.passed === true,
      degraded: record.degraded === true,
      detail: typeof record.detail === 'string' ? record.detail : 'Malformed verification record; evidence unavailable.',
      ...(evidence ? { evidence } : {}),
      ...(knownFailure && (!valid || record.evidenceState === 'failed')
        ? { evidenceState: 'failed' as const } : !valid ? { evidenceState: 'unknown' as const } : {}),
    };
  });
}

export function verificationState(result: VerificationGateResult): VerificationEvidenceState {
  if (result.evidenceState === 'failed') return 'failed';
  if (result.evidenceState === 'unknown') return 'unknown';
  if (result.evidence) {
    const evidence = result.evidence;
    if (evidence.status === 'failed' || (evidence.execution && evidence.execution.exit_code !== 0)) return 'failed';
    if (evidence.status === 'unknown' || evidence.status === 'skipped') return evidence.status;
    if (!result.passed) return 'failed';
    if (evidence.applicable !== true) return 'unknown';
    if (result.gate !== 'persistence' && !evidence.execution) return 'unknown';
    return result.degraded ? 'limited' : 'passed';
  }
  // A degraded failure is still a failure. Never downgrade it to a warning.
  if (result.passed === false) return 'failed';
  if (result.passed !== true || typeof result.degraded !== 'boolean') return 'unknown';
  return result.degraded ? 'limited' : 'passed';
}

export function summarizeVerification(results: readonly VerificationGateResult[] = []) {
  const counts = { passed: 0, failed: 0, limited: 0, unknown: 0, skipped: 0 };
  for (const result of results) counts[verificationState(result)] += 1;
  const state = results.length === 0 ? 'unreported'
    : counts.failed > 0 ? 'failed'
      : counts.unknown > 0 ? 'unknown'
        : counts.skipped > 0 ? 'skipped'
          : counts.limited > 0 ? 'limited' : 'reported-pass';
  return { state, counts, total: results.length } as const;
}
