import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { VerificationEvidence } from './VerificationEvidence';
import RunCompletionSummary from './RunCompletionSummary';

const gate = { gate: 'default', passed: true, degraded: false, detail: 'One file persisted' };

describe('shared verification evidence', () => {
  it('shows an explicit missing-evidence state for historical results', () => {
    render(<RunCompletionSummary run={{ plan: { id: 'old', goal: 'legacy goal', status: 'completed', steps: [] }, repair_actions: [], events: [] }} />);
    expect(screen.getByRole('heading', { name: 'No gate evidence reported' })).toBeInTheDocument();
  });

  it('does not equate reported passes with a verified build', () => {
    render(<VerificationEvidence results={[gate]} />);
    expect(screen.getByRole('heading', { name: 'Reported checks passed' })).toBeInTheDocument();
    expect(screen.getByText(/Reported passes do not establish a fully verified build/)).toBeInTheDocument();
  });

  it('exposes contradictory success and failed verification', () => {
    render(<VerificationEvidence reportedSuccess results={[{ ...gate, passed: false, degraded: true }]} />);
    expect(screen.getByRole('alert')).toHaveTextContent('also contains failed verification checks');
    expect(screen.getByText('One file persisted').closest('li')).toHaveClass('err');
    expect(screen.getByText('Failed')).toBeInTheDocument();
  });

  it('keeps unknown evidence distinct from a measured failure', () => {
    render(<VerificationEvidence results={[{ ...gate, passed: false, evidenceState: 'unknown' }]} />);
    expect(screen.getByText('Unknown')).toBeInTheDocument();
    expect(screen.getByText('0 failed')).toBeInTheDocument();
    expect(screen.getByText('1 unknown')).toBeInTheDocument();
  });
});
