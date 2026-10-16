use std::path::Path;
use std::io::Read;

use super::types::{GeneratedFile, GeneratedFileStatus};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct VerificationResult {
    pub passed: bool,
    pub degraded: bool,
    pub gate: String,
    pub detail: String,
    /// Absent for historical records; never infer execution from legacy booleans.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<GateEvidence>,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus { Passed, Failed, Skipped, Unknown }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct GateEvidence {
    pub schema_version: u32,
    pub status: GateStatus,
    pub required: bool,
    /// None means applicability has not been established, not "not applicable".
    pub applicable: Option<bool>,
    /// Only an executed subprocess may supply a receipt. In-process checks do not.
    pub execution: Option<ExecutionEvidence>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ExecutionEvidence {
    pub command: Vec<String>,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub tool_version: String,
}

impl VerificationResult {
    pub fn ok(gate: impl Into<String>, detail: impl Into<String>) -> Self {
        Self { passed: true, degraded: false, gate: gate.into(), detail: detail.into(), evidence: None }
    }

    pub fn failed(gate: impl Into<String>, detail: impl Into<String>) -> Self {
        Self { passed: false, degraded: false, gate: gate.into(), detail: detail.into(), evidence: None }
    }

    pub fn degraded(gate: impl Into<String>, detail: impl Into<String>) -> Self {
        Self { passed: true, degraded: true, gate: gate.into(), detail: detail.into(), evidence: None }
    }

    pub fn with_required_evidence(mut self, status: GateStatus, applicable: Option<bool>) -> Self {
        self.evidence = Some(GateEvidence {
            schema_version: 1, status, required: true, applicable, execution: None,
        });
        self
    }

    pub fn unknown(gate: &str, detail: &str, applicable: Option<bool>) -> Self {
        Self::failed(gate, detail).with_required_evidence(GateStatus::Unknown, applicable)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct RepairResult {
    pub applied: bool,
    pub description: String,
    pub path: Option<String>,
}

impl RepairResult {
    pub fn applied(description: impl Into<String>, path: Option<String>) -> Self {
        Self { applied: true, description: description.into(), path }
    }

    pub fn pending(description: impl Into<String>, path: Option<String>) -> Self {
        Self { applied: false, description: description.into(), path }
    }
}

pub trait VerificationStage {
    fn name(&self) -> &'static str;
    fn verify(&self, workspace_root: &Path, files: &[GeneratedFile]) -> VerificationResult;
}

pub struct DefaultVerificationStage;

impl VerificationStage for DefaultVerificationStage {
    fn name(&self) -> &'static str {
        "persistence"
    }

    fn verify(&self, workspace_root: &Path, files: &[GeneratedFile]) -> VerificationResult {
        if files.is_empty() {
            return VerificationResult::failed(self.name(), "no files were generated");
        }
        for file in files {
            if file.status != GeneratedFileStatus::Written
                && file.status != GeneratedFileStatus::Reused
            {
                return VerificationResult::failed(self.name(), format!("{} was not written", file.path));
            }
            let root = match crate::workspace::safe_io::RootDir::open(workspace_root) {
                Ok(root) => root,
                Err(error) => return VerificationResult::failed(self.name(), format!("{}: {error}", file.path)),
            };
            let actual = match root.read_file(&file.path, crate::workspace::MAX_PROJECT_FILE_BYTES) {
                Ok(bytes) => bytes,
                Err(error) => return VerificationResult::failed(self.name(), format!("{}: {error}", file.path)),
            };
            let expected = file.content.as_bytes();
            if actual != expected {
                return VerificationResult::failed(self.name(), format!("{} differs from generated content", file.path));
            }
        }
        VerificationResult::ok(self.name(), format!("{} files persisted with matching content", files.len()))
    }
}

pub struct DefaultRepairService;

impl DefaultRepairService {
    pub fn attempt_repair(&self, _files: &[GeneratedFile]) -> Vec<RepairResult> {
        Vec::new()
    }
}

pub fn verify_workspace(
    stages: &[Box<dyn VerificationStage + Send + Sync>],
    workspace_root: &Path,
    files: &[GeneratedFile],
) -> Vec<VerificationResult> {
    stages.iter().map(|stage| stage.verify(workspace_root, files)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::{GatewayRequest, GatewayResponse, GatewayService};
    use crate::pipeline::types::{FileGenerationPrompt, LocalFileGenerationService};
    use crate::workspace::LocalWorkspaceService;
    use std::sync::Arc;

    struct StubGateway;

    impl GatewayService for StubGateway {
        fn send(&self, _request: GatewayRequest) -> crate::domain::Result<GatewayResponse> {
            Ok(GatewayResponse {
                status: 200,
                body: serde_json::json!({"choices": [{"message": {"content": "export {};"}, "finish_reason": "stop"}]}),
            })
        }
    }

    fn written(path: &str, content: &str) -> GeneratedFile {
        GeneratedFile {
            path: path.into(),
            content: content.into(),
            language: None,
            status: GeneratedFileStatus::Written,
            error: None,
        }
    }

    #[test]
    fn generated_content_is_written_then_verified_against_disk() {
        let root = tempfile::tempdir().unwrap();
        let generator = LocalFileGenerationService::new(
            LocalWorkspaceService,
            Arc::new(StubGateway),
            "test-model".into(),
        );
        let files = generator.generate_and_write(root.path(), &[
            FileGenerationPrompt::new("src/main.ts".into(), "typescript".into(), "entry".into(), "build it".into()),
        ]).unwrap();
        assert_eq!(std::fs::read_to_string(root.path().join("src/main.ts")).unwrap(), "export {};");
        assert!(DefaultVerificationStage.verify(root.path(), &files).passed);
        std::fs::write(root.path().join("src/main.ts"), "tampered").unwrap();
        assert!(!DefaultVerificationStage.verify(root.path(), &files).passed);
    }

    #[test]
    fn rejects_written_file_missing_from_disk() {
        let root = tempfile::tempdir().unwrap();
        let result = DefaultVerificationStage.verify(root.path(), &[written("src/main.ts", "export {};")]);
        assert!(!result.passed);
        assert!(result.detail.contains("src/main.ts"));
    }

    #[test]
    fn accepts_matching_persisted_content_and_rejects_modified_content() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("index.ts"), "export {};").unwrap();
        let file = written("index.ts", "export {};");
        assert!(DefaultVerificationStage.verify(root.path(), &[file.clone()]).passed);
        std::fs::write(root.path().join("index.ts"), "changed").unwrap();
        let result = DefaultVerificationStage.verify(root.path(), &[file]);
        assert!(!result.passed);
        assert!(result.detail.contains("index.ts"));
    }

    #[test]
    fn rejects_unwritten_and_escaping_paths() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("file.ts"), "content").unwrap();
        let mut file = written("file.ts", "content");
        file.status = GeneratedFileStatus::Pending;
        assert!(!DefaultVerificationStage.verify(root.path(), &[file]).passed);
        assert!(!DefaultVerificationStage.verify(root.path(), &[written("../outside", "content")]).passed);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_output() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(outside.path(), "content").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("file.ts")).unwrap();
        assert!(!DefaultVerificationStage.verify(root.path(), &[written("file.ts", "content")]).passed);
    }
}
