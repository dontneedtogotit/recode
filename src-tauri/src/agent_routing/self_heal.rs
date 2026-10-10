use std::time::Duration;

use crate::runtime_monitor::MonitorEvent;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RepairAction {
    pub description: String,
    pub applied: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HealAdvice {
    pub label: String,
    pub action: RepairAction,
    pub retry: RetryAdvice,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RetryAdvice {
    pub attempts: u32,
    pub backoff: Duration,
    pub circuit_breaker: bool,
}

const DEFAULT_RETRY_ATTEMPTS: u32 = 3;
const DEFAULT_BACKOFF: Duration = Duration::from_millis(250);
const DEFAULT_CIRCUIT_BREAKER: bool = true;

pub fn recommend_repair_plan(events: &[MonitorEvent]) -> Vec<HealAdvice> {
    if events.is_empty() {
        return Vec::new();
    }

    let labels = classify(events);
    labels
        .into_iter()
        .map(|label| HealAdvice {
            label: label.to_string(),
            action: RepairAction {
                description: repair_for(&label),
                applied: false,
            },
            retry: retry_for(&label),
        })
        .collect()
}

fn classify(events: &[MonitorEvent]) -> Vec<&'static str> {
    let message = events
        .iter()
        .map(|event| event.message.as_str())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();

    let mut labels = Vec::new();
    if message.contains("timeout") || message.contains("timed out") {
        labels.push("timeout");
    }
    if message.contains("memory") || message.contains("heap") || message.contains("oom") {
        labels.push("memory");
    }
    if message.contains("provider") || message.contains("gateway") || message.contains("429") {
        labels.push("provider");
    }
    if message.contains("fs") || message.contains("file") || message.contains("permission") {
        labels.push("filesystem");
    }
    if labels.is_empty() && events.iter().any(|event| event.severity == "error") {
        labels.push("runtime");
    }
    labels
}

fn repair_for(label: &str) -> String {
    match label {
        "timeout" => "Retry with increased timeout and exponential backoff.".into(),
        "memory" => "Clear runtime caches, free unused buffers, then retry once.".into(),
        "provider" => "Cool down, then retry on alternate provider with circuit-breaker guard.".into(),
        "filesystem" => "Retry workspace path resolution with fallback root and verify permissions.".into(),
        "runtime" => "Restart runtime worker and replay last task from persisted state.".into(),
        other => format!("Inspect monitor event and apply targeted recovery for `{other}`."),
    }
}

fn retry_for(label: &str) -> RetryAdvice {
    match label {
        "timeout" => RetryAdvice {
            attempts: 4,
            backoff: Duration::from_millis(500),
            circuit_breaker: true,
        },
        "provider" => RetryAdvice {
            attempts: 3,
            backoff: Duration::from_millis(1000),
            circuit_breaker: true,
        },
        "memory" => RetryAdvice {
            attempts: 2,
            backoff: Duration::from_millis(200),
            circuit_breaker: false,
        },
        _ => RetryAdvice {
            attempts: DEFAULT_RETRY_ATTEMPTS,
            backoff: DEFAULT_BACKOFF,
            circuit_breaker: DEFAULT_CIRCUIT_BREAKER,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_no_advice_for_empty_events() {
        let advice = recommend_repair_plan(&[]);
        assert!(advice.is_empty());
    }

    #[test]
    fn returns_noop_action_for_runtime_error() {
        let events = vec![MonitorEvent {
            ts: "2026-01-01T00:00:00Z".into(),
            severity: "error".into(),
            message: "simulated failure".into(),
        }];
        let advice = recommend_repair_plan(&events);
        assert_eq!(advice.len(), 1);
        assert!(!advice[0].action.applied);
        assert_eq!(advice[0].label, "runtime");
        assert!(advice[0].action.description.contains("Restart runtime worker"));
    }

    #[test]
    fn classifies_timeout_events() {
        let events = vec![MonitorEvent {
            ts: "2026-01-01T00:00:00Z".into(),
            severity: "error".into(),
            message: "request timed out after 30s".into(),
        }];
        let advice = recommend_repair_plan(&events);
        assert_eq!(advice.len(), 1);
        assert_eq!(advice[0].label, "timeout");
        assert!(advice[0].action.description.contains("backoff"));
        assert_eq!(advice[0].retry.attempts, 4);
        assert!(advice[0].retry.circuit_breaker);
    }

    #[test]
    fn classifies_provider_events_with_circuit_breaker() {
        let events = vec![MonitorEvent {
            ts: "2026-01-01T00:00:00Z".into(),
            severity: "error".into(),
            message: "gateway 429 from provider".into(),
        }];
        let advice = recommend_repair_plan(&events);
        assert_eq!(advice.len(), 1);
        assert_eq!(advice[0].label, "provider");
        assert!(advice[0].action.description.contains("alternate provider"));
        assert_eq!(advice[0].retry.attempts, 3);
        assert_eq!(advice[0].retry.backoff, Duration::from_millis(1000));
    }

    #[test]
    fn preserves_multiple_error_labels() {
        let events = vec![
            MonitorEvent {
                ts: "2026-01-01T00:00:00Z".into(),
                severity: "error".into(),
                message: "timeout on memory gateway".into(),
            },
            MonitorEvent {
                ts: "2026-01-01T00:00:01Z".into(),
                severity: "error".into(),
                message: "fs permission denied".into(),
            },
        ];
        let advice = recommend_repair_plan(&events);
        let labels: Vec<String> = advice.iter().map(|item| item.label.clone()).collect();
        assert!(labels.contains(&"timeout".into()));
        assert!(labels.contains(&"memory".into()));
        assert!(labels.contains(&"provider".into()));
        assert!(labels.contains(&"filesystem".into()));
    }
}
