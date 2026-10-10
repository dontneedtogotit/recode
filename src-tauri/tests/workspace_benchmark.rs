use std::time::Instant;

use recode::workspace::{LocalWorkspaceService, WorkspaceService};

#[test]
fn bench_workspace_creation_under_3s() {
    let temp = std::env::temp_dir().join("recode-workspace-bench");
    let _ = std::fs::remove_dir_all(&temp);
    let _ = std::fs::create_dir_all(&temp);

    let service = LocalWorkspaceService;
    let start = Instant::now();
    let result = service.create_workspace("bench-ws", &temp);
    let elapsed = start.elapsed();

    assert!(result.is_ok(), "workspace creation should succeed");
    println!("workspace_creation_ms={}", elapsed.as_millis());
    assert!(
        elapsed.as_millis() < 3000,
        "workspace creation exceeded 3s target: {}ms",
        elapsed.as_millis()
    );

    let _ = std::fs::remove_dir_all(&temp);
}
