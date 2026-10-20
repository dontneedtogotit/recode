use super::domain::{EntityId, Result};
use super::plan_graph::PlanTask;
use super::planning_circuit_breaker::PlanCircuitBreaker;
use super::agent_runtime::{AgentRuntime, AgentTask};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanStep {
    pub id: EntityId,
    pub description: String,
    pub status: PlanStatus,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum PlanStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Plan {
    pub id: EntityId,
    pub goal: String,
    pub status: PlanStatus,
    pub steps: Vec<PlanStep>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepProgress {
    Started,
    Completed,
}

pub trait Planner {
    /// Return a list of file‑level tasks (produces/consumes) derived from the plan.
    /// The default implementation returns an empty list, which preserves existing
    /// behaviour. Implementors may provide richer information to drive manifest
    /// dependency wiring.
    fn plan_file_graph(&self, _goal: &str) -> Result<Vec<PlanTask>> {
        Ok(Vec::new())
    }
    fn create_plan(&self, goal: &str) -> Result<Plan>;
    fn execute_plan(&self, plan: &mut Plan, runtime: &dyn AgentRuntime);

    fn execute_plan_with_progress(
        &self,
        plan: &mut Plan,
        runtime: &dyn AgentRuntime,
        progress: &mut dyn FnMut(&PlanStep, StepProgress) -> bool,
    ) {
        self.execute_plan(plan, runtime);
        for step in &plan.steps {
            if !progress(step, StepProgress::Started) {
                return;
            }
            if !progress(step, StepProgress::Completed) {
                return;
            }
        }
    }
}

pub struct LocalPlanner {
    circuit_breaker: PlanCircuitBreaker,
}

impl LocalPlanner {
    pub fn new() -> Self {
        Self { circuit_breaker: PlanCircuitBreaker::new() }
    }
}

impl Default for LocalPlanner {
    fn default() -> Self {
        Self::new()
    }
}

impl Planner for LocalPlanner {
    fn create_plan(&self, goal: &str) -> Result<Plan> {
        Ok(Plan {
            id: EntityId::new(),
            goal: goal.into(),
            status: PlanStatus::Pending,
            steps: vec![
                PlanStep {
                    id: EntityId::new(),
                    description: format!("Analyze goal: {goal}"),
                    status: PlanStatus::Pending,
                },
                PlanStep {
                    id: EntityId::new(),
                    description: "Execute plan steps".into(),
                    status: PlanStatus::Pending,
                },
            ],
        })
    }

    fn execute_plan(&self, plan: &mut Plan, runtime: &dyn AgentRuntime) {
        self.execute_plan_with_progress(plan, runtime, &mut |_, _| true);
    }

    fn execute_plan_with_progress(
        &self,
        plan: &mut Plan,
        runtime: &dyn AgentRuntime,
        progress: &mut dyn FnMut(&PlanStep, StepProgress) -> bool,
    ) {
        plan.status = PlanStatus::InProgress;
        for step in &mut plan.steps {
            if step.status == PlanStatus::Pending {
                if self.circuit_breaker.should_skip_phase(&step.description, "", plan.id.as_str()) {
                    step.status = PlanStatus::Failed;
                    plan.status = PlanStatus::Failed;
                    return;
                }
                step.status = PlanStatus::InProgress;
                if !progress(step, StepProgress::Started) {
                    return;
                }
                let outcome = runtime.execute(AgentTask {
                    id: step.id.clone(),
                    agent_id: "planner".into(),
                    payload: serde_json::json!({ "step": step.description }),
                });
                if !matches!(outcome, Ok(result) if result.success) {
                    step.status = PlanStatus::Failed;
                    plan.status = PlanStatus::Failed;
                    return;
                }
                step.status = PlanStatus::Completed;
                if !progress(step, StepProgress::Completed) {
                    return;
                }
            }
        }
        plan.status = PlanStatus::Completed;
    }

    /// Returns file-level tasks (produces/consumes) for reproducible manifest generation.
    ///
    /// For the default two-step plan this task graph maps each step to an output file.
    /// > NOTE: This placeholder produces a task for the Execute plan steps step that generates the
    /// > PROJECT.md artifact in the workspace (not imported). Future steps can refine this mapping.
    fn plan_file_graph(&self, goal: &str) -> Result<Vec<PlanTask>> {
        // For our fixed two-step plan:
        // Step 0: "Analyze goal" produces nothing.
        // Step 1: "Execute plan steps" produces the generated project output.
        // The task maps the app entry point to its real Vite manifest
        // dependencies: main.tsx imports App, and the scaffold configs
        // (package.json / index.html) wrap it. Unmapped files keep the
        // manifest's canned wave defaults.
        let _ = goal;
        Ok(vec![PlanTask {
            id: EntityId::new().to_string(),
            produces: vec!["src/main.tsx".into()],
            consumes: vec![
                "src/App.tsx".into(),
                "src/App.css".into(),
                "index.html".into(),
                "package.json".into(),
            ],
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planner_creates_plan_from_goal() {
        let planner = LocalPlanner::new();
        let plan = planner.create_plan("build landing page").expect("plan");
        assert_eq!(plan.goal, "build landing page");
        assert_eq!(plan.status, PlanStatus::Pending);
        assert!(!plan.steps.is_empty());
    }

    #[test]
    fn execute_plan_reports_step_progress_in_order() {
        let planner = LocalPlanner::new();
        let mut plan = planner.create_plan("stream progress").expect("plan");
        let mut progress = Vec::new();
        planner.execute_plan_with_progress(
            &mut plan,
            &crate::agent_runtime::LocalAgentRuntime,
            &mut |step, event| {
                progress.push((step.id.as_str().to_string(), format!("{event:?}")));
                true
            },
        );
        assert_eq!(progress.len(), 4);
        assert_eq!(progress[0].1, "Started");
        assert_eq!(progress[1].1, "Completed");
        assert_eq!(progress[2].1, "Started");
        assert_eq!(progress[3].1, "Completed");
    }

    #[test]
    fn execute_plan_stops_when_progress_is_rejected() {
        let planner = LocalPlanner::new();
        let mut plan = planner.create_plan("cancel progress").expect("plan");
        let mut starts = 0;
        planner.execute_plan_with_progress(
            &mut plan,
            &crate::agent_runtime::LocalAgentRuntime,
            &mut |_, event| {
                if event == StepProgress::Started {
                    starts += 1;
                    return starts == 1;
                }
                true
            },
        );
        assert_eq!(starts, 2);
    }

    #[test]
    fn execute_plan_transitions_steps_to_completed() {
        let planner = LocalPlanner::new();
        let mut plan = planner.create_plan("build landing page").expect("plan");
        planner.execute_plan(&mut plan, &crate::agent_runtime::LocalAgentRuntime);
        assert_eq!(plan.status, PlanStatus::Completed);
        assert!(plan.steps.iter().all(|step| step.status == PlanStatus::Completed));
    }

    #[test]
    fn execute_plan_skips_phase_when_circuit_breaker_blocks() {
        let planner = LocalPlanner::new();
        let mut plan = planner.create_plan("build landing page").expect("plan");
        let project_id = plan.id.as_str().to_string();
        let phase = plan.steps[0].description.clone();
        let metrics = vec![
            crate::planning_circuit_breaker::PlanMetrics {
                status: crate::planning_circuit_breaker::PlanRunStatus::Failed,
                phases: std::collections::HashMap::from([(phase.clone(), Some(crate::planning_circuit_breaker::PhaseStatus::Failed))]),
            },
            crate::planning_circuit_breaker::PlanMetrics {
                status: crate::planning_circuit_breaker::PlanRunStatus::Failed,
                phases: std::collections::HashMap::from([(phase.clone(), Some(crate::planning_circuit_breaker::PhaseStatus::Failed))]),
            },
            crate::planning_circuit_breaker::PlanMetrics {
                status: crate::planning_circuit_breaker::PlanRunStatus::Failed,
                phases: std::collections::HashMap::from([(phase, Some(crate::planning_circuit_breaker::PhaseStatus::Failed))]),
            },
        ];
        planner.circuit_breaker.load_recent_metrics(&project_id, metrics);
        planner.execute_plan(&mut plan, &crate::agent_runtime::LocalAgentRuntime);
        assert_eq!(plan.steps[0].status, PlanStatus::Failed);
        assert_eq!(plan.status, PlanStatus::Failed);
    }

    #[test]
    fn plan_file_graph_returns_tasks() {
        let planner = LocalPlanner::new();
        let tasks = planner.plan_file_graph("dummy").expect("tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].produces, vec![String::from("src/main.tsx")]);
        // Consumes must reference real Vite manifest paths only.
        for dep in &tasks[0].consumes {
            assert!(
                crate::pipeline::VITE_PROJECT_FILES
                    .iter()
                    .any(|&(path, ..)| path == dep),
                "planner consumes references unknown manifest path: {dep}"
            );
        }
    }

    #[test]
    fn runtime_failure_cannot_complete_a_plan() {
        struct FailingRuntime;
        impl AgentRuntime for FailingRuntime {
            fn execute(&self, _task: AgentTask) -> Result<crate::agent_runtime::AgentResult> {
                Err(crate::domain::DomainError::internal("runtime unavailable"))
            }
        }
        let planner = LocalPlanner::new();
        let mut plan = planner.create_plan("build landing page").unwrap();
        planner.execute_plan(&mut plan, &FailingRuntime);
        assert_eq!(plan.steps[0].status, PlanStatus::Failed);
        assert_eq!(plan.status, PlanStatus::Failed);
    }
}
