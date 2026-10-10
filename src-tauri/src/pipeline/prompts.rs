use super::generation_errors::{GenerationError, GenerationErrorKind};
use super::manifest::{GenerationManifest, ManifestPrompt};
use super::types::FileGenerationPrompt;

pub struct LocalPromptBuilderService;

impl LocalPromptBuilderService {
    pub fn new() -> Self {
        Self
    }

    /// Kept for callers that still operate on manifest prompts (e.g. the
    /// legacy engine run_internal smoke path). New generation paths should
    /// use [`Self::build_file_prompts`], which produces typed prompts that the
    /// live file generation service can execute.
    pub fn build(&self, manifest: &GenerationManifest) -> Result<Vec<ManifestPrompt>, GenerationError> {
        if manifest.prompts.is_empty() {
            let mut prompts = Vec::new();
            for file in &manifest.files {
                prompts.push(ManifestPrompt {
                    file_path: file.path.clone(),
                    prompt: format!(
                        "Generate {} for project run {} ({})",
                        file.path, manifest.run_id, file.purpose
                    ),
                    context: Vec::new(),
                });
            }
            Ok(prompts)
        } else {
            Ok(manifest.prompts.clone())
        }
    }

    /// Build executable file-generation prompts from a bounded Vite manifest.
    ///
    /// Every prompt carries the full project manifest so the model can produce
    /// consistent imports and module boundaries, while the user prompt scopes
    /// the request to a single file.
    pub fn build_file_prompts(
        &self,
        manifest: &GenerationManifest,
        goal: &str,
    ) -> Result<Vec<FileGenerationPrompt>, GenerationError> {
        if manifest.files.is_empty() {
            return Err(GenerationError::new(GenerationErrorKind::PromptFailed(
                "manifest has no files".into(),
            )));
        }

        if manifest.files.len() > 24 {
            return Err(GenerationError::new(GenerationErrorKind::PromptFailed(
                "manifest exceeds the generation request limit".into(),
            )));
        }

        let manifest_lines: Vec<String> = manifest
            .files
            .iter()
            .map(|file| format!("- {} ({}): {}", file.path, file.language, file.purpose))
            .collect();
        let manifest_section = manifest_lines.join("\n");
        let goal = goal.trim();

        let mut prompts = Vec::with_capacity(manifest.files.len());
        for file in &manifest.files {
            let user_prompt = format!(
                "Project request: {goal}\n\n\
                 Project file manifest — generate one file per request:\
                 \n{manifest_section}\n\n\
                 You are responsible for generating exactly this one file: {path} ({language}).\
                 \nPurpose: {purpose}\n\n\
                 Return ONLY the raw, complete file content. \
                 Include all necessary imports and exports that are compatible with the manifest above. \
                 Do not wrap the response in markdown code fences, do not add explanations, \
                 and do not output other files.\n\n\
                 Requirements:\n\
                 - Use strict TypeScript (no any, no ts-ignore)\n\
                 - Handle errors with structured error types\n\
                 - Do NOT use mock shortcuts, TODO comments, placeholder strings, or throw new Error(\"Not implemented\")\n\
                 - Generate COMPLETE file content — never truncate or omit sections",
                goal = goal,
                manifest_section = manifest_section,
                path = file.path,
                language = file.language,
                purpose = file.purpose,
            );
            // Inject file-type-specific requirements (ported from devcode
            // GenerationPromptBuilder → getFileTypeRequirements). This prevents
            // common defect classes by giving the model path/language-specific
            // generation guidance (e.g., "use async/await for API routes",
            // "include error boundaries in components").
            let type_reqs = crate::pipeline::file_classification::get_file_type_requirements(
                &file.path,
                &file.language,
            );
            let user_prompt = if type_reqs.is_empty() {
                user_prompt
            } else {
                format!(
                    "{user_prompt}\n\nFile-type requirements:\n{}",
                    type_reqs.iter().map(|r| format!("- {r}")).collect::<Vec<_>>().join("\n")
                )
            };
            let mut prompt = FileGenerationPrompt::new(
                file.path.clone(),
                file.language.clone(),
                file.purpose.clone(),
                user_prompt,
            );
            // The manifest's expected_loc is authoritative: override the
            // path-keyword heuristic with the planner's LOC estimate via the
            // P1.7 formula clamp(LOC × 80, 2K, 16K).
            prompt.max_tokens = super::manifest::file_max_tokens(file);
            prompts.push(prompt);
        }

        Ok(prompts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::manifest::{ManifestFile, ManifestFileStatus};

    fn sample_manifest() -> GenerationManifest {
        GenerationManifest {
            run_id: "run-1".into(),
            files: vec![ManifestFile {
                path: "README.md".into(),
                language: "markdown".into(),
                purpose: "project readme".into(),
                wave: 0,
                max_bytes: 2048,
                status: ManifestFileStatus::Pending,
                expected_loc: 80,
                depends_on: Vec::new(),
            }],
            prompts: Vec::new(),
            budget: super::super::manifest_budget::ManifestBudget::default(),
        }
    }

    #[test]
    fn prompt_builder_generates_prompts_for_empty_manifest() {
        let service = LocalPromptBuilderService::new();
        let prompts = service.build(&sample_manifest()).expect("prompts");
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].file_path, "README.md");
    }

    #[test]
    fn file_prompt_builder_generates_typed_prompts() {
        let service = LocalPromptBuilderService::new();
        let prompts = service
            .build_file_prompts(&sample_manifest(), "build a landing page")
            .expect("file prompts");
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].file_path, "README.md");
        assert!(prompts[0].user_prompt.contains("build a landing page"));
        assert!(prompts[0].user_prompt.contains("Project file manifest"));
        assert!(prompts[0].user_prompt.contains("You are responsible for generating exactly this one file: README.md"));
    }

    #[test]
    fn file_prompt_uses_manifest_expected_loc_for_token_budget() {
        let service = LocalPromptBuilderService::new();
        let manifest = GenerationManifest {
            run_id: "run-1".into(),
            files: vec![ManifestFile {
                path: "src/App.tsx".into(),
                language: "typescript".into(),
                purpose: "root component".into(),
                wave: 1,
                max_bytes: 8192,
                status: ManifestFileStatus::Pending,
                expected_loc: 220,
                depends_on: Vec::new(),
            }],
            prompts: Vec::new(),
            budget: super::super::manifest_budget::ManifestBudget::default(),
        };
        let prompts = service.build_file_prompts(&manifest, "build app").unwrap();
        // clamp(220 × 80, 2K, 16K) = 16384, not the old heuristic.
        assert_eq!(prompts[0].max_tokens, 16_384);
    }

    #[test]
    fn file_prompt_builder_rejects_empty_manifest() {
        let service = LocalPromptBuilderService::new();
        let manifest = GenerationManifest {
            run_id: "run-empty".into(),
            files: Vec::new(),
            prompts: Vec::new(),
            budget: super::super::manifest_budget::ManifestBudget::default(),
        };
        let result = service.build_file_prompts(&manifest, "build a landing page");
        assert!(result.is_err());
    }

    #[test]
    fn file_prompt_includes_type_requirements_for_tsx() {
        let service = LocalPromptBuilderService::new();
        let manifest = GenerationManifest {
            run_id: "run-tsx".into(),
            files: vec![ManifestFile {
                path: "src/App.tsx".into(),
                language: "typescript".into(),
                purpose: "root component".into(),
                wave: 1,
                max_bytes: 8192,
                status: ManifestFileStatus::Pending,
                expected_loc: 100,
                depends_on: Vec::new(),
            }],
            prompts: Vec::new(),
            budget: super::super::manifest_budget::ManifestBudget::default(),
        };
        let prompts = service.build_file_prompts(&manifest, "build app").unwrap();
        // The prompt should include the general requirements section.
        assert!(prompts[0].user_prompt.contains("Requirements:"));
        assert!(prompts[0].user_prompt.contains("strict TypeScript"));
        // The prompt should also include file-type-specific requirements.
        assert!(
            prompts[0].user_prompt.contains("File-type requirements:"),
            "expected file-type requirements section, got: {}",
            prompts[0].user_prompt
        );
    }

    #[test]
    fn file_prompt_omits_type_requirements_when_empty() {
        let service = LocalPromptBuilderService::new();
        // get_file_type_requirements always returns at least a default
        // ("Keep functions focused and small"), so the File-type requirements
        // section always appears. This test verifies the section IS present
        // even for files without specific path matches (CSS).
        let manifest = GenerationManifest {
            run_id: "run-css".into(),
            files: vec![ManifestFile {
                path: "src/App.css".into(),
                language: "css".into(),
                purpose: "styles".into(),
                wave: 1,
                max_bytes: 4096,
                status: ManifestFileStatus::Pending,
                expected_loc: 100,
                depends_on: Vec::new(),
            }],
            prompts: Vec::new(),
            budget: super::super::manifest_budget::ManifestBudget::default(),
        };
        let prompts = service.build_file_prompts(&manifest, "build app").unwrap();
        // CSS gets the default requirement, so the section appears.
        assert!(prompts[0].user_prompt.contains("File-type requirements:"));
        assert!(prompts[0].user_prompt.contains("Keep functions focused"));
    }
}
