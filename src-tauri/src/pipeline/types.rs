use crate::domain::Result;
use crate::gateway::{GatewayRequest, GatewayService};
use crate::workspace::{LocalWorkspaceService, ProjectFile, WorkspaceService};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeneratedFile {
    pub path: String,
    pub content: String,
    pub language: Option<String>,
    pub status: GeneratedFileStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GeneratedFileStatus {
    Pending,
    Writing,
    Written,
    /// File skipped regeneration because on-disk bytes already match the
    /// generation ledger (unchanged inputs + matching sha256).
    Reused,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileGenerationPrompt {
    pub file_path: String,
    pub language: String,
    pub purpose: String,
    pub user_prompt: String,
    pub max_tokens: usize,
    /// Cross-file context built from previously generated files in the same
    /// project (ported from devcode's CrossFileContextBuilder). `None` for the
    /// first file; later files see imports/exports/type summaries so generated
    /// code stays consistent. Serialized for checkpoint round-trips.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cross_file_context: Option<String>,
}

impl FileGenerationPrompt {
    pub fn new(file_path: String, language: String, purpose: String, user_prompt: String) -> Self {
        let max_tokens = Self::estimate_max_tokens(&file_path);
        Self {
            file_path,
            language,
            purpose,
            user_prompt,
            max_tokens,
            cross_file_context: None,
        }
    }

    fn estimate_max_tokens(file_path: &str) -> usize {
        let lower = file_path.to_lowercase();
        let base = 2 * 1024;
        let cap = 16 * 1024;

        if lower.ends_with(".md") || lower.ends_with(".json") {
            return base;
        }
        if lower.contains("types") || lower.contains("interface") {
            return base;
        }
        if lower.contains("test") || lower.contains("spec") {
            return 3 * 1024;
        }
        if lower.contains("component") || lower.contains("page") || lower.contains("view") {
            return 8 * 1024;
        }

        let extension = lower.rsplit('.').next().unwrap_or("");
        let lines = match extension {
            "tsx" | "jsx" => 220,
            "ts" | "js" => 180,
            "css" | "scss" => 120,
            "html" => 80,
            "py" => 200,
            "go" | "rs" | "java" => 220,
            _ => 160,
        };
        let estimated = (lines * 80).max(base);
        estimated.min(cap)
    }

    pub fn build_messages(&self) -> Vec<serde_json::Value> {
        let system = format!(
            "You are an expert {language} developer. Generate ONLY the complete file content. \
             Return raw code only — no explanations, no markdown fences, no code block markers. \
             File: {path}\nPurpose: {purpose}",
            language = self.language,
            path = self.file_path,
            purpose = self.purpose
        );
        let cross_file = self
            .cross_file_context
            .as_deref()
            .unwrap_or_default();
        let user = format!(
            "Project request: {prompt}\n\n{context}Generate the file `{path}` now.",
            prompt = self.user_prompt,
            context = cross_file,
            path = self.file_path
        );
        vec![
            serde_json::json!({"role": "system", "content": system}),
            serde_json::json!({"role": "user", "content": user}),
        ]
    }

    pub fn gateway_request(&self, model: &str) -> GatewayRequest {
        GatewayRequest {
            provider: "openai".into(),
            payload: serde_json::json!({
                "model": model,
                "messages": self.build_messages(),
                "stream": false,
                "max_tokens": self.max_tokens,
            }),
            task_type: Some("generation".into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BuildOutcome {
    pub success: bool,
    pub degraded: bool,
    pub generated_file_count: usize,
    pub failed_file_count: usize,
    pub failed_files: Vec<GeneratedFile>,
    pub summary: String,
    #[serde(default)]
    pub verification_results: Option<Vec<crate::pipeline::verify::VerificationResult>>,
    #[serde(default)]
    pub repair_results: Option<crate::pipeline::repair::AdaptiveRepairResult>,
    #[serde(default)]
    pub verification_summary: Option<String>,
    #[serde(default)]
    pub analysis: Option<crate::pipeline::outcome_analyzer::BuildOutcomeAnalysis>,
    #[serde(default)]
    pub report: Option<crate::pipeline::report::VerificationReport>,
    #[serde(default)]
    pub static_analysis: Option<crate::pipeline::static_analysis::StaticAnalysisResult>,
    #[serde(default)]
    pub mental_simulation: Option<crate::pipeline::mental_simulator::SimulationResult>,
}

pub struct LocalFileGenerationService {
    workspace_service: LocalWorkspaceService,
    gateway: Arc<dyn GatewayService>,
    model: String,
}

impl LocalFileGenerationService {
    pub fn new(workspace_service: LocalWorkspaceService, gateway: Arc<dyn GatewayService>, model: String) -> Self {
        Self {
            workspace_service,
            gateway,
            model,
        }
    }

    pub fn generate_and_write(
        &self,
        workspace_root: &Path,
        prompts: &[FileGenerationPrompt],
    ) -> Result<Vec<GeneratedFile>> {
        self.generate_and_write_cancellable(workspace_root, prompts, &|| false)
    }

    /// Insert or replace a generated file by path (ported from devcode's
    /// `upsertGeneratedFile`). If the path already exists, the entry is
    /// replaced in place (position preserved); otherwise it is appended.
    /// This prevents duplicate entries accumulating across retry waves.
    pub fn upsert_generated_file(files: &mut Vec<GeneratedFile>, file: GeneratedFile) {
        if let Some(existing) = files.iter_mut().find(|f| f.path == file.path) {
            *existing = file;
            return;
        }
        files.push(file);
    }

    pub fn generate_and_write_cancellable(
        &self,
        workspace_root: &Path,
        prompts: &[FileGenerationPrompt],
        should_cancel: &dyn Fn() -> bool,
    ) -> Result<Vec<GeneratedFile>> {
        self.generate_and_write_tracked(workspace_root, prompts, should_cancel, &[], &mut |_, _| {})
    }

    /// Write path with a per-file progress callback: `on_file_written` fires
    /// after each successful workspace write, giving the engine a safe point to
    /// durably checkpoint completed files (resumable partial runs).
    /// `prior_summaries` seeds the cross-file context (Q29) with files that
    /// were reused/resumed rather than regenerated, so later generations still
    /// see their imports and types.
    pub fn generate_and_write_tracked(
        &self,
        workspace_root: &Path,
        prompts: &[FileGenerationPrompt],
        should_cancel: &dyn Fn() -> bool,
        prior_summaries: &[(String, String)],
        on_file_written: &mut dyn FnMut(&str, &str),
    ) -> Result<Vec<GeneratedFile>> {
        // Aggregate caps (P1.8): bound the total bytes and tokens this call
        // may produce, not just per-file limits. Reject upfront so a
        // runaway manifest cannot monopolize resources mid-generation.
        const MAX_AGGREGATE_TOKENS: usize = 196_608;
        const MAX_AGGREGATE_BYTES: usize = 256 * 1024;
        let total_tokens: usize = prompts.iter().map(|p| p.max_tokens).sum();
        if total_tokens > MAX_AGGREGATE_TOKENS {
            return Err(crate::domain::DomainError::invalid_input(format!(
                "aggregate token budget {total_tokens} exceeds the per-run limit {MAX_AGGREGATE_TOKENS}"
            )));
        }
        let estimated_bytes: usize = prompts.iter().map(|p| p.max_tokens * 4).sum();
        if estimated_bytes > MAX_AGGREGATE_BYTES {
            return Err(crate::domain::DomainError::invalid_input(format!(
                "aggregate byte estimate {estimated_bytes} exceeds the per-run limit {MAX_AGGREGATE_BYTES}"
            )));
        }
        // Cross-file context accumulator (ported from devcode's
        // CrossFileContextBuilder): each file generated after the first sees
        // focused summaries of the earlier ones, keeping imports/types
        // consistent across the project.
        let mut file_summaries: Vec<(String, String)> = prior_summaries.to_vec();
        // Finish the provider phase before replacing any existing workspace file.
        let mut results = Vec::with_capacity(prompts.len());
        for prompt in prompts {
            if should_cancel() {
                return Err(crate::domain::DomainError::conflict("pipeline cancelled by user"));
            }
            let mut file = GeneratedFile {
                path: prompt.file_path.clone(),
                content: String::new(),
                language: Some(prompt.language.clone()),
                status: GeneratedFileStatus::Pending,
                error: None,
            };
            file.status = GeneratedFileStatus::Writing;
            let cross_context = if file_summaries.is_empty() {
                None
            } else {
                Some(crate::pipeline::cross_file_context::build_cross_file_context(&file_summaries))
            };
            let generated = self.generate_file_content(prompt, cross_context.as_deref(), should_cancel);
            if should_cancel() {
                return Err(crate::domain::DomainError::conflict("pipeline cancelled by user"));
            }
            let content = match generated {
                Ok(content) => content,
                Err(error) => {
                    file.status = GeneratedFileStatus::Failed;
                    file.error = Some(error.to_string());
                    results.push(file);
                    continue;
                }
            };
            file_summaries.push((
                prompt.file_path.clone(),
                crate::pipeline::cross_file_context::create_file_summary(&content, Some(&prompt.file_path)),
            ));
            file.content = content;
            results.push(file);
        }

        for file in &mut results {
            if should_cancel() {
                return Err(crate::domain::DomainError::conflict("pipeline cancelled by user"));
            }
            if file.status == GeneratedFileStatus::Failed {
                continue;
            }
            // File path validation (ported from devcode filePathValidator):
            // reject unsafe paths (traversal, absolute, control chars, etc.)
            // before writing to disk. Auto-fixable paths are sanitized.
            let path_validation = crate::pipeline::file_path_validator::validate_file_path(
                &file.path,
                crate::pipeline::file_path_validator::PathValidationOptions::auto_fix(),
            );
            if !path_validation.valid {
                file.status = GeneratedFileStatus::Failed;
                file.error = Some(format!(
                    "file path rejected: {}",
                    path_validation.issues.iter()
                        .filter(|i| i.severity == crate::pipeline::file_path_validator::PathIssueSeverity::Error)
                        .map(|i| i.message.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                continue;
            }
            if path_validation.was_fixed {
                file.path = path_validation.sanitized_path;
            }
            match self.workspace_service.write_project_file(
                workspace_root,
                &crate::workspace::ProjectFile {
                    path: file.path.clone(),
                    content: file.content.clone(),
                    language: file.language.clone(),
                },
            ) {
                Ok(()) => {
                    file.status = GeneratedFileStatus::Written;
                    on_file_written(&file.path, &file.content);
                }
                Err(error) => {
                    file.status = GeneratedFileStatus::Failed;
                    file.error = Some(error.to_string());
                }
            }
        }
        Ok(results)
    }

    fn generate_file_content(
        &self,
        prompt: &FileGenerationPrompt,
        cross_file_context: Option<&str>,
        should_cancel: &dyn Fn() -> bool,
    ) -> Result<String> {
        let mut prompt = prompt.clone();
        prompt.cross_file_context = cross_file_context.map(|s| s.to_string());
        let mut request = prompt.gateway_request(&self.model);
        let retry_budget = prompt.max_tokens.saturating_mul(2).min(32 * 1024);
        for attempt in 0..2 {
            if should_cancel() {
                return Err(crate::domain::DomainError::conflict("pipeline cancelled by user"));
            }
            let response = self.gateway.send(request.clone())?;
            if should_cancel() {
                return Err(crate::domain::DomainError::conflict("pipeline cancelled by user"));
            }
            let choice = response.body
                .get("choices")
                .and_then(|value| value.as_array())
                .and_then(|array| array.first())
                .ok_or_else(|| crate::domain::DomainError::unavailable("provider response has no choices"))?;
            // A length-limited completion may not have text at all. Never write its partial content.
            let finish_reason = choice.get("finish_reason").and_then(|value| value.as_str());
            if crate::pipeline::helpers::is_truncated_response(finish_reason) {
                if attempt == 0 && retry_budget > prompt.max_tokens {
                    request.payload["max_tokens"] = serde_json::json!(retry_budget);
                    continue;
                }
                return Err(crate::domain::DomainError::unavailable(
                    "provider response remained truncated after bounded retry",
                ));
            }
            let content = choice
                .get("message")
                .and_then(|value| value.get("content"))
                .and_then(|value| value.as_str())
                .ok_or_else(|| crate::domain::DomainError::unavailable("provider response has no content"))?;
            if content.trim().is_empty() {
                return Err(crate::domain::DomainError::unavailable("provider returned empty content"));
            }
            if !prompt.file_path.to_ascii_lowercase().ends_with(".md")
                && content.trim_start().starts_with("```")
            {
                return Err(crate::domain::DomainError::unavailable(
                    "provider returned a markdown code fence for a source file",
                ));
            }
            // Post-generation validation hard gate (ported from devcode's
            // PostGenerationValidation): malformed package.json must fail the
            // file before anything is written. Prior bytes are preserved and
            // the file never enters the checkpoint or the reuse ledger.
            if let Err(error) = crate::pipeline::validation::validate_generated_content(
                &prompt.file_path,
                content,
            ) {
                return Err(crate::domain::DomainError::unavailable(error));
            }
            
            // Content Type validation (prevent file-content misalignment like TS in HTML)
            let validation = crate::pipeline::content_type::validate_file_content_matches(content, &prompt.file_path, Some(&prompt.language));
            let final_content = match validation.action {
                crate::pipeline::content_type::ContentValidationAction::ReplaceWithFallback => {
                    crate::pipeline::content_type::get_fallback_content(&prompt.file_path, Some(&prompt.language))
                        .unwrap_or_else(|| content.to_string())
                },
                _ => content.to_string(),
            };

            return Ok(final_content);
        }
        unreachable!("bounded generation retry always returns or retries once")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PipelineGeneratedResult {
    pub run_id: String,
    pub generated_files: Vec<GeneratedFile>,
    pub failed_files: Vec<GeneratedFile>,
    pub outcome: BuildOutcome,
}

#[cfg(test)]
mod generation_retry_tests {
    use super::*;
    use crate::gateway::GatewayResponse;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    struct RecordingGateway {
        responses: Mutex<Vec<GatewayResponse>>,
        budgets: Mutex<Vec<u64>>,
        cancel_on_send: Option<Arc<AtomicBool>>,
    }

    impl RecordingGateway {
        fn new(responses: Vec<GatewayResponse>) -> Self {
            Self { responses: Mutex::new(responses), budgets: Mutex::new(Vec::new()), cancel_on_send: None }
        }

        fn budgets(&self) -> Vec<u64> {
            self.budgets.lock().unwrap().clone()
        }
    }

    impl GatewayService for RecordingGateway {
        fn send(&self, request: GatewayRequest) -> Result<GatewayResponse> {
            self.budgets.lock().unwrap().push(request.payload["max_tokens"].as_u64().unwrap());
            let mut responses = self.responses.lock().unwrap();
            if responses.is_empty() {
                return Err(crate::domain::DomainError::unavailable("unexpected extra provider request"));
            }
            let response = responses.remove(0);
            if let Some(cancelled) = &self.cancel_on_send {
                cancelled.store(true, Ordering::SeqCst);
            }
            Ok(response)
        }
    }

    fn response(content: serde_json::Value, finish_reason: &str) -> GatewayResponse {
        GatewayResponse {
            status: 200,
            body: serde_json::json!({"choices": [{"message": {"content": content}, "finish_reason": finish_reason}]}),
        }
    }

    fn prompt() -> FileGenerationPrompt {
        FileGenerationPrompt::new("README.md".into(), "markdown".into(), "readme".into(), "build it".into())
    }

    #[test]
    fn truncated_reply_retries_with_extended_budget_then_writes_only_complete_content() {
        let root = tempfile::tempdir().unwrap();
        let gateway = Arc::new(RecordingGateway::new(vec![
            response(serde_json::Value::Null, "length"),
            response(serde_json::json!("complete project\n"), "stop"),
        ]));
        let generator = LocalFileGenerationService::new(LocalWorkspaceService, gateway.clone(), "test-model".into());
        let files = generator.generate_and_write(root.path(), &[prompt()]).unwrap();
        assert_eq!(gateway.budgets(), vec![2048, 4096]);
        assert_eq!(files[0].status, GeneratedFileStatus::Written);
        assert_eq!(std::fs::read_to_string(root.path().join("README.md")).unwrap(), "complete project\n");
    }

    #[test]
    fn repeated_truncation_fails_after_one_retry_without_writing_partial_output() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("README.md"), "original content").unwrap();
        let gateway = Arc::new(RecordingGateway::new(vec![
            response(serde_json::json!("partial first"), "length"),
            response(serde_json::json!("partial second"), "length"),
        ]));
        let generator = LocalFileGenerationService::new(LocalWorkspaceService, gateway.clone(), "test-model".into());
        let files = generator.generate_and_write(root.path(), &[prompt()]).unwrap();
        assert_eq!(gateway.budgets(), vec![2048, 4096]);
        assert_eq!(files[0].status, GeneratedFileStatus::Failed);
        assert!(files[0].error.as_deref().unwrap().contains("truncated"));
        assert_eq!(std::fs::read_to_string(root.path().join("README.md")).unwrap(), "original content");
    }

    #[test]
    fn truncated_reply_at_retry_cap_fails_without_an_extra_request() {
        let root = tempfile::tempdir().unwrap();
        let gateway = Arc::new(RecordingGateway::new(vec![response(serde_json::json!("partial"), "length")]));
        let generator = LocalFileGenerationService::new(LocalWorkspaceService, gateway.clone(), "test-model".into());
        let mut bounded = prompt();
        bounded.max_tokens = 32 * 1024;
        let files = generator.generate_and_write(root.path(), &[bounded]).unwrap();
        assert_eq!(gateway.budgets(), vec![32768]);
        assert_eq!(files[0].status, GeneratedFileStatus::Failed);
        assert!(!root.path().join("README.md").exists());
    }

    #[test]
    fn cancellation_during_provider_request_preserves_existing_file() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("README.md"), "existing").unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let gateway = Arc::new(RecordingGateway {
            responses: Mutex::new(vec![response(serde_json::json!("new content"), "stop")]),
            budgets: Mutex::new(Vec::new()),
            cancel_on_send: Some(cancelled.clone()),
        });
        let generator = LocalFileGenerationService::new(LocalWorkspaceService, gateway.clone(), "test-model".into());
        let result = generator.generate_and_write_cancellable(root.path(), &[prompt()], &|| cancelled.load(Ordering::SeqCst));
        assert!(result.unwrap_err().to_string().contains("cancelled"));
        assert_eq!(gateway.budgets(), vec![2048]);
        assert_eq!(std::fs::read_to_string(root.path().join("README.md")).unwrap(), "existing");
    }

    struct CancelOnSecondRequest {
        requests: Mutex<usize>,
        cancelled: Arc<AtomicBool>,
    }

    impl GatewayService for CancelOnSecondRequest {
        fn send(&self, _request: GatewayRequest) -> Result<GatewayResponse> {
            let mut requests = self.requests.lock().unwrap();
            *requests += 1;
            if *requests == 2 {
                self.cancelled.store(true, Ordering::SeqCst);
            }
            Ok(response(serde_json::json!(format!("new content {}", *requests)), "stop"))
        }
    }

    #[test]
    fn cancellation_during_second_provider_request_preserves_both_existing_files() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::write(root.path().join("README.md"), "old readme").unwrap();
        std::fs::write(root.path().join("src/main.ts"), "old code").unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let gateway = Arc::new(CancelOnSecondRequest { requests: Mutex::new(0), cancelled: cancelled.clone() });
        let generator = LocalFileGenerationService::new(LocalWorkspaceService, gateway.clone(), "test-model".into());
        let prompts = [prompt(), FileGenerationPrompt::new("src/main.ts".into(), "typescript".into(), "entry".into(), "build it".into())];
        let result = generator.generate_and_write_cancellable(root.path(), &prompts, &|| cancelled.load(Ordering::SeqCst));
        assert!(result.unwrap_err().to_string().contains("cancelled"));
        assert_eq!(*gateway.requests.lock().unwrap(), 2);
        assert_eq!(std::fs::read_to_string(root.path().join("README.md")).unwrap(), "old readme");
        assert_eq!(std::fs::read_to_string(root.path().join("src/main.ts")).unwrap(), "old code");
    }

    #[test]
    fn failed_provider_file_does_not_block_writing_other_generated_files() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("README.md"), "existing").unwrap();
        let gateway = Arc::new(RecordingGateway::new(vec![
            GatewayResponse { status: 200, body: serde_json::json!({"choices": []}) },
            response(serde_json::json!("export {};"), "stop"),
        ]));
        let generator = LocalFileGenerationService::new(LocalWorkspaceService, gateway, "test-model".into());
        let prompts = [prompt(), FileGenerationPrompt::new("src/main.ts".into(), "typescript".into(), "entry".into(), "build it".into())];
        let files = generator.generate_and_write(root.path(), &prompts).unwrap();
        assert_eq!(files[0].status, GeneratedFileStatus::Failed);
        assert!(files[0].error.as_deref().unwrap().contains("no choices"));
        assert_eq!(files[1].status, GeneratedFileStatus::Written);
        assert_eq!(std::fs::read_to_string(root.path().join("README.md")).unwrap(), "existing");
        assert_eq!(std::fs::read_to_string(root.path().join("src/main.ts")).unwrap(), "export {};");
    }

    #[test]
    fn fenced_source_response_fails_without_overwriting_code_or_rejecting_markdown() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::write(root.path().join("src/main.ts"), "export const old = true;").unwrap();
        let gateway = Arc::new(RecordingGateway::new(vec![
            response(serde_json::json!("# Readme\n```ts\nexample\n```\n"), "stop"),
            response(serde_json::json!("\n```ts\nexport const broken = true;\n```"), "stop"),
        ]));
        let generator = LocalFileGenerationService::new(LocalWorkspaceService, gateway, "test-model".into());
        let prompts = [prompt(), FileGenerationPrompt::new("src/main.ts".into(), "typescript".into(), "entry".into(), "build it".into())];
        let files = generator.generate_and_write(root.path(), &prompts).unwrap();
        assert_eq!(files[0].status, GeneratedFileStatus::Written);
        assert_eq!(files[1].status, GeneratedFileStatus::Failed);
        assert!(files[1].error.as_deref().unwrap().contains("markdown code fence"));
        assert_eq!(std::fs::read_to_string(root.path().join("src/main.ts")).unwrap(), "export const old = true;");
        assert!(std::fs::read_to_string(root.path().join("README.md")).unwrap().contains("```ts"));
    }

    #[test]
    fn upsert_generated_file_replaces_by_path_and_appends_new() {
        let mut files = Vec::new();
        let file_a = GeneratedFile {
            path: "a.ts".into(), content: "old".into(), language: None,
            status: GeneratedFileStatus::Written, error: None,
        };
        LocalFileGenerationService::upsert_generated_file(&mut files, file_a.clone());
        assert_eq!(files.len(), 1);
        // Same path → replace in place (position preserved).
        let file_a_v2 = GeneratedFile {
            path: "a.ts".into(), content: "new".into(), language: None,
            status: GeneratedFileStatus::Written, error: None,
        };
        LocalFileGenerationService::upsert_generated_file(&mut files, file_a_v2);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].content, "new");
        // New path → append.
        let file_b = GeneratedFile {
            path: "b.ts".into(), content: "b".into(), language: None,
            status: GeneratedFileStatus::Written, error: None,
        };
        LocalFileGenerationService::upsert_generated_file(&mut files, file_b);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "a.ts");
        assert_eq!(files[1].path, "b.ts");
    }

    #[test]
    fn aggregate_token_cap_rejects_oversized_prompt_set() {
        let root = tempfile::tempdir().unwrap();
        let gateway = Arc::new(RecordingGateway::new(vec![]));
        let generator = LocalFileGenerationService::new(LocalWorkspaceService, gateway, "test-model".into());
        // 13 prompts × 16K = 208K > 196K cap.
        let prompts: Vec<FileGenerationPrompt> = (0..13)
            .map(|i| {
                let mut p = FileGenerationPrompt::new(
                    format!("file{i}.ts"), "typescript".into(), "test".into(), "build".into(),
                );
                p.max_tokens = 16_384;
                p
            })
            .collect();
        let result = generator.generate_and_write(root.path(), &prompts);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("aggregate token budget"));
    }
}
