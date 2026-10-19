use crate::app_state::AppState;

use crate::agent_runtime::LocalAgentRuntime;
use crate::dev_server::{
    DevServerStatus, DevServerStatusReport, DevServerStatusReporter, LocalDevServerStatusReporter,
};
use crate::gateway::{DefaultGatewayService, GatewayRequest, GatewayResponse, GatewayService};
use crate::git::{GitCommitOutput, GitInitOutput, GitService, GitStatus, LocalGitService};
use crate::persistence::{AgentTaskRunRecord, PlanRecord};
use crate::pipeline::{LocalPipelineService, PipelineControl, PipelineInput, PipelineRun};
use crate::pipeline_history::PipelineRunHistoryRecord;
use crate::planner::{LocalPlanner, Plan, Planner};
use crate::planning_circuit_breaker::{
    PhaseStatus, PlanCircuitBreaker, PlanMetrics, PlanRunStatus,
};
use crate::provider_selection::{
    ProviderCapabilities, ProviderHealthStatus, ProviderSelectionConfig, ProviderSelectionEngine,
};
use crate::provider_settings::{
    ProviderSettings, ProviderSettingsRepository, SqliteProviderSettingsRepository,
};
use crate::runtime_monitor::{
    AgentTaskMetrics, DefaultRuntimeMonitorService, MonitorEvent, RuntimeMonitorService,
};
use crate::self_heal::{DefaultSelfHealService, RepairAction, SelfHealService};
use crate::workspace::{
    registered_workspace_root, safe_file_target, LocalWorkspaceService, PathValidation,
    ProjectFile, WorkspaceService,
};
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Instant, SystemTime};
use tauri::Emitter;
use tauri::Manager;

static PLAN_CIRCUIT_BREAKER: LazyLock<Mutex<PlanCircuitBreaker>> =
    LazyLock::new(|| Mutex::new(PlanCircuitBreaker::new()));
static PROVIDER_SELECTION: LazyLock<Mutex<ProviderSelectionEngine>> = LazyLock::new(|| {
    Mutex::new(ProviderSelectionEngine::new(
        ProviderSelectionConfig::default(),
    ))
});

/// Seconds since the epoch; a clock set before 1970 records 0 rather than
/// panicking inside an IPC command.
fn epoch_secs() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
        .to_string()
}

fn provider_selection() -> &'static Mutex<ProviderSelectionEngine> {
    &PROVIDER_SELECTION
}

/// Lock the active-runs table, recovering from a poisoned lock instead of
/// panicking on a user-driven IPC path: a panic in one command thread must
/// not take down every later pipeline interaction.
fn lock_active_runs()
-> std::sync::MutexGuard<'static, std::collections::HashMap<String, Arc<PipelineControl>>> {
    ACTIVE_RUNS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

static ACTIVE_RUNS: LazyLock<Mutex<std::collections::HashMap<String, Arc<PipelineControl>>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

fn monitor() -> &'static DefaultRuntimeMonitorService {
    crate::runtime_monitor::service()
}

/// Render a typed error at the IPC boundary without discarding its
/// machine-readable code: `[code] actionable message`, e.g.
/// `[invalid_input] symlink path not allowed: src/linked (os error 40)`.
/// The frontend surfaces this string directly, so the code stays in-band
/// instead of being stringified away by `to_string()`.
fn ipc_error(error: crate::domain::DomainError) -> String {
    use crate::domain::DomainError as E;
    let (code, message) = match error {
        E::NotFound { message } => ("not_found", message),
        E::InvalidInput { message } => ("invalid_input", message),
        E::Internal { message } => ("internal", message),
        E::Unauthorized => ("unauthorized", "unauthorized".to_string()),
        E::Conflict { message } => ("conflict", message),
        E::Unavailable { message } => ("unavailable", message),
    };
    format!("[{code}] {message}")
}

#[tauri::command]
pub fn list_agent_task_metrics() -> Result<Vec<AgentTaskMetrics>, String> {
    Ok(monitor().agent_task_metrics())
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SaveAgentTaskRunInput {
    pub id: String,
    pub agent_id: String,
    pub input: String,
    pub success: bool,
    pub output: Option<String>,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub timestamp: i64,
}

#[tauri::command]
pub fn save_agent_task_run(input: SaveAgentTaskRunInput) -> Result<(), String> {
    let record = AgentTaskRunRecord {
        id: input.id,
        agent_id: input.agent_id,
        input: input.input,
        success: input.success,
        output: input.output,
        error: input.error,
        duration_ms: input.duration_ms,
        timestamp: input.timestamp,
    };
    AppState::global()
        .task_runs()
        .insert_run(&record)
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_agent_task_runs(agent_id: String) -> Result<Vec<AgentTaskRunRecord>, String> {
    AppState::global()
        .task_runs()
        .list_runs(&agent_id)
        .map_err(|e| ipc_error(e.into()))
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SavePlanInput {
    pub id: String,
    pub goal: String,
}

#[tauri::command]
pub fn save_plan(input: SavePlanInput) -> Result<(), String> {
    let record = PlanRecord {
        id: input.id,
        goal: input.goal,
        steps: vec![],
    };
    AppState::global()
        .plans()
        .save_plan(&record)
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command]
pub fn list_plans() -> Result<Vec<PlanRecord>, String> {
    AppState::global()
        .plans()
        .list_plans()
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command(rename_all = "snake_case")]
pub fn record_plan_metrics(
    project_id: String,
    status: String,
    phases: Vec<String>,
) -> Result<(), String> {
    let run_status = match status.as_str() {
        "completed" => PlanRunStatus::Completed,
        _ => PlanRunStatus::Failed,
    };
    let phase_status = match run_status {
        PlanRunStatus::Completed => PhaseStatus::Completed,
        PlanRunStatus::Failed => PhaseStatus::Failed,
    };
    let metrics = PlanMetrics {
        status: run_status,
        phases: phases
            .into_iter()
            .map(|phase| (phase, Some(phase_status)))
            .collect(),
    };
    PLAN_CIRCUIT_BREAKER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .load_recent_metrics(&project_id, vec![metrics]);
    Ok(())
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct AttemptSelfHealResponse {
    pub actions: Vec<RepairAction>,
}

#[tauri::command]
pub fn attempt_self_heal() -> Result<AttemptSelfHealResponse, String> {
    let events = monitor().poll_once();
    let actions = DefaultSelfHealService
        .attempt_repair(&events)
        .map_err(|e| ipc_error(e.into()))?;
    Ok(AttemptSelfHealResponse { actions })
}

#[tauri::command]
pub fn attempt_startup_self_heal() -> Result<AttemptSelfHealResponse, String> {
    let events = monitor().poll_once();
    let actions = DefaultSelfHealService
        .attempt_repair(&events)
        .map_err(|e| ipc_error(e.into()))?;
    Ok(AttemptSelfHealResponse { actions })
}

#[tauri::command]
pub fn list_monitor_events() -> Result<Vec<MonitorEvent>, String> {
    Ok(monitor().recent_events())
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct CaptureDiagnosticsInput {
    pub kind: String,
    pub message: String,
    pub detail: Option<String>,
}

#[tauri::command]
pub fn capture_diagnostics(input: CaptureDiagnosticsInput) -> Result<(), String> {
    let detail = input.detail.unwrap_or_default();
    let message = if detail.is_empty() {
        input.message
    } else {
        format!("{}: {}", input.message, detail)
    };
    monitor().record_event(MonitorEvent {
        ts: epoch_secs(),
        severity: "error".into(),
        message,
    });
    Ok(())
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TauriCreateWorkspaceResponse {
    pub id: String,
    pub root: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct CreateManagedWorkspaceInput {
    pub id: String,
}

fn projects_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join("projects"))
        .map_err(|error| format!("app data directory unavailable: {error}"))
}

fn workspace_for_id(app: &tauri::AppHandle, id: &str) -> Result<std::path::PathBuf, String> {
    registered_workspace_root(&projects_dir(app)?, id).map_err(|error| ipc_error(error.into()))
}

#[tauri::command]
pub fn create_workspace(
    app: tauri::AppHandle,
    input: CreateManagedWorkspaceInput,
) -> Result<TauriCreateWorkspaceResponse, String> {
    let service = LocalWorkspaceService;
    let info = service
        .create_workspace(&input.id, &projects_dir(&app)?)
        .map_err(|e| ipc_error(e.into()))?;
    Ok(TauriCreateWorkspaceResponse {
        id: info.id,
        root: info.root.to_string_lossy().to_string(),
    })
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TauriWorkspaceInfo {
    pub id: String,
    pub root: String,
}

#[tauri::command]
pub fn list_workspaces(app: tauri::AppHandle) -> Result<Vec<TauriWorkspaceInfo>, String> {
    let root = projects_dir(&app)?;
    if !root.exists() {
        return Ok(Vec::new());
    }
    let service = LocalWorkspaceService;
    let workspaces = service
        .list_workspaces(&root)
        .map_err(|e| ipc_error(e.into()))?;
    workspaces
        .into_iter()
        .map(|w| {
            registered_workspace_root(&root, &w.id)
                .map(|path| TauriWorkspaceInfo {
                    id: w.id,
                    root: path.to_string_lossy().to_string(),
                })
                .map_err(|error| ipc_error(error.into()))
        })
        .collect()
}

#[tauri::command(rename_all = "snake_case")]
pub fn write_project_file(
    app: tauri::AppHandle,
    workspace_id: String,
    file: ProjectFile,
) -> Result<(), String> {
    let service = LocalWorkspaceService;
    service
        .write_project_file(&workspace_for_id(&app, &workspace_id)?, &file)
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command(rename_all = "snake_case")]
pub fn read_project_file(
    app: tauri::AppHandle,
    workspace_id: String,
    path: String,
) -> Result<Option<ProjectFile>, String> {
    let service = LocalWorkspaceService;
    service
        .read_project_file(&workspace_for_id(&app, &workspace_id)?, &path)
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_project_files(
    app: tauri::AppHandle,
    workspace_id: String,
) -> Result<Vec<ProjectFile>, String> {
    let service = LocalWorkspaceService;
    service
        .list_project_files(&workspace_for_id(&app, &workspace_id)?)
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command(rename_all = "snake_case")]
pub fn validate_workspace_path(
    app: tauri::AppHandle,
    workspace_id: String,
    path: String,
) -> Result<PathValidation, String> {
    match workspace_for_id(&app, &workspace_id)
        .and_then(|root| safe_file_target(&root, &path).map_err(|error| ipc_error(error.into())))
    {
        Ok(_) => Ok(PathValidation {
            valid: true,
            normalized: path,
            issues: Vec::new(),
        }),
        Err(error) => Ok(PathValidation {
            valid: false,
            normalized: String::new(),
            issues: vec![error.to_string()],
        }),
    }
}

#[tauri::command]
#[tracing::instrument(skip(app), fields(run_id = %input.run_id, project_id = %input.project_id.clone().unwrap_or_default()))]
pub async fn run_pipeline(
    app: tauri::AppHandle,
    input: PipelineInput,
) -> Result<PipelineRun, String> {
    let run_id = if input.run_id.trim().is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        input.run_id.clone()
    };
    if run_id.len() > 128
        || !run_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(
            "[invalid_input] pipeline run id is required: pass the id returned by run_pipeline"
                .into(),
        );
    }
    let workspace_root = input
        .project_id
        .as_deref()
        .map(|id| workspace_for_id(&app, id))
        .transpose()?;
    let build_requested = workspace_root.is_some();
    // Capture only after workspace_for_id succeeds. Never expose the native root
    // or accept provenance from generated output or the webview.
    let execution_context = crate::pipeline::RunExecutionContext {
        schema_version: 1,
        target: match input.project_id.clone() {
            Some(project_id) => crate::pipeline::RunTarget::Workspace { project_id },
            None => crate::pipeline::RunTarget::Simulation,
        },
    };
    let goal = input.goal.clone();
    let control = Arc::new(PipelineControl::new());
    {
        let mut active_runs = lock_active_runs();
        if active_runs.contains_key(&run_id) {
            return Err(format!("pipeline run already active: {run_id}"));
        }
        AppState::global()
            .pipeline_history()
            .record(&PipelineRunHistoryRecord {
                id: run_id.clone(),
                goal: goal.clone(),
                status: "running".into(),
                step_count: 0,
                repair_count: 0,
                success: false,
                error: None,
                finished_at: std::time::SystemTime::now(),
            })
            .map_err(|error| ipc_error(error.into()))?;
        active_runs.insert(run_id.clone(), control.clone());
    }

    let plan_revision_id = uuid::Uuid::new_v4().to_string();
    if let Err(error) =
        AppState::global()
            .plan_revisions()
            .insert(&crate::pipeline::identity::PlanRevision {
                schema_version: 1,
                revision_id: plan_revision_id.clone(),
                plan_id: run_id.clone(),
                previous_revision: None,
                created_at_millis: std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or_default(),
                status: crate::pipeline::identity::RevisionStatus::Draft,
            })
    {
        let _ = capture_diagnostics(CaptureDiagnosticsInput {
            kind: "database".into(),
            message: "plan revision persistence failed".into(),
            detail: Some(error.to_string()),
        });
    }
    if let Some(workspace_root) = workspace_root.as_ref() {
        let projects_dir = workspace_root
            .parent()
            .map(Path::new)
            .unwrap_or_else(|| Path::new("."));
        let registered_id = registered_workspace_root(
            projects_dir,
            input.project_id.as_deref().unwrap_or_default(),
        )
        .ok()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
        let grant_id = uuid::Uuid::new_v4().to_string();
        if let Err(error) = AppState::global().workspace_grants().insert(
            &crate::pipeline::identity::WorkspaceGrant {
                schema_version: 1,
                grant_id: grant_id.clone(),
                project_id: registered_id,
                created_at_millis: std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or_default(),
            },
        ) {
            let _ = capture_diagnostics(CaptureDiagnosticsInput {
                kind: "database".into(),
                message: "workspace grant persistence failed".into(),
                detail: Some(error.to_string()),
            });
        }
        if let Err(error) =
            AppState::global()
                .run_identities()
                .insert(&crate::pipeline::identity::RunIdentity {
                    schema_version: 1,
                    run_id: run_id.clone(),
                    plan_revision_id: plan_revision_id.clone(),
                    workspace_grant_id: grant_id,
                    created_at_millis: std::time::SystemTime::now()
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or_default(),
                })
        {
            let _ = capture_diagnostics(CaptureDiagnosticsInput {
                kind: "database".into(),
                message: "run identity persistence failed".into(),
                detail: Some(error.to_string()),
            });
        }
    }

    let (event_bus, receiver) = crate::event_bus::EventBus::new();
    let app_for_events = app.clone();
    let event_run_id = run_id.clone();
    let forwarder = std::thread::spawn(move || {
        while let Ok(event) = receiver.recv() {
            let payload =
                crate::event_bus::PipelineEvent::from_domain_event(&event, event_run_id.clone());
            let _ = app_for_events.emit("pipeline-event", payload);
        }
    });

    let service_input = PipelineInput {
        run_id: run_id.clone(),
        ..input
    };
    let control_for_run = control.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let planner = LocalPlanner::new();
        let service = LocalPipelineService::new(
            &planner,
            &LocalAgentRuntime,
            &event_bus,
            &DefaultSelfHealService,
        );
        let service = match workspace_root {
            Some(root) => service.with_workspace_root(root),
            None => service,
        };
        service
            .run_with_control(service_input, control_for_run.as_ref())
            .map(|mut run| {
                run.execution_context = Some(execution_context);
                run
            })
    })
    .await;
    let _ = forwarder.join();
    let cancelled = control.is_cancelling();
    let completed_run = match &result {
        Ok(Ok(run))
            if !cancelled && matches!(run.plan.status, crate::planner::PlanStatus::Completed) =>
        {
            Some(run)
        }
        _ => None,
    };
    let serialized_run = completed_run.and_then(|run| serde_json::to_string(run).ok());
    let stored_result = serialized_run
        .as_deref()
        .filter(|json| json.len() <= 1_048_576);
    let build_succeeded = completed_run.is_some_and(|run| {
        pipeline_build_succeeded(
            build_requested,
            run.outcome.as_ref().map(|outcome| outcome.success),
        )
    });
    let stored_success = build_succeeded && stored_result.is_some();
    let error = if cancelled {
        Some("pipeline cancelled by user".to_string())
    } else if completed_run.is_some() && stored_result.is_none() {
        Some("pipeline result could not be persisted".into())
    } else if completed_run.is_some() && !build_succeeded {
        Some(
            completed_run
                .and_then(|run| run.outcome.as_ref().map(|outcome| outcome.summary.clone()))
                .unwrap_or_else(|| "pipeline build outcome unavailable".into()),
        )
    } else {
        match &result {
            Ok(Err(error)) => Some(error.to_string()),
            Err(error) => Some(error.to_string()),
            Ok(Ok(run)) if !matches!(run.plan.status, crate::planner::PlanStatus::Completed) => {
                Some("pipeline plan did not complete".into())
            }
            _ => None,
        }
    };
    let terminal = PipelineRunHistoryRecord {
        id: run_id.clone(),
        goal,
        status: if cancelled {
            "cancelled"
        } else if stored_success {
            "completed"
        } else {
            "failed"
        }
        .into(),
        step_count: completed_run.map_or(0, |run| run.plan.steps.len() as u64),
        repair_count: completed_run.map_or(0, |run| run.repair_actions.len() as u64),
        success: stored_success,
        error,
        finished_at: std::time::SystemTime::now(),
    };
    let persisted = AppState::global()
        .pipeline_history()
        .finish(&terminal, stored_result);
    lock_active_runs().remove(&run_id);

    persisted.map_err(|error| ipc_error(error.into()))?;
    if cancelled {
        return Err("[cancelled] pipeline run was cancelled by user request".into());
    }
    // spawn_blocking join failure is a runtime/tauri::Error, not a DomainError
    let run = result
        .map_err(|error| format!("[internal] {error}"))?
        .map_err(ipc_error)?;
    if !matches!(run.plan.status, crate::planner::PlanStatus::Completed) {
        return Err("[internal] pipeline plan did not complete; check the pipeline events for the failing stage".into());
    }
    if stored_result.is_none() {
        return Err("[internal] pipeline result could not be persisted; check recode.db availability and diagnostics".into());
    }
    if build_requested && run.outcome.is_none() {
        return Err("[internal] pipeline build outcome unavailable for this run; re-run to produce a fresh result".into());
    }
    Ok(run)
}

fn pipeline_build_succeeded(build_requested: bool, outcome_success: Option<bool>) -> bool {
    !build_requested || outcome_success == Some(true)
}

#[tauri::command(rename_all = "snake_case")]
#[tracing::instrument(fields(run_id = %run_id))]
pub fn cancel_pipeline(run_id: String) -> Result<bool, String> {
    let active_runs = lock_active_runs();
    if let Some(control) = active_runs.get(&run_id) {
        Ok(control.abort())
    } else {
        Ok(false)
    }
}

/// Report of the workspace reconciliation command (ROADMAP 1.1): every
/// reconciler's result plus an aggregate summary.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReconciliationReport {
    pub results: Vec<crate::workspace::reconciler::ReconciliationResult>,
    pub modified_count: usize,
    pub summary: String,
}

#[tauri::command(rename_all = "snake_case")]
pub fn reconcile_workspace(
    app: tauri::AppHandle,
    project_id: String,
) -> Result<ReconciliationReport, String> {
    let root = workspace_for_id(&app, &project_id)?;
    let results = crate::workspace::reconciler::ReconciliationPipeline::default().run(&root);
    let modified_count = results.iter().filter(|result| result.modified).count();
    let summary = results
        .iter()
        .map(|result| format!("{}: {}", result.reconciler, result.summary))
        .collect::<Vec<_>>()
        .join("; ");
    Ok(ReconciliationReport {
        results,
        modified_count,
        summary,
    })
}

/// Result of the consented dependency-install command (ROADMAP 1.1, F07).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstallResult {
    /// Human-readable note; empty when nothing ran (already installed, no
    /// package.json, or consent not granted).
    pub note: String,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn install_dependencies(
    app: tauri::AppHandle,
    project_id: String,
    consent: crate::pipeline::supervisor::ExecutionConsent,
) -> Result<InstallResult, String> {
    let root = workspace_for_id(&app, &project_id)?;
    // npm install can run for minutes; keep it off the main thread.
    let note = tauri::async_runtime::spawn_blocking(move || {
        crate::pipeline::verifiers::install_step(&root, consent)
    })
    .await
    .map_err(|error| format!("[internal] {error}"))?;
    Ok(InstallResult { note })
}

/// Report of the standalone build-verification command (ROADMAP 1.3): the
/// per-gate results with supervised execution receipts plus a summary.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BuildVerificationReport {
    pub results: Vec<crate::pipeline::verify::VerificationResult>,
    pub summary: String,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn verify_build(
    app: tauri::AppHandle,
    project_id: String,
    consent: crate::pipeline::supervisor::ExecutionConsent,
) -> Result<BuildVerificationReport, String> {
    let root = workspace_for_id(&app, &project_id)?;
    // Verify the current on-disk workspace state: list real files and mark
    // them Written so the persistence stage re-reads and compares bytes.
    let files = LocalWorkspaceService
        .list_project_files(&root)
        .map_err(|error| ipc_error(error.into()))?;
    let report = tauri::async_runtime::spawn_blocking(move || {
        let generated: Vec<crate::pipeline::types::GeneratedFile> = files
            .into_iter()
            .map(|file| crate::pipeline::types::GeneratedFile {
                path: file.path,
                content: file.content,
                language: file.language,
                status: crate::pipeline::types::GeneratedFileStatus::Written,
                error: None,
            })
            .collect();
        // Standalone engine, mirroring the per-run construction in
        // LocalPipelineService::run_with_control. run_verification only
        // probes runners under granted consent, so the bus stays quiet.
        let (bus, _receiver) = crate::event_bus::EventBus::new();
        let heal = DefaultSelfHealService;
        let engine = crate::pipeline::engine::LocalPipelineEngine::new(&bus, &heal);
        let (results, summary) = engine.run_verification(Some(root.as_path()), &generated, consent);
        BuildVerificationReport { results, summary }
    })
    .await
    .map_err(|error| format!("[internal] {error}"))?;
    Ok(report)
}

#[tauri::command]
pub fn create_and_execute_plan(input: SavePlanInput) -> Result<Plan, String> {
    let planner = LocalPlanner::new();
    let mut plan = planner
        .create_plan(&input.goal)
        .map_err(|e| ipc_error(e.into()))?;
    planner.execute_plan(&mut plan, &LocalAgentRuntime);
    let record = PlanRecord {
        id: plan.id.as_str().into(),
        goal: plan.goal.clone(),
        steps: plan
            .steps
            .clone()
            .into_iter()
            .map(|step| crate::persistence::PlanStepRecord {
                id: step.id.as_str().into(),
                description: step.description,
                status: format!("{:?}", step.status).to_lowercase(),
            })
            .collect(),
    };
    AppState::global()
        .plans()
        .save_plan(&record)
        .map_err(|e| ipc_error(e.into()))?;
    Ok(plan)
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ReportDevServerStatusInput {
    pub port: Option<u16>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct QualityScanInput {
    pub root: String,
    pub include_tests: bool,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct QualityBaselineInput {
    pub file_path: String,
    pub content: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ListQualityBaselinesInput {
    pub root: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct DeleteQualityBaselineInput {
    pub file_path: String,
}

#[tauri::command]
pub fn report_dev_server_status(
    input: ReportDevServerStatusInput,
) -> Result<DevServerStatusReport, String> {
    let reporter = LocalDevServerStatusReporter;
    let report = reporter.report_status(input.port);
    if matches!(report.status, DevServerStatus::Failed) {
        let _ = capture_diagnostics(super::tauri_commands::CaptureDiagnosticsInput {
            kind: "devserver".into(),
            message: report.message.clone(),
            detail: None,
        });
    }
    Ok(report)
}

#[tauri::command]
pub fn run_quality_scan(input: QualityScanInput) -> Result<crate::quality::QualityReport, String> {
    let scanner = crate::quality::QualityScanner::new(crate::quality::SqliteQualityRepository);
    let conn = AppState::global().db();
    let options = crate::quality::ScanOptions {
        root: std::path::PathBuf::from(input.root),
        include_tests: input.include_tests,
        max_critical_findings: 0,
        max_warning_findings: 10,
    };
    scanner
        .scan(&conn, std::path::Path::new(&options.root), &options)
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command]
pub fn record_quality_baseline(input: QualityBaselineInput) -> Result<(), String> {
    let scanner = crate::quality::QualityScanner::new(crate::quality::SqliteQualityRepository);
    let conn = AppState::global().db();
    scanner
        .record_baseline(&conn, &input.file_path, &input.content)
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command]
pub fn list_quality_baselines(
    _input: ListQualityBaselinesInput,
) -> Result<Vec<crate::quality::QualityBaseline>, String> {
    let scanner = crate::quality::QualityScanner::new(crate::quality::SqliteQualityRepository);
    let conn = AppState::global().db();
    scanner.list_baselines(&conn).map_err(|e| ipc_error(e.into()))
}

#[tauri::command]
pub fn delete_quality_baseline(input: DeleteQualityBaselineInput) -> Result<(), String> {
    let scanner = crate::quality::QualityScanner::new(crate::quality::SqliteQualityRepository);
    let conn = AppState::global().db();
    scanner
        .delete_baseline(&conn, &input.file_path)
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command]
pub fn git_status(root: String) -> Result<GitStatus, String> {
    LocalGitService
        .status(std::path::Path::new(&root))
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command]
pub fn git_init(root: String) -> Result<GitInitOutput, String> {
    LocalGitService
        .init(std::path::Path::new(&root))
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command]
pub fn git_add_all(root: String) -> Result<(), String> {
    LocalGitService
        .add_all(std::path::Path::new(&root))
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command]
pub fn git_commit(root: String, message: String) -> Result<GitCommitOutput, String> {
    LocalGitService
        .commit(std::path::Path::new(&root), &message)
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command]
pub fn create_team_task(input: crate::agent_team::TeamTaskRecord) -> Result<(), String> {
    AppState::global()
        .agent_team()
        .create_task(&input)
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command]
pub fn list_team_tasks(team_id: String) -> Result<Vec<crate::agent_team::TeamTaskRecord>, String> {
    AppState::global()
        .agent_team()
        .list_tasks(&team_id)
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command]
pub fn record_agent_task_history(
    input: crate::agent_task_history::AgentTaskHistoryRecord,
) -> Result<(), String> {
    AppState::global()
        .agent_task_history()
        .record(&input)
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_agent_task_history(
    agent_id: String,
) -> Result<Vec<crate::agent_task_history::AgentTaskHistoryRecord>, String> {
    AppState::global()
        .agent_task_history()
        .list_by_agent(&agent_id)
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command]
pub fn ensure_learning_schema() -> Result<(), String> {
    AppState::global()
        .learning()
        .ensure_schema()
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command]
pub fn record_success_pattern(
    input: crate::app_state::SuccessPatternInput,
) -> Result<crate::app_state::SuccessPatternOutput, String> {
    AppState::global()
        .learning()
        .record_success_pattern(input)
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command]
pub fn record_anti_pattern(
    input: crate::app_state::AntiPatternInput,
) -> Result<crate::app_state::AntiPatternOutput, String> {
    AppState::global()
        .learning()
        .record_anti_pattern(input)
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_success_patterns(
    agent_role: String,
    task_type: String,
) -> Result<Vec<crate::learning::LearningPatternRecord>, String> {
    AppState::global()
        .learning()
        .list_success_patterns(&agent_role, &task_type)
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_anti_patterns(
    agent_role: String,
    task_type: String,
) -> Result<Vec<crate::learning::LearningPatternRecord>, String> {
    AppState::global()
        .learning()
        .list_anti_patterns(&agent_role, &task_type)
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command]
pub fn record_pipeline_run(record: PipelineRunHistoryRecord) -> Result<(), String> {
    AppState::global()
        .pipeline_history()
        .record(&record)
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command]
pub fn list_pipeline_runs(limit: u64) -> Result<Vec<PipelineRunHistoryRecord>, String> {
    AppState::global()
        .pipeline_history()
        .list_runs(limit)
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command]
pub fn run_gateway_task(input: GatewayRequest) -> Result<GatewayResponse, String> {
    crate::gateway::default_gateway()
        .send(input)
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command]
pub fn get_gateway_status() -> Result<String, String> {
    let gateway = crate::gateway::default_gateway();
    if !gateway.is_configured() {
        return Ok("Unconfigured".into());
    }
    let state = gateway
        .circuit_breaker_state()
        .map(|value| format!("{:?}", value))
        .unwrap_or_else(|| "unknown".into());
    Ok(state)
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ProviderSelectionStatusResponse {
    pub providers: Vec<ProviderSelectionProviderResponse>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ProviderSelectionProviderResponse {
    pub id: String,
    pub status: ProviderHealthStatus,
    pub consecutive_failures: u32,
}

#[tauri::command]
pub fn get_provider_selection_status() -> Result<ProviderSelectionStatusResponse, String> {
    let settings = SqliteProviderSettingsRepository::load_providers(&AppState::global().db())
        .unwrap_or_default();
    let mut engine = provider_selection()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for setting in settings.into_iter().filter(|setting| setting.enabled) {
        if engine.health_status(&setting.id).is_none() {
            engine.register_provider(ProviderCapabilities {
                id: setting.id,
                supports_streaming: false,
                supports_tools: false,
                preferred_task_types: Vec::new(),
            });
        }
    }
    let mut provider_ids = engine.registered_providers();
    provider_ids.sort();
    let providers = provider_ids
        .into_iter()
        .map(|id| ProviderSelectionProviderResponse {
            status: engine
                .health_status(&id)
                .unwrap_or(ProviderHealthStatus::Unknown),
            consecutive_failures: engine.consecutive_failures(&id).unwrap_or(0),
            id,
        })
        .collect();
    Ok(ProviderSelectionStatusResponse { providers })
}

#[tauri::command]
pub fn save_provider(provider: ProviderSettings) -> Result<(), String> {
    SqliteProviderSettingsRepository::save_provider(&AppState::global().db(), &provider)
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command]
pub fn list_providers() -> Result<Vec<ProviderSettings>, String> {
    SqliteProviderSettingsRepository::load_providers(&AppState::global().db())
        .map_err(|e| ipc_error(e.into()))
}

#[tauri::command]
pub fn delete_provider(id: String) -> Result<(), String> {
    AppState::global()
        .db()
        .execute("DELETE FROM provider_settings WHERE id = ?1", [id])
        .map_err(|e| ipc_error(e.into()))
        .map(|_| ())
}

// ---------------------------------------------------------------------------
// Dynamic provider / model management: catalog, discovery, routing,
// credentials, and the dynamic orchestrator. All synchronous (crate has no
// tokio). Credential values never cross IPC: they are resolved server-side
// (stored credential or env) and only redacted hints are returned.
// ---------------------------------------------------------------------------

pub fn credentials_db_path() -> String {
    // Same file the global app state opens (mirrors save_provider semantics).
    "recode.db".to_string()
}

/// Persist the in-memory model catalog back to disk so edits and discoveries
/// survive an app restart. Best-effort: a failed write is logged, not surfaced
/// to the caller, because the in-memory catalog is already authoritative for
/// the running session.
fn persist_model_catalog() {
    let state = AppState::global();
    let catalog = state.model_catalog();
    let conn = state.db();
    if let Err(e) = catalog.save(&conn) {
        eprintln!("[recode] failed to persist model catalog: {e}");
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertModelInput {
    pub provider: String,
    pub model_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub family: Option<String>,
    #[serde(default)]
    pub context_window: Option<u32>,
    #[serde(default)]
    pub max_output_tokens: Option<u32>,
    #[serde(default)]
    pub input_modalities: Option<Vec<crate::model_catalog::Modality>>,
    #[serde(default)]
    pub supports_tools: bool,
    #[serde(default)]
    pub supports_structured_output: bool,
    #[serde(default)]
    pub supports_reasoning: bool,
    #[serde(default)]
    pub cost_tier: Option<crate::model_catalog::CostTier>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub strengths: Option<Vec<String>>,
    #[serde(default)]
    pub task_affinity: Option<std::collections::HashMap<String, f32>>,
    #[serde(default)]
    pub prompt_price_per_mtok: Option<f64>,
    #[serde(default)]
    pub completion_price_per_mtok: Option<f64>,
}

#[tauri::command]
pub fn list_models(
    provider: Option<String>,
) -> Result<Vec<crate::model_catalog::ModelInfo>, String> {
    let catalog = AppState::global().model_catalog();
    Ok(match provider {
        Some(p) if !p.is_empty() => catalog.for_provider(&p),
        _ => catalog.all(),
    })
}

#[tauri::command]
pub fn upsert_model(input: UpsertModelInput) -> Result<crate::model_catalog::ModelInfo, String> {
    use crate::model_catalog::*;
    let model_id = input.model_id.trim();
    if model_id.is_empty() {
        return Err("[invalid_input] model_id must not be empty; select a model first".into());
    }
    let provider = input.provider.trim();
    if provider.is_empty() {
        return Err("[invalid_input] provider must not be empty; select a provider first".into());
    }
    let id = format!("{}/{}", provider, model_id);
    let name = input
        .name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| model_id.to_string());
    let model = ModelInfo {
        id: id.clone(),
        provider: provider.to_string(),
        model_id: model_id.to_string(),
        name,
        family: input
            .family
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| provider.to_string()),
        context_window: input.context_window.unwrap_or(128_000),
        max_output_tokens: input.max_output_tokens.unwrap_or(8_192),
        input_modalities: input
            .input_modalities
            .unwrap_or_else(|| vec![Modality::Text]),
        supports_tools: input.supports_tools,
        supports_structured_output: input.supports_structured_output,
        supports_reasoning: input.supports_reasoning,
        cost_tier: input.cost_tier.unwrap_or(CostTier::Medium),
        description: input.description.unwrap_or_default(),
        strengths: input.strengths.unwrap_or_default(),
        task_affinity: input.task_affinity.unwrap_or_default(),
        prompt_price_per_mtok: input.prompt_price_per_mtok,
        completion_price_per_mtok: input.completion_price_per_mtok,
        discovered: false,
    };
    AppState::global().model_catalog().upsert(model.clone());
    persist_model_catalog();
    Ok(model)
}

#[tauri::command]
pub fn remove_model(id: String) -> Result<bool, String> {
    let removed = AppState::global().model_catalog().get(&id).is_some();
    AppState::global().model_catalog().remove(&id);
    persist_model_catalog();
    Ok(removed)
}

#[tauri::command]
pub fn discover_provider_models(
    provider: String,
) -> Result<crate::model_discovery::DiscoveryResult, String> {
    let catalog = AppState::global().model_catalog();
    let db = credentials_db_path();
    Ok(crate::model_discovery::discover_provider(
        &provider, catalog, None, None, &db,
    ))
}

#[tauri::command]
pub fn import_env_api_key(
    provider: String,
) -> Result<crate::provider_credentials::ProviderCredentialInfo, String> {
    let db = credentials_db_path();
    let ep = crate::model_discovery::endpoint_for(&provider)
        .ok_or_else(|| format!("no known endpoint for provider '{provider}'"))?;
    let key = std::env::var(ep.env_var)
        .map_err(|_| format!("{} is not set in the app process environment", ep.env_var))?;
    if key.trim().is_empty() {
        return Err(format!("{} is set but empty", ep.env_var));
    }
    crate::provider_credentials::save_api_key(&db, &provider, &key).map_err(|e| format!("[internal] {e}"))?;
    crate::gateway::refresh_gateway();
    Ok(crate::provider_credentials::list_infos(&db)
        .remove(&provider)
        .ok_or_else(|| "imported credential not found after save".to_string())?)
}

#[tauri::command]
pub fn list_provider_credentials() -> Result<
    std::collections::HashMap<String, crate::provider_credentials::ProviderCredentialInfo>,
    String,
> {
    Ok(crate::provider_credentials::list_infos(
        &credentials_db_path(),
    ))
}

#[tauri::command]
pub fn save_provider_config(
    provider: String,
    base_url: Option<String>,
    default_model: Option<String>,
) -> Result<(), String> {
    crate::provider_credentials::save_config(
        &credentials_db_path(),
        &provider,
        base_url.as_deref(),
        default_model.as_deref(),
    )?;
    crate::gateway::refresh_gateway();
    Ok(())
}

/// Store (or overwrite) a provider's API key that the user typed into the GUI.
/// Mirrors `import_env_api_key`, but takes the key as an argument instead of
/// reading it from the process environment. The key is written to the local
/// credential store only; only a redacted `ProviderCredentialInfo` (hint +
/// has-key flag) crosses IPC back to the UI.
#[tauri::command]
pub fn save_provider_api_key(
    provider: String,
    api_key: String,
) -> Result<crate::provider_credentials::ProviderCredentialInfo, String> {
    let provider = provider.trim();
    if provider.is_empty() {
        return Err("[invalid_input] provider must not be empty; select a provider first".into());
    }
    let db = credentials_db_path();
    crate::provider_credentials::save_api_key(&db, provider, &api_key)
        .map_err(|e| format!("[internal] {e}"))?;
    crate::gateway::refresh_gateway();
    Ok(crate::provider_credentials::list_infos(&db)
        .remove(provider)
        .ok_or_else(|| "stored credential not found after save".to_string())?)
}

#[tauri::command]
pub fn delete_provider_api_key(provider: String) -> Result<bool, String> {
    let deleted = crate::provider_credentials::delete_api_key(&credentials_db_path(), &provider)?;
    crate::gateway::refresh_gateway();
    Ok(deleted)
}

// ---------------------------------------------------------------------------
// Provider key verification, model selection, and concurrency calibration.
// The key is resolved server-side (stored credential or env) and never crosses
// IPC; only redacted status/limits/selection state are returned.
// ---------------------------------------------------------------------------

/// Verify a provider's stored key by probing its models endpoint, then persist
/// the status snapshot the selection UI displays.
#[tauri::command]
pub fn verify_provider_key(
    provider: String,
) -> Result<crate::provider_verification::KeyVerificationResult, String> {
    let result =
        crate::provider_verification::verify_provider_key(&provider, &credentials_db_path());
    let conn = AppState::global().db();
    if let Err(error) = crate::model_selection::save_key_status(
        &conn,
        &provider,
        &result.status,
        &result.message,
        result.checked_at_ms,
    ) {
        let _ = capture_diagnostics(CaptureDiagnosticsInput {
            kind: "database".into(),
            message: "provider key status persistence failed".into(),
            detail: Some(error.to_string()),
        });
    }
    Ok(result)
}

/// Calibrate a model's max concurrency (ladder of probes until rate-limited),
/// persist the limit, and feed it into the provider-selection engine as a
/// per-provider in-flight override.
#[tauri::command]
pub fn calibrate_model_concurrency(
    provider: String,
    model_id: String,
) -> Result<crate::provider_verification::ConcurrencyCalibrationResult, String> {
    let result = crate::provider_verification::calibrate_model_concurrency(
        &provider,
        &model_id,
        &credentials_db_path(),
    );
    if result.ok {
        let conn = AppState::global().db();
        if let Err(error) = crate::model_selection::save_concurrency_limit(
            &conn,
            &format!("{provider}/{model_id}"),
            result.limit,
        ) {
            let _ = capture_diagnostics(CaptureDiagnosticsInput {
                kind: "database".into(),
                message: "concurrency limit persistence failed".into(),
                detail: Some(error.to_string()),
            });
        }
        let mut engine = provider_selection()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        engine.set_max_in_flight_override(&provider, Some(result.limit));
    }
    Ok(result)
}

/// Return the persisted selection state (selected models, disabled providers,
/// calibrated limits, last key-verification snapshot).
#[tauri::command]
pub fn get_selection_state() -> Result<crate::model_selection::SelectionState, String> {
    let conn = AppState::global().db();
    crate::model_selection::load_selection_state(&conn).map_err(|e| ipc_error(e.into()))
}

/// Toggle a model's selected flag and return the refreshed selection state.
#[tauri::command]
pub fn set_model_selected(
    model_id: String,
    selected: bool,
) -> Result<crate::model_selection::SelectionState, String> {
    let conn = AppState::global().db();
    crate::model_selection::set_model_selected(&conn, &model_id, selected)
        .map_err(|e| ipc_error(e.into()))?;
    crate::model_selection::load_selection_state(&conn).map_err(|e| ipc_error(e.into()))
}

/// Enable/disable a provider for routing and return the refreshed selection state.
#[tauri::command]
pub fn set_provider_enabled(
    provider: String,
    enabled: bool,
) -> Result<crate::model_selection::SelectionState, String> {
    let conn = AppState::global().db();
    crate::model_selection::set_provider_enabled(&conn, &provider, enabled)
        .map_err(|e| ipc_error(e.into()))?;
    crate::model_selection::load_selection_state(&conn).map_err(|e| ipc_error(e.into()))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteModelsInput {
    /// Free-form description of the task (inferred into a routing request).
    #[serde(default)]
    pub task: Option<String>,
    /// Explicit routing request; takes precedence over `task` inference.
    #[serde(default)]
    pub request: Option<crate::model_router::RoutingRequest>,
}

#[tauri::command]
pub fn route_models(
    input: RouteModelsInput,
) -> Result<crate::model_router::RoutingDecision, String> {
    let state = AppState::global();
    let catalog = state.model_catalog();
    // Route within the user's selection (selected models + enabled providers);
    // `route_candidates` falls back to the whole catalog when nothing is selected.
    let selection = {
        let conn = state.db();
        crate::model_selection::load_selection_state(&conn).map_err(|e| ipc_error(e.into()))?
    };
    let candidates = crate::model_selection::route_candidates(&selection, &catalog.all());
    let request = match input.request {
        Some(r) => r,
        None => {
            let desc = input.task.unwrap_or_default();
            crate::model_router::infer_request(&desc)
        }
    };
    Ok(crate::model_router::ModelRouter::new().route_live_filtered(catalog, &candidates, &request))
}

#[tauri::command]
pub fn infer_routing_request(task: String) -> Result<crate::model_router::RoutingRequest, String> {
    Ok(crate::model_router::infer_request(&task))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestratorStepInput {
    pub goal: String,
    pub step: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestratorStepResponse {
    pub decision: crate::dynamic_orchestrator::OrchestratorDecision,
    pub success: bool,
    pub output: serde_json::Value,
    pub error: Option<String>,
}

#[tauri::command]
pub fn run_orchestrator_step(
    input: OrchestratorStepInput,
) -> Result<OrchestratorStepResponse, String> {
    use crate::agent_runtime::LocalAgentRuntime;
    let state = AppState::global();
    let orchestrator = crate::dynamic_orchestrator::DynamicOrchestrator::default();
    let runtime = LocalAgentRuntime;
    let conn = state.db();
    let selection = crate::model_selection::load_selection_state(&conn).unwrap_or_default();
    drop(conn);
    let (result, decision) = orchestrator.dispatch(
        &input.goal,
        &input.step,
        state.model_catalog(),
        &selection,
        &runtime,
        None,
        "interactive",
    );
    match result {
        Ok(agent_result) => Ok(OrchestratorStepResponse {
            decision,
            success: agent_result.success,
            output: agent_result.output,
            error: agent_result.error,
        }),
        Err(e) => Ok(OrchestratorStepResponse {
            decision,
            success: false,
            output: serde_json::Value::Null,
            error: Some(e.to_string()),
        }),
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestratorAutonomousInput {
    pub goal: String,
    pub initial_step: Option<String>,
    #[serde(default)]
    pub max_steps: Option<usize>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestratorAutonomousResponse {
    pub completed: bool,
    pub aborted: bool,
    pub steps: Vec<String>,
}

/// Run the dynamic orchestrator autonomously: it decides, dispatches, and
/// re-decides each step until the goal is judged complete, a step fails, or
/// the step budget is exhausted. Budget is clamped to 1..=10.
#[tauri::command]
pub fn run_orchestrator_autonomous(
    input: OrchestratorAutonomousInput,
) -> Result<OrchestratorAutonomousResponse, String> {
    use crate::agent_runtime::LocalAgentRuntime;
    let state = AppState::global();
    let orchestrator = crate::dynamic_orchestrator::DynamicOrchestrator::default();
    let runtime = LocalAgentRuntime;
    let max_steps = input.max_steps.unwrap_or(5).clamp(1, 10);
    let conn = state.db();
    let selection = crate::model_selection::load_selection_state(&conn).unwrap_or_default();
    drop(conn);
    let report = orchestrator.run_until_done(
        &input.goal,
        input.initial_step.as_deref().unwrap_or(""),
        max_steps,
        state.model_catalog(),
        &selection,
        &runtime,
        None,
        "interactive",
    );
    Ok(OrchestratorAutonomousResponse {
        completed: report.completed,
        aborted: report.aborted,
        steps: report.steps,
    })
}

#[tauri::command(rename_all = "snake_case")]
#[tracing::instrument(fields(run_id = %run_id))]
pub fn pause_pipeline(run_id: String) -> Result<bool, String> {
    let active_runs = lock_active_runs();
    if let Some(control) = active_runs.get(&run_id) {
        Ok(control.pause())
    } else {
        Ok(false)
    }
}

#[tauri::command(rename_all = "snake_case")]
#[tracing::instrument(fields(run_id = %run_id))]
pub fn resume_pipeline(run_id: String) -> Result<bool, String> {
    let active_runs = lock_active_runs();
    Ok(active_runs
        .get(&run_id)
        .is_some_and(|control| control.resume()))
}

#[tauri::command(rename_all = "snake_case")]
#[tracing::instrument(fields(run_id = %run_id))]
pub fn abort_pipeline(run_id: String) -> Result<bool, String> {
    let active_runs = lock_active_runs();
    if let Some(control) = active_runs.get(&run_id) {
        Ok(control.abort())
    } else {
        Ok(false)
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct PipelineStatusResponse {
    pub run_id: String,
    pub active: bool,
    pub state: String,
}

fn persisted_pipeline_state(record: Option<&PipelineRunHistoryRecord>) -> &'static str {
    match record.map(|run| run.status.as_str()) {
        Some("completed") => "completed",
        Some("cancelled") => "cancelled",
        Some("failed") => "failed",
        // A running row without a local control may belong to an interrupted run or another process.
        _ => "unknown",
    }
}

#[tauri::command(rename_all = "snake_case")]
#[tracing::instrument(fields(run_id = %run_id))]
pub fn get_pipeline_status(run_id: String) -> Result<PipelineStatusResponse, String> {
    let active_runs = lock_active_runs();
    let state = active_runs.get(&run_id).map(|control| control.state());
    if let Some(state) = state {
        return Ok(PipelineStatusResponse {
            run_id,
            active: true,
            state: state.into(),
        });
    }
    drop(active_runs);
    let persisted = AppState::global()
        .pipeline_history()
        .get(&run_id)
        .map_err(|error| ipc_error(error.into()))?;
    Ok(PipelineStatusResponse {
        run_id,
        active: false,
        state: persisted_pipeline_state(persisted.as_ref()).into(),
    })
}
#[tauri::command(rename_all = "snake_case")]
pub fn get_pipeline_result(run_id: String) -> Result<Option<PipelineRun>, String> {
    let json = AppState::global()
        .pipeline_history()
        .get_result_json(&run_id)
        .map_err(|error| ipc_error(error.into()))?;
    decode_pipeline_result(&run_id, json)
}

fn decode_pipeline_result(
    run_id: &str,
    json: Option<String>,
) -> Result<Option<PipelineRun>, String> {
    let run = json
        .map(|json| {
            serde_json::from_str::<PipelineRun>(&json)
                .map_err(|error| format!("stored pipeline result is invalid: {error}"))
        })
        .transpose()?;
    if run.as_ref().is_some_and(|run| run.run_id != run_id) {
        return Err("[internal] stored pipeline result has a mismatched run ID; the run record is corrupt, re-run the pipeline".into());
    }
    Ok(run)
}

#[tauri::command(rename_all = "snake_case")]
pub fn cancel_build(project_id: String) -> Result<(), String> {
    crate::app_state::AppState::global()
        .progress_tracker
        .cancel_build(&project_id);
    Ok(())
}

#[cfg(test)]
mod pipeline_status_tests {
    use super::*;

    #[test]
    fn registered_build_requires_successful_outcome_but_simulated_plan_does_not() {
        assert!(pipeline_build_succeeded(false, None));
        assert!(!pipeline_build_succeeded(true, None));
        assert!(!pipeline_build_succeeded(true, Some(false)));
        assert!(pipeline_build_succeeded(true, Some(true)));
    }

    #[test]
    fn terminal_statuses_are_distinct_and_orphaned_running_rows_are_unknown() {
        let mut record = PipelineRunHistoryRecord {
            id: "run-1".into(),
            goal: "build".into(),
            status: "completed".into(),
            step_count: 0,
            repair_count: 0,
            success: true,
            error: None,
            finished_at: std::time::SystemTime::now(),
        };
        assert_eq!(persisted_pipeline_state(Some(&record)), "completed");
        record.status = "cancelled".into();
        assert_eq!(persisted_pipeline_state(Some(&record)), "cancelled");
        record.status = "failed".into();
        assert_eq!(persisted_pipeline_state(Some(&record)), "failed");
        record.status = "running".into();
        assert_eq!(persisted_pipeline_state(Some(&record)), "unknown");
        assert_eq!(persisted_pipeline_state(None), "unknown");
    }

    #[test]
    fn result_decoder_rejects_corruption_instead_of_fabricating_a_plan() {
        assert!(decode_pipeline_result("missing", None).unwrap().is_none());
        assert!(decode_pipeline_result("run-1", Some("not json".into())).is_err());
        let response = PipelineRun {
            plan: Plan {
                id: crate::domain::EntityId::from_string("plan-1"),
                goal: "build".into(),
                status: crate::planner::PlanStatus::Completed,
                steps: Vec::new(),
            },
            repair_actions: Vec::new(),
            events: Vec::new(),
            run_id: "run-1".into(),
            execution_context: None,
            generated_files: Vec::new(),
            failed_files: Vec::new(),
            outcome: None,
            verification_results: Some(Vec::new()),
            repair_results: None,
            verification_summary: Some(String::new()),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(decode_pipeline_result("other-run", Some(json.clone())).is_err());
        assert_eq!(
            decode_pipeline_result("run-1", Some(json))
                .unwrap()
                .unwrap()
                .plan
                .id
                .as_str(),
            "plan-1"
        );
    }
}
