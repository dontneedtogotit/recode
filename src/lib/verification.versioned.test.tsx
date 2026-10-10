import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { VerificationEvidence } from '../components/VerificationEvidence';
import type { GateEvidence, VerificationGateResult } from '../types/frontend';
import { normalizePipelineRun } from './ipc';
import { normalizeVerificationResults, summarizeVerification, verificationState } from './verification';

const unknown: VerificationGateResult = {
  gate: 'typecheck', passed: false, degraded: false, detail: 'Not executed',
  evidence: { schema_version: 1, status: 'unknown', required: true, applicable: null, execution: null },
};
const receipt: NonNullable<GateEvidence['execution']> = {
  command: ['fixture-check', '--check'], exit_code: 0, duration_ms: 4, tool_version: 'fixture-v1',
};
const passed: VerificationGateResult = {
  ...unknown, passed: true,
  evidence: { ...unknown.evidence!, status: 'passed', applicable: true, execution: receipt },
};

describe('versioned native gate evidence', () => {
  it('preserves unknown without manufacturing a measured failure from the compatibility boolean', () => {
    const results = normalizeVerificationResults([unknown]);
    expect(results).toEqual([unknown]);
    expect(summarizeVerification(results)).toMatchObject({ state: 'unknown', counts: { failed: 0, unknown: 1 } });
  });

  it('keeps skipped distinct from pass and failure', () => {
    const skipped = { ...unknown, evidence: { ...unknown.evidence!, status: 'skipped' as const } };
    expect(verificationState(skipped)).toBe('skipped');
    expect(summarizeVerification([skipped]).state).toBe('skipped');
  });

  it('rejects unsupported versions rather than falling back to a legacy pass', () => {
    expect(verificationState(normalizeVerificationResults([
      { ...passed, evidence: { ...passed.evidence, schema_version: 2 } },
    ])[0])).toBe('unknown');
  });

  it('requires applicable execution evidence for a process pass', () => {
    expect(verificationState(passed)).toBe('passed');
    expect(verificationState({ ...passed, evidence: { ...passed.evidence!, execution: null } })).toBe('unknown');
    expect(verificationState({ ...passed, evidence: { ...passed.evidence!, applicable: null } })).toBe('unknown');
  });

  it('does not invent a subprocess receipt for in-process persistence', () => {
    const persistence = { ...passed, gate: 'persistence', evidence: { ...passed.evidence!, execution: null } };
    expect(verificationState(normalizeVerificationResults([persistence])[0])).toBe('passed');
  });

  it('gives a nonzero exit precedence over a claimed pass', () => {
    const results = normalizeVerificationResults([
      { ...passed, evidence: { ...passed.evidence, execution: { ...receipt, exit_code: 1 } } },
    ]);
    expect(verificationState(results[0])).toBe('failed');
  });

  it('rejects malformed execution receipts', () => {
    for (const invalid of [{ ...receipt, command: [] }, { ...receipt, exit_code: '0' },
      { ...receipt, duration_ms: -1 }, { ...receipt, tool_version: '' }]) {
      const result = normalizeVerificationResults([{ ...passed, evidence: { ...passed.evidence, execution: invalid } }])[0];
      expect(verificationState(result)).toBe('unknown');
    }
  });

  it('retains versioned evidence across the pipeline IPC normalization', () => {
    const run = normalizePipelineRun({
      plan: { id: 'p', goal: 'build', status: 'completed', steps: [] },
      outcome: { success: false, verification_results: [unknown] },
    });
    expect(run.outcome?.verification_results).toEqual([unknown]);
  });

  it('retains explicit failures with incomplete metadata across repeated normalization', () => {
    const result = normalizeVerificationResults([{ ...unknown,
      evidence: { schema_version: 1, status: 'failed', execution: { exit_code: 1 } },
    }]);
    expect(verificationState(result[0])).toBe('failed');
    expect(verificationState(normalizeVerificationResults(result)[0])).toBe('failed');
  });

  it('shows unresolved required checks without calling them failed checks', () => {
    render(<VerificationEvidence reportedSuccess results={[unknown]} />);
    expect(screen.getByRole('alert')).toHaveTextContent('required verification is unresolved');
    expect(screen.getByText('0 failed')).toBeInTheDocument();
    expect(screen.getByText(/Required · Applicability unresolved/)).toBeInTheDocument();
  });

  it('displays supplied argv and measured receipt fields separately from the gate detail', () => {
    render(<VerificationEvidence results={[passed]} />);
    expect(screen.getByText('Execution receipt')).toBeInTheDocument();
    expect(screen.getByText('["fixture-check","--check"]')).toBeInTheDocument();
    expect(screen.getByText(/Exit 0 · 4 ms · fixture-v1/)).toBeInTheDocument();
  });
});
