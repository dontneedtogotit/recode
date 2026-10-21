use recode::persistence::{open_recode_db, ProjectRepository, SqliteProjectRepository};
use recode::workspace::{LocalWorkspaceService, WorkspaceService};
use std::path::PathBuf;

#[test]
fn workspace_metadata_is_persisted_to_sqlite() {
    let temp = std::env::temp_dir().join("recode-workspace-sqlite");
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).expect("create temp root");

    let db_path = temp.join("recode.db");
    let conn = open_recode_db(&db_path).expect("open sqlite");

    let service = LocalWorkspaceService;
    let created = service
        .create_workspace("sqlite-1", &temp)
        .expect("create workspace");

    assert!(PathBuf::from(&created.root).exists());
    assert_eq!(created.id, "sqlite-1");
    assert!(db_path.exists(), "sqlite database should be created");

    let records =
        SqliteProjectRepository::list_projects(&conn).expect("list persisted projects");
    assert!(records.iter().any(|r| r.id == "sqlite-1"));
}
