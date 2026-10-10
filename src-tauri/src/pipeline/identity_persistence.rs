use crate::domain::{DomainError, Result};
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::pipeline::identity::{PlanRevision, RevisionStatus, RunIdentity, WorkspaceGrant};

pub trait PlanRevisionRepository {
    fn ensure_schema(conn: &Connection) -> Result<(), DomainError>;
    fn insert_plan_revision(conn: &Connection, revision: &PlanRevision) -> Result<(), DomainError>;
    fn list_plan_revisions(conn: &Connection, plan_id: &str) -> Result<Vec<PlanRevision>, DomainError>;
}

pub struct SqlitePlanRevisionRepository;

impl PlanRevisionRepository for SqlitePlanRevisionRepository {
    fn ensure_schema(conn: &Connection) -> Result<(), DomainError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS plan_revisions (revision_id TEXT PRIMARY KEY, plan_id TEXT NOT NULL, previous_revision TEXT, created_at INTEGER NOT NULL, status TEXT NOT NULL);",
        )
        .map_err(|e| DomainError::internal(format!("plan revision schema creation failed: {e}")))
        .map(|_| ())
    }

    fn insert_plan_revision(conn: &Connection, revision: &PlanRevision) -> Result<(), DomainError> {
        conn.execute(
            "INSERT INTO plan_revisions (revision_id, plan_id, previous_revision, created_at, status) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params!(
                revision.revision_id.as_str(),
                revision.plan_id.as_str(),
                revision.previous_revision.as_deref().unwrap_or(""),
                revision.created_at_millis,
                format!("{revision_status:?}", revision_status = revision.status),
            ),
        )
        .map_err(|e| DomainError::internal(format!("plan revision insert failed: {e}")))
        .map(|_| ())
    }

    fn list_plan_revisions(conn: &Connection, plan_id: &str) -> Result<Vec<PlanRevision>, DomainError> {
        let mut stmt = conn
            .prepare("SELECT revision_id, plan_id, previous_revision, created_at, status FROM plan_revisions WHERE plan_id = ?1 ORDER BY created_at ASC")
            .map_err(|e| DomainError::internal(format!("plan revision query prepare failed: {e}")))?;
        let rows = stmt
            .query_map([plan_id], |row| {
                let previous: String = row.get(2)?;
                let status_str: String = row.get(4)?;
                let status = match status_str.as_str() {
                    "Draft" => RevisionStatus::Draft,
                    "Reviewed" => RevisionStatus::Reviewed,
                    "Approved" => RevisionStatus::Approved,
                    "Superseded" => RevisionStatus::Superseded,
                    _ => RevisionStatus::Draft,
                };
                Ok(PlanRevision {
                    schema_version: 1,
                    revision_id: row.get(0)?,
                    plan_id: row.get(1)?,
                    previous_revision: if previous.is_empty() { None } else { Some(previous) },
                    created_at_millis: row.get(3)?,
                    status,
                })
            })
            .map_err(|e| DomainError::internal(format!("plan revision query failed: {e}")))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| DomainError::internal(format!("plan revision row read failed: {e}")))?);
        }
        Ok(out)
    }
}

pub trait WorkspaceGrantRepository {
    fn ensure_schema(conn: &Connection) -> Result<(), DomainError>;
    fn insert_grant(conn: &Connection, grant: &WorkspaceGrant) -> Result<(), DomainError>;
    fn get_grant(conn: &Connection, grant_id: &str) -> Result<Option<WorkspaceGrant>, DomainError>;
}

pub struct SqliteWorkspaceGrantRepository;

impl WorkspaceGrantRepository for SqliteWorkspaceGrantRepository {
    fn ensure_schema(conn: &Connection) -> Result<(), DomainError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS workspace_grants (grant_id TEXT PRIMARY KEY, project_id TEXT NOT NULL, created_at INTEGER NOT NULL);",
        )
        .map_err(|e| DomainError::internal(format!("workspace grant schema creation failed: {e}")))
        .map(|_| ())
    }

    fn insert_grant(conn: &Connection, grant: &WorkspaceGrant) -> Result<(), DomainError> {
        conn.execute(
            "INSERT INTO workspace_grants (grant_id, project_id, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params!(grant.grant_id.as_str(), grant.project_id.as_str(), grant.created_at_millis),
        )
        .map_err(|e| DomainError::internal(format!("workspace grant insert failed: {e}")))
        .map(|_| ())
    }

    fn get_grant(conn: &Connection, grant_id: &str) -> Result<Option<WorkspaceGrant>, DomainError> {
        conn.query_row(
            "SELECT grant_id, project_id, created_at FROM workspace_grants WHERE grant_id = ?1",
            [grant_id],
            |row| {
                Ok(WorkspaceGrant {
                    schema_version: 1,
                    grant_id: row.get(0)?,
                    project_id: row.get(1)?,
                    created_at_millis: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(|e| DomainError::internal(format!("workspace grant lookup failed: {e}")))
    }
}

pub trait RunIdentityRepository {
    fn ensure_schema(conn: &Connection) -> Result<(), DomainError>;
    fn insert_identity(conn: &Connection, identity: &RunIdentity) -> Result<(), DomainError>;
    fn get_identity(conn: &Connection, run_id: &str) -> Result<Option<RunIdentity>, DomainError>;
}

pub struct SqliteRunIdentityRepository;

impl RunIdentityRepository for SqliteRunIdentityRepository {
    fn ensure_schema(conn: &Connection) -> Result<(), DomainError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS run_identities (run_id TEXT PRIMARY KEY, plan_revision_id TEXT NOT NULL, workspace_grant_id TEXT NOT NULL, created_at INTEGER NOT NULL);",
        )
        .map_err(|e| DomainError::internal(format!("run identity schema creation failed: {e}")))
        .map(|_| ())
    }

    fn insert_identity(conn: &Connection, identity: &RunIdentity) -> Result<(), DomainError> {
        conn.execute(
            "INSERT INTO run_identities (run_id, plan_revision_id, workspace_grant_id, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!(
                identity.run_id.as_str(),
                identity.plan_revision_id.as_str(),
                identity.workspace_grant_id.as_str(),
                identity.created_at_millis,
            ),
        )
        .map_err(|e| DomainError::internal(format!("run identity insert failed: {e}")))
        .map(|_| ())
    }

    fn get_identity(conn: &Connection, run_id: &str) -> Result<Option<RunIdentity>, DomainError> {
        conn.query_row(
            "SELECT run_id, plan_revision_id, workspace_grant_id, created_at FROM run_identities WHERE run_id = ?1",
            [run_id],
            |row| {
                Ok(RunIdentity {
                    schema_version: 1,
                    run_id: row.get(0)?,
                    plan_revision_id: row.get(1)?,
                    workspace_grant_id: row.get(2)?,
                    created_at_millis: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(|e| DomainError::internal(format!("run identity lookup failed: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn in_memory_conn() -> Result<Connection, DomainError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| DomainError::internal(format!("open in-memory failed: {e}")))?;
        SqlitePlanRevisionRepository::ensure_schema(&conn)?;
        SqliteWorkspaceGrantRepository::ensure_schema(&conn)?;
        SqliteRunIdentityRepository::ensure_schema(&conn)?;
        Ok(conn)
    }

    #[test]
    fn plan_revision_lifecycle_preserves_chain() {
        let conn = in_memory_conn().expect("in-memory db");
        let revision = PlanRevision {
            schema_version: 1,
            revision_id: "rev-1".into(),
            plan_id: "plan-1".into(),
            previous_revision: None,
            created_at_millis: 1,
            status: RevisionStatus::Approved,
        };
        SqlitePlanRevisionRepository::insert_plan_revision(&conn, &revision).unwrap();
        let listed = SqlitePlanRevisionRepository::list_plan_revisions(&conn, "plan-1").unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].revision_id, "rev-1");
        assert_eq!(listed[0].status, RevisionStatus::Approved);
    }

    #[test]
    fn workspace_grant_round_trips_through_reopen() {
        let path = std::env::temp_dir().join(format!("recode-identity-{}.db", uuid::Uuid::new_v4()));
        let grant = WorkspaceGrant {
            schema_version: 1,
            grant_id: "grant-1".into(),
            project_id: "proj-1".into(),
            created_at_millis: 5,
        };
        {
            let conn = Connection::open(&path).unwrap();
            SqliteWorkspaceGrantRepository::ensure_schema(&conn).unwrap();
            SqliteWorkspaceGrantRepository::insert_grant(&conn, &grant).unwrap();
        }
        {
            let conn = Connection::open(&path).unwrap();
            SqliteWorkspaceGrantRepository::ensure_schema(&conn).unwrap();
            let restored = SqliteWorkspaceGrantRepository::get_grant(&conn, "grant-1").unwrap().unwrap();
            assert_eq!(restored.grant_id, "grant-1");
            assert_eq!(restored.project_id, "proj-1");
            assert!(SqliteWorkspaceGrantRepository::get_grant(&conn, "missing").unwrap().is_none());
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn run_identity_binds_plan_and_workspace() {
        let conn = in_memory_conn().expect("in-memory db");
        let identity = RunIdentity {
            schema_version: 1,
            run_id: "run-1".into(),
            plan_revision_id: "rev-1".into(),
            workspace_grant_id: "grant-1".into(),
            created_at_millis: 10,
        };
        SqliteRunIdentityRepository::insert_identity(&conn, &identity).unwrap();
        let restored = SqliteRunIdentityRepository::get_identity(&conn, "run-1").unwrap().unwrap();
        assert_eq!(restored.plan_revision_id, "rev-1");
        assert_eq!(restored.workspace_grant_id, "grant-1");
        assert!(SqliteRunIdentityRepository::get_identity(&conn, "missing").unwrap().is_none());
    }
}
