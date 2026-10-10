import { describe, expect, it } from 'vitest';
import { normalizeVerificationResults, summarizeVerification, verificationState } from './verification';
import { normalizePipelineRun } from './ipc';

const pass = { gate: 'persistence', passed: true, degraded: false, detail: 'Bytes matched' };

describe('verification evidence classification', () => {
  it('keeps absent evidence distinct from a passing result', () => {
    expect(summarizeVerification()).toMatchObject({ state: 'unreported', total: 0 });
  });

  it('labels successful supplied checks without claiming an overall verified build', () => {
    expect(summarizeVerification([pass]).state).toBe('reported-pass');
  });

  it('keeps a degraded failure failed rather than reducing it to a warning', () => {
    expect(verificationState({ ...pass, passed: false, degraded: true })).toBe('failed');
    expect(summarizeVerification([{ ...pass, passed: false, degraded: true }]).counts.failed).toBe(1);
  });

  it('does not hide an explicit named failure when descriptive metadata is missing', () => {
    const results = normalizeVerificationResults([{ gate: 'compile', passed: false }]);
    expect(summarizeVerification(results).state).toBe('failed');
    expect(results[0].detail).toContain('evidence unavailable');
  });

  it('retains malformed records as unknown instead of dropping them', () => {
    const results = normalizeVerificationResults([pass, null, { gate: 'compile', passed: 'true' }]);
    expect(summarizeVerification(results)).toMatchObject({ state: 'unknown', total: 3, counts: { passed: 1, unknown: 2 } });
  });

  it('does not accept a malformed collection even if the object resembles a passing gate', () => {
    expect(summarizeVerification(normalizeVerificationResults(pass)).state).toBe('unknown');
  });

  it('preserves failure precedence and conflicting duplicate records', () => {
    const results = normalizeVerificationResults([pass, { ...pass, passed: false }, { gate: 'other' }]);
    expect(summarizeVerification(results)).toMatchObject({ state: 'failed', total: 3, counts: { passed: 1, failed: 1, unknown: 1 } });
  });

  it('preserves the old normalized shape for complete legacy records', () => {
    expect(normalizeVerificationResults([pass])).toEqual([pass]);
  });

  it('retains unknown evidence when normalizing a pipeline response', () => {
    const run = normalizePipelineRun({
      plan: { id: 'plan-1', goal: 'build', status: 'completed', steps: [] },
      outcome: { success: true, verification_results: [{ gate: 'compile', detail: 'Status missing' }] },
    });
    expect(run.outcome?.verification_results?.[0].evidenceState).toBe('unknown');
    expect(summarizeVerification(run.outcome?.verification_results).state).toBe('unknown');
  });
});
