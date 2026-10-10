use super::domain::{DomainError, Result};
use super::runtime_monitor::MonitorEvent;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RepairAction {
    pub description: String,
    pub applied: bool,
}

pub trait SelfHealService {
    fn attempt_repair(&self, events: &[MonitorEvent]) -> Result<Vec<RepairAction>, DomainError>;
}

pub struct DefaultSelfHealService;

impl DefaultSelfHealService {
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

    fn repair_for(label: &str) -> &'static str {
        match label {
            "timeout" => "Retry with increased timeout and backoff.",
            "memory" => "Clear runtime caches and retry once.",
            "provider" => "Retry on alternate provider after cooldown.",
            "filesystem" => "Retry workspace path resolution with fallback root.",
            "runtime" => "Restart runtime worker and replay last task.",
            _ => "Inspect monitor event and apply targeted recovery.",
        }
    }
}

impl SelfHealService for DefaultSelfHealService {
    fn attempt_repair(&self, events: &[MonitorEvent]) -> Result<Vec<RepairAction>, DomainError> {
        if events.is_empty() {
            return Ok(vec![]);
        }

        let labels = Self::classify(events);
        let actions = labels
            .into_iter()
            .map(|label| RepairAction {
                description: Self::repair_for(label).into(),
                applied: false,
            })
            .collect();

        Ok(actions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_noop_actions_for_empty_events() {
        let service = DefaultSelfHealService;
        let result = service.attempt_repair(&[]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn returns_noop_actions_for_events() {
        let service = DefaultSelfHealService;
        let events = vec![MonitorEvent {
            ts: "2026-01-01T00:00:00Z".into(),
            severity: "error".into(),
            message: "simulated failure".into(),
        }];
        let result = service.attempt_repair(&events).unwrap();
        assert_eq!(result.len(), 1);
        assert!(!result[0].applied);
    }

    #[test]
    fn classifies_timeout_events() {
        let service = DefaultSelfHealService;
        let events = vec![MonitorEvent {
            ts: "2026-01-01T00:00:00Z".into(),
            severity: "error".into(),
            message: "request timed out after 30s".into(),
        }];
        let result = service.attempt_repair(&events).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].description.contains("timeout"));
    }

    #[test]
    fn classifies_provider_events() {
        let service = DefaultSelfHealService;
        let events = vec![MonitorEvent {
            ts: "2026-01-01T00:00:00Z".into(),
            severity: "error".into(),
            message: "gateway 429 from provider".into(),
        }];
        let result = service.attempt_repair(&events).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].description.contains("alternate provider"));
    }
}
