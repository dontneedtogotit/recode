use std::sync::mpsc;

pub trait Event {
    fn topic(&self) -> &str;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DomainEvent {
    pub topic: String,
    pub payload: serde_json::Value,
}

impl Event for DomainEvent {
    fn topic(&self) -> &str {
        &self.topic
    }
}

pub const TOPIC_PLAN_CREATED: &str = "planner.plan_created";
pub const TOPIC_PLAN_FAILED: &str = "pipeline.plan_failed";
pub const TOPIC_STEP_STARTED: &str = "planner.step_started";
pub const TOPIC_STEP_COMPLETED: &str = "pipeline.step_completed";
pub const TOPIC_PLAN_COMPLETED: &str = "pipeline.plan_completed";
/// Single terminal event for a user-cancelled run (ROADMAP P0 item 5): the
/// UI observes exactly one cancellation signal on the event stream.
pub const TOPIC_PIPELINE_CANCELLED: &str = "pipeline.cancelled";
pub const TOPIC_RUNTIME_TASK: &str = "runtime.task";
pub const TOPIC_SELF_HEAL: &str = "self_heal.repair";
pub const TOPIC_MONITOR_EVENT: &str = "monitor.event";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineEvent {
    pub run_id: String,
    pub topic: String,
    pub severity: String,
    pub message: String,
    pub ts: u64,
}

impl PipelineEvent {
    pub fn from_domain_event(event: &DomainEvent, run_id: impl Into<String>) -> Self {
        let (severity, message) = match event.topic.as_str() {
            TOPIC_PLAN_FAILED => (
                "error".to_string(),
                event
                    .payload
                    .get("error")
                    .and_then(|value| value.as_str())
                    .unwrap_or("Pipeline failed")
                    .to_string(),
            ),
            TOPIC_PLAN_CREATED => (
                "info".to_string(),
                format!(
                    "Plan created: {}",
                    event
                        .payload
                        .get("goal")
                        .and_then(|value| value.as_str())
                        .unwrap_or("pipeline")
                ),
            ),
            TOPIC_STEP_STARTED => (
                "info".to_string(),
                format!(
                    "Step started: {}",
                    event
                        .payload
                        .get("description")
                        .and_then(|value| value.as_str())
                        .unwrap_or("step")
                ),
            ),
            TOPIC_STEP_COMPLETED => (
                "info".to_string(),
                format!(
                    "Step completed: {}",
                    event
                        .payload
                        .get("step_id")
                        .and_then(|value| value.as_str())
                        .unwrap_or("step")
                ),
            ),
            TOPIC_PLAN_COMPLETED => (
                "info".to_string(),
                format!(
                    "Plan completed: {}",
                    event
                        .payload
                        .get("plan_id")
                        .and_then(|value| value.as_str())
                        .unwrap_or("pipeline")
                ),
            ),
            TOPIC_PIPELINE_CANCELLED => (
                "warning".to_string(),
                format!(
                    "Pipeline cancelled: {}",
                    event
                        .payload
                        .get("reason")
                        .and_then(|value| value.as_str())
                        .unwrap_or("cancelled by user")
                ),
            ),
            _ => ("info".to_string(), event.topic.clone()),
        };
        Self {
            run_id: run_id.into(),
            topic: event.topic.clone(),
            severity,
            message,
            ts: now_millis(),
        }
    }
}

pub fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Clone)]
pub struct EventBus {
    sender: mpsc::Sender<DomainEvent>,
}

impl EventBus {
    pub fn new() -> (Self, mpsc::Receiver<DomainEvent>) {
        let (sender, receiver) = mpsc::channel();
        (Self { sender }, receiver)
    }

    pub fn publish(&self, event: DomainEvent) {
        let _ = self.sender.send(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publishes_and_receives_event() {
        let (bus, receiver) = EventBus::new();
        let event = DomainEvent {
            topic: "test".into(),
            payload: serde_json::json!({ "ok": true }),
        };
        bus.publish(event.clone());
        let received = receiver.recv().expect("event received");
        assert_eq!(received.topic(), "test");
        assert_eq!(received.payload, event.payload);
    }

    #[test]
    fn serializes_pipeline_event_for_frontend_contract() {
        let event = PipelineEvent {
            run_id: "run-1".into(),
            topic: TOPIC_PLAN_COMPLETED.into(),
            severity: "info".into(),
            message: "done".into(),
            ts: 42,
        };
        let value = serde_json::to_value(event).expect("serialize event");
        assert_eq!(value["runId"], "run-1");
        assert!(value.get("run_id").is_none());
    }

    #[test]
    fn converts_plan_event_to_console_payload() {
        let event = DomainEvent {
            topic: TOPIC_STEP_STARTED.into(),
            payload: serde_json::json!({ "description": "Analyze goal" }),
        };
        let converted = PipelineEvent::from_domain_event(&event, "run-1");
        assert_eq!(converted.run_id, "run-1");
        assert_eq!(converted.severity, "info");
        assert_eq!(converted.message, "Step started: Analyze goal");
        assert!(converted.ts > 0);
    }
}
