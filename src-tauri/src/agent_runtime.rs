use super::domain::{DomainError, EntityId, Result};
use super::tauri_commands::capture_diagnostics;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentTask {
    pub id: EntityId,
    pub agent_id: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentResult {
    pub task_id: EntityId,
    pub success: bool,
    pub output: serde_json::Value,
    pub error: Option<String>,
}

pub trait AgentRuntime {
    fn execute(&self, task: AgentTask) -> Result<AgentResult, DomainError>;
}

pub struct LocalAgentRuntime;

impl AgentRuntime for LocalAgentRuntime {
    fn execute(&self, task: AgentTask) -> Result<AgentResult, DomainError> {
        if task.payload.get("fail").and_then(|value| value.as_bool()) == Some(true) {
            let _ = capture_diagnostics(super::tauri_commands::CaptureDiagnosticsInput {
                kind: "runtime".into(),
                message: "agent task failed".into(),
                detail: Some("injected failure".into()),
            });
            return Err(DomainError::internal("agent task failed"));
        }

        Ok(AgentResult {
            task_id: task.id,
            success: true,
            output: serde_json::json!({"agent": task.agent_id, "status": "completed"}),
            error: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_monitor::RuntimeMonitorService;

    #[test]
    fn executes_agent_task_successfully() {
        let runtime = LocalAgentRuntime;
        let task = AgentTask {
            id: EntityId::new(),
            agent_id: "test-agent".into(),
            payload: serde_json::json!({"input": "hello"}),
        };

        let result = runtime.execute(task).unwrap();
        assert!(result.success);
        assert_eq!(result.output["agent"], "test-agent");
    }

    #[test]
    fn agent_result_contains_task_id() {
        let runtime = LocalAgentRuntime;
        let task_id = EntityId::new();
        let task = AgentTask {
            id: task_id.clone(),
            agent_id: "test-agent".into(),
            payload: serde_json::json!({}),
        };

        let result = runtime.execute(task).unwrap();
        assert_eq!(result.task_id.as_str(), task_id.as_str());
    }

    #[test]
    fn agent_runtime_failure_captures_diagnostics() {
        crate::runtime_monitor::service().clear_events();
        let runtime = LocalAgentRuntime;
        let task = AgentTask {
            id: EntityId::new(),
            agent_id: "test-agent".into(),
            payload: serde_json::json!({"fail": true}),
        };

        let result = runtime.execute(task);
        assert!(result.is_err());
        let events = crate::runtime_monitor::service().recent_events();
        assert!(
            events.iter().any(|event| event.message.contains("agent task failed")),
            "expected agent failure diagnostics, got: {events:?}"
        );
    }
}
