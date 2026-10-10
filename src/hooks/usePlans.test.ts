import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { usePlans } from './usePlans';
import { listPlans, savePlan } from '../lib/ipc';

vi.mock('../lib/ipc', () => ({ listPlans: vi.fn(), savePlan: vi.fn() }));
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(listPlans).mockResolvedValue([]);
  vi.mocked(savePlan).mockResolvedValue(undefined);
});

describe('saved-plan lifecycle', () => {
  it('exposes a failed refresh instead of pretending the library is empty', async () => {
    vi.mocked(listPlans).mockRejectedValueOnce(new Error('Runtime unavailable'));
    const { result } = renderHook(() => usePlans());
    await waitFor(() => expect(result.current.loadError).toBe('Runtime unavailable'));
    await act(async () => { await result.current.refreshPlans(); });
    expect(result.current.loadError).toBeNull();
  });

  it('reuses the upsert ID when retrying a failed save', async () => {
    vi.mocked(savePlan).mockRejectedValueOnce(new Error('Response lost'));
    const { result } = renderHook(() => usePlans({ autoRefresh: false }));
    await act(async () => { expect(await result.current.savePlan('build a page')).toBe(false); });
    expect(result.current.plans).toEqual([]);
    expect(result.current.saveError).toBe('Response lost');
    const firstRecord = vi.mocked(savePlan).mock.calls[0][0];
    await act(async () => { expect(await result.current.savePlan('build a page')).toBe(true); });
    expect(savePlan).toHaveBeenLastCalledWith(firstRecord);
    expect(result.current.plans).toEqual([firstRecord]);
    expect(result.current.saveError).toBeNull();
  });

  it('does not let an older refresh erase a confirmed save', async () => {
    let resolve!: (rows: Awaited<ReturnType<typeof listPlans>>) => void;
    vi.mocked(listPlans).mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    const { result } = renderHook(() => usePlans());
    await act(async () => { await result.current.savePlan('new goal'); });
    await act(async () => { resolve([]); });
    expect(result.current.plans).toHaveLength(1);
    expect(result.current.plans[0].goal).toBe('new goal');
    expect(result.current.loading).toBe(false);
  });

  it('rejects duplicate submissions before React renders the saving state', async () => {
    let resolve!: () => void;
    vi.mocked(savePlan).mockReturnValueOnce(new Promise<void>((done) => { resolve = done; }));
    const { result } = renderHook(() => usePlans({ autoRefresh: false }));
    let first!: Promise<boolean>;
    await act(async () => {
      first = result.current.savePlan('one goal');
      expect(await result.current.savePlan('second goal')).toBe(false);
    });
    expect(savePlan).toHaveBeenCalledTimes(1);
    await act(async () => { resolve(); await first; });
    expect(result.current.saving).toBe(false);
  });
});
