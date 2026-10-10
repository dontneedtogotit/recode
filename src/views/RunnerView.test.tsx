import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import RunnerView from './RunnerView';
import type { PipelineRun } from '../lib/ipc';

const failedRun: PipelineRun = {
  plan: { id: 'plan-1', goal: 'build', status: 'completed', steps: [] },
  repair_actions: [],
  events: [],
  generated_files: [],
  failed_files: [{ path: 'src/main.ts', status: 'failed', error: 'write failed' }],
  outcome: {
    success: false,
    degraded: true,
    generated_file_count: 0,
    failed_file_count: 1,
    failed_files: [{ path: 'src/main.ts', status: 'failed', error: 'write failed' }],
    summary: 'generated 0 of 1 files',
  },
};

describe('RunnerView failed build details', () => {
  it('does not call a completed plan with a failed build outcome a simulation', () => {
    render(<RunnerView
      runState={{ status: 'completed', goal: 'build', planId: 'plan-1', outcome: failedRun.outcome }}
      consoleEntries={[]}
      onToggleRightPanel={() => undefined}
    />);
    expect(screen.getByText(/Build failed for plan plan-1/)).toBeInTheDocument();
    expect(screen.queryByText(/Simulated plan/)).not.toBeInTheDocument();
    expect(screen.queryByText('completed', { selector: '.badge.ok' })).not.toBeInTheDocument();
  });

  it('shows a failed status and inspectable file outcome rather than a completed badge', () => {
    render(<RunnerView
      runState={{ status: 'failed', goal: 'build', error: 'generated 0 of 1 files', run: failedRun }}
      consoleEntries={[]}
      onToggleRightPanel={() => undefined}
    />);
    expect(screen.getByText('Failed files')).toBeInTheDocument();
    expect(screen.getByText('src/main.ts')).toBeInTheDocument();
    expect(screen.getAllByText('failed', { selector: '.badge.err' }).length).toBeGreaterThan(0);
    expect(screen.queryByText('completed', { selector: '.badge' })).not.toBeInTheDocument();
  });
});
