use std::collections::{HashMap, HashSet};

use crate::domain::{DomainError, Result};
use crate::pipeline::PipelineInput;
use serde::{Deserialize, Serialize};

use super::manifest_budget::{ManifestBudget, max_tokens_for_loc};

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
    (
        "package.json",
        "json",
        "Vite + React package manifest with dependencies, scripts and metadata",
        0,
        4_096,
        40,
    ),
    (
        "tsconfig.json",
        "json",
        "TypeScript compiler configuration for the project",
        0,
        2_048,
        30,
    ),
    (
        "tsconfig.app.json",
        "json",
        "TypeScript compiler configuration for the Vite app source",
        0,
        2_048,
        30,
    ),
    (
        "tsconfig.node.json",
        "json",
        "TypeScript compiler configuration for Vite config files",
        0,
        2_048,
        25,
    ),
    (
        "vite.config.ts",
        "typescript",
        "Vite build tool configuration",
        0,
        4_096,
        40,
    ),
    (
        ".gitignore",
        "text",
        "Git ignore rules for Node/Vite projects",
        0,
        1_024,
        30,
    ),
    (
        "index.html",
        "html",
        "HTML entry point that loads src/main.tsx",
        0,
        2_048,
        20,
    ),
    (
        "README.md",
        "markdown",
        "Project README with setup and usage instructions",
        0,
        4_096,
        80,
    ),
    (
        "src/main.tsx",
        "typescript",
        "React application entry point that renders App",
        1,
        8_192,
        30,
    ),
    (
        "src/App.tsx",
        "typescript",
        "Root React component for the application",
        1,
        8_192,
        220,
    ),
    (
        "src/App.css",
        "css",
        "Application stylesheet",
        1,
        4_096,
        120,
    ),
];

/// Vite + vanilla TypeScript static-site file set (no UI framework).
/// Same tuple shape as `VITE_PROJECT_FILES`; wave 0 is shared project
/// scaffolding, wave 1 is the app source.
pub const VANILLA_PROJECT_FILES: &[(&str, &str, &str, u8, usize, usize)] = &[
    (
        "package.json",
        "json",
        "Vite + vanilla TypeScript package manifest with dependencies, scripts and metadata",
        0,
        4_096,
        40,
    ),
    (
        "tsconfig.json",
        "json",
        "TypeScript compiler configuration for the project",
        0,
        2_048,
        30,
    ),
    (
        "tsconfig.app.json",
        "json",
        "TypeScript compiler configuration for the Vite app source",
        0,
        2_048,
        30,
    ),
    (
        "tsconfig.node.json",
        "json",
        "TypeScript compiler configuration for Vite config files",
        0,
        2_048,
        25,
    ),
    (
        "vite.config.ts",
        "typescript",
        "Vite build tool configuration",
        0,
        4_096,
        40,
    ),
    (
        ".gitignore",
        "text",
        "Git ignore rules for Node/Vite projects",
        0,
        1_024,
        30,
    ),
    (
        "index.html",
        "html",
        "HTML entry point that loads src/main.ts",
        0,
        2_048,
        20,
    ),
    (
        "README.md",
        "markdown",
        "Project README with setup and usage instructions",
        0,
        4_096,
        80,
    ),
    (
        "src/main.ts",
        "typescript",
        "Application entry point that mounts the app module",
        1,
        8_192,
        60,
    ),
    (
        "src/app.ts",
        "typescript",
        "Root app module that renders the page",
        1,
        8_192,
        220,
    ),
    (
        "src/styles.css",
        "css",
        "Application stylesheet",
        1,
        4_096,
        120,
    ),
];

pub struct LocalGenerationManifestService;

impl LocalGenerationManifestService {
    pub fn new() -> Self {
        Self
    }

    pub fn build(
        &self,
        run_id: &str,
        input: &PipelineInput,
        plan_tasks: &[crate::plan_graph::PlanTask],
    ) -> Result<GenerationManifest> {
        let goal = input.goal.trim();
        if goal.is_empty() {
            return Err(DomainError::invalid_input("goal must not be empty"));
        }

        // Planner-supplied file graph: a file's dependencies are the `consumes`
        // of the plan tasks that `produces` it, filtered to paths actually in
        // this manifest (dangling edges are a hard error in wave resolution).
        let produces: HashMap<String, Vec<String>> = plan_tasks
            .iter()
            .flat_map(|task| {
                task.produces
                    .iter()
                    .map(move |out| (out.clone(), task.consumes.clone()))
            })
            .collect();
        // Goal-driven stack selection (P1.7 non-React manifest paths): a
        // vanilla/static goal gets the vanilla Vite file set instead of the
        // React SPA set. React stays the default for ordinary goals.
        let stack = super::stack::ProjectStack::detect(goal);
        let project_files = stack.project_files();
        let manifest_paths: HashSet<String> = project_files
            .iter()
            .map(|&(path, ..)| path.to_string())
            .collect();

        let budget = ManifestBudget::default();
        let mut files = Vec::with_capacity(project_files.len().min(MAX_PROJECT_FILES));
        let mut aggregate_bytes: usize = 0;
        let mut total_tokens: usize = 0;

        for &(path, language, purpose, wave, max_bytes, expected_loc) in project_files {
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
                depends_on: file_dependencies(path, wave, &produces, &manifest_paths),
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

/// Dependency edges for a manifest file. Planner-supplied tasks win: a file
/// produced by a plan task depends on that task's `consumes`, filtered to
/// paths actually present in the manifest (dangling edges are a hard error
/// in wave resolution). Files the planner does not map fall back to the
/// canned wave-0/1 scaffold edges.
fn file_dependencies(
    path: &str,
    wave: u8,
    produces: &HashMap<String, Vec<String>>,
    manifest_paths: &HashSet<String>,
) -> Vec<String> {
    if wave == 0 {
        return Vec::new();
    }
    if let Some(consumes) = produces.get(path) {
        let mut deps: Vec<String> = consumes
            .iter()
            .filter(|dep| manifest_paths.contains(*dep))
            .cloned()
            .collect();
        deps.sort();
        deps.dedup();
        return deps;
    }
    wave_dependency(path, wave)
}

fn wave_dependency(path: &str, wave: u8) -> Vec<String> {
    if wave == 0 {
        return Vec::new();
    }
    let mut deps = Vec::new();
    // React SPA: main.tsx imports App from App.tsx, and the scaffold configs
    // wrap it.
    if path == "src/main.tsx" {
        deps.push("src/App.tsx".into());
        deps.push("index.html".into());
    }
    if path == "src/App.tsx" {
        deps.push("src/App.css".into());
    }
    // Vanilla static site: main.ts imports the app module and its stylesheet.
    if path == "src/main.ts" {
        deps.push("src/app.ts".into());
        deps.push("index.html".into());
    }
    if path == "src/app.ts" {
        deps.push("src/styles.css".into());
    }
    deps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(goal: &str) -> PipelineInput {
        PipelineInput {
            goal: goal.into(),
            emit_events: false,
            run_id: "run-1".into(),
            project_id: None,
            execution_consent: crate::pipeline::supervisor::ExecutionConsent::NotSupplied,
        }
    }

    #[test]
    fn planner_tasks_override_depends_on() {
        let service = LocalGenerationManifestService::new();
        let tasks = vec![crate::plan_graph::PlanTask {
            id: "t1".into(),
            produces: vec!["src/main.tsx".into()],
            consumes: vec![
                "src/App.tsx".into(),
                "index.html".into(),
                "NOT_IN_MANIFEST.md".into(),
            ],
        }];
        let manifest = service
            .build("run-1", &input("build landing page"), &tasks)
            .expect("manifest");
        let main = manifest
            .files
            .iter()
            .find(|f| f.path == "src/main.tsx")
            .expect("main.tsx in manifest");
        // Planner-supplied deps replace the canned wave edges, filtered to
        // manifest paths (dangling edge dropped) and deduplicated.
        assert_eq!(
            main.depends_on,
            vec!["index.html".to_string(), "src/App.tsx".to_string()]
        );
        // Unmapped files keep the canned defaults.
        let app = manifest
            .files
            .iter()
            .find(|f| f.path == "src/App.tsx")
            .expect("App.tsx in manifest");
        assert_eq!(app.depends_on, vec!["src/App.css".to_string()]);
        // Wave resolution still accepts the graph (acyclic, in-manifest).
        let dep_files: Vec<crate::pipeline::manifest_budget::DependencyFile> = manifest
            .files
            .iter()
            .map(|f| crate::pipeline::manifest_budget::DependencyFile {
                path: f.path.clone(),
                depends_on: f.depends_on.clone(),
            })
            .collect();
        crate::pipeline::manifest_budget::resolve_waves(&dep_files, MAX_WAVES)
            .expect("planner-overridden graph must resolve into waves");
    }

    #[test]
    fn empty_plan_tasks_keep_canned_wave_dependencies() {
        let service = LocalGenerationManifestService::new();
        let manifest = service
            .build("run-1", &input("build landing page"), &[])
            .expect("manifest");
        let main = manifest
            .files
            .iter()
            .find(|f| f.path == "src/main.tsx")
            .expect("main.tsx in manifest");
        assert_eq!(
            main.depends_on,
            vec!["src/App.tsx".to_string(), "index.html".to_string()]
        );
    }

    #[test]
    fn manifest_builds_from_goal() {
        let service = LocalGenerationManifestService::new();
        let manifest = service
            .build("run-1", &input("build landing page"), &[])
            .expect("manifest");
        assert_eq!(manifest.run_id, "run-1");
        assert_eq!(manifest.files.len(), VITE_PROJECT_FILES.len());
        assert!(manifest.prompts.is_empty());
        assert_eq!(manifest.files.first().unwrap().wave, 0);
        assert_eq!(manifest.files.last().unwrap().wave, 1);
    }

    #[test]
    fn vanilla_goal_builds_vanilla_manifest_paths() {
        let service = LocalGenerationManifestService::new();
        let manifest = service
            .build(
                "run-1",
                &input("build a vanilla typescript static site"),
                &[],
            )
            .expect("manifest");
        // The vanilla file set replaces every React path.
        let paths: Vec<&str> = manifest.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            VANILLA_PROJECT_FILES
                .iter()
                .map(|&(p, ..)| p)
                .collect::<Vec<_>>()
        );
        assert!(!paths.contains(&"src/main.tsx"));
        assert!(!paths.contains(&"src/App.tsx"));
        assert!(paths.contains(&"src/main.ts"));
        assert!(paths.contains(&"src/app.ts"));
        assert!(paths.contains(&"src/styles.css"));
    }

    #[test]
    fn vanilla_manifest_wires_default_dependencies() {
        let service = LocalGenerationManifestService::new();
        let manifest = service
            .build("run-1", &input("vanilla typescript site"), &[])
            .expect("manifest");
        let main = manifest
            .files
            .iter()
            .find(|f| f.path == "src/main.ts")
            .expect("main.ts in vanilla manifest");
        assert_eq!(
            main.depends_on,
            vec!["src/app.ts".to_string(), "index.html".to_string()]
        );
        let app = manifest
            .files
            .iter()
            .find(|f| f.path == "src/app.ts")
            .expect("app.ts in vanilla manifest");
        assert_eq!(app.depends_on, vec!["src/styles.css".to_string()]);
        // Scaffold files have no dependencies.
        let pkg = manifest
            .files
            .iter()
            .find(|f| f.path == "package.json")
            .expect("package.json");
        assert!(pkg.depends_on.is_empty());
    }

    #[test]
    fn vanilla_manifest_dependencies_resolve_into_waves() {
        let service = LocalGenerationManifestService::new();
        let manifest = service
            .build("run-1", &input("static site, no framework"), &[])
            .expect("manifest");
        let dep_files: Vec<crate::pipeline::manifest_budget::DependencyFile> = manifest
            .files
            .iter()
            .map(|f| crate::pipeline::manifest_budget::DependencyFile {
                path: f.path.clone(),
                depends_on: f.depends_on.clone(),
            })
            .collect();
        let ordered = crate::pipeline::manifest_budget::resolve_waves(&dep_files, MAX_WAVES)
            .expect("vanilla dependencies must resolve into waves");
        let map = crate::pipeline::manifest_budget::wave_map(&ordered);
        for file in &manifest.files {
            for dep in &file.depends_on {
                assert!(
                    map.get(dep).copied().is_some_and(|dw| dw < map[&file.path]),
                    "{} depends on {} but wave order is wrong",
                    file.path,
                    dep
                );
            }
        }
    }

    #[test]
    fn react_goal_still_builds_react_manifest() {
        let service = LocalGenerationManifestService::new();
        let manifest = service
            .build("run-1", &input("build a react landing page"), &[])
            .expect("manifest");
        let paths: Vec<&str> = manifest.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            VITE_PROJECT_FILES
                .iter()
                .map(|&(p, ..)| p)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn planner_vanilla_tasks_override_depends_on() {
        let service = LocalGenerationManifestService::new();
        let tasks = vec![crate::plan_graph::PlanTask {
            id: "t1".into(),
            produces: vec!["src/main.ts".into()],
            consumes: vec![
                "src/app.ts".into(),
                "index.html".into(),
                "NOT_IN_MANIFEST.md".into(),
            ],
        }];
        let manifest = service
            .build("run-1", &input("vanilla typescript site"), &tasks)
            .expect("manifest");
        let main = manifest
            .files
            .iter()
            .find(|f| f.path == "src/main.ts")
            .expect("main.ts");
        // Dangling edge dropped, deps filtered to vanilla manifest paths.
        assert_eq!(
            main.depends_on,
            vec!["index.html".to_string(), "src/app.ts".to_string()]
        );
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
                execution_consent: crate::pipeline::supervisor::ExecutionConsent::NotSupplied,
            },
            &[],
        );
        assert!(result.is_err());
    }

    #[test]
    fn manifest_orders_files_by_wave() {
        let service = LocalGenerationManifestService::new();
        let manifest = service
            .build("run-1", &input("build landing page"), &[])
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
            .build("run-1", &input("build landing page"), &[])
            .expect("manifest");
        let app = manifest
            .files
            .iter()
            .find(|f| f.path == "src/App.tsx")
            .unwrap();
        assert_eq!(app.expected_loc, 220);
        // clamp(220 × 80, 2K, 16K) = clamp(17600, 2048, 16384) = 16384
        assert_eq!(file_max_tokens(app), 16_384);
        let tsconfig = manifest
            .files
            .iter()
            .find(|f| f.path == "tsconfig.json")
            .unwrap();
        assert_eq!(tsconfig.expected_loc, 30);
        // clamp(30 × 80, 2K, 16K) = clamp(2400, 2048, 16384) = 2400
        assert_eq!(file_max_tokens(tsconfig), 2_400);
        let html = manifest
            .files
            .iter()
            .find(|f| f.path == "index.html")
            .unwrap();
        assert_eq!(html.expected_loc, 20);
        // clamp(20 × 80, 2K, 16K) = clamp(1600, 2048, 16384) = 2048 (floor)
        assert_eq!(file_max_tokens(html), 2_048);
    }

    #[test]
    fn manifest_wires_default_dependencies() {
        let service = LocalGenerationManifestService::new();
        let manifest = service
            .build("run-1", &input("build landing page"), &[])
            .expect("manifest");
        let main = manifest
            .files
            .iter()
            .find(|f| f.path == "src/main.tsx")
            .unwrap();
        assert!(main.depends_on.contains(&"src/App.tsx".to_string()));
        assert!(main.depends_on.contains(&"index.html".to_string()));
        let app = manifest
            .files
            .iter()
            .find(|f| f.path == "src/App.tsx")
            .unwrap();
        assert!(app.depends_on.contains(&"src/App.css".to_string()));
        // Scaffold files have no dependencies.
        let pkg = manifest
            .files
            .iter()
            .find(|f| f.path == "package.json")
            .unwrap();
        assert!(pkg.depends_on.is_empty());
    }

    #[test]
    fn manifest_dependencies_form_acyclic_graph_resolvable_within_wave_limit() {
        let service = LocalGenerationManifestService::new();
        let manifest = service
            .build("run-1", &input("build landing page"), &[])
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
            .build("run-1", &input("build landing page"), &[])
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
            .build("run-1", &input("build landing page"), &[])
            .expect("manifest");
        let json = serde_json::to_string(&manifest).unwrap();
        // The new fields appear in the wire format (or are skipped when empty).
        assert!(json.contains("expected_loc"));
        assert!(json.contains("budget"));
        let restored: GenerationManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, manifest);
    }
}
