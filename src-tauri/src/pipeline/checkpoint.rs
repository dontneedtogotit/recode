//! Generation checkpoint persistence for resumable partial runs (ported from
//! devcode's `CheckpointManager.ts`, with per-stage progress persistence from
//! `ProgressTracker.ts` adapted to recode's per-file write loop).
//!
//! The ledger (`pipeline/ledger.rs`) only records fully clean runs, so a run
//! cancelled or failed mid-write used to lose all partial progress and re-bill
//! the provider for every file on the next attempt. The checkpoint closes that
//! gap: each file written during an attempt is durably recorded as it lands,
//! and the next run resumes matching files instead of regenerating them. On a
//! clean run the checkpoint is cleared and the ledger takes over (devcode
//! clears the checkpoint after successful completion). All checkpoint IO is
//! fail-soft: a missing, corrupt or foreign checkpoint never fails the build,
//! it just disables resume.

use crate::domain::{DomainError, Result};
use crate::event_bus::now_millis;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Keyed to the workspace like the ledger: the engine generates into one
/// workspace root at a time, so the checkpoint reuses the ledger's project id.
pub const CHECKPOINT_DIR: &str = ".recode";
pub const CHECKPOINT_FILE: &str = "generation-checkpoint.json";

/// Bounded constants, mirroring the ledger's size envelope.
pub const MAX_CHECKPOINT_FILES: usize = 64;
pub const MAX_CHECKPOINT_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CheckpointStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl CheckpointStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            CheckpointStatus::Completed | CheckpointStatus::Failed | CheckpointStatus::Cancelled
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckpointFileEntry {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenerationCheckpoint {
    pub schema_version: u8,
    pub project_id: String,
    /// Identifies the run attempt; every run starts a fresh attempt so zombie
    /// progress writes from older attempts can be rejected.
    pub attempt_id: String,
    /// Hash of the generation inputs (goal + model + ordered file paths),
    /// matching the ledger's `compute_generation_inputs_hash`.
    pub inputs_hash: String,
    pub status: CheckpointStatus,
    /// Monotonic per attempt; stale writes are rejected (devcode's
    /// `sequenceNumber` guard).
    pub sequence: u64,
    pub files: Vec<CheckpointFileEntry>,
    pub saved_at: u64,
}

impl GenerationCheckpoint {
    pub fn running(
        project_id: impl Into<String>,
        attempt_id: impl Into<String>,
        inputs_hash: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: 1,
            project_id: project_id.into(),
            attempt_id: attempt_id.into(),
            inputs_hash: inputs_hash.into(),
            status: CheckpointStatus::Running,
            sequence: 0,
            files: Vec::new(),
            saved_at: now_millis(),
        }
    }

    /// Upsert one written file and advance the sequence. Bounded like the
    /// ledger; over-bound records are the caller's error to ignore.
    pub fn record(&mut self, path: &str, sha256: String) -> Result<()> {
        if self.files.len() >= MAX_CHECKPOINT_FILES
            && !self.files.iter().any(|entry| entry.path == path)
        {
            return Err(DomainError::conflict(
                "generation checkpoint file limit reached",
            ));
        }
        if let Some(entry) = self.files.iter_mut().find(|entry| entry.path == path) {
            entry.sha256 = sha256;
        } else {
            self.files.push(CheckpointFileEntry {
                path: path.to_string(),
                sha256,
            });
        }
        self.sequence += 1;
        Ok(())
    }

    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    pub fn to_json(&self) -> Result<String> {
        let json = serde_json::to_string(self)?;
        if json.len() > MAX_CHECKPOINT_BYTES {
            return Err(DomainError::conflict(
                "generation checkpoint exceeds size bound",
            ));
        }
        Ok(json)
    }
}

pub fn checkpoint_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join(CHECKPOINT_DIR).join(CHECKPOINT_FILE)
}

/// Fail-soft load: a missing, corrupt, wrong-schema or wrong-project
/// checkpoint reads as `None` and resume is simply disabled.
pub fn load_checkpoint(workspace_root: &Path, project_id: &str) -> Option<GenerationCheckpoint> {
    let bytes = crate::workspace::safe_io::RootDir::open(workspace_root)
        .and_then(|root| {
            root.read_file(
                &format!("{CHECKPOINT_DIR}/{CHECKPOINT_FILE}"),
                MAX_CHECKPOINT_BYTES,
            )
        })
        .ok()?;
    let checkpoint: GenerationCheckpoint = serde_json::from_slice(&bytes).ok()?;
    if checkpoint.schema_version != 1 {
        return None;
    }
    if checkpoint.project_id != project_id {
        return None;
    }
    Some(checkpoint)
}

/// Begin a new attempt: supersedes any previous checkpoint (the new run owns
/// the file), but is a no-op when the same attempt already started. Fail-soft.
pub fn start_attempt(
    workspace_root: &Path,
    project_id: &str,
    attempt_id: &str,
    inputs_hash: &str,
) {
    if let Some(existing) = load_checkpoint(workspace_root, project_id) {
        if existing.attempt_id == attempt_id {
            return;
        }
    }
    let fresh = GenerationCheckpoint::running(project_id, attempt_id, inputs_hash);
    write_checkpoint(workspace_root, &fresh);
}

/// Persist checkpoint progress with devcode `CheckpointManager` guards:
/// - reject writes onto a terminal checkpoint (zombie write after finalize);
/// - reject writes from a foreign attempt (never clobber another run);
/// - reject stale sequences for the same attempt.
/// Returns whether the checkpoint reached disk; IO failures are fail-soft.
pub fn persist_checkpoint(workspace_root: &Path, checkpoint: &GenerationCheckpoint) -> bool {
    if let Some(existing) = load_checkpoint(workspace_root, &checkpoint.project_id) {
        if existing.is_terminal() {
            return false;
        }
        if existing.attempt_id != checkpoint.attempt_id {
            return false;
        }
        if existing.sequence >= checkpoint.sequence {
            return false;
        }
    }
    write_checkpoint(workspace_root, checkpoint)
}

/// Mark the given attempt terminal (devcode records status transitions;
/// terminal checkpoints stop accepting progress writes). A no-op when the
/// checkpoint on disk belongs to a different attempt or does not exist.
pub fn finalize_attempt(
    workspace_root: &Path,
    project_id: &str,
    attempt_id: &str,
    status: CheckpointStatus,
) {
    let Some(mut checkpoint) = load_checkpoint(workspace_root, project_id) else {
        return;
    };
    if checkpoint.attempt_id != attempt_id {
        return;
    }
    checkpoint.status = status;
    write_checkpoint(workspace_root, &checkpoint);
}

/// Remove the checkpoint after a successful build (devcode `clearCheckpoint`);
/// fail-soft.
pub fn clear_checkpoint(workspace_root: &Path) {
    let _ = crate::workspace::safe_io::RootDir::open(workspace_root)
        .and_then(|root| root.remove_file(&format!("{CHECKPOINT_DIR}/{CHECKPOINT_FILE}")));
}

/// Files resumable from this checkpoint: entries recorded for the same inputs
/// whose bytes still match on disk. Symlinks are never resumed (same rule as
/// the ledger).
pub fn resume_paths(
    checkpoint: &GenerationCheckpoint,
    inputs_hash: &str,
    workspace_root: &Path,
    file_paths: &[String],
) -> Vec<String> {
    if checkpoint.inputs_hash != inputs_hash {
        return Vec::new();
    }
    file_paths
        .iter()
        .filter(|path| {
            let Some(entry) = checkpoint.files.iter().find(|entry| &entry.path == *path) else {
                return false;
            };
            let disk = workspace_root.join(path.as_str());
            if !std::fs::symlink_metadata(&disk).is_ok_and(|m| m.is_file()) {
                return false;
            }
            // Post-generation validation guard: a checkpoint entry whose
            // on-disk bytes are malformed (possible for checkpoints created
            // before the package.json gate) is not resumed.
            if !crate::pipeline::validation::is_valid_generated_file(path, workspace_root) {
                return false;
            }
            crate::pipeline::ledger::file_sha256_beneath(workspace_root, path)
                .is_some_and(|sha| sha == entry.sha256)
        })
        .cloned()
        .collect()
}

fn write_checkpoint(workspace_root: &Path, checkpoint: &GenerationCheckpoint) -> bool {
    let mut snapshot = checkpoint.clone();
    snapshot.saved_at = now_millis();
    let json = match snapshot.to_json() {
        Ok(json) => json,
        Err(_) => return false, // fail-soft on bounds breaches
    };
    let dir = workspace_root.join(CHECKPOINT_DIR);
    let _ = fs::create_dir_all(&dir); // bootstrap only; write re-opens without following links
    crate::workspace::safe_io::RootDir::open(workspace_root)
        .and_then(|root| {
            root.write_file(
                &format!("{CHECKPOINT_DIR}/{CHECKPOINT_FILE}"),
                json.as_bytes(),
                MAX_CHECKPOINT_BYTES,
            )
        })
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running_checkpoint(attempt: &str) -> GenerationCheckpoint {
        GenerationCheckpoint::running("workspace", attempt, "inputs-hash")
    }

    #[test]
    fn checkpoint_round_trips_fail_soft() {
        let root = tempfile::tempdir().unwrap();
        // Missing checkpoint -> None, not an error.
        assert!(load_checkpoint(root.path(), "workspace").is_none());
        start_attempt(root.path(), "workspace", "attempt-1", "inputs-hash");
        let mut checkpoint = running_checkpoint("attempt-1");
        checkpoint.record("README.md", "aa".repeat(32)).unwrap();
        assert!(persist_checkpoint(root.path(), &checkpoint));
        let loaded = load_checkpoint(root.path(), "workspace").unwrap();
        assert_eq!(loaded.attempt_id, "attempt-1");
        assert_eq!(loaded.sequence, 1);
        assert_eq!(loaded.files.len(), 1);
        assert!(loaded.saved_at > 0);
        // Corrupt checkpoint -> fail-soft None.
        std::fs::write(checkpoint_path(root.path()), "{not json").unwrap();
        assert!(load_checkpoint(root.path(), "workspace").is_none());
        // Wrong project id -> treated as no history.
        start_attempt(root.path(), "workspace", "attempt-2", "inputs-hash");
        assert!(load_checkpoint(root.path(), "other").is_none());
        // Wrong schema version -> treated as no history.
        let legacy = serde_json::json!({
            "schema_version": 0,
            "project_id": "workspace",
            "attempt_id": "attempt-3",
            "inputs_hash": "inputs-hash",
            "status": "running",
            "sequence": 1,
            "files": [],
            "saved_at": 0,
        });
        std::fs::write(checkpoint_path(root.path()), legacy.to_string()).unwrap();
        assert!(load_checkpoint(root.path(), "workspace").is_none());
    }

    #[test]
    fn persist_rejects_terminal_foreign_and_stale_writes() {
        let root = tempfile::tempdir().unwrap();
        start_attempt(root.path(), "workspace", "attempt-1", "inputs-hash");
        let mut checkpoint = running_checkpoint("attempt-1");
        checkpoint.record("README.md", "aa".repeat(32)).unwrap();
        assert!(persist_checkpoint(root.path(), &checkpoint));
        assert!(
            !persist_checkpoint(root.path(), &checkpoint),
            "same sequence must be rejected as stale"
        );
        checkpoint.record("src/main.tsx", "bb".repeat(32)).unwrap();
        assert!(persist_checkpoint(root.path(), &checkpoint));
        // A foreign attempt never clobbers the running attempt.
        let mut foreign = running_checkpoint("attempt-2");
        foreign.record("hijack.ts", "cc".repeat(32)).unwrap();
        assert!(!persist_checkpoint(root.path(), &foreign));
        assert_eq!(
            load_checkpoint(root.path(), "workspace").unwrap().attempt_id,
            "attempt-1"
        );
        // A terminal checkpoint rejects further progress writes (devcode guard).
        finalize_attempt(root.path(), "workspace", "attempt-1", CheckpointStatus::Cancelled);
        checkpoint.record("late.ts", "dd".repeat(32)).unwrap();
        assert!(!persist_checkpoint(root.path(), &checkpoint));
        let loaded = load_checkpoint(root.path(), "workspace").unwrap();
        assert_eq!(loaded.status, CheckpointStatus::Cancelled);
        assert_eq!(loaded.files.len(), 2, "late write must not be recorded");
    }

    #[test]
    fn start_attempt_supersedes_foreign_attempts_and_is_idempotent_per_attempt() {
        let root = tempfile::tempdir().unwrap();
        start_attempt(root.path(), "workspace", "attempt-1", "inputs-hash-1");
        let mut checkpoint = running_checkpoint("attempt-1");
        checkpoint.record("README.md", "aa".repeat(32)).unwrap();
        assert!(persist_checkpoint(root.path(), &checkpoint));
        // Same attempt: start is a no-op that keeps recorded progress.
        start_attempt(root.path(), "workspace", "attempt-1", "inputs-hash-1");
        assert_eq!(
            load_checkpoint(root.path(), "workspace").unwrap().files.len(),
            1
        );
        // New attempt: supersedes the previous one.
        start_attempt(root.path(), "workspace", "attempt-2", "inputs-hash-2");
        let loaded = load_checkpoint(root.path(), "workspace").unwrap();
        assert_eq!(loaded.attempt_id, "attempt-2");
        assert!(loaded.files.is_empty());
        assert_eq!(loaded.inputs_hash, "inputs-hash-2");
    }

    #[test]
    fn finalize_only_applies_to_the_matching_attempt() {
        let root = tempfile::tempdir().unwrap();
        // Missing checkpoint: no-op.
        finalize_attempt(root.path(), "workspace", "attempt-1", CheckpointStatus::Failed);
        assert!(load_checkpoint(root.path(), "workspace").is_none());
        start_attempt(root.path(), "workspace", "attempt-1", "inputs-hash");
        // Foreign attempt: no-op.
        finalize_attempt(root.path(), "workspace", "attempt-2", CheckpointStatus::Failed);
        assert_eq!(
            load_checkpoint(root.path(), "workspace").unwrap().status,
            CheckpointStatus::Running
        );
        finalize_attempt(root.path(), "workspace", "attempt-1", CheckpointStatus::Failed);
        assert_eq!(
            load_checkpoint(root.path(), "workspace").unwrap().status,
            CheckpointStatus::Failed
        );
    }

    #[test]
    fn clear_removes_the_checkpoint_fail_soft() {
        let root = tempfile::tempdir().unwrap();
        // Missing file: no panic.
        clear_checkpoint(root.path());
        start_attempt(root.path(), "workspace", "attempt-1", "inputs-hash");
        assert!(load_checkpoint(root.path(), "workspace").is_some());
        clear_checkpoint(root.path());
        assert!(load_checkpoint(root.path(), "workspace").is_none());
    }

    #[test]
    fn resume_paths_requires_matching_inputs_disk_bytes_and_regular_files() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("src")).unwrap();
        std::fs::write(root.path().join("README.md"), "v1").unwrap();
        std::fs::write(root.path().join("src/main.tsx"), "v1").unwrap();
        let mut checkpoint = running_checkpoint("attempt-1");
        checkpoint
            .record("README.md", crate::pipeline::ledger::content_sha256("v1"))
            .unwrap();
        checkpoint
            .record("src/main.tsx", "0".repeat(64)) // stale checksum on purpose
            .unwrap();
        let paths = vec![
            "README.md".to_string(),
            "src/main.tsx".to_string(),
            "src/App.tsx".to_string(),
        ];
        // Recorded sha matches disk bytes -> resumable.
        let resumed = resume_paths(&checkpoint, "inputs-hash", root.path(), &paths);
        assert_eq!(resumed, vec!["README.md".to_string()]);
        // Different inputs: nothing resumes even when bytes match.
        assert!(resume_paths(&checkpoint, "inputs-hash-2", root.path(), &paths).is_empty());
        // Disk drift: nothing resumes.
        std::fs::write(root.path().join("README.md"), "drifted").unwrap();
        assert!(resume_paths(&checkpoint, "inputs-hash", root.path(), &paths).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_never_resumed_even_with_matching_checksums() {
        let root = tempfile::tempdir().unwrap();
        let outside = root.path().join("outside-target");
        std::fs::write(&outside, "v1").unwrap();
        std::os::unix::fs::symlink("outside-target", root.path().join("README.md")).unwrap();
        let mut checkpoint = running_checkpoint("attempt-1");
        checkpoint
            .record("README.md", crate::pipeline::ledger::content_sha256("v1"))
            .unwrap();
        let paths = vec!["README.md".to_string()];
        assert!(resume_paths(&checkpoint, "inputs-hash", root.path(), &paths).is_empty());
    }

    #[test]
    fn record_upserts_and_enforces_bounds() {
        let mut checkpoint = running_checkpoint("attempt-1");
        checkpoint.record("README.md", "aa".repeat(32)).unwrap();
        checkpoint.record("README.md", "bb".repeat(32)).unwrap();
        // Updating an existing path replaces instead of duplicating.
        assert_eq!(checkpoint.files.len(), 1);
        assert_eq!(checkpoint.files[0].sha256, "bb".repeat(32));
        assert_eq!(checkpoint.sequence, 2);
        // Fill up to the cap (1 existing + 63 new unique paths).
        for index in 0..MAX_CHECKPOINT_FILES - 1 {
            checkpoint
                .record(&format!("file-{index}.ts"), "cc".repeat(64))
                .unwrap();
        }
        assert_eq!(checkpoint.files.len(), MAX_CHECKPOINT_FILES);
        assert!(checkpoint.record("overflow.ts", "cc".repeat(64)).is_err());
        // Upserting an existing path at the cap still succeeds.
        assert!(checkpoint.record("README.md", "dd".repeat(32)).is_ok());
        assert!(checkpoint.to_json().is_ok());
        assert!(GenerationCheckpoint::running("", "", "").to_json().is_ok());
    }
}
