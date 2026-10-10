use super::agent_runtime::{AgentRuntime, AgentTask, LocalAgentRuntime};
use super::domain::{DomainError, EntityId, Result};
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentDescriptor {
    pub id: String,
    pub name: String,
    pub role: String,
    pub description: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentTaskInput {
    pub agent_id: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentTaskResult {
    pub task_id: String,
    pub agent_id: String,
    pub success: bool,
    pub output: serde_json::Value,
    pub error: Option<String>,
}

pub trait AgentRegistry {
    fn register(&mut self, descriptor: AgentDescriptor) -> Result<()>;
    fn list(&self) -> Vec<&AgentDescriptor>;
    fn get(&self, agent_id: &str) -> Option<&AgentDescriptor>;
    fn execute(&self, input: AgentTaskInput) -> Result<AgentTaskResult>;
}

#[derive(Debug, Default)]
pub struct InMemoryAgentRegistry {
    agents: BTreeMap<String, AgentDescriptor>,
}

impl InMemoryAgentRegistry {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AgentRegistry for InMemoryAgentRegistry {
    fn register(&mut self, descriptor: AgentDescriptor) -> Result<()> {
        if self.agents.contains_key(&descriptor.id) {
            return Err(DomainError::internal(format!(
                "agent already registered: {}",
                descriptor.id
            )));
        }
        self.agents.insert(descriptor.id.clone(), descriptor);
        Ok(())
    }

    fn list(&self) -> Vec<&AgentDescriptor> {
        self.agents.values().collect()
    }

    fn get(&self, agent_id: &str) -> Option<&AgentDescriptor> {
        self.agents.get(agent_id)
    }

    fn execute(&self, input: AgentTaskInput) -> Result<AgentTaskResult> {
        let descriptor = self.get(&input.agent_id).ok_or_else(|| {
            DomainError::internal(format!("agent not found: {}", input.agent_id))
        })?;

        let runtime = LocalAgentRuntime;
        let task = AgentTask {
            id: EntityId::new(),
            agent_id: descriptor.id.clone(),
            payload: input.payload,
        };

        let result = runtime.execute(task)?;
        Ok(AgentTaskResult {
            task_id: result.task_id.as_str().into(),
            agent_id: descriptor.id.clone(),
            success: result.success,
            output: result.output,
            error: result.error,
        })
    }
}

static AGENT_REGISTRY: OnceLock<Mutex<InMemoryAgentRegistry>> = OnceLock::new();

pub fn registry() -> &'static Mutex<InMemoryAgentRegistry> {
    AGENT_REGISTRY.get_or_init(|| {
        let mut registry = InMemoryAgentRegistry::new();
        registry
            .register(AgentDescriptor {
                id: "planner".into(),
                name: "Planner".into(),
                role: "planning".into(),
                description: "Creates plans from goals".into(),
                capabilities: vec!["planning".into()],
            })
            .expect("default planner agent registration");

        registry
            .register(AgentDescriptor {
                id: "reviewer".into(),
                name: "Reviewer".into(),
                role: "quality".into(),
                description: "Reviews plans and outputs".into(),
                capabilities: vec!["review".into()],
            })
            .expect("default reviewer agent registration");

        registry
            .register(AgentDescriptor {
                id: "executor".into(),
                name: "Executor".into(),
                role: "execution".into(),
                description: "Executes plans and tasks".into(),
                capabilities: vec!["execution".into()],
            })
            .expect("default executor agent registration");

        Mutex::new(registry)
    })
}

#[tauri::command]
pub fn list_agents() -> Vec<AgentDescriptor> {
    registry()
        .lock()
        .expect("lock agent registry")
        .list()
        .into_iter()
        .cloned()
        .collect()
}

#[tauri::command]
pub fn register_agent(descriptor: AgentDescriptor) -> Result<(), String> {
    registry()
        .lock()
        .expect("lock agent registry")
        .register(descriptor)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn execute_agent_task(input: AgentTaskInput) -> Result<AgentTaskResult, String> {
    registry()
        .lock()
        .expect("lock agent registry")
        .execute(input)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_has_agents() {
        let registry = registry().lock().expect("lock agent registry");
        let agents = registry.list();
        assert!(!agents.is_empty());
        assert!(agents.iter().any(|agent| agent.id == "planner"));
    }

    #[test]
    fn registry_lookup_and_execution() {
        let registry = registry().lock().expect("lock agent registry");
        let result = registry
            .execute(AgentTaskInput {
                agent_id: "planner".into(),
                payload: serde_json::json!({"input": "hello"}),
            })
            .unwrap();

        assert!(result.success);
        assert_eq!(result.agent_id, "planner");
        assert!(!result.task_id.is_empty());
    }

    #[test]
    fn registry_returns_error_for_unknown_agent() {
        let registry = registry().lock().expect("lock agent registry");
        let error = registry
            .execute(AgentTaskInput {
                agent_id: "missing".into(),
                payload: serde_json::json!({}),
            })
            .unwrap_err();
        assert!(error.to_string().contains("agent not found"));
    }
}
