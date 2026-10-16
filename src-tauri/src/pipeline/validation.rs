//! Post-generation content validation (ported from devcode's
//! `PostGenerationValidation.ts`).
//!
//! devcode runs five audits after generation; four of them (contract lock,
//! behavior completeness, accessibility, UX consistency, production
//! readiness — all warn-only there) depend on devcode's DI-registered
//! analysis services, which recode does not have. The one HARD gate ports
//! cleanly and is enforced here at three points in recode's pipeline:
//!
//! 1. provider phase (`LocalFileGenerationService::generate_file_content`):
//!    malformed `package.json` content fails the file *before* anything is
//!    written — prior bytes are preserved, and the file is never written,
//!    checkpointed or promoted into the reuse ledger. recode's two-phase
//!    design (stage all provider output, then write) makes this stronger
//!    than devcode, which validates after generation and only marks the
//!    file failed;
//! 2. ledger reuse (`ledger::decide_regeneration`): a legacy ledger entry
//!    whose on-disk bytes are a malformed `package.json` regenerates
//!    instead of being reused;
//! 3. checkpoint resume (`checkpoint::resume_paths`): the same guard for a
//!    legacy checkpoint.
//!
//! The reuse guards exist because ledgers/checkpoints created before this
//! gate may already carry malformed bytes with a matching checksum; without
//! them a bad `package.json` would be reused forever.

use std::path::Path;

/// devcode's path normalization: only a leading `./` or `/` is stripped, so
/// `./package.json` and `/package.json` name the same gated file, while
/// `src/package.json` (a nested manifest devcode also ignores) is not gated.
fn is_package_json_path(path: &str) -> bool {
    let normalized = path
        .strip_prefix("./")
        .or_else(|| path.strip_prefix('/'))
        .unwrap_or(path);
    normalized == "package.json"
}

/// The hard gate: validate generated file content before it may be written,
/// reused or resumed. `Err` carries the per-file failure reason, mirroring
/// devcode's `failureReason: 'parse_error'` record.
pub fn validate_generated_content(path: &str, content: &str) -> Result<(), String> {
    if is_package_json_path(path) {
        if let Err(error) = serde_json::from_str::<serde_json::Value>(content) {
            return Err(format!("package.json is malformed JSON: {error}"));
        }
    }
    Ok(())
}

/// Disk variant for the reuse/resume guards: `true` when the file on disk is
/// valid to reuse. Non-`package.json` files are never gated; an unreadable or
/// malformed `package.json` is never reused. Reads are descriptor-relative
/// beneath the workspace root.
pub fn is_valid_generated_file(path: &str, workspace_root: &Path) -> bool {
    if !is_package_json_path(path) {
        return true;
    }
    match crate::workspace::safe_io::RootDir::open(workspace_root)
        .and_then(|root| root.read_file("package.json", crate::workspace::MAX_PROJECT_FILE_BYTES))
    {
        Ok(bytes) => {
            let content = String::from_utf8_lossy(&bytes).into_owned();
            validate_generated_content(path, &content).is_ok()
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_package_json_passes_the_gate() {
        assert!(validate_generated_content("package.json", r#"{"name":"app","version":"0.1.0"}"#).is_ok());
        // Trailing whitespace (a real package.json ends with a newline) is fine.
        assert!(validate_generated_content("package.json", "{\"name\":\"app\"}\n").is_ok());
        // Any JSON value parses, matching JS JSON.parse semantics.
        assert!(validate_generated_content("package.json", "5").is_ok());
    }

    #[test]
    fn malformed_package_json_fails_with_the_parse_error_reason() {
        let error = validate_generated_content("package.json", "{not valid json")
            .expect_err("malformed package.json must fail");
        assert!(
            error.starts_with("package.json is malformed JSON:"),
            "unexpected reason: {error}"
        );
        // Empty content is malformed JSON, not a valid manifest.
        assert!(validate_generated_content("package.json", "").is_err());
    }

    #[test]
    fn leading_path_prefixes_are_normalized_like_devcode() {
        assert!(validate_generated_content("./package.json", "{bad").is_err());
        assert!(validate_generated_content("/package.json", "{bad").is_err());
        // Nested manifests are not the workspace package.json.
        assert!(validate_generated_content("src/package.json", "{bad").is_ok());
        assert!(validate_generated_content("nested/package.json", "{bad").is_ok());
    }

    #[test]
    fn non_package_json_files_are_never_gated() {
        assert!(validate_generated_content("src/main.ts", "export const x = {").is_ok());
        assert!(validate_generated_content("README.md", "# heading\n```json\n{not json\n```\n").is_ok());
        assert!(validate_generated_content("tsconfig.json", "definitely not json").is_ok());
    }

    #[test]
    fn disk_guard_rejects_malformed_package_json_bytes_only() {
        let root = tempfile::tempdir().unwrap();
        let manifest = root.path().join("package.json");
        std::fs::write(&manifest, "{not json").unwrap();
        assert!(!is_valid_generated_file("package.json", root.path()));
        std::fs::write(&manifest, "{\"name\":\"ok\"}").unwrap();
        assert!(is_valid_generated_file("package.json", root.path()));
        // Non-package files pass without even requiring the file to exist.
        assert!(is_valid_generated_file("src/main.ts", root.path()));
        // Invalid UTF-8 can never be valid JSON.
        std::fs::write(&manifest, [0xff, 0xfe]).unwrap();
        assert!(!is_valid_generated_file("package.json", root.path()));
    }
}
