use recode::workspace::{LocalWorkspaceService, WorkspaceService};
use std::path::PathBuf;

#[test]
fn workspace_persistence_roundtrip() {
    let temp = std::env::temp_dir().join("recode-workspace-roundtrip");
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).expect("create temp root");

    let service = LocalWorkspaceService;
    let created = service
        .create_workspace("roundtrip-1", &temp)
        .expect("create workspace");

    assert!(PathBuf::from(&created.root).exists());
    assert_eq!(created.id, "roundtrip-1");
}
