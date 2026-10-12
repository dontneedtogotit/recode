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

/// System prompts giving each agent role a distinct personality and purpose.
/// These are used when routing agent tasks through the AI gateway so agents
/// are utilised to their full potential as specialised AI assistants.
fn system_prompt_for(agent_id: &str) -> &'static str {
    match agent_id {
        "planner" => "You are the Planner agent. Given a goal, break it down into clear, actionable steps. Return a JSON object with a 'plan' array of steps, each with 'id', 'description', and 'status' (pending). Be concrete and specific.",
        "reviewer" => "You are the Reviewer agent. Review the provided plan or code for correctness, completeness, and quality. Return a JSON object with 'approved' (boolean), 'issues' (array of strings), and 'suggestions' (array of strings). Be thorough and constructive.",
        "executor" => "You are the Executor agent. Execute the given task or step and produce the output. Return a JSON object with 'status' (completed/failed), 'output' (the result), and 'notes' (any relevant context). Be precise and produce working artifacts.",
        _ => "You are a helpful AI agent. Process the given task and return a JSON object describing the result.",
    }
}

/// Extract the assistant message content from an OpenAI-compatible chat
/// completion response. Returns the text content if present, otherwise tries
/// to extract a structured JSON object from the response body.
fn extract_chat_content(body: &serde_json::Value) -> Option<serde_json::Value> {
    if let Some(choices) = body.get("choices").and_then(|v| v.as_array()) {
        if let Some(first) = choices.first() {
            if let Some(message) = first.get("message") {
                if let Some(content) = message.get("content").and_then(|v| v.as_str()) {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(content) {
                        return Some(parsed);
                    }
                    return Some(serde_json::json!({"content": content}));
                }
            }
        }
    }
    None
}

pub struct LocalAgentRuntime;

impl AgentRuntime for LocalAgentRuntime {
    fn execute(&self, task: AgentTask) -> Result<AgentResult, DomainError> {
        if task.payload.get("fail").and_then(|v| v.as_bool()) == Some(true) {
            let _ = capture_diagnostics(super::tauri_commands::CaptureDiagnosticsInput {
                kind: "runtime".into(),
                message: "agent task failed".into(),
                detail: Some("injected failure".into()),
            });
            return Err(DomainError::internal("agent task failed"));
        }

        // Attempt to route through the AI gateway if configured.
        let gateway = crate::gateway::default_gateway();
        if gateway.is_configured() {
            let user_input = task.payload.get("input")
                .and_then(|v| v.as_str())
                .or_else(|| task.payload.get("goal").and_then(|v| v.as_str()))
                .unwrap_or("Process this task and return a JSON result.");

            let messages = serde_json::json!([
                {"role": "system", "content": system_prompt_for(&task.agent_id)},
                {"role": "user", "content": user_input},
            ]);

            let request = crate::gateway::GatewayRequest {
                provider: "openai".into(),
                payload: messages,
                task_type: Some("chat".into()),
            };

            match gateway.send(request) {
                Ok(response) => {
                    let output = extract_chat_content(&response.body)
                        .unwrap_or_else(|| serde_json::json!({"status": "completed", "gateway_status": response.status}));
                    return Ok(AgentResult {
                        task_id: task.id,
                        success: true,
                        output,
                        error: None,
                    });
                }
                Err(err) => {
                    let _ = capture_diagnostics(super::tauri_commands::CaptureDiagnosticsInput {
                        kind: "gateway".into(),
                        message: format!("agent gateway call failed: {err}"),
                        detail: Some(format!("agent_id: {}", task.agent_id)),
                    });
                    return Err(err);
                }
            }
        }

        // Fallback stub when no gateway is available (e.g., preview mode).
        Ok(AgentResult {
            task_id: task.id,
            success: true,
            output: serde_json::json!({"agent": task.agent_id, "status": "completed", "mode": "offline"}),
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
        // In offline mode we get the stub response.
        assert_eq!(result.output["agent"], "test-agent");
        assert_eq!(result.output["mode"], "offline");
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
        assert!(events.iter().any(|e| e.message.contains("agent task failed")));
    }

    #[test]
    fn system_prompts_are_specialized() {
        assert!(system_prompt_for("planner").contains("Planner"));
        assert!(system_prompt_for("reviewer").contains("Reviewer"));
        assert!(system_prompt_for("executor").contains("Executor"));
        assert!(system_prompt_for("unknown").contains("helpful AI agent"));
    }

    #[test]
    fn extract_chat_content_parses_standard_format() {
        let body = serde_json::json!({
            "choices": [{
                "message": { "role": "assistant", "content": "{\"status\": \"done\"}" }
            }]
        });
        let result = extract_chat_content(&body);
        assert_eq!(result.unwrap()["status"], "done");
    }

    #[test]
    fn extract_chat_content_handles_plain_text() {
        let body = serde_json::json!({
            "choices": [{
                "message": { "role": "assistant", "content": "Here is the result" }
            }]
        });
        let result = extract_chat_content(&body);
        assert_eq!(result.unwrap()["content"], "Here is the result");
    }
}
