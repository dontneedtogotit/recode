use crate::domain::{DomainError, Result};
use crate::pipeline::PipelineInput;
use serde::{Deserialize, Serialize};

use super::manifest_budget::{max_tokens_for_loc, ManifestBudget};

const MAX_PROJECT_FILES: usize = 12;
const MAX_AGGREGATE_MANIFEST_BYTES: usize = 64 * 1024;
const MAX_WAVES: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenerationManifest {
    pub run_id: String,
    pub files: Vec<ManifestFile>,
    pub prompts: Vec<ManifestPrompt>,
    #[serde(default = "ManifestBudget::default")]
    pub budget: ManifestBudget,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestFile {
    pub path: String,
    pub language: String,
    pub purpose: String,
    pub wave: u8,
    pub max_bytes: usize,
    pub status: ManifestFileStatus,
    /// Planner's expected line count. Drives `max_tokens` via
    /// `clamp(LOC × 80, 2K, 16K)`. Defaults to a per-extension heuristic when
    /// the planner does not supply one.
    #[serde(default = "default_expected_loc")]
    pub expected_loc: usize,
    /// Paths this file imports or requires before it can generate correctly.
    /// Resolved into waves by `manifest_budget::resolve_waves`; a cycle or a
    /// dangling edge is a hard error.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
}

fn default_expected_loc() -> usize {
    160
}

/// Derived per-file token budget from `expected_loc` (P1.7 formula).
pub fn file_max_tokens(file: &ManifestFile) -> usize {
    max_tokens_for_loc(file.expected_loc)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ManifestFileStatus {
    Pending,
    Generating,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestPrompt {
    pub file_path: String,
    pub prompt: String,
    pub context: Vec<String>,
}

/// Tuple entries: (path, language, purpose, wave, max_bytes, expected_loc).
/// Wave 0 is shared project scaffolding; wave 1 depends on the scaffold.
pub const VITE_PROJECT_FILES: &[(&str, &str, &str, u8, usize, usize)] = &[
    ("package.json", "json", "Vite + React package manifest with dependencies, scripts and metadata", 0, 4_096, 40),
    ("tsconfig.json", "json", "TypeScript compiler configuration for the project", 0, 2_048, 30),
    ("tsconfig.app.json", "json", "TypeScript compiler configuration for the Vite app source", 0, 2_048, 30),
    ("vite.config.ts", "typescript", "Vite build tool configuration", 0, 4_096, 40),
    ("index.html", "html", "HTML entry point that loads src/main.tsx", 0, 2_048, 20),
    ("README.md", "markdown", "Project README with setup and usage instructions", 0, 4_096, 80),
    ("src/main.tsx", "typescript", "React application entry point that renders App", 1, 8_192, 30),
    ("src/App.tsx", "typescript", "Root React component for the application", 1, 8_192, 220),
    ("src/App.css", "css", "Application stylesheet", 1, 4_096, 120),
];

pub struct LocalGenerationManifestService;

impl LocalGenerationManifestService {
    pub fn new() -> Self {
        Self
    }

    pub fn build(&self, run_id: &str, input: &PipelineInput) -> Result<GenerationManifest> {
        let goal = input.goal.trim();
        if goal.is_empty() {
            return Err(DomainError::invalid_input("goal must not be empty"));
        }

        let budget = ManifestBudget::default();
        let mut files = Vec::with_capacity(VITE_PROJECT_FILES.len().min(MAX_PROJECT_FILES));
        let mut aggregate_bytes: usize = 0;
        let mut total_tokens: usize = 0;

        for &(path, language, purpose, wave, max_bytes, expected_loc) in VITE_PROJECT_FILES {
            if files.len() >= MAX_PROJECT_FILES {
                break;
            }
            aggregate_bytes = aggregate_bytes.saturating_add(max_bytes);
            if aggregate_bytes > MAX_AGGREGATE_MANIFEST_BYTES {
                break;
            }
            // Derive per-file token budget from expected LOC via the P1.7
            // formula: clamp(LOC × 80, 2K, 16K).
            let file = ManifestFile {
                path: path.into(),
                language: language.into(),
                purpose: purpose.into(),
                wave,
                max_bytes,
                status: ManifestFileStatus::Pending,
                expected_loc,
                depends_on: wave_dependency(path, wave),
            };
            total_tokens = total_tokens.saturating_add(file_max_tokens(&file));
            files.push(file);
        }

        // Stable generation order: scaffold first, then source files.
        files.sort_by_key(|f| f.wave);

        super::manifest_budget::validate_budget(
            files.len(),
            total_tokens,
            aggregate_bytes,
            &budget,
        )?;

        Ok(GenerationManifest {
            run_id: run_id.into(),
            files,
            prompts: Vec::new(),
            budget,
        })
    }

    pub fn mark_completed(&self, manifest: &mut GenerationManifest, path: &str) {
        for file in &mut manifest.files {
            if file.path == path {
                file.status = ManifestFileStatus::Completed;
                break;
            }
        }
    }

    pub fn mark_failed(&self, manifest: &mut GenerationManifest, path: &str, error: String) {
        for file in &mut manifest.files {
            if file.path == path {
                file.status = ManifestFileStatus::Failed(error.clone());
                break;
            }
        }
    }
}

/// Default dependency edges for the canned Vite project: source files in
/// wave 1 depend on the wave-0 scaffold they import. A planner-supplied
/// manifest passes its own `depends_on`; this only seeds the default set.
fn wave_dependency(path: &str, wave: u8) -> Vec<String> {
    if wave == 0 {
        return Vec::new();
    }
    let mut deps = Vec::new();
    // main.tsx imports App from App.tsx, and the scaffold configs wrap it.
    if path == "src/main.tsx" {
        deps.push("src/App.tsx".into());
        deps.push("index.html".into());
    }
    if path == "src/App.tsx" {
        deps.push("src/App.css".into());
    }
    deps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_builds_from_goal() {
        let service = LocalGenerationManifestService::new();
        let manifest = service
            .build("run-1", &PipelineInput {
                goal: "build landing page".into(),
                emit_events: false,
                run_id: "run-1".into(),
                project_id: None,
            })
            .expect("manifest");
        assert_eq!(manifest.run_id, "run-1");
        assert_eq!(manifest.files.len(), VITE_PROJECT_FILES.len());
        assert!(manifest.prompts.is_empty());
        assert_eq!(manifest.files.first().unwrap().wave, 0);
        assert_eq!(manifest.files.last().unwrap().wave, 1);
    }

    #[test]
    fn manifest_rejects_empty_goal() {
        let service = LocalGenerationManifestService::new();
        let result = service.build(
            "run-1",
            &PipelineInput {
                goal: "   ".into(),
                emit_events: false,
                run_id: "run-1".into(),
                project_id: None,
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn manifest_orders_files_by_wave() {
        let service = LocalGenerationManifestService::new();
        let manifest = service
            .build("run-1", &PipelineInput {
                goal: "build landing page".into(),
                emit_events: false,
                run_id: "run-1".into(),
                project_id: None,
            })
            .expect("manifest");
        let mut last_wave = 0u8;
        for file in &manifest.files {
            assert!(file.wave >= last_wave, "waves must be non-decreasing");
            last_wave = file.wave;
        }
    }

    #[test]
    fn manifest_assigns_expected_loc_and_derives_token_budget() {
        let service = LocalGenerationManifestService::new();
        let manifest = service
            .build("run-1", &PipelineInput {
                goal: "build landing page".into(),
                emit_events: false,
                run_id: "run-1".into(),
                project_id: None,
            })
            .expect("manifest");
        let app = manifest.files.iter().find(|f| f.path == "src/App.tsx").unwrap();
        assert_eq!(app.expected_loc, 220);
        // clamp(220 × 80, 2K, 16K) = clamp(17600, 2048, 16384) = 16384
        assert_eq!(file_max_tokens(app), 16_384);
        let tsconfig = manifest.files.iter().find(|f| f.path == "tsconfig.json").unwrap();
        assert_eq!(tsconfig.expected_loc, 30);
        // clamp(30 × 80, 2K, 16K) = clamp(2400, 2048, 16384) = 2400
        assert_eq!(file_max_tokens(tsconfig), 2_400);
        let html = manifest.files.iter().find(|f| f.path == "index.html").unwrap();
        assert_eq!(html.expected_loc, 20);
        // clamp(20 × 80, 2K, 16K) = clamp(1600, 2048, 16384) = 2048 (floor)
        assert_eq!(file_max_tokens(html), 2_048);
    }

    #[test]
    fn manifest_wires_default_dependencies() {
        let service = LocalGenerationManifestService::new();
        let manifest = service
            .build("run-1", &PipelineInput {
                goal: "build landing page".into(),
                emit_events: false,
                run_id: "run-1".into(),
                project_id: None,
            })
            .expect("manifest");
        let main = manifest.files.iter().find(|f| f.path == "src/main.tsx").unwrap();
        assert!(main.depends_on.contains(&"src/App.tsx".to_string()));
        assert!(main.depends_on.contains(&"index.html".to_string()));
        let app = manifest.files.iter().find(|f| f.path == "src/App.tsx").unwrap();
        assert!(app.depends_on.contains(&"src/App.css".to_string()));
        // Scaffold files have no dependencies.
        let pkg = manifest.files.iter().find(|f| f.path == "package.json").unwrap();
        assert!(pkg.depends_on.is_empty());
    }

    #[test]
    fn manifest_dependencies_form_acyclic_graph_resolvable_within_wave_limit() {
        let service = LocalGenerationManifestService::new();
        let manifest = service
            .build("run-1", &PipelineInput {
                goal: "build landing page".into(),
                emit_events: false,
                run_id: "run-1".into(),
                project_id: None,
            })
            .expect("manifest");
        let dep_files: Vec<super::super::manifest_budget::DependencyFile> = manifest
            .files
            .iter()
            .map(|f| super::super::manifest_budget::DependencyFile {
                path: f.path.clone(),
                depends_on: f.depends_on.clone(),
            })
            .collect();
        let ordered = super::super::manifest_budget::resolve_waves(&dep_files, MAX_WAVES)
            .expect("default Vite dependencies must resolve");
        // Every dependency is emitted strictly before its dependent.
        let map = super::super::manifest_budget::wave_map(&ordered);
        for file in &manifest.files {
            for dep in &file.depends_on {
                let file_wave = map.get(&file.path).copied();
                let dep_wave = map.get(dep).copied();
                assert!(
                    dep_wave.is_some_and(|dw| file_wave.is_some_and(|fw| dw < fw)),
                    "{} depends on {} but wave order is wrong",
                    file.path,
                    dep
                );
            }
        }
    }

    #[test]
    fn manifest_budget_is_populated_and_within_limits() {
        let service = LocalGenerationManifestService::new();
        let manifest = service
            .build("run-1", &PipelineInput {
                goal: "build landing page".into(),
                emit_events: false,
                run_id: "run-1".into(),
                project_id: None,
            })
            .expect("manifest");
        let total_tokens: usize = manifest.files.iter().map(file_max_tokens).sum();
        let total_bytes: usize = manifest.files.iter().map(|f| f.max_bytes).sum();
        super::super::manifest_budget::validate_budget(
            manifest.files.len(),
            total_tokens,
            total_bytes,
            &manifest.budget,
        )
        .expect("built manifest must be within its own budget");
    }

    #[test]
    fn manifest_serializes_and_round_trips_with_new_fields() {
        let service = LocalGenerationManifestService::new();
        let manifest = service
            .build("run-1", &PipelineInput {
                goal: "build landing page".into(),
                emit_events: false,
                run_id: "run-1".into(),
                project_id: None,
            })
            .expect("manifest");
        let json = serde_json::to_string(&manifest).unwrap();
        // The new fields appear in the wire format (or are skipped when empty).
        assert!(json.contains("expected_loc"));
        assert!(json.contains("budget"));
        let restored: GenerationManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, manifest);
    }
}
