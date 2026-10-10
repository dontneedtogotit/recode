use crate::app_state::AppState;

use crate::dev_server::{
    DevServerStatus, DevServerStatusReport, DevServerStatusReporter, LocalDevServerStatusReporter,
};
use crate::git::{GitCommitOutput, GitInitOutput, GitService, GitStatus, LocalGitService};
use crate::gateway::{DefaultGatewayService, GatewayRequest, GatewayResponse, GatewayService};
use crate::provider_settings::{ProviderSettings, ProviderSettingsRepository, SqliteProviderSettingsRepository};
use crate::pipeline::{LocalPipelineService, PipelineControl, PipelineInput, PipelineRun};
use crate::pipeline_history::PipelineRunHistoryRecord;
use crate::planner::{LocalPlanner, Plan, Planner};
use crate::planning_circuit_breaker::{PhaseStatus, PlanCircuitBreaker, PlanMetrics, PlanRunStatus};
use crate::provider_selection::{
    ProviderCapabilities, ProviderHealthStatus, ProviderSelectionConfig, ProviderSelectionEngine,
};
use crate::runtime_monitor::{
    AgentTaskMetrics, DefaultRuntimeMonitorService, MonitorEvent, RuntimeMonitorService,
};
use crate::self_heal::{DefaultSelfHealService, RepairAction, SelfHealService};
use crate::workspace::{LocalWorkspaceService, WorkspaceService, ProjectFile, PathValidation, safe_file_target, registered_workspace_root};
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Instant, SystemTime};
use tauri::Emitter;
use tauri::Manager;
use crate::agent_runtime::LocalAgentRuntime;
use crate::persistence::{AgentTaskRunRecord, PlanRecord};

static PLAN_CIRCUIT_BREAKER: LazyLock<Mutex<PlanCircuitBreaker>> =
    LazyLock::new(|| Mutex::new(PlanCircuitBreaker::new()));
static PROVIDER_SELECTION: LazyLock<Mutex<ProviderSelectionEngine>> = LazyLock::new(|| {
    Mutex::new(ProviderSelectionEngine::new(
        ProviderSelectionConfig::default(),
    ))
});

fn provider_selection() -> &'static Mutex<ProviderSelectionEngine> {
    &PROVIDER_SELECTION
}

static ACTIVE_RUNS: LazyLock<Mutex<std::collections::HashMap<String, Arc<PipelineControl>>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

fn monitor() -> &'static DefaultRuntimeMonitorService {
    crate::runtime_monitor::service()
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
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_agent_task_runs(
    agent_id: String,
) -> Result<Vec<AgentTaskRunRecord>, String> {
    AppState::global()
        .task_runs()
        .list_runs(&agent_id)
        .map_err(|e| e.to_string())
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
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_plans() -> Result<Vec<PlanRecord>, String> {
    AppState::global()
        .plans()
        .list_plans()
        .map_err(|e| e.to_string())
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
        .expect("lock plan circuit breaker")
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
        .map_err(|e| e.to_string())?;
    Ok(AttemptSelfHealResponse { actions })
}

#[tauri::command]
pub fn attempt_startup_self_heal() -> Result<AttemptSelfHealResponse, String> {
    let events = monitor().poll_once();
    let actions = DefaultSelfHealService
        .attempt_repair(&events)
        .map_err(|e| e.to_string())?;
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
        ts: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time went backwards")
            .as_secs()
            .to_string(),
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
    app.path().app_data_dir()
        .map(|path| path.join("projects"))
        .map_err(|error| format!("app data directory unavailable: {error}"))
}

fn workspace_for_id(app: &tauri::AppHandle, id: &str) -> Result<std::path::PathBuf, String> {
    registered_workspace_root(&projects_dir(app)?, id).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn create_workspace(
    app: tauri::AppHandle,
    input: CreateManagedWorkspaceInput,
) -> Result<TauriCreateWorkspaceResponse, String> {
    let service = LocalWorkspaceService;
    let info = service
        .create_workspace(&input.id, &projects_dir(&app)?)
        .map_err(|e| e.to_string())?;
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
    if !root.exists() { return Ok(Vec::new()); }
    let service = LocalWorkspaceService;
    let workspaces = service
        .list_workspaces(&root)
        .map_err(|e| e.to_string())?;
    workspaces
        .into_iter()
        .map(|w| {
            registered_workspace_root(&root, &w.id)
                .map(|path| TauriWorkspaceInfo { id: w.id, root: path.to_string_lossy().to_string() })
                .map_err(|error| error.to_string())
        })
        .collect()
}

#[tauri::command(rename_all = "snake_case")]
pub fn write_project_file(app: tauri::AppHandle, workspace_id: String, file: ProjectFile) -> Result<(), String> {
    let service = LocalWorkspaceService;
    service
        .write_project_file(&workspace_for_id(&app, &workspace_id)?, &file)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn read_project_file(app: tauri::AppHandle, workspace_id: String, path: String) -> Result<Option<ProjectFile>, String> {
    let service = LocalWorkspaceService;
    service
        .read_project_file(&workspace_for_id(&app, &workspace_id)?, &path)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_project_files(app: tauri::AppHandle, workspace_id: String) -> Result<Vec<ProjectFile>, String> {
    let service = LocalWorkspaceService;
    service
        .list_project_files(&workspace_for_id(&app, &workspace_id)?)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn validate_workspace_path(app: tauri::AppHandle, workspace_id: String, path: String) -> Result<PathValidation, String> {
    match workspace_for_id(&app, &workspace_id).and_then(|root| safe_file_target(&root, &path).map_err(|error| error.to_string())) {
        Ok(_) => Ok(PathValidation { valid: true, normalized: path, issues: Vec::new() }),
        Err(error) => Ok(PathValidation { valid: false, normalized: String::new(), issues: vec![error.to_string()] }),
    }
}

#[tauri::command]
pub async fn run_pipeline(
    app: tauri::AppHandle,
    input: PipelineInput,
) -> Result<PipelineRun, String> {
    let run_id = if input.run_id.trim().is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        input.run_id.clone()
    };
    if run_id.len() > 128 || !run_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_') {
        return Err("invalid pipeline run id".into());
    }
    let workspace_root = input.project_id.as_deref()
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
        let mut active_runs = ACTIVE_RUNS.lock().expect("lock active runs");
        if active_runs.contains_key(&run_id) {
            return Err(format!("pipeline run already active: {run_id}"));
        }
        AppState::global().pipeline_history().record(&PipelineRunHistoryRecord {
            id: run_id.clone(),
            goal: goal.clone(),
            status: "running".into(),
            step_count: 0,
            repair_count: 0,
            success: false,
            error: None,
            finished_at: std::time::SystemTime::now(),
        }).map_err(|error| error.to_string())?;
        active_runs.insert(run_id.clone(), control.clone());
    }

    let plan_revision_id = uuid::Uuid::new_v4().to_string();
    let _ = AppState::global().plan_revisions().insert(&crate::pipeline::identity::PlanRevision {
        schema_version: 1,
        revision_id: plan_revision_id.clone(),
        plan_id: run_id.clone(),
        previous_revision: None,
        created_at_millis: std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or_default(),
        status: crate::pipeline::identity::RevisionStatus::Draft,
    });
    if let Some(workspace_root) = workspace_root.as_ref() {
        let projects_dir = workspace_root.parent().map(Path::new).unwrap_or_else(|| Path::new("."));
        let registered_id = registered_workspace_root(projects_dir, input.project_id.as_deref().unwrap_or_default())
            .ok()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let grant_id = uuid::Uuid::new_v4().to_string();
        let _ = AppState::global().workspace_grants().insert(&crate::pipeline::identity::WorkspaceGrant {
            schema_version: 1,
            grant_id: grant_id.clone(),
            project_id: registered_id,
            created_at_millis: std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or_default(),
        });
        let _ = AppState::global().run_identities().insert(&crate::pipeline::identity::RunIdentity {
            schema_version: 1,
            run_id: run_id.clone(),
            plan_revision_id: plan_revision_id.clone(),
            workspace_grant_id: grant_id,
            created_at_millis: std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or_default(),
        });
    }

    let (event_bus, receiver) = crate::event_bus::EventBus::new();
    let app_for_events = app.clone();
    let event_run_id = run_id.clone();
    let forwarder = std::thread::spawn(move || {
        while let Ok(event) = receiver.recv() {
            let payload = crate::event_bus::PipelineEvent::from_domain_event(&event, event_run_id.clone());
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
        service.run_with_control(service_input, control_for_run.as_ref()).map(|mut run| {
            run.execution_context = Some(execution_context);
            run
        })
    })
    .await;
    let _ = forwarder.join();
    let cancelled = control.is_cancelling();
    let completed_run = match &result {
        Ok(Ok(run)) if !cancelled && matches!(run.plan.status, crate::planner::PlanStatus::Completed) => Some(run),
        _ => None,
    };
    let serialized_run = completed_run.and_then(|run| serde_json::to_string(run).ok());
    let stored_result = serialized_run.as_deref().filter(|json| json.len() <= 1_048_576);
    let build_succeeded = completed_run.is_some_and(|run| pipeline_build_succeeded(
        build_requested,
        run.outcome.as_ref().map(|outcome| outcome.success),
    ));
    let stored_success = build_succeeded && stored_result.is_some();
    let error = if cancelled {
        Some("pipeline cancelled by user".to_string())
    } else if completed_run.is_some() && stored_result.is_none() {
        Some("pipeline result could not be persisted".into())
    } else if completed_run.is_some() && !build_succeeded {
        Some(completed_run.and_then(|run| run.outcome.as_ref().map(|outcome| outcome.summary.clone()))
            .unwrap_or_else(|| "pipeline build outcome unavailable".into()))
    } else {
        match &result {
            Ok(Err(error)) => Some(error.to_string()),
            Err(error) => Some(error.to_string()),
            Ok(Ok(run)) if !matches!(run.plan.status, crate::planner::PlanStatus::Completed) => Some("pipeline plan did not complete".into()),
            _ => None,
        }
    };
    let terminal = PipelineRunHistoryRecord {
        id: run_id.clone(),
        goal,
        status: if cancelled { "cancelled" } else if stored_success { "completed" } else { "failed" }.into(),
        step_count: completed_run.map_or(0, |run| run.plan.steps.len() as u64),
        repair_count: completed_run.map_or(0, |run| run.repair_actions.len() as u64),
        success: stored_success,
        error,
        finished_at: std::time::SystemTime::now(),
    };
    let persisted = AppState::global().pipeline_history().finish(&terminal, stored_result);
    ACTIVE_RUNS
        .lock()
        .expect("lock active runs")
        .remove(&run_id);

    persisted.map_err(|error| error.to_string())?;
    if cancelled {
        return Err("pipeline cancelled by user".into());
    }
    let run = result
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
    if !matches!(run.plan.status, crate::planner::PlanStatus::Completed) {
        return Err("pipeline plan did not complete".into());
    }
    if stored_result.is_none() {
        return Err("pipeline result could not be persisted".into());
    }
    if build_requested && run.outcome.is_none() {
        return Err("pipeline build outcome unavailable".into());
    }
    Ok(run)
}

fn pipeline_build_succeeded(build_requested: bool, outcome_success: Option<bool>) -> bool {
    !build_requested || outcome_success == Some(true)
}

#[tauri::command(rename_all = "snake_case")]
pub fn cancel_pipeline(run_id: String) -> Result<bool, String> {
    let active_runs = ACTIVE_RUNS.lock().expect("lock active runs");
    if let Some(control) = active_runs.get(&run_id) {
        Ok(control.abort())
    } else {
        Ok(false)
    }
}

#[tauri::command]
pub fn create_and_execute_plan(input: SavePlanInput) -> Result<Plan, String> {
    let planner = LocalPlanner::new();
    let mut plan = planner
        .create_plan(&input.goal)
        .map_err(|e| e.to_string())?;
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
        .map_err(|e| e.to_string())?;
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
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn record_quality_baseline(input: QualityBaselineInput) -> Result<(), String> {
    let scanner = crate::quality::QualityScanner::new(crate::quality::SqliteQualityRepository);
    let conn = AppState::global().db();
    scanner
        .record_baseline(&conn, &input.file_path, &input.content)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_quality_baselines(
    _input: ListQualityBaselinesInput,
) -> Result<Vec<crate::quality::QualityBaseline>, String> {
    let scanner = crate::quality::QualityScanner::new(crate::quality::SqliteQualityRepository);
    let conn = AppState::global().db();
    scanner.list_baselines(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_quality_baseline(input: DeleteQualityBaselineInput) -> Result<(), String> {
    let scanner = crate::quality::QualityScanner::new(crate::quality::SqliteQualityRepository);
    let conn = AppState::global().db();
    scanner
        .delete_baseline(&conn, &input.file_path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn git_status(root: String) -> Result<GitStatus, String> {
    LocalGitService
        .status(std::path::Path::new(&root))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn git_init(root: String) -> Result<GitInitOutput, String> {
    LocalGitService
        .init(std::path::Path::new(&root))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn git_add_all(root: String) -> Result<(), String> {
    LocalGitService
        .add_all(std::path::Path::new(&root))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn git_commit(root: String, message: String) -> Result<GitCommitOutput, String> {
    LocalGitService
        .commit(std::path::Path::new(&root), &message)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_team_task(input: crate::agent_team::TeamTaskRecord) -> Result<(), String> {
    AppState::global()
        .agent_team()
        .create_task(&input)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_team_tasks(team_id: String) -> Result<Vec<crate::agent_team::TeamTaskRecord>, String> {
    AppState::global()
        .agent_team()
        .list_tasks(&team_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn record_agent_task_history(input: crate::agent_task_history::AgentTaskHistoryRecord) -> Result<(), String> {
    AppState::global()
        .agent_task_history()
        .record(&input)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_agent_task_history(agent_id: String) -> Result<Vec<crate::agent_task_history::AgentTaskHistoryRecord>, String> {
    AppState::global()
        .agent_task_history()
        .list_by_agent(&agent_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn ensure_learning_schema() -> Result<(), String> {
    AppState::global()
        .learning()
        .ensure_schema()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn record_success_pattern(input: crate::app_state::SuccessPatternInput) -> Result<crate::app_state::SuccessPatternOutput, String> {
    AppState::global()
        .learning()
        .record_success_pattern(input)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn record_anti_pattern(input: crate::app_state::AntiPatternInput) -> Result<crate::app_state::AntiPatternOutput, String> {
    AppState::global()
        .learning()
        .record_anti_pattern(input)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_success_patterns(agent_role: String, task_type: String) -> Result<Vec<crate::learning::LearningPatternRecord>, String> {
    AppState::global()
        .learning()
        .list_success_patterns(&agent_role, &task_type)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_anti_patterns(
    agent_role: String,
    task_type: String,
) -> Result<Vec<crate::learning::LearningPatternRecord>, String> {
    AppState::global()
        .learning()
        .list_anti_patterns(&agent_role, &task_type)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn record_pipeline_run(
    record: PipelineRunHistoryRecord,
) -> Result<(), String> {
    AppState::global()
        .pipeline_history()
        .record(&record)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_pipeline_runs(
    limit: u64,
) -> Result<Vec<PipelineRunHistoryRecord>, String> {
    AppState::global()
        .pipeline_history()
        .list_runs(limit)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn run_gateway_task(input: GatewayRequest) -> Result<GatewayResponse, String> {
    crate::gateway::default_gateway()
        .send(input)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_gateway_status() -> Result<String, String> {
    let gateway = crate::gateway::default_gateway();
    if !gateway.is_configured() { return Ok("Unconfigured".into()); }
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
        .expect("lock provider selection");
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
    SqliteProviderSettingsRepository::save_provider(&AppState::global().db(), &provider).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_providers() -> Result<Vec<ProviderSettings>, String> {
    SqliteProviderSettingsRepository::load_providers(&AppState::global().db()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_provider(id: String) -> Result<(), String> {
    AppState::global()
        .db()
        .execute("DELETE FROM provider_settings WHERE id = ?1", [id])
        .map_err(|e| e.to_string())
        .map(|_| ())
}


#[tauri::command(rename_all = "snake_case")]
pub fn pause_pipeline(run_id: String) -> Result<bool, String> {
    let active_runs = ACTIVE_RUNS.lock().expect("lock active runs");
    if let Some(control) = active_runs.get(&run_id) {
        Ok(control.pause())
    } else {
        Ok(false)
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn resume_pipeline(run_id: String) -> Result<bool, String> {
    let active_runs = ACTIVE_RUNS.lock().expect("lock active runs");
    Ok(active_runs.get(&run_id).is_some_and(|control| control.resume()))
}

#[tauri::command(rename_all = "snake_case")]
pub fn abort_pipeline(run_id: String) -> Result<bool, String> {
    let active_runs = ACTIVE_RUNS.lock().expect("lock active runs");
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
pub fn get_pipeline_status(run_id: String) -> Result<PipelineStatusResponse, String> {
    let active_runs = ACTIVE_RUNS.lock().expect("lock active runs");
    let state = active_runs.get(&run_id).map(|control| control.state());
    if let Some(state) = state {
        return Ok(PipelineStatusResponse { run_id, active: true, state: state.into() });
    }
    drop(active_runs);
    let persisted = AppState::global().pipeline_history().get(&run_id).map_err(|error| error.to_string())?;
    Ok(PipelineStatusResponse {
        run_id,
        active: false,
        state: persisted_pipeline_state(persisted.as_ref()).into(),
    })
}
#[tauri::command(rename_all = "snake_case")]
pub fn get_pipeline_result(run_id: String) -> Result<Option<PipelineRun>, String> {
    let json = AppState::global().pipeline_history().get_result_json(&run_id).map_err(|error| error.to_string())?;
    decode_pipeline_result(&run_id, json)
}

fn decode_pipeline_result(run_id: &str, json: Option<String>) -> Result<Option<PipelineRun>, String> {
    let run = json.map(|json| serde_json::from_str::<PipelineRun>(&json).map_err(|error| format!("stored pipeline result is invalid: {error}"))).transpose()?;
    if run.as_ref().is_some_and(|run| run.run_id != run_id) {
        return Err("stored pipeline result has a mismatched run ID".into());
    }
    Ok(run)
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
            id: "run-1".into(), goal: "build".into(), status: "completed".into(),
            step_count: 0, repair_count: 0, success: true, error: None,
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
        assert_eq!(decode_pipeline_result("run-1", Some(json)).unwrap().unwrap().plan.id.as_str(), "plan-1");
    }
}
