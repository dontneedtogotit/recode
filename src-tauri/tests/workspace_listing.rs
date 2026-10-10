use recode::workspace::{LocalWorkspaceService, WorkspaceService};
use std::path::PathBuf;

#[test]
fn workspace_listing_roundtrip() {
    let temp = std::env::temp_dir().join("recode-workspace-list");
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).expect("create temp root");

    let service = LocalWorkspaceService;
    service.create_workspace("list-1", &temp).expect("create 1");
    service.create_workspace("list-2", &temp).expect("create 2");

    // This will fail because listing is not implemented yet.
    let list = service.list_workspaces(&temp).expect("list workspaces");
    assert_eq!(list.len(), 2);
    assert!(list.iter().any(|w| w.id == "list-1"));
    assert!(list.iter().any(|w| w.id == "list-2"));
}
