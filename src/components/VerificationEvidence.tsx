import type { VerificationGateResult } from '../types/frontend';
import { summarizeVerification, verificationState } from '../lib/verification';
import '../styles/verification.css';

type Props = { results?: readonly VerificationGateResult[]; runtimeSummary?: string; reportedSuccess?: boolean };
const TITLES = {
  unreported: 'No gate evidence reported',
  failed: 'Verification failures reported',
  unknown: 'Verification evidence is incomplete',
  limited: 'Limited verification reported',
  skipped: 'Verification checks were skipped',
  'reported-pass': 'Reported checks passed',
};
const LABELS = { passed: 'Passed', failed: 'Failed', limited: 'Limited', unknown: 'Unknown', skipped: 'Skipped' };
const VARIANTS = { passed: 'ok', failed: 'err', limited: 'warn', unknown: '', skipped: 'warn' };

export function VerificationEvidence({ results = [], runtimeSummary, reportedSuccess }: Props) {
  const summary = summarizeVerification(results);
  const unresolvedRequired = results.some((result) => result.evidence?.required && verificationState(result) !== 'passed');
  return (
    <section className="verification-evidence stack-md" aria-label="Verification evidence">
      <div className="verification-heading">
        <h2>{TITLES[summary.state]}</h2>
        <span className="text-xs text-muted">{summary.total} reported checks</span>
      </div>
      {reportedSuccess && summary.counts.failed > 0 && <p role="alert" className="text-danger text-sm">The run reports success but also contains failed verification checks. Do not treat this result as a verified build.</p>}
      {reportedSuccess && summary.counts.failed === 0 && unresolvedRequired && <p role="alert" className="text-danger text-sm">The run reports success but required verification is unresolved. Do not treat this result as a verified build.</p>}
      {summary.total > 0 ? <>
        <div className="verification-counts" aria-label="Reported verification totals">
          <span>{summary.counts.passed} passed</span><span>{summary.counts.failed} failed</span><span>{summary.counts.limited} limited</span><span>{summary.counts.unknown} unknown</span>
          {summary.counts.skipped > 0 && <span>{summary.counts.skipped} skipped</span>}
        </div>
        <ul className="verification-results">
          {results.map((result, index) => {
            const state = verificationState(result);
            return <li key={`${result.gate}-${index}`} className={`verification-result ${VARIANTS[state]}`}>
              <div className="verification-result-heading"><strong>{result.gate}</strong><span className={`badge ${VARIANTS[state]}`}>{LABELS[state]}</span></div>
              <p>{result.detail || 'No check details were provided.'}</p>
              {result.evidence && <div className="text-xs text-muted">
                <span>{result.evidence.required ? 'Required' : 'Optional'} · {result.evidence.applicable === null ? 'Applicability unresolved' : result.evidence.applicable ? 'Applicable' : 'Not applicable'}</span>
                {result.evidence.execution ? <details><summary>Execution receipt</summary>
                  <pre>{JSON.stringify(result.evidence.execution.command)}</pre>
                  <p>Exit {result.evidence.execution.exit_code} · {result.evidence.execution.duration_ms} ms · {result.evidence.execution.tool_version}</p>
                </details> : <p>No subprocess execution receipt. In-process persistence checks do not execute a command.</p>}
              </div>}
              {state === 'unknown' && <span className="text-xs text-muted">Evidence is unavailable, unresolved or malformed; this is not a pass or a measured failure.</span>}
            </li>;
          })}
        </ul>
      </> : <p className="text-sm text-secondary">This run has no structured gate results. Generating files does not establish that compilation, tests or startup succeeded.</p>}
      <p className="verification-limit">Reported passes do not establish a fully verified build. Legacy records lack applicability and execution receipts. Versioned records expose these fields, but historical receipts are not fresh verification of the current workspace.</p>
      {runtimeSummary && <details className="verification-runtime-summary"><summary>Runtime verification summary</summary><p>{runtimeSummary}</p></details>}
    </section>
  );
}
