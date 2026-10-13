use crate::agent_runtime::AgentRuntime;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::domain::Result;
use crate::event_bus::{
    now_millis, DomainEvent, EventBus, TOPIC_PLAN_COMPLETED, TOPIC_PLAN_CREATED, TOPIC_PLAN_FAILED,
    TOPIC_STEP_COMPLETED, TOPIC_STEP_STARTED,
};
use crate::planner::{Plan, PlanStep, Planner, StepProgress};
use crate::runtime_monitor::MonitorEvent;
use crate::self_heal::{RepairAction, SelfHealService};
use std::sync::{Condvar, Mutex};
use std::path::PathBuf;

#[cfg(test)]
use crate::agent_runtime::LocalAgentRuntime;
#[cfg(test)]
use crate::planner::LocalPlanner;
#[cfg(test)]
use crate::self_heal::DefaultSelfHealService;

pub mod engine;
mod manifest;
mod generator;
mod prompts;
mod generation_errors;
mod ledger;
mod checkpoint;
mod validation;
mod policy;
mod repair;
mod error_handler;
mod report;
mod outcome_analyzer;
mod content_type;
mod file_budget;
mod metrics;
mod pattern_learning;
mod evidence;
mod traceability;
mod file_classification;
mod adaptation_policy;
mod helpers;
mod file_path_validator;
mod prompt_cache;
mod generated_module_paths;
mod static_analysis;
mod stub_generator;
mod cross_file_context;
mod error_budget;
mod manifest_budget;
mod mental_simulator;
mod scaffolder;
mod contract_lock;
mod production_readiness;
mod acceptance_test_derivation;
mod edge_case_exercise;
mod accessibility_audit;
mod ux_consistency;
pub mod progress_tracker;
pub mod executor;
pub mod types;
pub mod verify;
pub mod supervisor;
pub mod verifiers;
mod outcome;
pub mod identity;
pub mod identity_persistence;

use crate::pipeline::engine::LocalPipelineEngine;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PipelineInput {
    pub goal: String,
    pub emit_events: bool,
    #[serde(default)]
    pub run_id: String,
    #[serde(default)]
    pub project_id: Option<String>,
    /// Scoped consent to run supervised verifier subprocesses (npm
    /// install/type-check/build/test:run and the dev-server boot check) for
    /// this run. Defaults to not supplied; only `granted` ever spawns.
    #[serde(default)]
    pub execution_consent: crate::pipeline::supervisor::ExecutionConsent,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PipelineRun {
    pub plan: Plan,
    pub repair_actions: Vec<RepairAction>,
    pub events: Vec<MonitorEvent>,
    #[serde(alias = "runId")]
    pub run_id: String,
    /// Native-resolved provenance only; never a replay or filesystem grant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_context: Option<RunExecutionContext>,
    #[serde(default)]
    pub generated_files: Vec<crate::pipeline::types::GeneratedFile>,
    #[serde(default)]
    pub failed_files: Vec<crate::pipeline::types::GeneratedFile>,
    #[serde(default)]
    pub outcome: Option<crate::pipeline::types::BuildOutcome>,
    #[serde(default)]
    pub verification_results: Option<Vec<crate::pipeline::verify::VerificationResult>>,
    #[serde(default)]
    pub repair_results: Option<crate::pipeline::repair::AdaptiveRepairResult>,
    #[serde(default)]
    pub verification_summary: Option<String>,
}

pub trait PipelineService {
    fn run(&self, input: PipelineInput) -> Result<PipelineRun>;
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RunExecutionContext {
    pub schema_version: u32,
    #[serde(flatten)]
    pub target: RunTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum RunTarget {
    Simulation,
    Workspace { project_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ControlState {
    Running,
    PauseRequested,
    Paused,
    Cancelling,
}

/// Cooperative controls; pausing takes effect before the next step, never mid-step.
pub struct PipelineControl {
    state: Mutex<ControlState>,
    wake: Condvar,
}

impl Default for PipelineControl {
    fn default() -> Self { Self::new() }
}

impl PipelineControl {
    pub fn new() -> Self {
        Self { state: Mutex::new(ControlState::Running), wake: Condvar::new() }
    }

    pub fn state(&self) -> &'static str {
        match *self.state.lock().unwrap_or_else(|poison| poison.into_inner()) {
            ControlState::Running => "running",
            ControlState::PauseRequested => "pausing",
            ControlState::Paused => "paused",
            ControlState::Cancelling => "cancelling",
        }
    }

    pub fn pause(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|poison| poison.into_inner());
        if *state != ControlState::Running { return false; }
        *state = ControlState::PauseRequested;
        true
    }

    pub fn resume(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|poison| poison.into_inner());
        if !matches!(*state, ControlState::PauseRequested | ControlState::Paused) { return false; }
        *state = ControlState::Running;
        self.wake.notify_all();
        true
    }

    pub fn abort(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|poison| poison.into_inner());
        if *state == ControlState::Cancelling { return false; }
        *state = ControlState::Cancelling;
        self.wake.notify_all();
        true
    }

    pub fn is_cancelling(&self) -> bool {
        *self.state.lock().unwrap_or_else(|poison| poison.into_inner()) == ControlState::Cancelling
    }

    pub fn wait_if_paused(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|poison| poison.into_inner());
        if *state == ControlState::PauseRequested { *state = ControlState::Paused; }
        while *state == ControlState::Paused {
            state = self.wake.wait(state).unwrap_or_else(|poison| poison.into_inner());
        }
        *state != ControlState::Cancelling
    }
}

pub struct LocalPipelineService<'a> {
    pub planner: &'a dyn Planner,
    pub runtime: &'a dyn AgentRuntime,
    pub event_bus: &'a EventBus,
    pub self_heal: &'a dyn SelfHealService,
    workspace_root: Option<PathBuf>,
}

impl<'a> LocalPipelineService<'a> {
    pub const fn new(
        planner: &'a dyn Planner,
        runtime: &'a dyn AgentRuntime,
        event_bus: &'a EventBus,
        self_heal: &'a dyn SelfHealService,
    ) -> Self {
        Self {
            planner,
            runtime,
            event_bus,
            self_heal,
            workspace_root: None,
        }
    }

    /// The caller must supply a previously authorized managed workspace root.
    pub fn with_workspace_root(mut self, root: PathBuf) -> Self {
        self.workspace_root = Some(root);
        self
    }

    fn record_plan_metrics(&self, plan: &Plan) -> Result<(), crate::domain::DomainError> {
        let status = match plan.status {
            crate::planner::PlanStatus::Completed => "completed",
            crate::planner::PlanStatus::Failed => "failed",
            _ => "failed",
        };
        let phases: Vec<String> = plan
            .steps
            .iter()
            .map(|step| step.id.as_str().into())
            .collect();
        let _ = crate::tauri_commands::record_plan_metrics(
            plan.id.as_str().into(),
            status.into(),
            phases,
        );
        Ok(())
    }

    pub fn run_with_cancellation(
        &self,
        input: PipelineInput,
        cancel: Option<&AtomicBool>,
    ) -> Result<PipelineRun> {
        self.run_internal(input, cancel, None)
    }

    pub fn run_with_control(&self, input: PipelineInput, control: &PipelineControl) -> Result<PipelineRun> {
        self.run_internal(input, None, Some(control))
    }

    fn run_internal(
        &self,
        input: PipelineInput,
        cancel: Option<&AtomicBool>,
        control: Option<&PipelineControl>,
    ) -> Result<PipelineRun> {
        let mut plan = self.planner.create_plan(&input.goal).map_err(|e| {
            if input.emit_events {
                self.event_bus.publish(DomainEvent {
                    topic: TOPIC_PLAN_FAILED.into(),
                    payload: serde_json::json!({ "goal": input.goal, "error": e.to_string() }),
                });
            }
            e
        })?;

        let mut events = Vec::new();
        if input.emit_events {
            self.event_bus.publish(DomainEvent {
                topic: TOPIC_PLAN_CREATED.into(),
                payload: serde_json::json!({ "plan_id": plan.id.as_str(), "goal": plan.goal }),
            });
        }

        let mut cancelled = false;
        let mut report_step = |step: &PlanStep, progress: StepProgress| {
            if cancel
                .map(|token| token.load(Ordering::SeqCst))
                .unwrap_or(false)
                || control.is_some_and(PipelineControl::is_cancelling)
            {
                cancelled = true;
                return false;
            }
            if progress == StepProgress::Started && control.is_some_and(|token| !token.wait_if_paused()) {
                cancelled = true;
                return false;
            }
            if !input.emit_events {
                return true;
            }
            let (topic, message) = match progress {
                StepProgress::Started => (
                    TOPIC_STEP_STARTED,
                    format!("Step started: {}", step.description),
                ),
                StepProgress::Completed => (
                    TOPIC_STEP_COMPLETED,
                    format!("Step completed: {}", step.id.as_str()),
                ),
            };
            self.event_bus.publish(DomainEvent {
                topic: topic.into(),
                payload: serde_json::json!({
                    "step_id": step.id.as_str(),
                    "description": step.description,
                }),
            });
            events.push(MonitorEvent {
                ts: now_millis().to_string(),
                severity: "info".into(),
                message,
            });
            true
        };
        self.planner
            .execute_plan_with_progress(&mut plan, self.runtime, &mut report_step);
        if cancelled || control.is_some_and(PipelineControl::is_cancelling) {
            return Err(crate::domain::DomainError::conflict("pipeline cancelled by user"));
        }

        if plan.status != crate::planner::PlanStatus::Completed {
            if input.emit_events {
                self.event_bus.publish(DomainEvent {
                    topic: TOPIC_PLAN_FAILED.into(),
                    payload: serde_json::json!({ "plan_id": plan.id.as_str(), "error": "step execution failed" }),
                });
            }
            return Err(crate::domain::DomainError::internal("pipeline step execution failed"));
        }

        if input.emit_events {
            self.event_bus.publish(DomainEvent {
                topic: TOPIC_PLAN_COMPLETED.into(),
                payload: serde_json::json!({ "plan_id": plan.id.as_str(), "steps": plan.steps.len() }),
            });
            events.push(MonitorEvent {
                ts: now_millis().to_string(),
                severity: "info".into(),
                message: format!("plan {} completed", plan.id),
            });
        }

        let repair_actions = self
            .self_heal
            .attempt_repair(&events)
            .map_err(|e| crate::domain::DomainError::internal(e.to_string()))?;
        let _ = self.record_plan_metrics(&plan);
        let mut engine = LocalPipelineEngine::new(self.event_bus, self.self_heal);
        // Thread the caller's verifier-subprocess consent through to the
        // engine so only `granted` ever spawns supervised commands.
        engine.execution_consent = input.execution_consent;
        let should_cancel = || {
            cancel.is_some_and(|token| token.load(Ordering::SeqCst))
                || control.is_some_and(PipelineControl::is_cancelling)
        };
        if should_cancel() {
            return Err(crate::domain::DomainError::conflict("pipeline cancelled by user"));
        }
        let (generated_files, failed_files, outcome) =
            engine.generate_project_files_cancellable(self.workspace_root.as_deref(), &input.goal, &should_cancel);
        if should_cancel() {
            return Err(crate::domain::DomainError::conflict("pipeline cancelled by user"));
        }
        let verification_results = outcome.verification_results.clone();
        let repair_results = outcome.repair_results.clone();
        let verification_summary = outcome.verification_summary.clone();
        Ok(PipelineRun {
            plan,
            repair_actions,
            events,
            run_id: input.run_id,
            execution_context: None,
            generated_files,
            failed_files,
            outcome: Some(outcome),
            verification_results,
            repair_results,
            verification_summary,
        })
    }
}

impl PipelineService for LocalPipelineService<'_> {
    fn run(&self, input: PipelineInput) -> Result<PipelineRun> {
        self.run_internal(input, None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::DomainError;
    use crate::event_bus::Event;
    use crate::planner::PlanStatus;

    struct CancelAfterPlan<'a>(&'a PipelineControl);

    impl SelfHealService for CancelAfterPlan<'_> {
        fn attempt_repair(&self, _events: &[MonitorEvent]) -> Result<Vec<RepairAction>> {
            self.0.abort();
            Ok(Vec::new())
        }
    }

    #[test]
    fn cancellation_after_plan_prevents_registered_generation() {
        let (event_bus, _receiver) = EventBus::new();
        let planner = LocalPlanner::new();
        let control = PipelineControl::new();
        let healer = CancelAfterPlan(&control);
        let root = tempfile::tempdir().unwrap();
        let service = LocalPipelineService::new(&planner, &LocalAgentRuntime, &event_bus, &healer)
            .with_workspace_root(root.path().to_path_buf());
        let result = service.run_with_control(PipelineInput {
            goal: "build a page".into(), emit_events: false, run_id: "cancel-before-generation".into(),
            project_id: Some("registered".into()),
            execution_consent: crate::pipeline::supervisor::ExecutionConsent::NotSupplied,
        }, &control);
        assert!(result.unwrap_err().to_string().contains("cancelled"));
        assert!(!root.path().join("README.md").exists());
        assert!(!root.path().join("src/main.ts").exists());
    }

    #[test]
    fn service_uses_explicit_managed_workspace_root() {
        let (event_bus, _receiver) = EventBus::new();
        let planner = LocalPlanner::new();
        let root = tempfile::tempdir().unwrap();
        let service = LocalPipelineService::new(
            &planner,
            &LocalAgentRuntime,
            &event_bus,
            &DefaultSelfHealService,
        ).with_workspace_root(root.path().to_path_buf());
        assert_eq!(service.workspace_root.as_deref(), Some(root.path()));
    }

    #[test]
    fn pipeline_creates_and_executes_plan() {
        let (event_bus, _receiver) = EventBus::new();
        let planner = LocalPlanner::new();
        let service = LocalPipelineService::new(
            &planner,
            &LocalAgentRuntime,
            &event_bus,
            &DefaultSelfHealService,
        );
        let run = service
            .run(PipelineInput {
                goal: "build landing page".into(),
                emit_events: true,
                run_id: "run-1".into(),
                project_id: None,
                execution_consent: crate::pipeline::supervisor::ExecutionConsent::NotSupplied,
            })
            .expect("pipeline run");
        assert_eq!(run.plan.status, PlanStatus::Completed);
        assert_eq!(run.events.len(), 5);
        assert!(run.events.iter().any(|event| event.message.starts_with("Step started:")));
        assert!(run.events.iter().any(|event| event.message.starts_with("Step completed:")));
        assert_eq!(run.run_id, "run-1");
    }

    #[test]
    fn pipeline_emits_plan_lifecycle_events() {
        let (event_bus, receiver) = EventBus::new();
        let planner = LocalPlanner::new();
        let service = LocalPipelineService::new(
            &planner,
            &LocalAgentRuntime,
            &event_bus,
            &DefaultSelfHealService,
        );
        let _ = service.run(PipelineInput {
            goal: "wire event bus".into(),
            emit_events: true,
            run_id: "run-1".into(),
            project_id: None,
            execution_consent: crate::pipeline::supervisor::ExecutionConsent::NotSupplied,
        });
        let mut topics = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            topics.push(event.topic().to_string());
        }
        assert_eq!(
            topics,
            vec![
                TOPIC_PLAN_CREATED.to_string(),
                TOPIC_STEP_STARTED.to_string(),
                TOPIC_STEP_COMPLETED.to_string(),
                TOPIC_STEP_STARTED.to_string(),
                TOPIC_STEP_COMPLETED.to_string(),
                TOPIC_PLAN_COMPLETED.to_string()
            ]
        );
    }

    #[test]
    fn pipeline_honors_cancellation_before_executing_a_step() {
        let (event_bus, _receiver) = EventBus::new();
        let planner = LocalPlanner::new();
        let service = LocalPipelineService::new(
            &planner,
            &LocalAgentRuntime,
            &event_bus,
            &DefaultSelfHealService,
        );
        let cancel = AtomicBool::new(true);
        let result = service.run_with_cancellation(
            PipelineInput {
                goal: "cancel this".into(),
                emit_events: true,
                run_id: "run-cancel".into(),
                project_id: None,
                execution_consent: crate::pipeline::supervisor::ExecutionConsent::NotSupplied,
            },
            Some(&cancel),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }

    #[test]
    fn pipeline_propagates_plan_error() {
        struct BrokenPlanner;
        impl Planner for BrokenPlanner {
            fn create_plan(&self, _goal: &str) -> Result<Plan> {
                Err(DomainError::internal("boom"))
            }
            fn execute_plan(&self, _plan: &mut Plan, _runtime: &dyn AgentRuntime) {}
        }
        let (event_bus, receiver) = EventBus::new();
        let service = LocalPipelineService::new(
            &BrokenPlanner,
            &LocalAgentRuntime,
            &event_bus,
            &DefaultSelfHealService,
        );
        let result = service.run(PipelineInput {
            goal: "broken".into(),
            emit_events: true,
            run_id: "run-1".into(),
            project_id: None,
            execution_consent: crate::pipeline::supervisor::ExecutionConsent::NotSupplied,
        });
        assert!(result.is_err());
        let event = receiver.recv().expect("failure event");
        assert_eq!(event.topic(), TOPIC_PLAN_FAILED);
    }

    #[test]
    fn pipeline_does_not_publish_completion_when_aborted_during_pause() {
        use std::sync::Arc;
        use std::time::{Duration, Instant};
        let control = Arc::new(PipelineControl::new());
        assert!(control.pause());
        let worker = Arc::clone(&control);
        let join = std::thread::spawn(move || {
            let (bus, receiver) = EventBus::new();
            let planner = LocalPlanner::new();
            let service = LocalPipelineService::new(&planner, &LocalAgentRuntime, &bus, &DefaultSelfHealService);
            let result = service.run_with_control(PipelineInput {
                goal: "build app".into(), emit_events: true, run_id: "paused-run".into(),
                project_id: None,
                execution_consent: crate::pipeline::supervisor::ExecutionConsent::NotSupplied,
            }, &worker);
            let topics: Vec<_> = receiver.try_iter().map(|event| event.topic().to_string()).collect();
            (result, topics)
        });
        let deadline = Instant::now() + Duration::from_secs(1);
        while control.state() != "paused" && Instant::now() < deadline { std::thread::yield_now(); }
        assert_eq!(control.state(), "paused");
        assert!(control.abort());
        let (result, topics) = join.join().unwrap();
        assert!(result.is_err());
        assert!(!topics.iter().any(|topic| topic == TOPIC_PLAN_COMPLETED));
    }

    #[test]
    fn completed_pipeline_run_reports_truthful_outcome_with_failed_files() {
        let (event_bus, _receiver) = EventBus::new();
        let planner = LocalPlanner::new();
        let service = LocalPipelineService::new(
            &planner,
            &LocalAgentRuntime,
            &event_bus,
            &DefaultSelfHealService,
        );
        let run = service
            .run(PipelineInput {
                goal: "build landing page".into(),
                emit_events: true,
                run_id: "run-1".into(),
                project_id: None,
                execution_consent: crate::pipeline::supervisor::ExecutionConsent::NotSupplied,
            })
            .expect("pipeline run");
        assert_eq!(run.plan.status, PlanStatus::Completed);
        assert_eq!(run.run_id, "run-1");

        let engine = crate::pipeline::engine::LocalPipelineEngine::new(&event_bus, &DefaultSelfHealService);
        let engine_run = engine
            .run_with_cancellation(
                PipelineInput {
                    goal: "build landing page".into(),
                    emit_events: true,
                    run_id: "run-1".into(),
                    project_id: None,
                    execution_consent: crate::pipeline::supervisor::ExecutionConsent::NotSupplied,
                },
                None,
                &|_goal| planner.create_plan("build landing page"),
                &|plan, _progress, _cancel| {
                    planner.execute_plan(plan, &LocalAgentRuntime);
                    Ok(())
                },
            )
            .expect("engine run");
        assert!(engine_run.outcome.is_some(), "truthful outcome should be populated");
        let outcome = engine_run.outcome.expect("outcome");
        assert_eq!(
            outcome.generated_file_count + outcome.failed_file_count,
            engine_run.generated_files.len() + engine_run.failed_files.len()
        );
        assert!(outcome.failed_files.iter().all(|file| !file.path.is_empty()));
    }
}
