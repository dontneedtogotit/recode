use std::collections::HashMap;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolParameterType {
    String,
    Number,
    Boolean,
    Array,
    Object,
}

impl std::fmt::Display for ToolParameterType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolParameterType::String => write!(f, "string"),
            ToolParameterType::Number => write!(f, "number"),
            ToolParameterType::Boolean => write!(f, "boolean"),
            ToolParameterType::Array => write!(f, "array"),
            ToolParameterType::Object => write!(f, "object"),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    Code,
    File,
    Search,
    Analysis,
    Network,
    System,
    Memory,
    Ai,
}

impl std::fmt::Display for ToolCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolCategory::Code => write!(f, "code"),
            ToolCategory::File => write!(f, "file"),
            ToolCategory::Search => write!(f, "search"),
            ToolCategory::Analysis => write!(f, "analysis"),
            ToolCategory::Network => write!(f, "network"),
            ToolCategory::System => write!(f, "system"),
            ToolCategory::Memory => write!(f, "memory"),
            ToolCategory::Ai => write!(f, "ai"),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DynamicToolParameter {
    pub name: String,
    #[serde(rename = "type")]
    pub parameter_type: ToolParameterType,
    pub description: String,
    pub required: bool,
    pub default_value: Option<serde_json::Value>,
}

impl DynamicToolParameter {
    pub fn new(name: impl Into<String>, parameter_type: ToolParameterType) -> Self {
        Self {
            name: name.into(),
            parameter_type,
            description: String::new(),
            required: false,
            default_value: None,
        }
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    pub fn default_value(mut self, default_value: impl Into<serde_json::Value>) -> Self {
        self.default_value = Some(default_value.into());
        self
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct DynamicToolSnapshot {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: ToolCategory,
    pub created_by: String,
    pub created_at: i64,
    pub permissions: Vec<String>,
    pub version: Option<u32>,
    pub usage_count: Option<u64>,
    pub success_rate: Option<f64>,
    pub parameters: Vec<DynamicToolParameter>,
    pub source_code: Option<String>,
}

impl DynamicToolSnapshot {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        category: ToolCategory,
        created_by: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            category,
            created_by: created_by.into(),
            created_at: chrono_now(),
            permissions: Vec::new(),
            version: None,
            usage_count: None,
            success_rate: None,
            parameters: Vec::new(),
            source_code: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_permissions(mut self, permissions: Vec<String>) -> Self {
        self.permissions = permissions;
        self
    }

    pub fn with_parameters(mut self, parameters: Vec<DynamicToolParameter>) -> Self {
        self.parameters = parameters;
        self
    }

    pub fn with_source_code(mut self, source_code: impl Into<String>) -> Self {
        self.source_code = Some(source_code.into());
        self
    }

    pub fn record_usage(&mut self, success: bool) {
        let usage_count = self.usage_count.unwrap_or(0) + 1;
        self.usage_count = Some(usage_count);

        if let Some(success_rate) = self.success_rate {
            let new_success_rate =
                (success_rate * (usage_count - 1) as f64 + if success { 1.0 } else { 0.0 })
                    / usage_count as f64;
            self.success_rate = Some(new_success_rate);
        } else if success {
            self.success_rate = Some(1.0);
        } else {
            self.success_rate = Some(0.0);
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateDynamicToolRequest {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: ToolCategory,
    pub parameters: Vec<DynamicToolParameter>,
    pub source_code: String,
    pub permissions: Vec<String>,
    pub created_by: String,
}

impl CreateDynamicToolRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("tool id is required".into());
        }

        if self.name.trim().is_empty() {
            return Err("tool name is required".into());
        }

        if self.source_code.trim().is_empty() {
            return Err("source code is required".into());
        }

        if self.created_by.trim().is_empty() {
            return Err("created_by is required".into());
        }

        let mut seen_names = HashMap::new();
        for parameter in &self.parameters {
            if parameter.name.trim().is_empty() {
                return Err(format!("parameter name is required for tool {}", self.id));
            }

            if seen_names.contains_key(parameter.name.as_str()) {
                return Err(format!(
                    "duplicate parameter name `{}` in tool {}",
                    parameter.name, self.id
                ));
            }
            seen_names.insert(parameter.name.clone(), true);
        }

        Ok(())
    }
}

pub struct DynamicToolRegistry {
    tools: HashMap<String, DynamicToolSnapshot>,
}

impl DynamicToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: DynamicToolSnapshot) -> Result<(), String> {
        if self.tools.contains_key(&tool.id) {
            return Err(format!("tool `{}` already exists", tool.id));
        }

        self.tools.insert(tool.id.clone(), tool);
        Ok(())
    }

    pub fn get(&self, tool_id: &str) -> Option<&DynamicToolSnapshot> {
        self.tools.get(tool_id)
    }

    pub fn get_mut(&mut self, tool_id: &str) -> Option<&mut DynamicToolSnapshot> {
        self.tools.get_mut(tool_id)
    }

    pub fn remove(&mut self, tool_id: &str) -> Option<DynamicToolSnapshot> {
        self.tools.remove(tool_id)
    }

    pub fn list(&self) -> Vec<&DynamicToolSnapshot> {
        self.tools.values().collect()
    }

    pub fn list_by_category(&self, category: ToolCategory) -> Vec<&DynamicToolSnapshot> {
        self.tools
            .values()
            .filter(|tool| tool.category == category)
            .collect()
    }

    pub fn list_by_creator(&self, creator: &str) -> Vec<&DynamicToolSnapshot> {
        self.tools
            .values()
            .filter(|tool| tool.created_by == creator)
            .collect()
    }

    pub fn record_usage(&mut self, tool_id: &str, success: bool) -> Result<(), String> {
        let tool = self
            .tools
            .get_mut(tool_id)
            .ok_or_else(|| format!("tool `{}` not found", tool_id))?;
        tool.record_usage(success);
        Ok(())
    }

    pub fn count(&self) -> usize {
        self.tools.len()
    }
}

impl Default for DynamicToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn chrono_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tool() -> DynamicToolSnapshot {
        DynamicToolSnapshot::new(
            "tool-1",
            "Code Formatter",
            ToolCategory::Code,
            "agent-1",
        )
        .with_description("Formats code files")
        .with_permissions(vec!["read".into(), "write".into()])
        .with_parameters(vec![
            DynamicToolParameter::new("file_path", ToolParameterType::String)
                .required(true),
            DynamicToolParameter::new("style", ToolParameterType::String)
                .default_value("default"),
        ])
    }

    #[test]
    fn register_and_retrieve_tool() {
        let mut registry = DynamicToolRegistry::new();
        let tool = sample_tool();
        registry.register(tool.clone()).expect("register tool");

        assert_eq!(registry.count(), 1);
        assert_eq!(registry.get("tool-1"), Some(&tool));
    }

    #[test]
    fn register_rejects_duplicate_ids() {
        let mut registry = DynamicToolRegistry::new();
        registry
            .register(sample_tool())
            .expect("first registration");
        let result = registry.register(sample_tool());

        assert!(result.is_err());
        assert_eq!(registry.count(), 1);
    }

    #[test]
    fn remove_tool() {
        let mut registry = DynamicToolRegistry::new();
        registry.register(sample_tool()).expect("register tool");

        let removed = registry.remove("tool-1");
        assert!(removed.is_some());
        assert_eq!(registry.count(), 0);
        assert!(registry.get("tool-1").is_none());
    }

    #[test]
    fn list_filters_by_category() {
        let mut registry = DynamicToolRegistry::new();
        registry
            .register(
                DynamicToolSnapshot::new("t1", "Formatter", ToolCategory::Code, "a1")
                    .with_description("formats code"),
            )
            .expect("register code tool");
        registry
            .register(
                DynamicToolSnapshot::new("t2", "Linter", ToolCategory::Analysis, "a1")
                    .with_description("lints code"),
            )
            .expect("register analysis tool");

        let code_tools = registry.list_by_category(ToolCategory::Code);
        assert_eq!(code_tools.len(), 1);
        assert_eq!(code_tools[0].id, "t1");
    }

    #[test]
    fn record_usage_updates_counts() {
        let mut registry = DynamicToolRegistry::new();
        registry.register(sample_tool()).expect("register tool");

        registry.record_usage("tool-1", true).expect("record success");
        let tool = registry.get("tool-1").expect("tool exists");
        assert_eq!(tool.usage_count, Some(1));
        assert_eq!(tool.success_rate, Some(1.0));

        registry.record_usage("tool-1", false).expect("record failure");
        let tool = registry.get("tool-1").expect("tool exists");
        assert_eq!(tool.usage_count, Some(2));
        assert_eq!(tool.success_rate, Some(0.5));
    }

    #[test]
    fn create_request_validates_required_fields() {
        let request = CreateDynamicToolRequest {
            id: "".into(),
            name: "".into(),
            description: String::new(),
            category: ToolCategory::Code,
            parameters: Vec::new(),
            source_code: "".into(),
            permissions: Vec::new(),
            created_by: "".into(),
        };

        assert!(request.validate().is_err());
    }

    #[test]
    fn create_request_accepts_valid_tool() {
        let request = CreateDynamicToolRequest {
            id: "tool-1".into(),
            name: "Formatter".into(),
            description: "formats code".into(),
            category: ToolCategory::Code,
            parameters: vec![
                DynamicToolParameter::new("path", ToolParameterType::String).required(true),
            ],
            source_code: "fn main() {}".into(),
            permissions: vec!["read".into()],
            created_by: "agent-1".into(),
        };

        assert!(request.validate().is_ok());
    }

    #[test]
    fn create_request_rejects_duplicate_parameter_names() {
        let request = CreateDynamicToolRequest {
            id: "tool-1".into(),
            name: "Formatter".into(),
            description: "formats code".into(),
            category: ToolCategory::Code,
            parameters: vec![
                DynamicToolParameter::new("path", ToolParameterType::String).required(true),
                DynamicToolParameter::new("path", ToolParameterType::String).required(true),
            ],
            source_code: "fn main() {}".into(),
            permissions: vec!["read".into()],
            created_by: "agent-1".into(),
        };

        assert!(request.validate().is_err());
    }
}
