use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::agent_routing::descriptors::{AgentRoutingCatalog, RoutableTask};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentExecutionInput {
    pub agent_id: String,
    pub prompt: String,
    pub objective: Option<String>,
    pub code: Option<String>,
    pub file_path: Option<String>,
    pub language: Option<String>,
    pub focus: Option<String>,
    pub context: BTreeMap<String, serde_json::Value>,
}

impl AgentExecutionInput {
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }
}

/// Build a normalized execution input for a routed agent.
pub fn build_agent_execution_input(task: &RoutableTask, agent_id: &str) -> AgentExecutionInput {
    let catalog = AgentRoutingCatalog;
    let normalized = catalog.normalize(agent_id);
    let context = task.context.clone().unwrap_or_default();
    let objective = string_context(&context, "objective").unwrap_or_else(|| task.description.clone());
    let code = string_context(&context, "code")
        .or_else(|| extract_code_fence(&task.description))
        .unwrap_or_else(|| task.description.clone());
    let file_path = string_context(&context, "filePath")
        .or_else(|| first_string(context.get("files")))
        .unwrap_or_else(|| "inline".into());
    let language = string_context(&context, "language")
        .map(normalize_language)
        .or_else(|| infer_language(&file_path));

    let description_lower = task.description.to_lowercase();

    match normalized.as_str() {
        "planning-agent" => AgentExecutionInput {
            agent_id: normalized,
            prompt: task.description.clone(),
            objective: objective.into(),
            code: code.into(),
            file_path: file_path.into(),
            language: language.clone(),
            focus: Some(infer_planning_focus(&description_lower)),
            context,
        },
        "architecture-agent" => {
            let focus = if description_lower.contains("security")
                || description_lower.contains("auth")
                || description_lower.contains("secret")
            {
                "security"
            } else {
                "boundaries"
            };

            AgentExecutionInput {
                agent_id: normalized,
                prompt: task.description.clone(),
                objective: objective.into(),
                code: code.into(),
                file_path: file_path.into(),
                language: language.clone(),
                focus: Some(focus.into()),
                context,
            }
        }
        "code-generator" => AgentExecutionInput {
            agent_id: normalized,
            prompt: task.description.clone(),
            objective: objective.into(),
            code: code.into(),
            file_path: file_path.into(),
            language: language.clone(),
            focus: Some("implementation".into()),
            context: context
                .into_iter()
                .chain([
                    (
                        "requirements".into(),
                        serde_json::json!([
                            "Preserve project conventions and strict TypeScript.",
                            "Follow clean architecture principles.",
                            "Include comprehensive error handling.",
                            "Add JSDoc comments for public APIs."
                        ]),
                    ),
                    (
                        "securityRequirements".into(),
                        serde_json::json!(["Validate all inputs", "Sanitize outputs"]),
                    ),
                    (
                        "testRequirements".into(),
                        serde_json::json!(["Write unit tests for all public methods"]),
                    ),
                ])
                .collect(),
        },
        "test-agent" => AgentExecutionInput {
            agent_id: normalized,
            prompt: task.description.clone(),
            objective: objective.into(),
            code: code.into(),
            file_path: file_path.into(),
            language: language.clone(),
            focus: Some(if description_lower.contains("coverage") {
                "coverage-analysis"
            } else {
                "unit-test-generation"
            }.into()),
            context,
        },
        _ => AgentExecutionInput {
            agent_id: normalized,
            prompt: task.description.clone(),
            objective: objective.into(),
            code: code.into(),
            file_path: file_path.into(),
            language: language.clone(),
            focus: None,
            context,
        },
    }
}

fn string_context(context: &BTreeMap<String, serde_json::Value>, key: &str) -> Option<String> {
    context
        .get(key)
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

fn first_string(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(|value| value.as_array())
        .and_then(|array| {
            array
                .first()
                .and_then(|item| item.as_str())
                .map(|value| value.to_string())
        })
}

fn extract_code_fence(content: &str) -> Option<String> {
    let mut remainder = content;
    while let Some(start) = remainder.find("```") {
        remainder = &remainder[start + 3..];
        if let Some(end) = remainder.find("```") {
            let fence = remainder[..end].trim_start();
            if let Some(first_newline) = fence.find('\n') {
                return Some(fence[first_newline + 1..].trim().to_string());
            }
            return Some(fence.to_string());
        }
    }
    None
}

fn infer_language(path: &str) -> Option<String> {
    if path.ends_with(".tsx") {
        return Some("tsx".into());
    }
    if path.ends_with(".ts") {
        return Some("typescript".into());
    }
    if path.ends_with(".jsx") {
        return Some("jsx".into());
    }
    if path.ends_with(".js") {
        return Some("javascript".into());
    }
    if path.ends_with(".html") {
        return Some("html".into());
    }
    Some("typescript".into())
}

fn normalize_language(language: String) -> String {
    match language.as_str() {
        "ts" => "typescript",
        "js" => "javascript",
        other => other,
    }
    .into()
}

fn infer_planning_focus(description: &str) -> String {
    if description.contains("risk")
        || description.contains("quality")
        || description.contains("security")
    {
        return "risk-assessment".into();
    }
    if description.contains("architecture")
        || description.contains("design")
        || description.contains("boundary")
    {
        return "architecture".into();
    }
    if description.contains("refine")
        || description.contains("update")
        || description.contains("modify")
        || description.contains("improve")
    {
        return "plan-refinement".into();
    }
    "task-decomposition".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn build_default_input_preserves_task_description() {
        let task = RoutableTask {
            description: "Please review this inline change".into(),
            context: None,
        };

        let input = build_agent_execution_input(&task, "reviewer");

        assert_eq!(input.agent_id, "reviewer");
        assert_eq!(input.prompt, task.description);
        assert_eq!(input.objective.as_deref(), Some(task.description.as_str()));
    }

    #[test]
    fn build_planner_input_infers_focus() {
        let task = RoutableTask {
            description: "Refine the security-focused architecture plan".into(),
            context: None,
        };

        let input = build_agent_execution_input(&task, "planning-agent");

        assert_eq!(input.focus.as_deref(), Some("risk-assessment"));
        assert_eq!(input.file_path.as_deref(), Some("inline"));
    }

    #[test]
    fn build_test_agent_uses_coverage_focus_for_coverage_keywords() {
        let task = RoutableTask {
            description: "Analyze coverage for these files".into(),
            context: None,
        };

        let input = build_agent_execution_input(&task, "test-agent");

        assert_eq!(input.focus.as_deref(), Some("coverage-analysis"));
    }

    #[test]
    fn build_code_generator_injects_default_requirements() {
        let task = RoutableTask {
            description: "Implement a new service".into(),
            context: None,
        };

        let input = build_agent_execution_input(&task, "code-generator");

        assert!(input.objective.is_some());
        let requirements = input
            .context
            .get("requirements")
            .and_then(|value| value.as_array())
            .expect("default requirements should be injected");
        assert_eq!(requirements.len(), 4);
    }
}
