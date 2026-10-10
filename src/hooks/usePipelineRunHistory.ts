import { listPipelineRuns } from '../lib/ipc';
import { useCallback } from 'react';

export type { PipelineRunHistoryRecord } from '../lib/ipc';

export function usePipelineRunHistory() {
  const load = useCallback(async (limit = 20) => {
    return listPipelineRuns(limit);
  }, []);

  return { load };
}
