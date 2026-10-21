//! P0.2 native command integration test: the workspace command slice is
//! exercised through the real `generate_handler!` IPC dispatch on Tauri's
//! `MockRuntime` — the same `on_message` path the production webview uses —
//! with command bodies running against real SQLite registration and real
//! on-disk workspaces under a temp `XDG_DATA_HOME`.
//!
//! This is the native proof ROADMAP P0 §2 calls for: a mocked `__TAURI__`
//! frontend shim is not involved. The commands are generic over
//! `tauri::Runtime` so they can run under `MockRuntime`; `Wry` stays the
//! production runtime.

use std::sync::Mutex;
use tauri::{
    ipc::{CallbackFn, InvokeBody},
    test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY},
    webview::InvokeRequest,
    WebviewUrl, WebviewWindowBuilder,
};

/// Serializes app creation: `XDG_DATA_HOME` is process-global, so concurrent
/// tests in this binary must not race on the temp root.
static XDG_LOCK: Mutex<()> = Mutex::new(());

fn invoke_request(cmd: &str, body: serde_json::Value) -> InvokeRequest {
    InvokeRequest {
        cmd: cmd.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: "tauri://localhost".parse().unwrap(),
        body: InvokeBody::Json(body),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_string(),
    }
}

/// Builds a mock-runtime app with the workspace command slice registered
/// exactly as in `main.rs`, and a main webview window to dispatch through.
/// Returns (webview, xdg_root). Callers hold XDG_LOCK.
fn build_native_app(tag: &str) -> (tauri::WebviewWindow<tauri::test::MockRuntime>, std::path::PathBuf) {
    let xdg = std::env::temp_dir().join(format!("recode-native-ipc-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&xdg);
    std::fs::create_dir_all(&xdg).expect("create XDG_DATA_HOME temp root");
    std::env::set_var("XDG_DATA_HOME", &xdg);

    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![
            recode::tauri_commands::create_workspace,
            recode::tauri_commands::list_workspaces,
            recode::tauri_commands::write_project_file,
            recode::tauri_commands::read_project_file,
            recode::tauri_commands::list_project_files,
            recode::tauri_commands::validate_workspace_path,
        ])
        .build(mock_context(noop_assets()))
        .expect("build mock app");

    let webview = WebviewWindowBuilder::new(&app, "main", WebviewUrl::default())
        .build()
        .expect("build main webview window");
    (webview, xdg)
}

fn cleanup(xdg: &std::path::PathBuf) {
    std::env::remove_var("XDG_DATA_HOME");
    let _ = std::fs::remove_dir_all(xdg);
}

#[test]
fn native_ipc_workspace_round_trip() {
    let _guard = XDG_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let (webview, xdg) = build_native_app("roundtrip");

    // create_workspace → registered under the redirected app data dir.
    let created = get_ipc_response(
        &webview,
        invoke_request(
            "create_workspace",
            serde_json::json!({ "input": { "id": "native-ipc-1" } }),
        ),
    )
    .expect("create_workspace dispatch failed")
    .deserialize::<serde_json::Value>()
    .expect("create_workspace response deserialization");
    assert_eq!(created["id"], "native-ipc-1");
    let root = created["root"].as_str().expect("root is a string").to_string();
    let projects_root = xdg.join("projects");
    assert!(
        std::path::Path::new(&root).starts_with(&projects_root),
        "workspace root {root} should live under {}",
        projects_root.display()
    );

    // list_workspaces → the registered row is visible.
    let listed = get_ipc_response(&webview, invoke_request("list_workspaces", serde_json::json!({})))
        .expect("list_workspaces dispatch failed")
        .deserialize::<serde_json::Value>()
        .expect("list_workspaces response deserialization");
    let entries = listed.as_array().expect("list is an array");
    assert_eq!(entries.len(), 1, "exactly one registered workspace");
    assert_eq!(entries[0]["id"], "native-ipc-1");

    // write_project_file → real bytes on the real disk.
    get_ipc_response(
        &webview,
        invoke_request(
            "write_project_file",
            serde_json::json!({
                "workspace_id": "native-ipc-1",
                "file": {
                    "path": "src/App.tsx",
                    "content": "export function App() { return null; }",
                    "language": "tsx"
                }
            }),
        ),
    )
    .expect("write_project_file dispatch failed")
    .deserialize::<serde_json::Value>()
    .expect("write_project_file response deserialization");
    let on_disk = std::fs::read_to_string(std::path::Path::new(&root).join("src/App.tsx"))
        .expect("written file exists on disk");
    assert_eq!(on_disk, "export function App() { return null; }");

    // read_project_file → round-trips the content.
    let read = get_ipc_response(
        &webview,
        invoke_request(
            "read_project_file",
            serde_json::json!({
                "workspace_id": "native-ipc-1",
                "path": "src/App.tsx"
            }),
        ),
    )
    .expect("read_project_file dispatch failed")
    .deserialize::<serde_json::Value>()
    .expect("read_project_file response deserialization");
    assert_eq!(read["content"], "export function App() { return null; }");

    // Missing file is Ok(None), not an error.
    let missing = get_ipc_response(
        &webview,
        invoke_request(
            "read_project_file",
            serde_json::json!({
                "workspace_id": "native-ipc-1",
                "path": "src/nope.tsx"
            }),
        ),
    )
    .expect("missing-file read dispatch should not error")
    .deserialize::<serde_json::Value>()
    .expect("missing-file read response deserialization");
    assert!(missing.is_null(), "missing file returns null, got {missing}");

    // list_project_files → the written file.
    let files = get_ipc_response(
        &webview,
        invoke_request("list_project_files", serde_json::json!({ "workspace_id": "native-ipc-1" })),
    )
    .expect("list_project_files dispatch failed")
    .deserialize::<serde_json::Value>()
    .expect("list_project_files response deserialization");
    let entries = files.as_array().expect("file list is an array");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["path"], "src/App.tsx");

    cleanup(&xdg);
}

#[test]
fn native_ipc_rejects_traversal_and_unregistered_ids() {
    let _guard = XDG_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let (webview, xdg) = build_native_app("reject");

    // Workspace must exist before path validation means anything.
    get_ipc_response(
        &webview,
        invoke_request("create_workspace", serde_json::json!({ "input": { "id": "native-ipc-2" } })),
    )
    .expect("create_workspace dispatch failed")
    .deserialize::<serde_json::Value>()
    .expect("create_workspace response deserialization");

    // In-root path validates.
    let ok = get_ipc_response(
        &webview,
        invoke_request(
            "validate_workspace_path",
            serde_json::json!({ "workspace_id": "native-ipc-2", "path": "src/main.tsx" }),
        ),
    )
    .expect("validate dispatch failed")
    .deserialize::<serde_json::Value>()
    .expect("validate response deserialization");
    assert_eq!(ok["valid"], true, "in-root path should validate: {ok}");

    // Traversal is rejected with valid=false, not an error.
    let traversal = get_ipc_response(
        &webview,
        invoke_request(
            "validate_workspace_path",
            serde_json::json!({ "workspace_id": "native-ipc-2", "path": "../../etc/passwd" }),
        ),
    )
    .expect("validate dispatch failed")
    .deserialize::<serde_json::Value>()
    .expect("validate response deserialization");
    assert_eq!(traversal["valid"], false, "traversal must be rejected");
    assert!(
        !traversal["issues"].as_array().unwrap().is_empty(),
        "rejection carries issues: {traversal}"
    );

    // Unregistered workspace id is a hard IPC error.
    let orphan = get_ipc_response(
        &webview,
        invoke_request(
            "read_project_file",
            serde_json::json!({ "workspace_id": "not-registered", "path": "src/App.tsx" }),
        ),
    );
    let error = orphan.expect_err("unregistered id must be a command error");
    let message = match error.as_str() {
        Some(s) => s.to_string(),
        None => error.to_string(),
    };
    assert!(
        message.contains("workspace unavailable"),
        "error should report the unknown workspace, got: {message}"
    );

    // Duplicate registration of the same id is rejected (unique-ID registration).
    let dup = get_ipc_response(
        &webview,
        invoke_request("create_workspace", serde_json::json!({ "input": { "id": "native-ipc-2" } })),
    );
    assert!(dup.is_err(), "duplicate workspace id must be rejected");

    cleanup(&xdg);
}
