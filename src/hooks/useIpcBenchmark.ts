import { runIpcBenchmark as runTypedIpcBenchmark, type IpcBenchmarkResult } from '../lib/ipc';

export type { IpcBenchmarkResult } from '../lib/ipc';

export async function runIpcBenchmark(iterations = 100): Promise<IpcBenchmarkResult> {
  try {
    return await runTypedIpcBenchmark(iterations);
  } catch {
    return {
      iterations,
      total_ms: 0,
      min_ms: 0,
      max_ms: 0,
      avg_ms: 0,
      p95_ms: 0,
    };
  }
}
