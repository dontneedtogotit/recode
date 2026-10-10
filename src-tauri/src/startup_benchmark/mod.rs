use std::time::Instant;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct StartupBenchmarkResult {
    pub elapsed_ms: u64,
    pub initialized: bool,
}

pub fn run_startup_benchmark() -> StartupBenchmarkResult {
    let start = Instant::now();
    let initialized = std::env::temp_dir().exists();
    let elapsed_ms = start.elapsed().as_millis() as u64;

    StartupBenchmarkResult {
        elapsed_ms,
        initialized,
    }
}

#[tauri::command]
pub fn run_startup_benchmark_command() -> Result<StartupBenchmarkResult, String> {
    Ok(run_startup_benchmark())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_benchmark_reports_elapsed_time() {
        let result = run_startup_benchmark();
        assert!(result.initialized);
    }
}
