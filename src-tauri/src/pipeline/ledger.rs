//! Generation ledger + regeneration policy (ported from devcode's
//! `GenerationLedger.ts`, `regenerationPolicy.ts` and `generationReuse.ts`).
//!
//! The ledger persists, per project, the inputs hash and per-file sha256 of
//! the last successful generation. The policy decides per file whether the
//! on-disk bytes still match the ledger and the requested inputs, so repeat
//! runs with an unchanged goal skip unchanged files instead of re-billing the
//! provider. Reused files are reported back as first-class results with a
//! distinct `Reused` status instead of being silently dropped.

use crate::domain::{DomainError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The generation ledger is keyed to the workspace, not a specific project
/// id, because the engine generates into one workspace root at a time.
pub const LEDGER_PROJECT_ID: &str = "workspace";

pub const LEDGER_DIR: &str = ".recode";
pub const LEDGER_FILE: &str = "generation-ledger.json";

/// Bounded constants: the ledger only tracks the bounded manifest files and
/// never grows beyond a compact size envelope.
pub const MAX_LEDGER_FILES: usize = 64;
pub const MAX_LEDGER_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedgerFileEntry {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenerationLedger {
    pub schema_version: u8,
    pub project_id: String,
    /// Hash of the generation inputs the recorded files were produced from.
    pub inputs_hash: String,
    pub files: Vec<LedgerFileEntry>,
}

impl GenerationLedger {
    pub fn new(project_id: impl Into<String>, inputs_hash: impl Into<String>) -> Self {
        Self {
            schema_version: 1,
            project_id: project_id.into(),
            inputs_hash: inputs_hash.into(),
            files: Vec::new(),
        }
    }

    pub fn record(&mut self, path: &str, sha256: String) -> Result<()> {
        if self.files.len() >= MAX_LEDGER_FILES {
            return Err(DomainError::conflict("generation ledger file limit reached"));
        }
        if let Some(entry) = self.files.iter_mut().find(|entry| entry.path == path) {
            entry.sha256 = sha256;
        } else {
            self.files.push(LedgerFileEntry {
                path: path.to_string(),
                sha256,
            });
        }
        Ok(())
    }

    pub fn checksum(&self, path: &str) -> Option<&str> {
        self.files
            .iter()
            .find(|entry| entry.path == path)
            .map(|entry| entry.sha256.as_str())
    }

    pub fn to_json(&self) -> Result<String> {
        let json = serde_json::to_string(self)?;
        if json.len() > MAX_LEDGER_BYTES {
            return Err(DomainError::conflict("generation ledger exceeds size bound"));
        }
        Ok(json)
    }
}

/// SHA-256 of the generation inputs, matching devcode's
/// `computeGenerationInputsHash` order: goal + model + manifest file paths.
pub fn compute_generation_inputs_hash(goal: &str, model: &str, file_paths: &[String]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(goal.as_bytes());
    hasher.update([0]);
    hasher.update(model.as_bytes());
    hasher.update([0]);
    for path in file_paths {
        hasher.update(path.as_bytes());
        hasher.update([0]);
    }
    hex(&hasher.finalize())
}

/// SHA-256 of a file's bytes, read descriptor-relative beneath the workspace
/// root; `None` when the file is absent, a symlink, non-regular, or oversized.
pub fn file_sha256(root: &crate::workspace::safe_io::RootDir, relative: &str) -> Option<String> {
    let bytes = root.read_file(relative, crate::workspace::MAX_PROJECT_FILE_BYTES).ok()?;
    Some(hex(&Sha256::digest(bytes)))
}

/// Path-of-workspace-root convenience for callers that hold only the root
/// path; opens the root descriptor per call.
pub fn file_sha256_beneath(workspace_root: &Path, relative: &str) -> Option<String> {
    let root = crate::workspace::safe_io::RootDir::open(workspace_root).ok()?;
    file_sha256(&root, relative)
}

/// SHA-256 of in-memory content; the checkpoint records the hash of the bytes
/// it just wrote so resume can verify disk state without re-reading.
pub fn content_sha256(content: &str) -> String {
    hex(&Sha256::digest(content.as_bytes()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegenerationDecision {
    /// Disk bytes match the ledger entry and the ledger's inputs hash matches
    /// the requested inputs: the file can be reused as-is.
    Reuse,
    /// File must be regenerated (missing on disk, drifted from the ledger, or
    /// the generation inputs changed).
    Regenerate,
}

/// Per-file decision, mirroring devcode's `decideRegeneration`.
pub fn decide_regeneration(
    ledger: Option<&GenerationLedger>,
    inputs_hash: &str,
    workspace_root: &Path,
    file_path: &str,
) -> RegenerationDecision {
    let Some(ledger) = ledger else {
        return RegenerationDecision::Regenerate;
    };
    if ledger.inputs_hash != inputs_hash {
        return RegenerationDecision::Regenerate;
    }
    let Some(recorded) = ledger.checksum(file_path) else {
        return RegenerationDecision::Regenerate;
    };
    // Read descriptor-relative beneath the workspace root; `read_file` refuses
    // symlinks and non-regular files, so a symlinked output regenerates.
    let Some(actual) = crate::workspace::safe_io::RootDir::open(workspace_root)
        .ok()
        .and_then(|root| file_sha256(&root, file_path))
    else {
        return RegenerationDecision::Regenerate;
    };
    if actual != recorded {
        return RegenerationDecision::Regenerate;
    }
    // Post-generation validation guard: a ledger entry whose on-disk bytes
    // are malformed (possible for ledgers created before the package.json
    // gate) must regenerate instead of being reused forever.
    if !crate::pipeline::validation::is_valid_generated_file(file_path, &workspace_root) {
        return RegenerationDecision::Regenerate;
    }
    RegenerationDecision::Reuse
}

/// Aggregate plan, mirroring devcode's `planRegeneration`: splits the
/// requested file paths into `reuse` (unchanged) and `regenerate` (changed).
pub fn plan_regeneration(
    ledger: Option<&GenerationLedger>,
    inputs_hash: &str,
    workspace_root: &Path,
    file_paths: &[String],
) -> (Vec<String>, Vec<String>) {
    let mut reuse = Vec::new();
    let mut regenerate = Vec::new();
    for path in file_paths {
        match decide_regeneration(ledger, inputs_hash, workspace_root, path) {
            RegenerationDecision::Reuse => reuse.push(path.clone()),
            RegenerationDecision::Regenerate => regenerate.push(path.clone()),
        }
    }
    (reuse, regenerate)
}

/// Fail-soft persistence: a missing or malformed ledger means "no history",
/// never a hard failure — the run just regenerates everything.
pub fn load_ledger(workspace_root: &Path, project_id: &str) -> Option<GenerationLedger> {
    let bytes = crate::workspace::safe_io::RootDir::open(workspace_root)
        .and_then(|root| root.read_file(&format!("{LEDGER_DIR}/{LEDGER_FILE}"), MAX_LEDGER_BYTES))
        .ok()?;
    let ledger: GenerationLedger = serde_json::from_slice(&bytes).ok()?;
    if ledger.schema_version != 1 {
        return None;
    }
    if ledger.project_id != project_id {
        return None;
    }
    Some(ledger)
}

#[allow(dead_code)]  // test-only
pub fn ledger_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join(LEDGER_DIR).join(LEDGER_FILE)
}

/// Persist the ledger after a successful write of `written` files. Ledger IO
/// failures never fail the build (fail-soft), matching devcode.
pub fn store_ledger(
    workspace_root: &Path,
    project_id: &str,
    inputs_hash: &str,
    written: &BTreeMap<String, String>,
) -> std::io::Result<()> {
    let mut ledger = load_ledger(workspace_root, project_id)
        .unwrap_or_else(|| GenerationLedger::new(project_id, inputs_hash));
    ledger.inputs_hash = inputs_hash.to_string();
    for (path, sha256) in written {
        let _ = ledger.record(path, sha256.clone());
    }
    let json = match ledger.to_json() {
        Ok(json) => json,
        Err(_) => return Ok(()), // fail-soft on bounds breaches
    };
    let dir = workspace_root.join(LEDGER_DIR);
    let _ = fs::create_dir_all(&dir); // bootstrap only; write re-opens without following links
    crate::workspace::safe_io::RootDir::open(workspace_root)
        .and_then(|root| root.write_file(&format!("{LEDGER_DIR}/{LEDGER_FILE}"), json.as_bytes(), MAX_LEDGER_BYTES))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_hash_is_stable_and_order_sensitive() {
        let paths = vec!["README.md".to_string(), "src/main.tsx".to_string()];
        let a = compute_generation_inputs_hash("goal", "model", &paths);
        let b = compute_generation_inputs_hash("goal", "model", &paths);
        assert_eq!(a, b);
        let swapped = vec!["src/main.tsx".to_string(), "README.md".to_string()];
        let c = compute_generation_inputs_hash("goal", "model", &swapped);
        assert_ne!(a, c);
        let other_goal = compute_generation_inputs_hash("goal 2", "model", &paths);
        assert_ne!(a, other_goal);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn ledger_records_and_looks_up_checksums() {
        let mut ledger = GenerationLedger::new("proj", "hash");
        ledger
            .record("README.md", "aa".repeat(32))
            .expect("record within bounds");
        assert_eq!(ledger.checksum("README.md"), Some("aa".repeat(32).as_str()));
        assert_eq!(ledger.checksum("src/main.tsx"), None);
        // Updating an existing path replaces instead of duplicating.
        ledger
            .record("README.md", "bb".repeat(32))
            .expect("replace within bounds");
        assert_eq!(ledger.files.len(), 1);
        assert_eq!(ledger.checksum("README.md"), Some("bb".repeat(32).as_str()));
    }

    #[test]
    fn ledger_bounds_are_enforced() {
        let mut ledger = GenerationLedger::new("proj", "hash");
        for index in 0..MAX_LEDGER_FILES {
            let path = format!("file-{index}.ts");
            ledger
                .record(&path, "c".repeat(64))
                .expect("within file bound");
        }
        assert!(ledger.record("overflow.ts", "c".repeat(64)).is_err());
    }

    #[test]
    fn to_json_enforces_size_bound() {
        let mut ledger = GenerationLedger::new("proj", "hash");
        for index in 0..MAX_LEDGER_FILES {
            ledger
                .record(&format!("src/deep/path/file-{index}.tsx"), "c".repeat(64))
                .expect("within file bound");
        }
        assert!(ledger.to_json().is_ok());
        assert!(GenerationLedger::new("", "").to_json().is_ok());
    }

    #[test]
    fn decide_regeneration_reuses_only_matching_disk_bytes() {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(root.path().join("src")).expect("mkdir");
        std::fs::write(root.path().join("README.md"), "same bytes").expect("write");
        std::fs::write(root.path().join("src/main.tsx"), "v1").expect("write");
        let mut ledger = GenerationLedger::new("proj", "hash-1");
        ledger
            .record("README.md", file_sha256_beneath(root.path(), "README.md").unwrap())
            .expect("record");
        ledger
            .record("src/main.tsx", "0".repeat(64)) // stale checksum on purpose
            .expect("record");

        // Same inputs + matching bytes -> reuse.
        assert_eq!(
            decide_regeneration(Some(&ledger), "hash-1", root.path(), "README.md"),
            RegenerationDecision::Reuse
        );
        // Disk drifted from ledger -> regenerate.
        assert_eq!(
            decide_regeneration(Some(&ledger), "hash-1", root.path(), "src/main.tsx"),
            RegenerationDecision::Regenerate
        );
        // Inputs changed -> regenerate even if bytes match.
        assert_eq!(
            decide_regeneration(Some(&ledger), "hash-2", root.path(), "README.md"),
            RegenerationDecision::Regenerate
        );
        // No ledger -> regenerate.
        assert_eq!(
            decide_regeneration(None, "hash-1", root.path(), "README.md"),
            RegenerationDecision::Regenerate
        );
        // Missing file -> regenerate.
        assert_eq!(
            decide_regeneration(Some(&ledger), "hash-1", root.path(), "src/App.tsx"),
            RegenerationDecision::Regenerate
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_never_reused_even_with_matching_checksums() {
        let root = tempfile::tempdir().expect("tempdir");
        let outside = root.path().join("outside-target");
        std::fs::write(&outside, "same bytes").expect("write");
        std::fs::create_dir_all(root.path().join("src")).expect("mkdir");
        std::os::unix::fs::symlink("../outside-target", root.path().join("src/App.tsx"))
            .expect("symlink");
        let mut ledger = GenerationLedger::new("proj", "hash-1");
        // Hash the *target* bytes so checksum comparison alone would pass.
        ledger
            .record(
                "src/App.tsx",
                crate::pipeline::ledger::content_sha256("same bytes"),
            )
            .expect("record");
        assert_eq!(
            decide_regeneration(Some(&ledger), "hash-1", root.path(), "src/App.tsx"),
            RegenerationDecision::Regenerate
        );
    }

    #[test]
    fn plan_regeneration_splits_reuse_from_regenerate() {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::write(root.path().join("README.md"), "same bytes").expect("write");
        let mut ledger = GenerationLedger::new("proj", "hash-1");
        ledger
            .record("README.md", file_sha256_beneath(root.path(), "README.md").unwrap())
            .expect("record");
        let paths = vec![
            "README.md".to_string(),
            "src/main.tsx".to_string(),
        ];
        let (reuse, regenerate) = plan_regeneration(Some(&ledger), "hash-1", root.path(), &paths);
        assert_eq!(reuse, vec!["README.md".to_string()]);
        assert_eq!(regenerate, vec!["src/main.tsx".to_string()]);
    }

    #[test]
    fn ledger_round_trips_through_disk_fail_soft() {
        let root = tempfile::tempdir().expect("tempdir");
        // Missing ledger -> None, not an error.
        assert!(load_ledger(root.path(), "proj").is_none());
        // Persist and reload.
        std::fs::write(root.path().join("README.md"), "same bytes").expect("write");
        let written = BTreeMap::from([(
            "README.md".to_string(),
            file_sha256_beneath(root.path(), "README.md").expect("hash"),
        )]);
        store_ledger(root.path(), "proj", "hash-1", &written).expect("store");
        let loaded = load_ledger(root.path(), "proj").expect("reload");
        assert_eq!(loaded.inputs_hash, "hash-1");
        assert_eq!(loaded.checksum("README.md").unwrap().len(), 64);
        // Corrupted ledger -> fail-soft None.
        std::fs::write(ledger_path(root.path()), "{not json").expect("corrupt");
        assert!(load_ledger(root.path(), "proj").is_none());
        // Different project id -> treated as no history.
        std::fs::remove_file(ledger_path(root.path())).expect("remove");
        store_ledger(root.path(), "proj", "hash-1", &written).expect("store");
        assert!(load_ledger(root.path(), "other").is_none());
    }
}
