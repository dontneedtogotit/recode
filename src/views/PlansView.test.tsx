import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import PlansView from './PlansView';
import { listPlans, savePlan } from '../lib/ipc';

vi.mock('../lib/ipc', () => ({ listPlans: vi.fn(), savePlan: vi.fn() }));
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(listPlans).mockResolvedValue([
    { id: 'p-1', goal: 'Build account settings', status: 'pending', steps: [] },
    { id: 'p-2', goal: 'Improve search', status: 'pending', steps: [] },
  ]);
  vi.mocked(savePlan).mockResolvedValue(undefined);
});

describe('plan library', () => {
  it('filters saved goals and hands selection to review instead of execution', async () => {
    const onUsePlan = vi.fn();
    render(<PlansView runState={{ status: 'idle' }} onUsePlan={onUsePlan} />);
    await screen.findByText('Build account settings');
    fireEvent.change(screen.getByLabelText('Search saved plans'), { target: { value: 'account' } });
    expect(screen.queryByText('Improve search')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Review plan p-1' }));
    expect(onUsePlan).toHaveBeenCalledWith({ id: 'p-1', goal: 'Build account settings' });
    expect(savePlan).not.toHaveBeenCalled();
  });

  it('retains the draft on failure and clears it only after persistence succeeds', async () => {
    vi.mocked(savePlan).mockRejectedValueOnce(new Error('Disk unavailable'));
    render(<PlansView runState={{ status: 'idle' }} />);
    fireEvent.change(screen.getByLabelText('Plan goal'), { target: { value: 'keep my work' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save plan' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Disk unavailable');
    expect(screen.getByLabelText('Plan goal')).toHaveValue('keep my work');
    fireEvent.click(screen.getByRole('button', { name: 'Save plan' }));
    await waitFor(() => expect(screen.getByLabelText('Plan goal')).toHaveValue(''));
    expect(screen.getByText('Saved for review. No run started.')).toBeInTheDocument();
  });

  it('disables review while a run is active but leaves planning available', async () => {
    render(<PlansView runState={{ status: 'running', goal: 'active' }} onUsePlan={vi.fn()} />);
    expect(await screen.findByRole('button', { name: 'Review plan p-1' })).toBeDisabled();
    fireEvent.change(screen.getByLabelText('Plan goal'), { target: { value: 'future work' } });
    expect(screen.getByRole('button', { name: 'Save plan' })).toBeEnabled();
  });
});
