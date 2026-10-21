//! Immutable identity records for plans, workspace grants and runs.
//! These types provide audit fields only; they do not authorize replay.

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PlanRevision {
    pub schema_version: u32,
    pub revision_id: String,
    pub plan_id: String,
    pub previous_revision: Option<String>,
    pub created_at_millis: i64,
    pub status: RevisionStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionStatus {
    Draft,
    Reviewed,
    Approved,
    Superseded,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceGrant {
    pub schema_version: u32,
    pub grant_id: String,
    pub project_id: String,
    pub created_at_millis: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RunIdentity {
    pub schema_version: u32,
    pub run_id: String,
    pub plan_revision_id: String,
    pub workspace_grant_id: String,
    pub created_at_millis: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_revision_round_trips_with_status() {
        let revision = PlanRevision {
            schema_version: 1,
            revision_id: "rev-1".into(),
            plan_id: "plan-1".into(),
            previous_revision: None,
            created_at_millis: 1,
            status: RevisionStatus::Approved,
        };
        let json = serde_json::to_string(&revision).unwrap();
        assert_eq!(revision, serde_json::from_str(&json).unwrap());
    }

    #[test]
    fn workspace_grant_round_trips_with_project_scope() {
        let grant = WorkspaceGrant {
            schema_version: 1,
            grant_id: "grant-1".into(),
            project_id: "proj-1".into(),
            created_at_millis: 2,
        };
        let json = serde_json::to_string(&grant).unwrap();
        assert_eq!(grant, serde_json::from_str(&json).unwrap());
    }

    #[test]
    fn run_identity_binds_plan_and_workspace() {
        let identity = RunIdentity {
            schema_version: 1,
            run_id: "run-1".into(),
            plan_revision_id: "rev-1".into(),
            workspace_grant_id: "grant-1".into(),
            created_at_millis: 3,
        };
        let json = serde_json::to_string(&identity).unwrap();
        assert_eq!(identity, serde_json::from_str(&json).unwrap());
    }
}
