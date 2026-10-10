import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { normalizePipelineRun } from './ipc';
import RunCompletionSummary from '../components/RunCompletionSummary';

const fixture = { plan: { id: 'p', goal: 'build', status: 'completed', steps: [] }, repair_actions: [], events: [] };

describe('native run provenance', () => {
  it('keeps historical missing context unknown rather than inferring simulation', () => {
    const run = normalizePipelineRun(fixture);
    expect(run.execution_context).toBeUndefined();
    render(<RunCompletionSummary run={run} />);
    expect(screen.getByText(/Workspace provenance unavailable/)).toBeInTheDocument();
  });

  it('retains native workspace identity and labels it as provenance rather than replay authority', () => {
    const context = { schema_version: 1, mode: 'workspace', project_id: 'my project.v2' };
    const run = normalizePipelineRun({ ...fixture, execution_context: context });
    expect(run.execution_context).toEqual(context);
    render(<RunCompletionSummary run={run} />);
    expect(screen.getByText(/Recorded workspace: my project.v2/)).toHaveTextContent('not replay authorization');
  });

  it('labels an explicit simulation separately from an old unknown record', () => {
    const run = normalizePipelineRun({ ...fixture, execution_context: { schema_version: 1, mode: 'simulation' } });
    render(<RunCompletionSummary run={run} />);
    expect(screen.getByText('Plan-only simulation — no workspace targeted.')).toBeInTheDocument();
  });

  it('rejects malformed, unsupported and contradictory context without inventing a target', () => {
    for (const context of [true, [], {}, { schema_version: 2, mode: 'simulation' },
      { schema_version: 1, mode: 'simulation', project_id: 'unexpected' },
      ...['', '.', '..', 'recode.db', '../outside', 'a/b', 'a\\b', 'a\0b'].map(project_id =>
        ({ schema_version: 1, mode: 'workspace', project_id }))]) {
      expect(() => normalizePipelineRun({ ...fixture, execution_context: context })).toThrow(/execution context/);
    }
  });
});
