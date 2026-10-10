//! File Classification Utilities (ported from devcode FileClassificationUtils.ts)
//!
//! Utilities for classifying and categorizing files during code generation.
//! Used to determine generation order and identify high-risk files.
//! Pure module — no stores, no DI, no I/O.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileToGenerate {
    pub path: String,
    pub language: String,
    pub purpose: String,
}

/// Split files into generation waves: test files first, then contract files
/// (types, interfaces, configs, routes), then consumer files.
pub fn compute_generation_waves(files: &[FileToGenerate]) -> Vec<Vec<FileToGenerate>> {
    if files.len() <= 1 {
        return vec![files.to_vec()];
    }

    let mut test_files = Vec::new();
    let mut contract_files = Vec::new();
    let mut impl_files = Vec::new();

    let contract_patterns = [
        "/types/", "/interfaces/", "/models/", "/schemas/", "/routes/",
        "/constants/", "/storage/", "/repositories/", "/repository/",
        ".types.ts", ".interfaces.ts", ".repository.ts", ".d.ts",
        "types.ts", "interfaces.ts", "routes.ts", "constants.ts",
        "config.ts", "repository.ts",
    ];

    for file in files {
        let lower_path = file.path.to_ascii_lowercase();
        let is_test = crate::pipeline::generated_module_paths::is_test_path(&file.path)
            || lower_path.contains("__tests__/")
            || lower_path.contains("/tests/");
        if is_test {
            test_files.push(file.clone());
        } else if contract_patterns.iter().any(|p| lower_path.contains(p)) {
            contract_files.push(file.clone());
        } else {
            impl_files.push(file.clone());
        }
    }

    let mut waves = Vec::new();
    if !test_files.is_empty() { waves.push(test_files); }
    if !contract_files.is_empty() { waves.push(contract_files); }
    if !impl_files.is_empty() { waves.push(impl_files); }

    if waves.len() <= 1 {
        return vec![files.to_vec()];
    }
    waves
}

/// Determine if a file should have tests generated for it.
pub fn is_testable_file(path: &str, language: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    // Test/spec files are test outputs, not inputs
    if lower.ends_with(".test.ts") || lower.ends_with(".test.tsx")
        || lower.ends_with(".test.js") || lower.ends_with(".test.jsx")
        || lower.ends_with(".test.mts") || lower.ends_with(".test.mjs")
        || lower.ends_with(".spec.ts") || lower.ends_with(".spec.tsx")
        || lower.ends_with(".spec.js") || lower.ends_with(".spec.jsx")
        || lower.ends_with(".spec.mts") || lower.ends_with(".spec.mjs")
    {
        return false;
    }
    if lower.ends_with(".d.ts") { return false; }
    if lower.ends_with(".css") || lower.ends_with(".scss")
        || lower.ends_with(".html") || lower.ends_with(".json")
        || lower.ends_with(".md") || lower.ends_with(".svg")
        || lower.ends_with(".png")
    {
        return false;
    }
    // Tooling configs
    let filename = lower.rsplit('/').next().unwrap_or(&lower);
    if (filename.ends_with(".config.ts") || filename.ends_with(".config.js")
        || filename.ends_with(".config.mts") || filename.ends_with(".config.mjs")
        || filename.ends_with(".setup.ts") || filename.ends_with(".setup.js"))
    {
        return false;
    }
    let lang = language.to_ascii_lowercase();
    matches!(lang.as_str(), "typescript" | "javascript" | "python" | "go" | "rust" | "java")
}

/// Determine if a file matches high-risk patterns requiring extra validation.
pub fn is_high_risk_file(path: &str) -> bool {
    let high_risk_suffixes = [
        "index.ts", "index.tsx", "index.js", "index.jsx",
        "App.ts", "App.tsx", "App.jsx",
        "main.ts", "main.tsx", "main.js",
        "server.ts", "server.js",
        "router.ts", "router.tsx",
    ];
    let filename = path.rsplit('/').next().unwrap_or(path);
    if high_risk_suffixes.iter().any(|s| filename == *s) {
        return true;
    }
    let lower = path.to_ascii_lowercase();
    lower.contains("/api/") || lower.contains("/auth/")
        || lower.contains("/services/") || lower.contains("/controllers/")
        || filename.ends_with("Service.ts") || filename.ends_with("Service.tsx")
        || filename.ends_with("Controller.ts") || filename.ends_with("Controller.tsx")
}

/// Returns file-type-specific generation requirements based on path/language.
pub fn get_file_type_requirements(path: &str, language: &str) -> Vec<String> {
    let p = path.to_ascii_lowercase();
    let lang = language.to_ascii_lowercase();

    if p.ends_with(".tsx") || p.contains("/components/") || p.contains("/pages/") {
        return vec![
            "Use React functional components with explicit TypeScript props interfaces".into(),
            "Prefer named exports; use default export only for page/route components".into(),
            "Implement real interactive event handlers — NO empty () => {} or alert() stubs".into(),
            "Bind form submissions with full validation and state updates".into(),
            "Include aria-label / role attributes for interactive elements".into(),
        ];
    }
    if p.contains("/hooks/") || p.starts_with("use") {
        return vec![
            "Prefix the hook name with \"use\"".into(),
            "Return a stable object/array with concrete state mutation functions".into(),
            "Clean up subscriptions and timers in useEffect return functions".into(),
        ];
    }
    if p.contains("types.ts") || p.contains("interfaces.ts") || p.ends_with(".d.ts") || p.contains("/domain/") {
        return vec![
            "Export only type/interface/enum definitions — no runtime code".into(),
            "Use discriminated unions for variant types".into(),
            "Prefer readonly properties for immutable data shapes".into(),
        ];
    }
    if p.contains("/services/") || p.ends_with("service.ts") {
        return vec![
            "Use dependency injection via constructor parameters".into(),
            "Return typed result objects rather than throwing for expected failures".into(),
            "Log errors with structured context (file path, operation, error message)".into(),
        ];
    }
    if p.contains("/api/") || p.contains("/routes/") || p.contains("controller") {
        return vec![
            "Validate all inputs at the boundary before processing".into(),
            "Return consistent error shapes with status codes and messages".into(),
            "Use async/await with explicit error handling for every external call".into(),
        ];
    }
    if lang == "python" {
        return vec![
            "Add type hints on every function signature".into(),
            "Use dataclasses or Pydantic models for structured data".into(),
            "Write Google-style docstrings for public functions".into(),
        ];
    }
    if lang == "go" {
        return vec![
            "Return errors as the last value; never discard them".into(),
            "Use context.Context as the first parameter for cancellable operations".into(),
            "Prefer table-driven tests for coverage".into(),
        ];
    }
    vec!["Keep functions focused and small (single responsibility)".into()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_generation_waves() {
        let files = vec![
            FileToGenerate { path: "src/App.test.ts".into(), language: "typescript".into(), purpose: "test".into() },
            FileToGenerate { path: "src/types.ts".into(), language: "typescript".into(), purpose: "types".into() },
            FileToGenerate { path: "src/App.tsx".into(), language: "typescript".into(), purpose: "main".into() },
        ];
        let waves = compute_generation_waves(&files);
        assert_eq!(waves.len(), 3);
        assert_eq!(waves[0][0].path, "src/App.test.ts");
        assert_eq!(waves[1][0].path, "src/types.ts");
        assert_eq!(waves[2][0].path, "src/App.tsx");
    }

    #[test]
    fn test_is_testable_file() {
        assert!(is_testable_file("src/App.tsx", "typescript"));
        assert!(!is_testable_file("src/App.test.tsx", "typescript"));
        assert!(!is_testable_file("src/App.css", "css"));
        assert!(!is_testable_file("index.html", "html"));
        assert!(!is_testable_file("vite.config.ts", "typescript"));
        assert!(is_testable_file("src/utils.go", "go"));
    }

    #[test]
    fn test_is_high_risk_file() {
        assert!(is_high_risk_file("src/index.ts"));
        assert!(is_high_risk_file("src/App.tsx"));
        assert!(is_high_risk_file("src/main.tsx"));
        assert!(is_high_risk_file("src/api/handler.ts"));
        assert!(is_high_risk_file("src/services/AuthService.ts"));
        assert!(!is_high_risk_file("src/components/Button.tsx"));
    }

    #[test]
    fn test_get_file_type_requirements() {
        let tsx_reqs = get_file_type_requirements("src/components/Button.tsx", "typescript");
        assert!(!tsx_reqs.is_empty());
        assert!(tsx_reqs.iter().any(|r| r.contains("React")));

        let type_reqs = get_file_type_requirements("src/types.ts", "typescript");
        assert!(type_reqs.iter().any(|r| r.contains("type/interface")));

        let go_reqs = get_file_type_requirements("main.go", "go");
        assert!(go_reqs.iter().any(|r| r.contains("context")));
    }
}
