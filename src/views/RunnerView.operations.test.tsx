import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import RunnerView from './RunnerView';

describe('RunnerView operational clarity', () => {
  it('does not invent completion percentages without step measurements', () => {
    render(<RunnerView runState={{ status: 'completed', goal: 'simulate', planId: 'p-1' }} consoleEntries={[]} onToggleRightPanel={() => undefined} />);
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
    expect(screen.getByText('No step measurements')).toBeInTheDocument();
    expect(screen.queryByText('All steps completed')).not.toBeInTheDocument();
  });

  it('shows measured plan progress independently from overall build success', () => {
    render(<RunnerView runState={{ status: 'running', goal: 'build', steps: [
      { id: '1', description: 'Plan work', status: 'completed' },
      { id: '2', description: 'Generate files', status: 'in_progress' },
    ] }} consoleEntries={[]} onToggleRightPanel={() => undefined} />);
    expect(screen.getByRole('progressbar', { name: 'Plan step progress' })).toHaveAttribute('aria-valuenow', '50');
  });

  it('combines text search, source and severity filters', () => {
    render(<RunnerView runState={{ status: 'idle' }} consoleEntries={[
      { id: '1', source: 'pipeline', level: 'error', message: 'Build failed', ts: 1000 },
      { id: '2', source: 'agent', level: 'info', message: 'Build started', ts: 2000 },
    ]} onToggleRightPanel={() => undefined} />);
    fireEvent.change(screen.getByLabelText('Search output'), { target: { value: 'BUILD' } });
    fireEvent.change(screen.getByLabelText('Filter by level'), { target: { value: 'error' } });
    expect(screen.getByText('Build failed')).toBeInTheDocument();
    expect(screen.queryByText('Build started')).not.toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('Filter by source'), { target: { value: 'agent' } });
    expect(screen.getByText('No matching entries')).toBeInTheDocument();
  });

  it('distinguishes limited verification from a clean pass', () => {
    render(<RunnerView runState={{ status: 'completed', goal: 'build', planId: 'p-2', outcome: {
      success: true, degraded: true, generated_file_count: 1, failed_file_count: 0, summary: 'Generated a file',
      verification_results: [{ gate: 'types', passed: true, degraded: true, detail: 'Fallback check only' }],
    } }} consoleEntries={[]} onToggleRightPanel={() => undefined} />);
    expect(screen.getByText('Fallback check only').closest('li')).toHaveClass('warn');
    expect(screen.getByRole('heading', { name: 'Limited verification reported' })).toBeInTheDocument();
  });
});
