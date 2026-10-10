import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import RunHistoryView from './RunHistoryView';
import { getPipelineRunResult, listPipelineRuns, type PipelineRun } from '../lib/ipc';

vi.mock('../lib/ipc', async (importOriginal) => ({
  ...await importOriginal<typeof import('../lib/ipc')>(),
  listPipelineRuns: vi.fn(),
  getPipelineRunResult: vi.fn(),
}));

const record = { id: 'run-1', goal: 'build', status: 'completed', step_count: 0, repair_count: 0, success: true };
const result: PipelineRun = { plan: { id: 'p-1', goal: 'build', status: 'completed', steps: [] }, repair_actions: [], events: [] };

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(listPipelineRuns).mockResolvedValue([record]);
  vi.mocked(getPipelineRunResult).mockResolvedValue(result);
});

describe('Run history recovery', () => {
  it('loads once rather than refetching on every render', async () => {
    render(<RunHistoryView />);
    await screen.findByRole('button', { name: 'Inspect' });
    fireEvent.change(screen.getByLabelText('Search run history'), { target: { value: 'build' } });
    expect(listPipelineRuns).toHaveBeenCalledTimes(1);
  });

  it('shows detail loading immediately and keeps errors recoverable', async () => {
    let reject!: (error: Error) => void;
    vi.mocked(getPipelineRunResult).mockReturnValueOnce(new Promise<PipelineRun>((_, failure) => { reject = failure; }));
    render(<RunHistoryView />);
    fireEvent.click(await screen.findByRole('button', { name: 'Inspect' }));
    expect(screen.getByText('Loading run result…')).toBeInTheDocument();
    await act(async () => { reject(new Error('Runtime disconnected')); });
    expect(screen.getByRole('alert')).toHaveTextContent('Runtime disconnected');
    fireEvent.click(screen.getByRole('button', { name: 'Retry loading result' }));
    await waitFor(() => expect(getPipelineRunResult).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.queryByRole('alert')).not.toBeInTheDocument());
  });

  it('does not reopen a detail panel when a closed request completes', async () => {
    let resolve!: (value: PipelineRun) => void;
    vi.mocked(getPipelineRunResult).mockReturnValueOnce(new Promise<PipelineRun>((success) => { resolve = success; }));
    render(<RunHistoryView />);
    fireEvent.click(await screen.findByRole('button', { name: 'Inspect' }));
    fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    await act(async () => { resolve(result); });
    expect(screen.queryByText('Run detail')).not.toBeInTheDocument();
  });

  it('does not disguise a history fetch failure as an empty history', async () => {
    vi.mocked(listPipelineRuns).mockRejectedValueOnce(new Error('History unavailable'));
    render(<RunHistoryView />);
    expect(await screen.findByRole('alert')).toHaveTextContent('History unavailable');
    expect(screen.queryByText('No pipeline runs recorded yet')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Retry history' }));
    expect(await screen.findByRole('button', { name: 'Inspect' })).toBeInTheDocument();
  });
});
