//! Dynamic orchestration.
//!
//! A steering loop that decides the *next* action instead of following a
//! rigid path: given a goal and the work done so far, it picks which **agent**
//! to run, which **model** to route that agent's task through, and the task
//! description — then dispatches it through the [`AgentRuntime`] and reports
//! the outcome back onto the [`EventBus`].
//!
//! Two steering strategies:
//!
//!   - [`HeuristicAdvisor`] (default, offline, deterministic): derives the
//!     task kind from the step description ([`infer_request`]), maps the phase
//!     to an agent role, and lets the [`ModelRouter`] pick the best model.
//!   - [`GatewayAdvisor`] (opt-in): asks a live LLM via the gateway to propose
//!     the next action and model, and **falls back to the heuristic** when the
//!     gateway is unconfigured, the call fails, or the response is invalid.
//!
//! Every dispatched completion feeds latency/error back into the
//! [`ModelCatalog`] via `record_latency`, so routing gets sharper over time.
//!
//! Synchronous to match the crate.
use crate::agent_runtime::{AgentResult, AgentRuntime, AgentTask};
use crate::domain::{DomainError, EntityId, Result};
use crate::event_bus::{DomainEvent, EventBus, TOPIC_STEP_COMPLETED, TOPIC_STEP_STARTED};
use crate::model_catalog::{ModelCatalog, TaskKind};
use crate::model_router::{infer_request, ModelRouter};
use crate::model_selection::{route_candidates, SelectionState};
use std::sync::Arc;

/// The orchestrator's single decision: one agent, one model, one task.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestratorDecision {
    pub agent_id: String,
    pub model_id: Option<String>,
    pub task_kind: TaskKind,
    pub task_description: String,
    /// Human-readable reason for the choice (surfaced in the UI / events).
    pub rationale: String,
    /// The routing request that produced the model choice (for transparency).
    pub request: Option<serde_json::Value>,
}

impl OrchestratorDecision {
    fn to_event_payload(&self) -> serde_json::Value {
        serde_json::json!({
            "agentId": self.agent_id,
            "model": self.model_id,
            "taskKind": self.task_kind.as_str(),
            "description": self.task_description,
            "rationale": self.rationale,
        })
    }
}

/// A pluggable source of "what to do next". The heuristic is the default; the
/// gateway advisor layers an LLM on top with a safe fallback.
pub trait NextActionAdvisor {
    fn advise(
        &self,
        goal: &str,
        step_description: &str,
        catalog: &ModelCatalog,
        router: &ModelRouter,
        selection: &SelectionState,
    ) -> OrchestratorDecision;

    /// Decide whether `goal` is complete given the work done so far. Each
    /// history entry is `(step_description, output)`. Default: never
    /// complete — the autonomous loop stays bounded by its step budget.
    fn goal_completed(&self, _goal: &str, _history: &[(String, String)]) -> bool {
        false
    }
}

/// Map a phase/step description to an agent role.
pub fn agent_role_for(description: &str) -> &'static str {
    let lower = description.to_lowercase();
    if lower.contains("review") || lower.contains("check") || lower.contains("validate") {
        "reviewer"
    } else if lower.contains("plan")
        || lower.contains("analy")
        || lower.contains("architect")
        || lower.contains("design")
    {
        "planner"
    } else {
        "executor"
    }
}

/// Offline, deterministic steering: heuristics + weighted model routing.
#[derive(Debug, Default, Clone)]
pub struct HeuristicAdvisor;

impl NextActionAdvisor for HeuristicAdvisor {
    fn advise(
        &self,
        goal: &str,
        step_description: &str,
        catalog: &ModelCatalog,
        router: &ModelRouter,
        selection: &SelectionState,
    ) -> OrchestratorDecision {
        let request = infer_request(step_description);
        let agent_id = agent_role_for(step_description).to_string();

        // Only route among models the user has actually selected / whose
        // provider is enabled; `route_candidates` falls back to the whole
        // catalog when nothing is selected and nothing is disabled.
        let candidates = route_candidates(selection, &catalog.all());
        let decision = router.route_live_filtered(catalog, &candidates, &request);
        let model_id = decision
            .chosen
            .as_ref()
            .map(|c| c.model_id.clone())
            .or_else(|| {
                // No fully-eligible model: relax to the top-ranked candidate.
                decision.ranked.first().map(|c| c.model_id.clone())
            });

        let rationale = match &decision.chosen {
            Some(c) => format!(
                "{}: {} (score {:.3}, affinity {:.2})",
                agent_id, c.model_id, c.score, c.affinity
            ),
            None => format!("{agent_id}: no eligible model; using heuristic fallback"),
        };

        let mut task_description = step_description.to_string();
        if !goal.is_empty()
            && !task_description
                .to_lowercase()
                .contains(&goal.to_lowercase())
        {
            task_description = format!("[Goal: {goal}] {step_description}");
        }

        OrchestratorDecision {
            agent_id,
            model_id,
            task_kind: request.task_kind,
            task_description,
            rationale,
            request: Some(serde_json::to_value(&request).unwrap_or_default()),
        }
    }
}

/// Asks a live LLM to choose the next action + model, falling back to the
/// heuristic on any problem. `provider` + `model` name the gateway target used
/// for the *steering* call (not the routed model).
#[derive(Clone)]
pub struct GatewayAdvisor {
    pub steering_provider: String,
    pub steering_model: String,
    pub fallback: Arc<dyn NextActionAdvisor>,
}

impl GatewayAdvisor {
    /// Parse an advisor LLM response into a decision. Accepts a JSON object
    /// with `agentId`/`agent`, `model`, and optional `description`. Returns
    /// `None` when the payload is unusable so the caller falls back.
    fn parse_advice(body: &serde_json::Value) -> Option<(String, Option<String>, Option<String>)> {
        // Unwrap an {"output": {...}} / {"content": "..."} envelope if present.
        let obj = body
            .get("output")
            .cloned()
            .or_else(|| body.get("result").cloned())
            .unwrap_or_else(|| body.clone());
        let obj = if let Some(s) = obj.get("content").and_then(|v| v.as_str()) {
            serde_json::from_str::<serde_json::Value>(s).ok()?
        } else {
            obj
        };
        let agent = obj
            .get("agentId")
            .or_else(|| obj.get("agent"))
            .and_then(|v| v.as_str())?
            .trim()
            .to_string();
        if agent.is_empty() {
            return None;
        }
        let model = obj
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let description = obj
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        Some((agent, model, description))
    }

    fn try_llm(&self, goal: &str, step_description: &str) -> Option<OrchestratorDecision> {
        let gateway = crate::gateway::default_gateway();
        if !gateway.is_configured() {
            return None;
        }
        let prompt = format!(
            "You are steering an autonomous software pipeline. Goal: {goal}\n\
             Current step: {step_description}\n\
             Decide the single next action. Respond with a JSON object only: \
             {{\"agentId\": \"planner|executor|reviewer\", \"model\": \"<provider>/<model-id>\", \
             \"description\": \"<one-line task description>\"}}. \
             Pick the model that best fits the step. No prose."
        );
        let messages = serde_json::json!([
            {"role": "system", "content": "You are a precise orchestration controller. Emit only valid JSON."},
            {"role": "user", "content": prompt},
        ]);
        let request = crate::gateway::GatewayRequest {
            provider: self.steering_provider.clone(),
            payload: messages,
            task_type: Some("chat".into()),
        };
        let response = gateway.send(request).ok()?;
        let body = &response.body;
        // Extract content (mirror agent_runtime::extract_chat_content shape).
        let content = body
            .get("choices")?
            .as_array()?
            .first()?
            .get("message")?
            .get("content")?
            .as_str()?;
        let parsed: serde_json::Value = serde_json::from_str(content)
            .or_else(|_| {
                // Strip a ```json fenced block if the model added one.
                let trimmed = content.trim();
                let inner = trimmed
                    .strip_prefix("```json")
                    .or_else(|| trimmed.strip_prefix("```"))
                    .and_then(|s| s.strip_suffix("```"))
                    .unwrap_or(trimmed);
                serde_json::from_str(inner)
            })
            .ok()?;
        let (agent, model, description) = Self::parse_advice(&parsed)?;
        Some(OrchestratorDecision {
            agent_id: agent,
            model_id: model,
            task_kind: infer_request(&step_description).task_kind,
            task_description: description.unwrap_or_else(|| step_description.to_string()),
            rationale: "steered by LLM advisor".into(),
            request: None,
        })
    }
}

impl NextActionAdvisor for GatewayAdvisor {
    fn advise(
        &self,
        goal: &str,
        step_description: &str,
        catalog: &ModelCatalog,
        router: &ModelRouter,
        selection: &SelectionState,
    ) -> OrchestratorDecision {
        if let Some(llm) = self.try_llm(goal, step_description) {
            // Validate the LLM's model choice against the catalog; if it
            // named an unknown model, keep the agent but re-route the model.
            let model_ok = llm
                .model_id
                .as_ref()
                .map(|m| catalog.get(m).is_some())
                .unwrap_or(true);
            if model_ok {
                return llm;
            }
            let mut heuristic =
                self.fallback
                    .advise(goal, step_description, catalog, router, selection);
            heuristic.rationale = format!(
                "LLM chose unknown model '{}'; fell back to routed model",
                llm.model_id.as_deref().unwrap_or("?")
            );
            return heuristic;
        }
        // No gateway / failed call / unparseable: pure heuristic.
        self.fallback
            .advise(goal, step_description, catalog, router, selection)
    }
}

/// The orchestrator. Stateless container of an advisor + router; the catalog
/// is passed per call so it stays shareable.
#[derive(Clone)]
pub struct DynamicOrchestrator {
    advisor: Arc<dyn NextActionAdvisor>,
    router: ModelRouter,
}

impl Default for DynamicOrchestrator {
    fn default() -> Self {
        Self::new(HeuristicAdvisor)
    }
}

impl DynamicOrchestrator {
    pub fn new(advisor: impl NextActionAdvisor + 'static) -> Self {
        Self {
            advisor: Arc::new(advisor),
            router: ModelRouter::new(),
        }
    }

    pub fn with_router(mut self, router: ModelRouter) -> Self {
        self.router = router;
        self
    }

    /// Decide the next action for `step_description` within `goal`.
    pub fn decide(
        &self,
        goal: &str,
        step_description: &str,
        catalog: &ModelCatalog,
        selection: &SelectionState,
    ) -> OrchestratorDecision {
        self.advisor
            .advise(goal, step_description, catalog, &self.router, selection)
    }

    /// Decide, dispatch through `runtime`, feed learning back into
    /// `catalog`, and publish step events to the bus. Returns the agent
    /// result plus the decision actually taken.
    pub fn dispatch(
        &self,
        goal: &str,
        step_description: &str,
        catalog: &ModelCatalog,
        selection: &SelectionState,
        runtime: &dyn AgentRuntime,
        bus: Option<&EventBus>,
        run_id: &str,
    ) -> (Result<AgentResult, DomainError>, OrchestratorDecision) {
        let decision = self.decide(goal, step_description, catalog, selection);
        let task_id = EntityId::new();

        if let Some(bus) = bus {
            bus.publish(DomainEvent {
                topic: TOPIC_STEP_STARTED.into(),
                payload: decision.to_event_payload(),
            });
        }

        let started = std::time::Instant::now();
        let payload = serde_json::json!({
            "input": decision.task_description,
            "goal": goal,
            "model": decision.model_id,
            "taskKind": decision.task_kind.as_str(),
            "routedBy": "dynamic_orchestrator",
        });
        let task = AgentTask {
            id: task_id.clone(),
            agent_id: decision.agent_id.clone(),
            payload,
        };
        let result = runtime.execute(task);
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;

        // Learn: feed latency/error back so future routing is sharper.
        let ok = matches!(result, Ok(ref r) if r.success);
        if let Some(model_id) = &decision.model_id {
            catalog.record_latency(model_id, elapsed_ms, !ok);
        }

        if let Some(bus) = bus {
            let (severity, message) = if ok {
                ("info", format!("step completed via {}", decision.agent_id))
            } else {
                ("error", format!("step failed via {}", decision.agent_id))
            };
            let _ = severity;
            bus.publish(DomainEvent {
                topic: TOPIC_STEP_COMPLETED.into(),
                payload: serde_json::json!({
                    "step_id": task_id.as_str(),
                    "agent": decision.agent_id,
                    "model": decision.model_id,
                    "ok": ok,
                    "message": message,
                }),
            });
        }

        (result, decision)
    }

    /// Steer autonomously until the advisor reports the goal is complete, a
    /// step fails hard, or `max_steps` is reached. Each iteration decides the
    /// next action from `initial_step` plus a rolling summary of what happened,
    /// dispatches it, and folds the outcome back in. Returns the full step
    /// log and whether the goal was reported complete (vs. budget-exhausted
    /// or aborted on failure).
    pub fn run_until_done(
        &self,
        goal: &str,
        initial_step: &str,
        max_steps: usize,
        catalog: &ModelCatalog,
        selection: &SelectionState,
        runtime: &dyn AgentRuntime,
        bus: Option<&EventBus>,
        run_id: &str,
    ) -> OrchestratorRunReport {
        let mut report = OrchestratorRunReport {
            goal: goal.to_string(),
            completed: false,
            steps: Vec::new(),
            aborted: false,
        };
        if goal.trim().is_empty() || max_steps == 0 {
            return report;
        }

        let mut history: Vec<(String, String)> = Vec::new();
        let mut current = initial_step.trim().to_string();

        for i in 0..max_steps {
            // Completion check runs before spending the step (skips step 0's
            // redundant check only when there is no history yet).
            if !history.is_empty() && self.advisor.goal_completed(goal, &history) {
                report.completed = true;
                break;
            }
            if current.is_empty() {
                current = "Assess the current state and take the next necessary action to advance the goal."
                    .to_string();
            }

            let (result, decision) =
                self.dispatch(goal, &current, catalog, selection, runtime, bus, run_id);
            let summary = match &result {
                Ok(r) => {
                    let text = match &r.output {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    if r.success {
                        format!(
                            "step {} ({}) succeeded: {}",
                            i + 1,
                            decision.agent_id,
                            truncate(&text, 500)
                        )
                    } else {
                        let err = r.error.clone().unwrap_or_else(|| "unknown error".into());
                        format!(
                            "step {} ({}) FAILED: {}",
                            i + 1,
                            decision.agent_id,
                            truncate(&err, 500)
                        )
                    }
                }
                Err(e) => format!("step {} ({}) errored: {}", i + 1, decision.agent_id, e),
            };

            let succeeded = matches!(&result, Ok(r) if r.success);
            report.steps.push(summary.clone());
            history.push((current.clone(), summary));

            // A hard failure aborts the run rather than looping on a broken
            // state; the caller can resume with a corrected step.
            if !succeeded {
                report.aborted = true;
                break;
            }
        }

        if !report.completed && !report.aborted && !report.steps.is_empty() {
            // Ran out of budget without the advisor saying "done".
            report.aborted = true;
        }
        report
    }
}

/// A hard cap for truncating step summaries folded back into the advisor.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}…")
    }
}

/// Full outcome of an autonomous `run_until_done` run.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestratorRunReport {
    pub goal: String,
    /// True when the advisor judged the goal complete.
    pub completed: bool,
    /// True when the run stopped early (failure or step budget).
    pub aborted: bool,
    pub steps: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_catalog::{CostTier, Modality, ModelInfo};

    fn seed_catalog() -> ModelCatalog {
        let catalog = ModelCatalog::new();
        for (id, affinity, tier, tools) in [
            ("openai/coder", 0.95f32, CostTier::High, true),
            ("openai/cheap", 0.6f32, CostTier::Low, true),
            ("anthropic/reviewer", 0.9f32, CostTier::Medium, false),
        ] {
            let (provider, model_id) = id.split_once('/').unwrap();
            catalog.upsert(ModelInfo {
                id: id.into(),
                provider: provider.into(),
                model_id: model_id.into(),
                name: model_id.into(),
                family: "test".into(),
                context_window: 128_000,
                max_output_tokens: 8192,
                input_modalities: vec![Modality::Text],
                supports_tools: tools,
                supports_structured_output: true,
                supports_reasoning: false,
                cost_tier: tier,
                description: String::new(),
                strengths: vec![],
                task_affinity: std::collections::HashMap::from([("coding".to_string(), affinity)]),
                prompt_price_per_mtok: None,
                completion_price_per_mtok: None,
                discovered: false,
            });
        }
        catalog
    }

    struct RecordingRuntime {
        calls: std::sync::Mutex<Vec<String>>,
    }
    impl AgentRuntime for RecordingRuntime {
        fn execute(&self, task: AgentTask) -> Result<AgentResult, DomainError> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("{}/{}", task.agent_id, task.id.as_str()));
            Ok(AgentResult {
                task_id: task.id,
                success: true,
                output: serde_json::json!({"status": "completed"}),
                error: None,
            })
        }
    }

    /// Fails every task, to test hard-failure abort.
    struct FailingRuntime;
    impl AgentRuntime for FailingRuntime {
        fn execute(&self, task: AgentTask) -> Result<AgentResult, DomainError> {
            Ok(AgentResult {
                task_id: task.id,
                success: false,
                output: serde_json::Value::Null,
                error: Some("boom".into()),
            })
        }
    }

    /// Heuristic advisor that declares the goal complete after `after` steps.
    struct CompletingAfter {
        after: usize,
    }
    impl NextActionAdvisor for CompletingAfter {
        fn advise(
            &self,
            goal: &str,
            step_description: &str,
            catalog: &ModelCatalog,
            router: &ModelRouter,
            selection: &SelectionState,
        ) -> OrchestratorDecision {
            HeuristicAdvisor.advise(goal, step_description, catalog, router, selection)
        }
        fn goal_completed(&self, _goal: &str, history: &[(String, String)]) -> bool {
            history.len() >= self.after
        }
    }

    #[test]
    fn run_until_done_exhausts_step_budget_by_default() {
        let catalog = seed_catalog();
        let orch = DynamicOrchestrator::default(); // heuristic never says "done"
        let runtime = RecordingRuntime {
            calls: std::sync::Mutex::new(Vec::new()),
        };
        let report = orch.run_until_done(
            "ship it",
            "start",
            3,
            &catalog,
            &SelectionState::default(),
            &runtime,
            None,
            "run-1",
        );
        assert!(!report.completed);
        assert!(report.aborted);
        assert_eq!(report.steps.len(), 3);
        assert_eq!(runtime.calls.lock().unwrap().len(), 3);
    }

    #[test]
    fn run_until_done_stops_when_advisor_reports_complete() {
        let catalog = seed_catalog();
        let orch = DynamicOrchestrator::new(CompletingAfter { after: 2 });
        let runtime = RecordingRuntime {
            calls: std::sync::Mutex::new(Vec::new()),
        };
        let report = orch.run_until_done(
            "ship it",
            "start",
            10,
            &catalog,
            &SelectionState::default(),
            &runtime,
            None,
            "run-1",
        );
        assert!(report.completed);
        assert!(!report.aborted);
        // 2 steps executed, then the completion check stops the loop.
        assert_eq!(report.steps.len(), 2);
    }

    #[test]
    fn run_until_done_aborts_on_failing_step() {
        let catalog = seed_catalog();
        let orch = DynamicOrchestrator::default();
        let runtime = FailingRuntime;
        let report = orch.run_until_done(
            "ship it",
            "start",
            5,
            &catalog,
            &SelectionState::default(),
            &runtime,
            None,
            "run-1",
        );
        assert!(report.aborted);
        assert!(!report.completed);
        assert_eq!(report.steps.len(), 1);
        assert!(report.steps[0].contains("FAILED"));
    }

    #[test]
    fn run_until_done_refuses_empty_goal() {
        let catalog = seed_catalog();
        let orch = DynamicOrchestrator::default();
        let runtime = RecordingRuntime {
            calls: std::sync::Mutex::new(Vec::new()),
        };
        let report = orch.run_until_done(
            "  ",
            "start",
            3,
            &catalog,
            &SelectionState::default(),
            &runtime,
            None,
            "run-1",
        );
        assert!(report.steps.is_empty());
        assert!(!report.completed && !report.aborted);
    }

    #[test]
    fn heuristic_routes_coding_step_to_a_coder() {
        let catalog = seed_catalog();
        let orch = DynamicOrchestrator::default();
        let decision = orch.decide(
            "build a REST service",
            "Implement the user endpoint",
            &catalog,
            &SelectionState::default(),
        );
        assert_eq!(decision.agent_id, "executor");
        assert_eq!(decision.task_kind, TaskKind::Coding);
        // The high-affinity coder should be picked for a coding step.
        assert_eq!(decision.model_id.as_deref(), Some("openai/coder"));
    }

    #[test]
    fn heuristic_maps_phase_to_reviewer() {
        let catalog = seed_catalog();
        let orch = DynamicOrchestrator::default();
        let decision = orch.decide(
            "build a REST service",
            "Review the diff for correctness",
            &catalog,
            &SelectionState::default(),
        );
        assert_eq!(decision.agent_id, "reviewer");
    }

    #[test]
    fn heuristic_maps_analysis_to_planner() {
        let catalog = seed_catalog();
        let orch = DynamicOrchestrator::default();
        let decision = orch.decide(
            "build a REST service",
            "Analyze the goal and design the API",
            &catalog,
            &SelectionState::default(),
        );
        assert_eq!(decision.agent_id, "planner");
    }

    #[test]
    fn dispatch_executes_and_returns_decision() {
        let catalog = seed_catalog();
        let orch = DynamicOrchestrator::default();
        let runtime = RecordingRuntime {
            calls: std::sync::Mutex::new(Vec::new()),
        };
        let (result, decision) = orch.dispatch(
            "goal",
            "Implement the login endpoint",
            &catalog,
            &SelectionState::default(),
            &runtime,
            None,
            "run-1",
        );
        assert!(result.unwrap().success);
        assert_eq!(decision.agent_id, "executor");
        assert_eq!(runtime.calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn dispatch_publishes_step_events() {
        let catalog = seed_catalog();
        let orch = DynamicOrchestrator::default();
        let runtime = RecordingRuntime {
            calls: std::sync::Mutex::new(Vec::new()),
        };
        let (bus, receiver) = EventBus::new();
        orch.dispatch(
            "goal",
            "Implement the login endpoint",
            &catalog,
            &SelectionState::default(),
            &runtime,
            Some(&bus),
            "run-1",
        );
        let mut topics: Vec<String> = receiver.try_iter().map(|e| e.topic).collect();
        // We expect at least the started + completed events.
        assert!(
            topics.contains(&TOPIC_STEP_STARTED.to_string()),
            "expected step started, got {topics:?}"
        );
        assert!(
            topics.contains(&TOPIC_STEP_COMPLETED.to_string()),
            "expected step completed, got {topics:?}"
        );
    }

    #[test]
    fn gateway_advisor_falls_back_without_gateway() {
        // default_gateway() is unconfigured in tests -> must fall back to
        // the heuristic advisor and still produce a valid decision.
        let catalog = seed_catalog();
        let advisor = GatewayAdvisor {
            steering_provider: "openai".into(),
            steering_model: "gpt-4o".into(),
            fallback: Arc::new(HeuristicAdvisor),
        };
        let orch = DynamicOrchestrator::new(advisor);
        let decision = orch.decide(
            "goal",
            "Implement the login endpoint",
            &catalog,
            &SelectionState::default(),
        );
        assert_eq!(decision.agent_id, "executor");
        assert!(decision.model_id.is_some());
    }

    #[test]
    fn parse_advice_accepts_envelope() {
        let v = serde_json::json!({
            "choices": [{"message": {"content": "{\"agentId\":\"executor\",\"model\":\"openai/coder\"}"}}]
        });
        let content = v["choices"][0]["message"]["content"].as_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(content).unwrap();
        let (agent, model, _desc) = GatewayAdvisor::parse_advice(&parsed).unwrap();
        assert_eq!(agent, "executor");
        assert_eq!(model.as_deref(), Some("openai/coder"));
    }
}
