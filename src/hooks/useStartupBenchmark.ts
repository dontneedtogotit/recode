import { runStartupBenchmark as runTypedStartupBenchmark, type StartupBenchmarkResult } from '../lib/ipc';

export type { StartupBenchmarkResult } from '../lib/ipc';

export async function runStartupBenchmark(): Promise<StartupBenchmarkResult> {
  return runTypedStartupBenchmark();
}
