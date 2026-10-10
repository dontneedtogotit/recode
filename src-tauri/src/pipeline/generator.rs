use crate::domain::Result;
use super::generation_errors::{GenerationError, GenerationErrorKind};
use super::manifest::{GenerationManifest, ManifestFile, ManifestFileStatus, ManifestPrompt};

pub struct LocalFileGeneratorService;

impl LocalFileGeneratorService {
    pub fn new() -> Self {
        Self
    }

    pub fn generate(&self, manifest: &mut GenerationManifest, prompt: &ManifestPrompt) -> Result<String, GenerationError> {
        let file = manifest
            .files
            .iter_mut()
            .find(|file| file.path == prompt.file_path)
            .ok_or_else(|| GenerationError::new(GenerationErrorKind::NotFound(prompt.file_path.clone())))?;

        file.status = ManifestFileStatus::Generating;
        let content = if prompt.file_path == "README.md" {
            format!("# {}\nGenerated project.\n", manifest.run_id)
        } else if prompt.file_path == "package.json" {
            "{\n  \"name\": \"generated-project\",\n  \"version\": \"0.1.0\"\n}\n".into()
        } else {
            format!("// Generated: {}\n", prompt.file_path)
        };
        file.status = ManifestFileStatus::Completed;
        Ok(content)
    }

    pub fn write(&self, _workspace_root: &str, _path: &str, _content: &str) -> Result<(), GenerationError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::pipeline::manifest::{GenerationManifest, ManifestFile, ManifestFileStatus, ManifestPrompt};
    use super::LocalFileGeneratorService;

    #[test]
    fn generator_produces_content_for_prompt() {
        let service = LocalFileGeneratorService::new();
        let mut manifest = GenerationManifest {
            run_id: "run-1".into(),
            files: vec![ManifestFile {
                path: "README.md".into(),
                language: "markdown".into(),
                purpose: "readme".into(),
                wave: 0,
                max_bytes: 2048,
                status: ManifestFileStatus::Pending,
                expected_loc: 80,
                depends_on: Vec::new(),
            }],
            prompts: Vec::new(),
            budget: super::super::manifest_budget::ManifestBudget::default(),
        };
        let prompt = ManifestPrompt {
            file_path: "README.md".into(),
            prompt: "Generate README".into(),
            context: Vec::new(),
        };
        manifest.prompts.push(prompt.clone());
        let content = service
            .generate(&mut manifest, &prompt)
            .expect("generated content");
        assert!(content.contains("Generated project"));
        assert!(manifest.files[0].status == ManifestFileStatus::Completed);
    }
}
