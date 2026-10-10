use crate::domain::DomainError;
use crate::persistence::{
    open_recode_db, AgentTaskRunRepository, PlanRepository, SqliteAgentTaskRunRepository,
    SqlitePlanRepository,
};
use crate::provider_settings::{ProviderSettingsRepository, SqliteProviderSettingsRepository};
use crate::quality::{QualityRepository, QualityScanner, SqliteQualityRepository};
use crate::runtime_monitor::DefaultRuntimeMonitorService;
use crate::agent_task_history::{
    AgentTaskHistoryRecord, AgentTaskHistoryRepository, SqliteAgentTaskHistoryRepository,
};
use crate::agent_team::{
    SqliteTeamTaskRepository, TeamTaskRecord, TeamTaskRepository,
};
use crate::learning::{
    AntiPattern, LearningPatternRecord, LearningPatternService, PatternRepository,
    SqlitePatternRepository, SuccessPattern,
};
use crate::pipeline_history::{
    PipelineRunHistoryRecord, PipelineRunHistoryRepository, SqlitePipelineRunHistoryRepository,
};
use crate::pipeline::identity_persistence::{
    PlanRevisionRepository, RunIdentityRepository, SqlitePlanRevisionRepository,
    SqliteRunIdentityRepository, WorkspaceGrantRepository, SqliteWorkspaceGrantRepository,
};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use crate::pipeline::progress_tracker::ProgressTracker;

#[derive(Debug)]
pub struct AppState {
    pub connection: Mutex<rusqlite::Connection>,
    pub monitor: &'static DefaultRuntimeMonitorService,
    pub progress_tracker: ProgressTracker,
}

impl AppState {
    pub fn open(path: &Path) -> Result<Self, DomainError> {
        let connection = Mutex::new(open_recode_db(path)?);
        let state = Self {
            connection,
            monitor: crate::runtime_monitor::service(),
            progress_tracker: ProgressTracker::new(),
        };
        {
            let conn = state.db();
            SqliteAgentTaskRunRepository::ensure_schema(&conn)?;
            SqlitePlanRepository::ensure_schema(&conn)?;
            SqliteAgentTaskHistoryRepository::ensure_schema(&conn)?;
            SqliteTeamTaskRepository::ensure_schema(&conn)?;
            SqlitePipelineRunHistoryRepository::ensure_schema(&conn)?;
            // Crash recovery: a fresh process has no active runs, so any
            // `running` row visible here belongs to a process that died
            // before reaching a terminal state. Record that honestly instead
            // of leaving it "unknown" forever.
            SqlitePipelineRunHistoryRepository::fail_stale_running_runs(&conn, SystemTime::now())?;
            PatternRepository::ensure_schema(&SqlitePatternRepository, &conn)?;
            <SqliteQualityRepository as QualityRepository>::ensure_schema(
                &SqliteQualityRepository,
                &conn,
            )?;
        }
        Ok(state)
    }

    pub fn task_runs(&self) -> TaskRunStore<'_> {
        TaskRunStore { state: self }
    }

    pub fn plans(&self) -> PlanStore<'_> {
        PlanStore { state: self }
    }

    pub fn quality(&self) -> QualityStore<'_> {
        QualityStore {
            scanner: QualityScanner::new(SqliteQualityRepository),
            state: self,
        }
    }

    pub fn provider_settings(&self) -> ProviderSettingsStore<'_> {
        ProviderSettingsStore { state: self }
    }

    pub fn agent_task_history(&self) -> AgentTaskHistoryStore<'_> {
        AgentTaskHistoryStore { state: self }
    }

    pub fn agent_team(&self) -> AgentTeamStore<'_> {
        AgentTeamStore { state: self }
    }

    pub fn learning(&self) -> LearningStore<'_> {
        LearningStore { state: self }
    }

    pub fn pipeline_history(&self) -> PipelineHistoryStore<'_> {
        PipelineHistoryStore { state: self }
    }

    pub fn plan_revisions(&self) -> PlanRevisionStore<'_> {
        PlanRevisionStore { state: self }
    }

    pub fn workspace_grants(&self) -> WorkspaceGrantStore<'_> {
        WorkspaceGrantStore { state: self }
    }

    pub fn run_identities(&self) -> RunIdentityStore<'_> {
        RunIdentityStore { state: self }
    }

    pub fn db(&self) -> std::sync::MutexGuard<'_, rusqlite::Connection> {
        self.connection.lock().expect("lock db connection")
    }
}

static APP_STATE: std::sync::OnceLock<AppState> = std::sync::OnceLock::new();

impl AppState {
    pub fn global() -> &'static Self {
        APP_STATE.get_or_init(|| {
            Self::open(Path::new("recode.db")).expect("failed to open recode.db")
        })
    }
}

pub struct TaskRunStore<'a> {
    state: &'a AppState,
}

impl<'a> TaskRunStore<'a> {
    pub fn insert_run(
        &self,
        record: &crate::persistence::AgentTaskRunRecord,
    ) -> Result<(), DomainError> {
        let conn = self.state.db();
        SqliteAgentTaskRunRepository::insert_run(&conn, record)
    }

    pub fn list_runs(
        &self,
        agent_id: &str,
    ) -> Result<Vec<crate::persistence::AgentTaskRunRecord>, DomainError> {
        let conn = self.state.db();
        SqliteAgentTaskRunRepository::list_runs(&conn, agent_id)
    }
}

pub struct PlanStore<'a> {
    state: &'a AppState,
}

impl<'a> PlanStore<'a> {
    pub fn save_plan(
        &self,
        record: &crate::persistence::PlanRecord,
    ) -> Result<(), DomainError> {
        let conn = self.state.db();
        SqlitePlanRepository::save_plan(&conn, record)
    }

    pub fn list_plans(&self) -> Result<Vec<crate::persistence::PlanRecord>, DomainError> {
        let conn = self.state.db();
        SqlitePlanRepository::list_plans(&conn)
    }
}

pub struct QualityStore<'a> {
    scanner: QualityScanner<SqliteQualityRepository>,
    state: &'a AppState,
}

impl<'a> QualityStore<'a> {
    pub fn scan(
        &self,
        root: &Path,
        options: &crate::quality::ScanOptions,
    ) -> Result<crate::quality::QualityReport, DomainError> {
        let conn = self.state.db();
        self.scanner.scan(&conn, root, options)
    }

    pub fn record_baseline(&self, file_path: &str, content: &str) -> Result<(), DomainError> {
        let conn = self.state.db();
        self.scanner.record_baseline(&conn, file_path, content)
    }

    pub fn list_baselines(&self) -> Result<Vec<crate::quality::QualityBaseline>, DomainError> {
        let conn = self.state.db();
        self.scanner.list_baselines(&conn)
    }

    pub fn delete_baseline(&self, file_path: &str) -> Result<(), DomainError> {
        let conn = self.state.db();
        self.scanner.delete_baseline(&conn, file_path)
    }
}

pub struct ProviderSettingsStore<'a> {
    state: &'a AppState,
}

impl<'a> ProviderSettingsStore<'a> {
    pub fn save_provider(
        &self,
        settings: &crate::provider_settings::ProviderSettings,
    ) -> Result<(), DomainError> {
        let conn = self.state.db();
        SqliteProviderSettingsRepository::save_provider(&conn, settings)
    }

    pub fn load_providers(
        &self,
    ) -> Result<Vec<crate::provider_settings::ProviderSettings>, DomainError> {
        let conn = self.state.db();
        SqliteProviderSettingsRepository::load_providers(&conn)
    }
}

pub struct AgentTaskHistoryStore<'a> {
    state: &'a AppState,
}

impl<'a> AgentTaskHistoryStore<'a> {
    pub fn record(&self, record: &AgentTaskHistoryRecord) -> Result<(), DomainError> {
        let conn = self.state.db();
        SqliteAgentTaskHistoryRepository::insert_record(&conn, record)
    }

    pub fn list_by_agent(
        &self,
        agent_id: &str,
    ) -> Result<Vec<AgentTaskHistoryRecord>, DomainError> {
        let conn = self.state.db();
        SqliteAgentTaskHistoryRepository::list_by_agent(&conn, agent_id)
    }
}

pub struct AgentTeamStore<'a> {
    state: &'a AppState,
}

impl<'a> AgentTeamStore<'a> {
    pub fn create_task(&self, record: &TeamTaskRecord) -> Result<(), DomainError> {
        let conn = self.state.db();
        SqliteTeamTaskRepository::insert_task(&conn, record)
    }

    pub fn list_tasks(&self, team_id: &str) -> Result<Vec<TeamTaskRecord>, DomainError> {
        let conn = self.state.db();
        SqliteTeamTaskRepository::list_tasks(&conn, team_id)
    }
}

pub struct PipelineHistoryStore<'a> {
    state: &'a AppState,
}

impl<'a> PipelineHistoryStore<'a> {
    pub fn record(&self, record: &PipelineRunHistoryRecord) -> Result<(), DomainError> {
        let conn = self.state.db();
        SqlitePipelineRunHistoryRepository::insert_run(&conn, record)
    }

    pub fn finish(&self, record: &PipelineRunHistoryRecord, result_json: Option<&str>) -> Result<(), DomainError> {
        let conn = self.state.db();
        SqlitePipelineRunHistoryRepository::finish_run(&conn, record, result_json)
    }

    pub fn get(&self, run_id: &str) -> Result<Option<PipelineRunHistoryRecord>, DomainError> {
        let conn = self.state.db();
        SqlitePipelineRunHistoryRepository::get_run(&conn, run_id)
    }

    pub fn get_result_json(&self, run_id: &str) -> Result<Option<String>, DomainError> {
        let conn = self.state.db();
        SqlitePipelineRunHistoryRepository::get_result_json(&conn, run_id)
    }

    pub fn list_runs(&self, limit: u64) -> Result<Vec<PipelineRunHistoryRecord>, DomainError> {
        let conn = self.state.db();
        SqlitePipelineRunHistoryRepository::list_runs(&conn, limit)
    }
}

pub struct PlanRevisionStore<'a> {
    state: &'a AppState,
}

impl<'a> PlanRevisionStore<'a> {
    pub fn insert(
        &self,
        revision: &crate::pipeline::identity::PlanRevision,
    ) -> Result<(), DomainError> {
        let conn = self.state.db();
        SqlitePlanRevisionRepository::insert_plan_revision(&conn, revision)
    }
}

pub struct WorkspaceGrantStore<'a> {
    state: &'a AppState,
}

impl<'a> WorkspaceGrantStore<'a> {
    pub fn insert(
        &self,
        grant: &crate::pipeline::identity::WorkspaceGrant,
    ) -> Result<(), DomainError> {
        let conn = self.state.db();
        SqliteWorkspaceGrantRepository::insert_grant(&conn, grant)
    }
}

pub struct RunIdentityStore<'a> {
    state: &'a AppState,
}

impl<'a> RunIdentityStore<'a> {
    pub fn insert(
        &self,
        identity: &crate::pipeline::identity::RunIdentity,
    ) -> Result<(), DomainError> {
        let conn = self.state.db();
        SqliteRunIdentityRepository::insert_identity(&conn, identity)
    }
}

pub struct LearningStore<'a> {
    state: &'a AppState,
}

impl<'a> LearningStore<'a> {
    pub fn ensure_schema(&self) -> Result<(), DomainError> {
        let conn = self.state.db();
        PatternRepository::ensure_schema(&SqlitePatternRepository, &conn)
    }

    pub fn record_success_pattern(
        &self,
        input: SuccessPatternInput,
    ) -> Result<SuccessPatternOutput, DomainError> {
        let conn = self.state.db();
        let id = input.id.clone();
        let pattern = SuccessPattern {
            id: input.id,
            agent_role: input.agent_role,
            task_type: input.task_type,
            pattern: input.pattern,
            confidence: input.confidence,
            success_count: input.success_count,
            sample_count: input.sample_count,
            created_at: input.created_at,
            updated_at: input.updated_at,
        };
        LearningPatternService::new(SqlitePatternRepository)
            .record_success_pattern(&conn, pattern)?;
        Ok(SuccessPatternOutput { id })
    }

    pub fn record_anti_pattern(
        &self,
        input: AntiPatternInput,
    ) -> Result<AntiPatternOutput, DomainError> {
        let conn = self.state.db();
        let id = input.id.clone();
        let pattern = AntiPattern {
            id: input.id,
            agent_role: input.agent_role,
            task_type: input.task_type,
            pattern: input.pattern,
            severity: input.severity,
            occurrence_count: input.occurrence_count,
            created_at: input.created_at,
            updated_at: input.updated_at,
        };
        LearningPatternService::new(SqlitePatternRepository)
            .record_anti_pattern(&conn, pattern)?;
        Ok(AntiPatternOutput { id })
    }

    pub fn list_success_patterns(
        &self,
        agent_role: &str,
        task_type: &str,
    ) -> Result<Vec<LearningPatternRecord>, DomainError> {
        let conn = self.state.db();
        LearningPatternService::new(SqlitePatternRepository)
            .top_success_patterns(&conn, agent_role, task_type)
    }

    pub fn list_anti_patterns(
        &self,
        agent_role: &str,
        task_type: &str,
    ) -> Result<Vec<LearningPatternRecord>, DomainError> {
        let conn = self.state.db();
        LearningPatternService::new(SqlitePatternRepository)
            .top_anti_patterns(&conn, agent_role, task_type)
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SuccessPatternInput {
    pub id: String,
    pub agent_role: String,
    pub task_type: String,
    pub pattern: String,
    pub confidence: f64,
    pub success_count: u64,
    pub sample_count: u64,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SuccessPatternOutput {
    pub id: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct AntiPatternInput {
    pub id: String,
    pub agent_role: String,
    pub task_type: String,
    pub pattern: String,
    pub severity: f64,
    pub occurrence_count: u64,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct AntiPatternOutput {
    pub id: String,
}

pub type SharedAppState = Arc<AppState>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_db() -> PathBuf {
        // Parallel tests must not delete another test's open SQLite database.
        std::env::temp_dir().join(format!("recode-app-state-{}.db", uuid::Uuid::new_v4()))
    }

    #[test]
    fn opens_app_state_and_bootstraps_schemas() {
        let state = AppState::open(&temp_db()).expect("open app state");
        let plans = state.plans().list_plans().expect("list plans");
        assert!(plans.is_empty());
        let runs = state
            .task_runs()
            .list_runs("planner")
            .expect("list task runs");
        assert!(runs.is_empty());
    }

    #[test]
    fn quality_store_round_trips_baseline() {
        let state = AppState::open(&temp_db()).expect("open app state");
        state
            .quality()
            .record_baseline("app.rs", "fn main() {}\n")
            .expect("record baseline");
        let baselines = state.quality().list_baselines().expect("list baselines");
        assert_eq!(baselines.len(), 1);
        assert_eq!(baselines[0].file_path, "app.rs");
    }

    #[test]
    fn provider_settings_store_persists_settings() {
        let state = AppState::open(&temp_db()).expect("open app state");
        state
            .provider_settings()
            .save_provider(&crate::provider_settings::ProviderSettings {
                id: "ps-1".into(),
                provider: "openai".into(),
                api_key: String::new(),
                enabled: true,
            })
            .expect("save provider settings");
        let providers = state
            .provider_settings()
            .load_providers()
            .expect("load providers");
        assert_eq!(providers.len(), 1);
        assert!(providers[0].api_key.is_empty());
    }

    #[test]
    fn learning_store_round_trips_patterns() {
        use std::time::SystemTime;
        let state = AppState::open(&temp_db()).expect("open app state");
        state
            .learning()
            .record_success_pattern(crate::app_state::SuccessPatternInput {
                id: "success-1".into(),
                agent_role: "runtime".into(),
                task_type: "agent_task".into(),
                pattern: "use streaming for chat".into(),
                confidence: 0.9,
                success_count: 1,
                sample_count: 1,
                created_at: SystemTime::now(),
                updated_at: SystemTime::now(),
            })
            .expect("record success pattern");
        state
            .learning()
            .record_anti_pattern(crate::app_state::AntiPatternInput {
                id: "anti-1".into(),
                agent_role: "runtime".into(),
                task_type: "agent_task".into(),
                pattern: "do not retry timeouts immediately".into(),
                severity: 0.7,
                occurrence_count: 1,
                created_at: SystemTime::now(),
                updated_at: SystemTime::now(),
            })
            .expect("record anti pattern");

        let success = state
            .learning()
            .list_success_patterns("runtime", "agent_task")
            .expect("list success patterns");
        let anti = state
            .learning()
            .list_anti_patterns("runtime", "agent_task")
            .expect("list anti patterns");
        assert_eq!(success.len(), 1);
        assert_eq!(anti.len(), 1);
        assert_eq!(success[0].id, "success-1");
        assert_eq!(anti[0].id, "anti-1");
    }

    #[test]
    fn learning_schema_bootstraps_with_app_state() {
        let state = AppState::open(&temp_db()).expect("open app state");
        state
            .learning()
            .ensure_schema()
            .expect("ensure learning schema");
        let success = state
            .learning()
            .list_success_patterns("runtime", "agent_task")
            .expect("list success patterns");
        assert!(success.is_empty());
    }
}
