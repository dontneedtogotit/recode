use super::domain::{DomainError, Result};
use self::safe_io::RootDir;
use super::tauri_commands::capture_diagnostics;
use rusqlite::OptionalExtension;
use std::io::Read;
use std::path::{Path, PathBuf};

pub mod safe_io;
mod reconciler;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceInfo {
    pub id: String,
    pub root: PathBuf,
}

pub trait WorkspaceService {
    fn create_workspace(&self, id: &str, root: &Path) -> Result<WorkspaceInfo>;
    fn list_workspaces(&self, root: &Path) -> Result<Vec<WorkspaceInfo>, DomainError>;
}

pub struct LocalWorkspaceService;

const MAX_PROJECT_FILE_BYTES: usize = 1024 * 1024;
const MAX_LISTED_FILES: usize = 256;
const MAX_LISTED_BYTES: usize = 8 * 1024 * 1024;
const MAX_VISITED_ENTRIES: usize = 2048;
const MAX_LIST_DEPTH: usize = 6;

struct PendingWorkspaceDirectory {
    path: PathBuf,
    committed: bool,
}

impl Drop for PendingWorkspaceDirectory {
    fn drop(&mut self) {
        if !self.committed {
            // Never recursively remove a directory another actor has started using.
            let _ = std::fs::remove_dir(&self.path);
        }
    }
}

impl WorkspaceService for LocalWorkspaceService {
    fn create_workspace(&self, id: &str, root: &Path) -> Result<WorkspaceInfo> {
        if id.is_empty() || id == "." || id == ".." || id == "recode.db" || id.contains(['/', '\\', '\0']) {
            return Err(DomainError::invalid_input("workspace ID must be one safe path component"));
        }
        reject_symlink(root)?;
        let target = root.join(id);
        reject_symlink(&target)?;
        std::fs::create_dir_all(root).map_err(|e| DomainError::internal(format!("workspace root mkdir failed: {e}")))?;
        reject_symlink(root)?;
        reject_symlink(&root.join("recode.db"))?;
        std::fs::create_dir(&target)
            .map_err(|e| {
                let _ = capture_diagnostics(super::tauri_commands::CaptureDiagnosticsInput {
                    kind: "filesystem".into(),
                    message: "workspace mkdir failed".into(),
                    detail: Some(e.to_string()),
                });
                DomainError::internal(format!("workspace mkdir failed: {e}"))
            })?;
        let mut pending = PendingWorkspaceDirectory { path: target.clone(), committed: false };

        let db_path = root.join("recode.db");
        reject_symlink(&db_path)?;
        let conn = rusqlite::Connection::open(&db_path)
            .map_err(|e| {
                let _ = capture_diagnostics(super::tauri_commands::CaptureDiagnosticsInput {
                    kind: "filesystem".into(),
                    message: "workspace db open failed".into(),
                    detail: Some(e.to_string()),
                });
                DomainError::internal(format!("workspace db open failed: {e}"))
            })?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS projects (id TEXT PRIMARY KEY, name TEXT NOT NULL, path TEXT NOT NULL)",
            [],
        )
        .map_err(|e| {
            let _ = capture_diagnostics(super::tauri_commands::CaptureDiagnosticsInput {
                kind: "filesystem".into(),
                message: "workspace schema failed".into(),
                detail: Some(e.to_string()),
            });
            DomainError::internal(format!("workspace schema failed: {e}"))
        })?;
        conn.execute(
            "INSERT INTO projects (id, name, path) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, id, target.display().to_string()],
        )
        .map_err(|e| {
            let _ = capture_diagnostics(super::tauri_commands::CaptureDiagnosticsInput {
                kind: "filesystem".into(),
                message: "workspace insert failed".into(),
                detail: Some(e.to_string()),
            });
            DomainError::internal(format!("workspace insert failed: {e}"))
        })?;
        pending.committed = true;
        // Make both the new directory entry and its parent durable before
        // reporting success; a crash right after creation should not leave a
        // registered workspace whose directory never reached disk.
        let _ = self::safe_io::sync_directory(&target);
        let _ = self::safe_io::sync_directory(root);

        Ok(WorkspaceInfo {
            id: id.into(),
            root: target,
        })
    }

    fn list_workspaces(&self, root: &Path) -> Result<Vec<WorkspaceInfo>, DomainError> {
        reject_symlink(root)?;
        let db_path = root.join("recode.db");
        reject_symlink(&db_path)?;
        if !db_path.exists() { return Ok(Vec::new()); }
        let conn = rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| {
                let _ = capture_diagnostics(super::tauri_commands::CaptureDiagnosticsInput {
                    kind: "filesystem".into(),
                    message: "workspace db open failed".into(),
                    detail: Some(e.to_string()),
                });
                DomainError::internal(format!("workspace db open failed: {e}"))
            })?;
        let mut stmt = conn
            .prepare("SELECT id, path FROM projects")
            .map_err(|e| {
                let _ = capture_diagnostics(super::tauri_commands::CaptureDiagnosticsInput {
                    kind: "filesystem".into(),
                    message: "workspace query prepare failed".into(),
                    detail: Some(e.to_string()),
                });
                DomainError::internal(format!("workspace query prepare failed: {e}"))
            })?;
        let rows = stmt
            .query_map([], |row| {
                Ok(WorkspaceInfo {
                    id: row.get(0)?,
                    root: PathBuf::from(row.get::<_, String>(1)?),
                })
            })
            .map_err(|e| {
                let _ = capture_diagnostics(super::tauri_commands::CaptureDiagnosticsInput {
                    kind: "filesystem".into(),
                    message: "workspace query failed".into(),
                    detail: Some(e.to_string()),
                });
                DomainError::internal(format!("workspace query failed: {e}"))
            })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(
                row.map_err(|e| {
                    let _ = capture_diagnostics(super::tauri_commands::CaptureDiagnosticsInput {
                        kind: "filesystem".into(),
                        message: "workspace row read failed".into(),
                        detail: Some(e.to_string()),
                    });
                    DomainError::internal(format!("workspace row read failed: {e}"))
                })?,
            );
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectFile {
    pub path: String,
    pub content: String,
    pub language: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PathValidation {
    pub valid: bool,
    pub normalized: String,
    pub issues: Vec<String>,
}

pub fn is_path_traversal(path: &str) -> bool {
    let mut depth: i32 = 0;
    for c in std::path::Path::new(path).components() {
        match c {
            std::path::Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return true;
                }
            }
            std::path::Component::Normal(_) => depth += 1,
            _ => {}
        }
    }
    false
}

pub fn sanitize_path(path: &str) -> String {
    let mut out = Vec::new();
    for c in std::path::Path::new(path).components() {
        match c {
            std::path::Component::ParentDir => {
                if !out.is_empty() {
                    out.pop();
                }
            }
            std::path::Component::Normal(s) => out.push(s.to_string_lossy().to_string()),
            std::path::Component::RootDir => out.push("".into()),
            _ => {}
        }
    }
    out.join("/").trim_start_matches('/').to_string()
}

/// Reject rather than silently normalize ambiguous or escaping file names.
pub fn validate_project_path(path: &str) -> Result<String> {
    if path.is_empty() || path.contains(['\\', '\0'])
        || path.split('/').any(|part| part.is_empty() || part == "." || part == ".." || part.starts_with(".recode-write-"))
        || Path::new(path).components().any(|part| !matches!(part, std::path::Component::Normal(_))) {
        return Err(DomainError::invalid_input("project path must be a non-empty relative file path"));
    }
    if path.split('/').any(is_private_component) {
        return Err(DomainError::invalid_input("private workspace paths cannot be accessed over IPC"));
    }
    Ok(path.into())
}

fn is_private_component(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower.starts_with(".env")
        || matches!(
            lower.as_str(),
            ".git" | ".ssh" | ".aws" | ".npmrc" | ".netrc" | ".pypirc" | ".recode"
        )
    {
        return true;
    }
    // Comprehensive secret classification: credential stores and tool state
    // that hold tokens, keys or cluster credentials, plus common private-key
    // file names anywhere in the tree.
    const SECRET_DIRECTORIES: [&str; 11] = [
        ".kube", ".docker", ".gnupg", ".azure", ".databricks",
        ".netlify", ".vercel", ".wrangler", ".heroku", ".terraform.d", ".vault",
    ];
    if SECRET_DIRECTORIES.contains(&lower.as_str()) {
        return true;
    }
    const SECRET_FILES: [&str; 10] = [
        ".git-credentials", ".gitconfig", ".htpasswd", ".pgpass", ".my.cnf",
        ".vault-token", ".terraform", ".kube-config", "secrets.toml", "credentials.json",
    ];
    if SECRET_FILES.contains(&lower.as_str()) {
        return true;
    }
    const SECRET_EXTENSIONS: [&str; 8] = [".pem", ".key", ".p12", ".pfx", ".jks", ".tfstate", ".tfvars", ".secret"];
    if SECRET_EXTENSIONS.iter().any(|ext| lower.ends_with(ext)) {
        return true;
    }
    // Conventional SSH key names: id_rsa, id_ed25519, id_ecdsa (+ variants).
    if lower.starts_with("id_") {
        return true;
    }
    false
}

fn is_generated_directory(name: &str) -> bool {
    matches!(name, "node_modules" | "dist" | "target" | "coverage" | ".next")
}

fn reject_symlink(path: &Path) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(DomainError::invalid_input(format!("symlink path not allowed: {}", path.display()))),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(DomainError::internal(format!("path inspection failed: {error}"))),
    }
}

/// Resolve only a named, existing workspace under the app-owned projects directory.
pub fn managed_workspace_root(projects: &Path, id: &str) -> Result<PathBuf> {
    if id.is_empty() || id == "." || id == ".." || id == "recode.db" || id.contains(['/', '\\', '\0']) {
        return Err(DomainError::invalid_input("invalid workspace ID"));
    }
    reject_symlink(projects)?;
    let base = projects.canonicalize()
        .map_err(|e| DomainError::invalid_input(format!("projects directory unavailable: {e}")))?;
    let target = projects.join(id);
    reject_symlink(&target)?;
    let resolved = target.canonicalize()
        .map_err(|e| DomainError::invalid_input(format!("workspace unavailable: {e}")))?;
    if resolved.parent() != Some(base.as_path()) || !resolved.is_dir() {
        return Err(DomainError::invalid_input("workspace is outside the managed projects directory"));
    }
    Ok(resolved)
}

/// Require both a contained directory and a matching registration before IPC file access.
pub fn registered_workspace_root(projects: &Path, id: &str) -> Result<PathBuf> {
    let resolved = managed_workspace_root(projects, id)?;
    let db_path = projects.join("recode.db");
    reject_symlink(&db_path)?;
    let conn = rusqlite::Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ).map_err(|error| DomainError::not_found(format!("workspace registration unavailable: {error}")))?;
    let registered: Option<String> = conn.query_row(
        "SELECT path FROM projects WHERE id = ?1",
        [id],
        |row| row.get(0),
    ).optional().map_err(|error| DomainError::internal(format!("workspace registration lookup failed: {error}")))?;
    let registered = registered.ok_or_else(|| DomainError::not_found("workspace is not registered"))?;
    let registered_path = Path::new(&registered).canonicalize()
        .map_err(|error| DomainError::invalid_input(format!("registered workspace path unavailable: {error}")))?;
    if registered_path != resolved {
        return Err(DomainError::invalid_input("workspace registration points outside its managed directory"));
    }
    Ok(resolved)
}

pub(crate) fn safe_file_target(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = validate_project_path(relative)?;
    let metadata = std::fs::symlink_metadata(root)
        .map_err(|error| DomainError::invalid_input(format!("workspace root unavailable: {error}")))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(DomainError::invalid_input("workspace root must be a directory, not a symlink"));
    }
    let mut target = root.to_path_buf();
    for component in Path::new(&path).components() {
        target.push(component);
        reject_symlink(&target)?;
    }
    Ok(target)
}

impl LocalWorkspaceService {
    pub fn write_project_file(&self, workspace_root: &Path, file: &ProjectFile) -> Result<()> {
        let sanitized = validate_project_path(&file.path)?;
        if file.content.len() > MAX_PROJECT_FILE_BYTES {
            return Err(DomainError::invalid_input("project file exceeds the size limit"));
        }
        safe_file_target(workspace_root, &sanitized)?;
        // Descriptor-relative IO: the root is opened and verified once, every
        // component is resolved with O_NOFOLLOW from that descriptor, so a
        // symlink swapped in after validation cannot redirect the write.
        let root = RootDir::open(workspace_root).map_err(|e| {
            let _ = capture_diagnostics(super::tauri_commands::CaptureDiagnosticsInput {
                kind: "filesystem".into(),
                message: format!("root descriptor open failed for {}", sanitized),
                detail: Some(e.to_string()),
            });
            DomainError::invalid_input(format!("workspace root unavailable: {e}"))
        })?;
        let result = root
            .write_file(&sanitized, file.content.as_bytes(), MAX_PROJECT_FILE_BYTES)
            .map_err(|e| {
                let _ = capture_diagnostics(super::tauri_commands::CaptureDiagnosticsInput {
                    kind: "filesystem".into(),
                    message: format!("write failed for {}", sanitized),
                    detail: Some(e.to_string()),
                });
                DomainError::internal(format!("write failed: {e}"))
            });
        // Release the descriptor before checking that no temp artifact leaked.
        drop(root);
        result?;
        if let Ok(entries) = std::fs::read_dir(workspace_root) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if name.to_str().is_some_and(|n| n.starts_with(".recode-write-")) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        Ok(())
    }

    pub fn read_project_file(&self, workspace_root: &Path, path: &str) -> Result<Option<ProjectFile>> {
        let sanitized = validate_project_path(path)?;
        safe_file_target(workspace_root, &sanitized)?;
        // Same descriptor-relative guarantee as writes; missing files stay a
        // None result rather than an error.
        let root = RootDir::open(workspace_root).map_err(|e| {
            DomainError::invalid_input(format!("workspace root unavailable: {e}"))
        })?;
        let bytes = match root.read_file(&sanitized, MAX_PROJECT_FILE_BYTES) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                let _ = capture_diagnostics(super::tauri_commands::CaptureDiagnosticsInput {
                    kind: "filesystem".into(),
                    message: format!("read failed for {}", sanitized),
                    detail: Some(e.to_string()),
                });
                return Err(DomainError::internal(format!("read failed: {e}")));
            }
        };
        let content = String::from_utf8(bytes).map_err(|_| DomainError::invalid_input("project file is not UTF-8 text"))?;
        let language = sanitized
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_string())
            .filter(|ext| !ext.is_empty());
        Ok(Some(ProjectFile {
            path: sanitized,
            content,
            language,
        }))
    }

    pub fn list_project_files(&self, workspace_root: &Path) -> Result<Vec<ProjectFile>, DomainError> {
        let mut out = Vec::new();
        let metadata = std::fs::symlink_metadata(workspace_root)
            .map_err(|e| DomainError::invalid_input(format!("workspace root unavailable: {e}")))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(DomainError::invalid_input("workspace root must be a directory, not a symlink"));
        }
        let mut visited = 0;
        let mut bytes = 0;
        self.collect_files(workspace_root, workspace_root, 0, &mut out, &mut visited, &mut bytes)?;
        Ok(out)
    }

    fn collect_files(
        &self,
        root: &Path,
        current: &Path,
        depth: usize,
        out: &mut Vec<ProjectFile>,
        visited: &mut usize,
        total_bytes: &mut usize,
    ) -> Result<()> {
        if depth > MAX_LIST_DEPTH {
            return Err(DomainError::invalid_input("workspace tree exceeds the depth limit"));
        }
        let entries = std::fs::read_dir(current)
            .map_err(|e| DomainError::internal(format!("workspace directory read failed: {e}")))?;
        for entry in entries {
            let entry = entry.map_err(|e| DomainError::internal(format!("workspace entry read failed: {e}")))?;
            *visited += 1;
            if *visited > MAX_VISITED_ENTRIES {
                return Err(DomainError::invalid_input("workspace tree exceeds the entry limit"));
            }
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.starts_with(".recode-write-") || is_private_component(name) {
                continue;
            }
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|e| DomainError::internal(format!("workspace entry inspection failed: {e}")))?;
            if metadata.file_type().is_symlink() { continue; }
            if metadata.is_dir() {
                if is_generated_directory(name) { continue; }
                self.collect_files(root, &path, depth + 1, out, visited, total_bytes)?;
            } else if metadata.is_file() {
                let rel = path.strip_prefix(root)
                    .map_err(|_| DomainError::internal("workspace entry left its root"))?
                    .to_str().ok_or_else(|| DomainError::invalid_input("workspace file path is not UTF-8"))?.to_string();
                safe_file_target(root, &rel)?;
                if metadata.len() > MAX_PROJECT_FILE_BYTES as u64 {
                    return Err(DomainError::invalid_input(format!("workspace file exceeds size limit: {rel}")));
                }
                let mut bytes = Vec::new();
                std::fs::File::open(&path)
                    .and_then(|handle| handle.take(MAX_PROJECT_FILE_BYTES as u64 + 1).read_to_end(&mut bytes))
                    .map_err(|e| DomainError::internal(format!("workspace file read failed for {rel}: {e}")))?;
                if bytes.len() > MAX_PROJECT_FILE_BYTES {
                    return Err(DomainError::invalid_input(format!("workspace file exceeds size limit: {rel}")));
                }
                let Ok(content) = String::from_utf8(bytes) else { continue };
                *total_bytes += content.len();
                if *total_bytes > MAX_LISTED_BYTES || out.len() >= MAX_LISTED_FILES {
                    return Err(DomainError::invalid_input("workspace listing exceeds its file or byte limit"));
                }
                let language = rel.rsplit_once('.').map(|(_, ext)| ext.to_string()).filter(|ext| !ext.is_empty());
                out.push(ProjectFile { path: rel, content, language });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    static SERVICE: OnceLock<LocalWorkspaceService> = OnceLock::new();
    fn service() -> &'static LocalWorkspaceService {
        SERVICE.get_or_init(|| LocalWorkspaceService)
    }

    #[test]
    fn creates_workspace_directory() {
        let temp = std::env::temp_dir().join("recode-workspace-test");
        let _ = std::fs::remove_dir_all(&temp);
        let info = service()
            .create_workspace("ws-1", &temp)
            .expect("create workspace");
        assert!(info.root.exists());
        assert_eq!(info.id, "ws-1");
    }

    #[test]
    fn lists_workspaces_after_creation() {
        let temp = std::env::temp_dir().join("recode-workspace-list-test");
        let _ = std::fs::remove_dir_all(&temp);
        service().create_workspace("ws-a", &temp).expect("create a");
        service().create_workspace("ws-b", &temp).expect("create b");
        let list = service().list_workspaces(&temp).expect("list workspaces");
        let ids: Vec<_> = list.iter().map(|w| w.id.as_str()).collect();
        assert!(ids.contains(&"ws-a"));
        assert!(ids.contains(&"ws-b"));
    }

    #[test]
    fn rejects_hostile_workspace_ids_without_creating_outside_directories() {
        let root = std::env::temp_dir().join(format!("recode-id-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        for id in ["", ".", "..", "../escape", "/tmp/escape", "nested/child", "bad\\child", "a\0b"] {
            assert!(service().create_workspace(id, &root).is_err(), "accepted {id:?}");
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_unsafe_file_paths_and_preserves_valid_nested_files() {
        let root = std::env::temp_dir().join(format!("recode-files-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        for path in ["", ".", "../escape", "/tmp/escape", "a\\..\\escape", "a\0b"] {
            let file = ProjectFile { path: path.into(), content: "bad".into(), language: None };
            assert!(service().write_project_file(&root, &file).is_err(), "accepted {path:?}");
            assert!(service().read_project_file(&root, path).is_err(), "read accepted {path:?}");
        }
        let file = ProjectFile { path: "src/App.tsx".into(), content: "hello".into(), language: None };
        service().write_project_file(&root, &file).unwrap();
        assert_eq!(service().read_project_file(&root, &file.path).unwrap().unwrap().content, "hello");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_symlinks_and_private_paths() {
        let root = std::env::temp_dir().join(format!("recode-sym-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let outside = std::env::temp_dir().join(format!("recode-outside-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.txt"), "private").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("linked")).unwrap();
        std::os::unix::fs::symlink(outside.join("secret.txt"), root.join("file.txt")).unwrap();
        let file = ProjectFile { path: "linked/secret.txt".into(), content: "overwrite".into(), language: None };
        assert!(service().write_project_file(&root, &file).is_err());
        assert!(service().read_project_file(&root, &file.path).is_err());
        assert!(service().read_project_file(&root, "file.txt").is_err());
        assert!(service().write_project_file(&root, &ProjectFile { path: "file.txt".into(), ..file }).is_err());
        assert!(service().list_project_files(&root).unwrap().is_empty());
        assert!(safe_file_target(&root, "linked/secret.txt").is_err());
        assert_eq!(std::fs::read_to_string(outside.join("secret.txt")).unwrap(), "private");
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(outside);
    }

    #[test]
    fn managed_workspace_requires_registration() {
        let data = tempfile::tempdir().unwrap();
        let base = data.path().join("projects");
        std::fs::create_dir_all(base.join("mine")).unwrap();
        assert_eq!(managed_workspace_root(&base, "mine").unwrap(), base.join("mine"));
        for id in ["", "../outside", "/tmp", "missing"] {
            assert!(managed_workspace_root(&base, id).is_err(), "accepted {id:?}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(data.path(), base.join("linked")).unwrap();
            assert!(managed_workspace_root(&base, "linked").is_err());
        }
    }

    #[test]
    fn registered_workspace_checks_sqlite_row() {
        let data = tempfile::tempdir().unwrap();
        let projects = std::env::temp_dir().join(format!("recode-registered-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&projects).unwrap();
        service().create_workspace("mine", &projects).unwrap();
        std::fs::create_dir(projects.join("rogue")).unwrap();
        assert_eq!(registered_workspace_root(&projects, "mine").unwrap(), projects.join("mine"));
        assert!(registered_workspace_root(&projects, "rogue").is_err());
        let db = rusqlite::Connection::open(projects.join("recode.db")).unwrap();
        db.execute(
            "UPDATE projects SET path = ?1 WHERE id = 'mine'",
            [data.path().display().to_string()],
        )
        .unwrap();
        assert!(registered_workspace_root(&projects, "mine").is_err());
    }

    #[test]
    fn failed_registration_does_not_leave_an_unowned_workspace_directory() {
        let projects = std::env::temp_dir().join(format!("recode-failed-reg-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&projects).unwrap();
        let db = rusqlite::Connection::open(projects.join("recode.db")).unwrap();
        db.execute("CREATE TABLE projects (id TEXT PRIMARY KEY)", []).unwrap();
        assert!(service().create_workspace("failed", &projects).is_err());
        assert!(!projects.join("failed").exists());
    }

    #[test]
    fn file_replacement_preserves_old_data_on_error_and_cleans_temp_files() {
        let data = tempfile::tempdir().unwrap();
        let root = data.path();
        let file = ProjectFile { path: "main.ts".into(), content: "before".into(), language: None };
        service().write_project_file(root, &file).unwrap();
        service().write_project_file(root, &ProjectFile { content: "after".into(), ..file }).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("main.ts")).unwrap(), "after");
        std::fs::create_dir(root.join("directory.ts")).unwrap();
        let invalid = ProjectFile { path: "directory.ts".into(), content: "no".into(), language: None };
        assert!(service().write_project_file(root, &invalid).is_err());
        assert_eq!(std::fs::read_to_string(root.join("main.ts")).unwrap(), "after");
        assert!(std::fs::read_dir(root).unwrap().all(|entry| !entry.unwrap().file_name().to_string_lossy().starts_with(".recode-write-")));
    }

    #[test]
    fn listing_skips_private_and_generated_paths() {
        let root = std::env::temp_dir().join(format!("recode-listing-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("main.ts"), "export {};").unwrap();
        std::fs::create_dir(root.join("node_modules")).unwrap();
        std::fs::write(root.join("node_modules/package.js"), "dependency").unwrap();
        for path in [".env", "nested/.env.production", "nested/.npmrc", ".git/config", ".ssh/id_rsa"] {
            assert!(validate_project_path(path).is_err(), "accepted private path {path}");
            assert!(service().read_project_file(&root, path).is_err());
        }
        assert_eq!(service().list_project_files(&root).unwrap().iter().map(|file| file.path.as_str()).collect::<Vec<_>>(), vec!["main.ts"]);
        std::fs::write(root.join("huge.ts"), vec![b'a'; 1024 * 1024 + 1]).unwrap();
        assert!(service().read_project_file(&root, "huge.ts").is_err());
        assert!(service().write_project_file(&root, &ProjectFile { path: "written-huge.ts".into(), content: "a".repeat(1024 * 1024 + 1), language: None }).is_err());
        assert!(!root.join("written-huge.ts").exists());
        assert!(service().list_project_files(&root).is_err());
        assert!(service().list_project_files(&root.join("missing")).is_err());
    }

    #[test]
    fn write_preserves_existing_file_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let data = tempfile::tempdir().unwrap();
        let path = data.path().join("private.txt");
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        service().write_project_file(data.path(), &ProjectFile {
            path: "private.txt".into(),
            content: "new".into(),
            language: None,
        }).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        service().write_project_file(data.path(), &ProjectFile {
            path: "fresh.txt".into(),
            content: "private".into(),
            language: None,
        }).unwrap();
        assert_eq!(
            std::fs::metadata(data.path().join("fresh.txt")).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn temporary_write_artifacts_are_not_user_project_files() {
        let data = tempfile::tempdir().unwrap();
        std::fs::write(data.path().join(".recode-write-orphan.tmp"), "partial").unwrap();
        assert!(service().list_project_files(data.path()).unwrap().is_empty());
        assert!(service().read_project_file(data.path(), ".recode-write-orphan.tmp").is_err());
        assert!(service().write_project_file(data.path(), &ProjectFile {
            path: ".recode-write-orphan.tmp".into(),
            content: "bad".into(),
            language: None,
        }).is_err());
    }
}
