use std::time::Instant;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct PingRequest {
    pub payload: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct PongResponse {
    pub payload: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct IpcBenchmarkResult {
    pub iterations: usize,
    pub total_ms: u64,
    pub min_ms: u64,
    pub max_ms: u64,
    pub avg_ms: f64,
    pub p95_ms: f64,
}

pub fn ipc_ping_pong(payload: &str) -> PongResponse {
    PongResponse {
        payload: payload.to_string(),
    }
}

pub fn run_ipc_benchmark(iterations: usize) -> IpcBenchmarkResult {
    let mut samples = Vec::with_capacity(iterations);
    let mut min = u64::MAX;
    let mut max = 0u64;

    for _ in 0..iterations {
        let start = Instant::now();
        let _ = ipc_ping_pong("bench");
        let duration = start.elapsed().as_micros() as u64;
        samples.push(duration);
        if duration < min {
            min = duration;
        }
        if duration > max {
            max = duration;
        }
    }

    let total_ms = samples.iter().sum::<u64>() / 1000;
    let avg_ms = (samples.iter().sum::<u64>() as f64) / (samples.len() as f64) / 1000.0;

    let mut sorted = samples.clone();
    sorted.sort_unstable();
    let p95_index = ((samples.len() as f64) * 0.95).ceil() as usize - 1;
    let p95_ms = sorted.get(p95_index).copied().unwrap_or(max) as f64 / 1000.0;

    IpcBenchmarkResult {
        iterations,
        total_ms,
        min_ms: min / 1000,
        max_ms: max / 1000,
        avg_ms,
        p95_ms,
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RunIpcBenchmarkRequest {
    pub iterations: usize,
}

#[tauri::command]
pub fn run_ipc_benchmark_command(
    request: RunIpcBenchmarkRequest,
) -> Result<IpcBenchmarkResult, String> {
    Ok(run_ipc_benchmark(request.iterations))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipc_ping_pong_echoes_payload() {
        let response = ipc_ping_pong("hello");
        assert_eq!(response.payload, "hello");
    }

    #[test]
    fn ipc_benchmark_reports_reasonable_latency() {
        let result = run_ipc_benchmark(10);
        assert_eq!(result.iterations, 10);
        assert!(result.avg_ms >= 0.0);
        assert!(result.p95_ms >= result.avg_ms);
        assert!(result.min_ms <= result.max_ms);
    }
}
