import type { PipelineRun } from '../lib/ipc';
import { VerificationEvidence } from './VerificationEvidence';

type Props = {
  run: PipelineRun;
};

export default function RunCompletionSummary({ run }: Props) {
  const terminal = run.plan.status === 'completed' || run.plan.status === 'failed' || run.plan.status === 'cancelled';
  const success = run.outcome?.success ?? run.plan.status === 'completed';
  const statusVariant = success ? 'ok' : /cancel/i.test(run.plan.status) ? 'warn' : 'err';
  const displayStatus = !success && run.plan.status === 'completed' ? 'failed' : run.plan.status;

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
      <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
        <div className={`badge ${statusVariant}`}>{displayStatus}</div>
        <div className="badge info">{run.plan.goal}</div>
        {run.repair_actions.length > 0 && <div className="badge warn">{run.repair_actions.length} repairs</div>}
      </div>

      <p className="text-xs text-muted">
        {run.execution_context?.mode === 'workspace'
          ? `Recorded workspace: ${run.execution_context.project_id}. This is historical provenance, not replay authorization.`
          : run.execution_context?.mode === 'simulation'
            ? 'Plan-only simulation — no workspace targeted.'
            : 'Workspace provenance unavailable for this record; do not infer it from the current selection.'}
      </p>

      {run.outcome && (
        <div
          style={{
            display: 'flex',
            gap: 10,
            flexWrap: 'wrap',
            padding: '10px 12px',
            background: 'var(--bg-inset)',
            border: '1px solid var(--border-subtle)',
            borderRadius: 'var(--radius-sm)',
          }}
        >
          <div className={`badge ${run.outcome.success ? 'ok' : 'err'}`}>
            {run.outcome.success ? 'success' : 'failed'}
          </div>
          {run.outcome.degraded && <div className="badge warn">degraded</div>}
          <div className="badge info">{run.outcome.generated_file_count} generated</div>
          <div className="badge err">{run.outcome.failed_file_count} failed</div>
          <span className="text-xs text-muted">{run.outcome.summary}</span>
        </div>
      )}

      <VerificationEvidence results={run.outcome?.verification_results} runtimeSummary={run.outcome?.verification_summary} reportedSuccess={run.outcome?.success} />

      {run.generated_files && run.generated_files.length > 0 && (
        <div className="stack-sm">
          <div className="section-header-title">Generated files</div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            {run.generated_files.map((file) => (
              <div
                key={file.path}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 10,
                  padding: '8px 10px',
                  background: 'var(--bg-inset)',
                  border: '1px solid var(--border-subtle)',
                  borderRadius: 'var(--radius-sm)',
                }}
              >
                <span style={{ fontSize: 14, minWidth: 24, textAlign: 'center' }}>✓</span>
                <span style={{ flex: 1, fontSize: 13 }}>{file.path}</span>
                <span className="badge ok" style={{ fontSize: 11, padding: '2px 6px' }}>
                  {file.status}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}

      {run.failed_files && run.failed_files.length > 0 && (
        <div className="stack-sm">
          <div className="section-header-title">Failed files</div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            {run.failed_files.map((file) => (
              <div
                key={file.path}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 10,
                  padding: '8px 10px',
                  background: 'var(--bg-inset)',
                  border: '1px solid var(--border-subtle)',
                  borderRadius: 'var(--radius-sm)',
                }}
              >
                <span style={{ fontSize: 14, minWidth: 24, textAlign: 'center' }}>✗</span>
                <span style={{ flex: 1, fontSize: 13 }}>{file.path}</span>
                <span className="badge err" style={{ fontSize: 11, padding: '2px 6px' }}>
                  {file.status}
                </span>
                {file.error && (
                  <span className="text-xs text-muted" style={{ maxWidth: 260, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    {file.error}
                  </span>
                )}
              </div>
            ))}
          </div>
        </div>
      )}

      {terminal && !success && (
        <div className="text-sm text-secondary">
          Run did not complete successfully. Review the error above and retry from run history.
        </div>
      )}
    </div>
  );
}
