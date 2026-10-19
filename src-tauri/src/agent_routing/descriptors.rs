use std::sync::OnceLock;
use std::collections::BTreeMap;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SpecializationLevel {
    Novice,
    Competent,
    Proficient,
    Expert,
    Master,
    Generalist,
    Senior,
    Specialist,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentRoutingDescriptor {
    pub id: String,
    pub name: String,
    pub category: String,
    pub role: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub keywords: Vec<String>,
    pub specialization_level: Option<SpecializationLevel>,
    pub supervision_required: Option<bool>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoutedAgentSelection {
    pub agent_id: String,
    pub action: RoutedAgentAction,
    pub priority: RoutedAgentPriority,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RoutedAgentAction {
    Execute,
    Subscribe,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RoutedAgentPriority {
    Immediate,
    Deferred,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoutedTaskPlan {
    pub agents: Vec<RoutedAgentSelection>,
    pub execution_mode: RoutedExecutionMode,
    pub reasoning: String,
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RoutedExecutionMode {
    Sequential,
    Parallel,
    Hybrid,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoutableTask {
    pub description: String,
    pub context: Option<BTreeMap<String, serde_json::Value>>,
}

pub struct AgentRoutingCatalog;

impl AgentRoutingCatalog {
    pub fn descriptor(&self, agent_id: &str) -> Option<&'static AgentRoutingDescriptor> {
        self.all().get(agent_id)
    }

    pub fn all(&self) -> &'static BTreeMap<String, AgentRoutingDescriptor> {
        ROUTING_CATALOG.get_or_init(|| {
            let mut map = BTreeMap::new();
            for entry in DESCRIPTOR_ENTRIES {
                let descriptor = entry.descriptor();
                map.insert(descriptor.id.clone(), descriptor.clone());
                for alias in entry.aliases {
                    map.insert(alias.to_string(), descriptor.clone());
                }
            }
            map
        })
    }

    pub fn normalize(&self, agent_id: &str) -> String {
        self.all()
            .get(agent_id)
            .map(|descriptor| descriptor.id.clone())
            .unwrap_or_else(|| agent_id.to_string())
    }

    pub fn route(&self, task: &RoutableTask) -> RoutedTaskPlan {
        let requested = task
            .context
            .as_ref()
            .and_then(|context| context.get("agentId"))
            .and_then(|value| value.as_str())
            .and_then(|normalized| self.descriptor(normalized))
            .map(|descriptor| descriptor.id.clone());

        if let Some(requested_agent) = requested {
            return RoutedTaskPlan {
                agents: vec![RoutedAgentSelection {
                    agent_id: requested_agent.clone(),
                    action: RoutedAgentAction::Execute,
                    priority: RoutedAgentPriority::Immediate,
                    depends_on: Vec::new(),
                }],
                execution_mode: RoutedExecutionMode::Sequential,
                reasoning: format!("Requested agent {requested_agent} was selected directly."),
                confidence: Some(1.0),
            };
        }

        let description = task.description.to_lowercase();
        let mut scored: Vec<(&AgentRoutingDescriptor, usize)> = self
            .all()
            .values()
            .map(|descriptor| {
                let matches = descriptor
                    .keywords
                    .iter()
                    .filter(|keyword| description.contains(*keyword))
                    .count();
                (descriptor, matches)
            })
            .filter(|(_, matches)| *matches > 0)
            .collect();

        scored.sort_by(|left, right| right.1.cmp(&left.1));
        let ranked: Vec<&AgentRoutingDescriptor> = scored.into_iter().take(5).map(|tuple| tuple.0).collect();

        let mut selected: Vec<&AgentRoutingDescriptor> = if ranked.is_empty() {
            match self.descriptor("agentic-prompt-enhancer") {
                Some(fallback) => vec![fallback],
                // Registry misconfiguration must not panic an IPC path; route to
                // whatever descriptors are registered, or an explicit empty plan.
                None => self.all().values().take(1).collect(),
            }
        } else {
            ranked
        };

        if is_strategic(&description) {
            ensure_agent(self, &mut selected, "planning-agent");
            ensure_agent(self, &mut selected, "architecture-agent");
            if is_implementation_keyword(&description) {
                ensure_agent(self, &mut selected, "code-generator");
            }
            ensure_agent(self, &mut selected, "test-agent");
        }

        let ordered_agents = order_agents(selected);
        let max_agents = ordered_agents.iter().take(6).cloned().collect::<Vec<_>>();
        let agents: Vec<RoutedAgentSelection> = max_agents
            .into_iter()
            .enumerate()
            .map(|(index, descriptor)| RoutedAgentSelection {
                agent_id: descriptor.id.clone(),
                action: RoutedAgentAction::Execute,
                priority: if index == 0 {
                    RoutedAgentPriority::Immediate
                } else {
                    RoutedAgentPriority::Deferred
                },
                depends_on: Vec::new(),
            })
            .collect();

        let confidence = agents.len();
        let execution_mode = if agents.len() > 2 {
            RoutedExecutionMode::Hybrid
        } else if agents.len() > 1 {
            RoutedExecutionMode::Parallel
        } else {
            RoutedExecutionMode::Sequential
        };

        RoutedTaskPlan {
            agents,
            execution_mode,
            reasoning: "Selected from agent capability and keyword matching.".into(),
            confidence: Some(confidence as f64),
        }
    }
}

fn ensure_agent(
    catalog: &AgentRoutingCatalog,
    agents: &mut Vec<&AgentRoutingDescriptor>,
    agent_id: &str,
) {
    let normalized = catalog.normalize(agent_id);
    if !agents.iter().any(|descriptor| descriptor.id == normalized) {
        if let Some(descriptor) = catalog.descriptor(&normalized) {
            agents.push(descriptor);
        }
    }
}

fn is_strategic(description: &str) -> bool {
    description.contains("implement")
        || description.contains("upgrade")
        || description.contains("improve")
        || description.contains("refactor")
        || description.contains("agentic")
        || description.contains("workflow")
        || description.contains("orchestrat")
        || description.contains("hub")
}

fn is_implementation_keyword(description: &str) -> bool {
    description.contains("implement")
        || description.contains("upgrade")
        || description.contains("ui")
        || description.contains("component")
        || description.contains("service")
        || description.contains("code")
}

fn order_agents(agents: Vec<&AgentRoutingDescriptor>) -> Vec<&AgentRoutingDescriptor> {
    let mut owned = agents;
    owned.sort_by(|left, right| order_rank(left.id.as_str()).cmp(&order_rank(right.id.as_str())));
    owned.dedup_by(|left, right| left.id == right.id);
    owned
}

fn order_rank(agent_id: &str) -> usize {
    let preferred = [
        "agentic-prompt-enhancer",
        "planning-agent",
        "architecture-agent",
        "code-generator",
        "test-agent",
    ];
    preferred
        .iter()
        .position(|candidate| *candidate == agent_id)
        .unwrap_or(preferred.len())
}

#[derive(Debug, Clone)]
struct DescriptorEntry {
    id: &'static str,
    name: &'static str,
    category: &'static str,
    role: &'static str,
    description: &'static str,
    capabilities: &'static [&'static str],
    keywords: &'static [&'static str],
    specialization_level: Option<SpecializationLevel>,
    supervision_required: Option<bool>,
    aliases: &'static [&'static str],
}

impl DescriptorEntry {
    fn descriptor(&self) -> AgentRoutingDescriptor {
        AgentRoutingDescriptor {
            id: self.id.into(),
            name: self.name.into(),
            category: self.category.into(),
            role: self.role.into(),
            description: self.description.into(),
            capabilities: self.capabilities.iter().map(|value| value.to_string()).collect(),
            keywords: self.keywords.iter().map(|value| value.to_string()).collect(),
            specialization_level: self.specialization_level.clone(),
            supervision_required: self.supervision_required,
        }
    }
}

const DESCRIPTOR_ENTRIES: &[DescriptorEntry] = &[
    DescriptorEntry {
        id: "agentic-prompt-enhancer",
        name: "Agentic Prompt Enhancer",
        category: "intelligence",
        role: "prompt",
        description: "Refines prompts into executable agent instructions.",
        capabilities: &["prompt", "intelligence"],
        keywords: &["prompt", "intelligence"],
        specialization_level: Some(SpecializationLevel::Senior),
        supervision_required: Some(false),
        aliases: &["prompt-enhancer"],
    },
    DescriptorEntry {
        id: "planning-agent",
        name: "Planning Agent",
        category: "planning",
        role: "planner",
        description: "Decomposes goals into actionable plans and milestones.",
        capabilities: &["planning", "roadmap"],
        keywords: &["plan", "planning"],
        specialization_level: Some(SpecializationLevel::Senior),
        supervision_required: Some(true),
        aliases: &[],
    },
    DescriptorEntry {
        id: "architecture-agent",
        name: "Architecture Agent",
        category: "architecture",
        role: "architect",
        description: "Defines system boundaries and architecture decisions.",
        capabilities: &["architecture", "design"],
        keywords: &["architecture", "design", "boundary"],
        specialization_level: Some(SpecializationLevel::Senior),
        supervision_required: Some(true),
        aliases: &[],
    },
    DescriptorEntry {
        id: "code-generator",
        name: "Code Generator",
        category: "codegen",
        role: "developer",
        description: "Generates implementation code from specifications.",
        capabilities: &["codegen", "typescript"],
        keywords: &["code", "component", "service"],
        specialization_level: Some(SpecializationLevel::Specialist),
        supervision_required: Some(true),
        aliases: &[],
    },
    DescriptorEntry {
        id: "test-agent",
        name: "Test Agent",
        category: "testing",
        role: "qa",
        description: "Creates and validates automated tests.",
        capabilities: &["testing", "vitest"],
        keywords: &["test", "coverage"],
        specialization_level: Some(SpecializationLevel::Specialist),
        supervision_required: Some(false),
        aliases: &["testing-agent"],
    },
];

static ROUTING_CATALOG: OnceLock<BTreeMap<String, AgentRoutingDescriptor>> = OnceLock::new();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_maps_known_alias() {
        let catalog = AgentRoutingCatalog;
        assert_eq!(catalog.normalize("testing-agent"), "test-agent");
        assert_eq!(catalog.normalize("unknown-agent"), "unknown-agent");
    }

    #[test]
    fn route_direct_request_returns_sequential_plan() {
        let catalog = AgentRoutingCatalog;
        let mut context = BTreeMap::new();
        context.insert("agentId".into(), serde_json::Value::String("testing-agent".into()));
        let task = RoutableTask {
            description: "please use testing-agent".into(),
            context: Some(context),
        };

        let plan = catalog.route(&task);

        assert_eq!(plan.agents.len(), 1);
        assert_eq!(plan.agents[0].agent_id, "test-agent");
        assert_eq!(plan.execution_mode, RoutedExecutionMode::Sequential);
        assert_eq!(plan.confidence, Some(1.0));
    }

    #[test]
    fn route_unknown_request_returns_keyword_selection() {
        let catalog = AgentRoutingCatalog;
        let task = RoutableTask {
            description: "Create tests for new code".into(),
            context: None,
        };

        let plan = catalog.route(&task);

        assert!(
            plan.agents
                .iter()
                .any(|selection| selection.agent_id == "test-agent")
        );
        assert!(
            plan.confidence.is_some(),
            "expected confidence for keyword-selected plan"
        );
    }

    #[test]
    fn route_strategic_keywords_include_required_agents() {
        let catalog = AgentRoutingCatalog;
        let task = RoutableTask {
            description: "Refactor the workflow orchestration hub".into(),
            context: None,
        };

        let plan = catalog.route(&task);
        let ids: Vec<String> = plan
            .agents
            .iter()
            .map(|selection| selection.agent_id.clone())
            .collect();

        assert!(ids.contains(&"planning-agent".into()));
        assert!(ids.contains(&"architecture-agent".into()));
        assert!(ids.contains(&"test-agent".into()));
    }

    #[test]
    fn route_implementation_keywords_add_code_generator() {
        let catalog = AgentRoutingCatalog;
        let task = RoutableTask {
            description: "Implement a new UI component service".into(),
            context: None,
        };

        let plan = catalog.route(&task);
        let ids: Vec<String> = plan
            .agents
            .iter()
            .map(|selection| selection.agent_id.clone())
            .collect();

        assert!(ids.contains(&"code-generator".into()));
    }
}
