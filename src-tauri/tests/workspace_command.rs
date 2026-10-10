use recode::workspace::{LocalWorkspaceService, WorkspaceService};
use std::path::PathBuf;

#[test]
fn command_layer_creates_workspace() {
    let temp = std::env::temp_dir().join("recode-command-integration");
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).expect("create temp root");

    let command = LocalWorkspaceService;
    let output = command
        .create_workspace("cmd-integration-1", &temp)
        .expect("create workspace");
    assert_eq!(output.id, "cmd-integration-1");
    assert!(PathBuf::from(output.root).exists());
}
