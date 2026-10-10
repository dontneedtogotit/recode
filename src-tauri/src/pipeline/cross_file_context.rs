//! Cross-File Context Builder (ported from devcode CrossFileContextBuilder.ts)
//!
//! Utilities for building cross-file context from previously generated files,
//! enabling consistent imports, types, and exports across file generations.
//! Uses a 4-tier strategic file selection algorithm and content summarization.
//! Pure module — no stores, no DI, no I/O.

use std::collections::BTreeSet;

const NOISE_FILE_EXTENSIONS: &[&str] = &[
    ".svg", ".png", ".jpg", ".jpeg", ".gif", ".ico", ".webp",
    ".woff", ".woff2", ".ttf", ".eot", ".pdf", ".zip", ".tar",
    ".gz", ".exe", ".dll", ".so", ".dylib", ".map",
];

const NOISE_FILE_NAMES: &[&str] = &[
    "package-lock.json", "yarn.lock", "pnpm-lock.yaml", "cargo.lock",
];

const NOISE_FILE_DIRS: &[&str] = &[
    "dist/", "build/", ".git/", ".next/", ".cache/", "coverage/", "node_modules/",
];

const MIN_CONTEXT_FILES: usize = 4;
const MAX_CONTEXT_FILES: usize = 24;
const CONTEXT_FILE_RATIO: f64 = 0.6;
const MIN_TYPE_FILES: usize = 2;
const MAX_TYPE_FILES: usize = 8;
const TYPE_FILE_RATIO: f64 = 0.5;

/// True when a file should be rejected from strategic context (noise/binary/bloat).
pub fn is_noise_file(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    if NOISE_FILE_NAMES.iter().any(|name| lower.ends_with(name)) {
        return true;
    }
    if NOISE_FILE_EXTENSIONS.iter().any(|ext| lower.ends_with(ext)) {
        return true;
    }
    if lower.ends_with(".min.js") || lower.ends_with(".min.css") {
        return true;
    }
    NOISE_FILE_DIRS.iter().any(|dir| lower.contains(dir))
}

fn is_manifest_config(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with("package.json")
        || lower.starts_with("tsconfig") && lower.ends_with(".json")
        || lower.ends_with("vite.config.ts") || lower.ends_with("vite.config.js")
        || lower.ends_with("next.config.ts") || lower.ends_with("next.config.js")
        || lower.ends_with("dockerfile")
        || lower.ends_with("docker-compose.yml") || lower.ends_with("docker-compose.yaml")
        || lower.ends_with(".env") || lower.ends_with(".env.example")
        || lower.ends_with("readme.md")
        || lower.ends_with("cargo.toml")
        || lower.ends_with("go.mod")
        || lower.ends_with("pyproject.toml")
}

fn is_entrypoint(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let filename = lower.rsplit('/').next().unwrap_or(&lower);
    matches!(filename,
        "server.ts" | "server.tsx" | "server.js" | "server.jsx"
        | "app.ts" | "app.tsx" | "app.js" | "app.jsx"
        | "index.ts" | "index.tsx" | "index.js" | "index.jsx"
        | "main.ts" | "main.tsx" | "main.js" | "main.jsx"
        | "routes.ts" | "routes.tsx" | "routes.js" | "routes.jsx"
        | "route.ts" | "route.tsx" | "route.js" | "route.jsx"
    )
}

const STRATEGIC_KEYWORDS: &[&str] = &[
    "auth", "security", "api", "db", "database", "route",
    "controller", "service", "util", "lib", "hook",
    "store", "repository", "model",
];

/// 4-Tier Strategic File Selection: manifests → entrypoints → domain services → core source.
pub fn select_strategic_files(file_paths: &[String], max_files: usize) -> Vec<String> {
    let clean: Vec<&String> = file_paths.iter().filter(|p| !is_noise_file(p)).collect();
    let mut selected: BTreeSet<String> = BTreeSet::new();

    // Tier 1: Manifests & Configs
    for path in &clean {
        if selected.len() >= max_files { break; }
        if is_manifest_config(path) {
            selected.insert((*path).clone());
        }
    }

    // Tier 2: Entrypoints & Routes
    for path in &clean {
        if selected.len() >= max_files { break; }
        if !selected.contains(*path) && is_entrypoint(path) {
            selected.insert((*path).clone());
        }
    }

    // Tier 3: Critical Domain Services (max 3 per keyword bucket)
    let mut keyword_counts: Vec<usize> = vec![0; STRATEGIC_KEYWORDS.len()];
    for path in &clean {
        if selected.len() >= max_files { break; }
        if selected.contains(*path) { continue; }
        let lower = path.to_ascii_lowercase();
        for (i, kw) in STRATEGIC_KEYWORDS.iter().enumerate() {
            if lower.contains(kw) && keyword_counts[i] < 3 {
                selected.insert((*path).clone());
                keyword_counts[i] += 1;
                break;
            }
        }
    }

    // Tier 4: Core Domain Source Files
    for path in &clean {
        if selected.len() >= max_files { break; }
        if !selected.contains(*path) {
            let lower = path.to_ascii_lowercase();
            if lower.starts_with("src/") || lower.starts_with("lib/") || lower.starts_with("app/") {
                selected.insert((*path).clone());
            }
        }
    }

    // Fallback: fill remaining
    for path in &clean {
        if selected.len() >= max_files { break; }
        if !selected.contains(*path) {
            selected.insert((*path).clone());
        }
    }

    selected.into_iter().collect()
}

/// Check whether a file path likely contains type/interface definitions.
pub fn is_type_definition_file(path: &str, code: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with("types.ts")
        || lower.ends_with("interfaces.ts")
        || lower.ends_with(".d.ts")
        || lower.contains("types/")
        || lower.contains("shared/types")
        || lower.contains("domain/")
        || lower.contains("storage")
        || lower.contains("repository")
    {
        return true;
    }

    let export_lines: Vec<&str> = code.lines()
        .filter(|l| l.trim_start().starts_with("export"))
        .collect();
    if export_lines.is_empty() {
        return false;
    }
    let type_exports = export_lines.iter().filter(|l| {
        l.contains("export type")
            || l.contains("export interface")
            || l.contains("export enum")
            || l.contains("export const enum")
            || l.contains("export class")
    }).count();
    type_exports as f64 / export_lines.len() as f64 > 0.4
}

/// Create a focused summary of generated code for cross-file context sharing.
/// Extracts imports, exports, and key type definitions rather than raw first lines.
pub fn create_file_summary(code: &str, file_path: Option<&str>) -> String {
    let is_contract = file_path.map(|p| is_type_definition_file(p, code)).unwrap_or(false);
    let max_lines = if is_contract { 45 } else { 20 };
    let mut important_lines: Vec<&str> = Vec::new();

    for line in code.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ")
            || trimmed.starts_with("export ")
            || trimmed.starts_with("interface ")
            || trimmed.starts_with("type ")
            || trimmed.starts_with("enum ")
            || trimmed.starts_with("class ")
            || trimmed.starts_with("function ")
            || trimmed.starts_with("public ")
            || trimmed.starts_with("async ")
            || trimmed.starts_with("/**")
            || trimmed.starts_with(" * ")
            || (trimmed.starts_with("const ")
                && trimmed.contains('=')
                && (trimmed.contains('(') || trimmed.contains("=>")))
        {
            important_lines.push(line);
        }
        if important_lines.len() >= max_lines {
            break;
        }
    }

    if important_lines.is_empty() {
        return code.lines().take(10).collect::<Vec<_>>().join("\n");
    }
    important_lines.join("\n")
}

/// Build a cross-file context string from previously generated file summaries.
pub fn build_cross_file_context(summaries: &[(String, String)]) -> String {
    if summaries.is_empty() {
        return String::new();
    }

    let total_files = summaries.len();
    let max_context_files = (MIN_CONTEXT_FILES)
        .max(((total_files as f64) * CONTEXT_FILE_RATIO).ceil() as usize)
        .min(MAX_CONTEXT_FILES);

    let mut type_entries: Vec<&(String, String)> = Vec::new();
    let mut impl_entries: Vec<&(String, String)> = Vec::new();

    for entry in summaries {
        if is_type_definition_file(&entry.0, &entry.1) {
            type_entries.push(entry);
        } else {
            impl_entries.push(entry);
        }
    }

    let max_type_files = (MIN_TYPE_FILES)
        .max(((type_entries.len() as f64) * TYPE_FILE_RATIO).ceil() as usize)
        .min(MAX_TYPE_FILES);

    let selected_types: Vec<&(String, String)> = if type_entries.len() > max_type_files {
        type_entries[type_entries.len() - max_type_files..].iter().copied().collect()
    } else {
        type_entries.clone()
    };

    let remaining_slots = max_context_files.saturating_sub(selected_types.len());
    let selected_impl: Vec<&(String, String)> = if impl_entries.len() > remaining_slots {
        impl_entries[impl_entries.len() - remaining_slots..].iter().copied().collect()
    } else {
        impl_entries.clone()
    };

    let total_selected = selected_types.len() + selected_impl.len();
    let mut sections: Vec<String> = vec![
        "Previously generated files in this project (ensure consistent imports, types, and exports):".into(),
    ];

    if !selected_types.is_empty() {
        sections.push("\n// Type definitions (always reference these):".into());
        for (path, summary) in &selected_types {
            sections.push(format!("\n### {}\n```\n{}\n```", path, summary));
        }
    }

    if !selected_impl.is_empty() {
        sections.push("\n// Recent implementation files:".into());
        for (path, summary) in &selected_impl {
            sections.push(format!("\n### {}\n```\n{}\n```", path, summary));
        }
    }

    let omitted = total_files - total_selected;
    if omitted > 0 {
        sections.push(format!("\n// Note: {} earlier file(s) omitted for brevity", omitted));
    }

    sections.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_noise_file() {
        assert!(is_noise_file("node_modules/react/index.js"));
        assert!(is_noise_file("dist/bundle.js"));
        assert!(is_noise_file("logo.png"));
        assert!(is_noise_file("package-lock.json"));
        assert!(!is_noise_file("src/App.tsx"));
    }

    #[test]
    fn test_select_strategic_files_priorities() {
        let files = vec![
            "package.json".into(),
            "src/App.tsx".into(),
            "src/main.tsx".into(),
            "src/components/Button.tsx".into(),
            "src/auth/login.ts".into(),
            "src/types.ts".into(),
        ];
        let selected = select_strategic_files(&files, 24);
        assert!(selected.contains(&"package.json".to_string()));
        assert!(selected.contains(&"src/main.tsx".to_string()));
        assert!(selected.contains(&"src/auth/login.ts".to_string()));
    }

    #[test]
    fn test_select_strategic_files_respects_max() {
        let files: Vec<String> = (0..50).map(|i| format!("src/file{}.tsx", i)).collect();
        let selected = select_strategic_files(&files, 10);
        assert_eq!(selected.len(), 10);
    }

    #[test]
    fn test_is_type_definition_file_by_path() {
        assert!(is_type_definition_file("src/types.ts", ""));
        assert!(is_type_definition_file("src/interfaces.ts", ""));
        assert!(is_type_definition_file("src/types/index.ts", ""));
        assert!(!is_type_definition_file("src/App.tsx", ""));
    }

    #[test]
    fn test_is_type_definition_file_by_content() {
        let code = "export type Foo = string;\nexport interface Bar {}\nexport const baz = 1;";
        assert!(is_type_definition_file("src/stuff.ts", code));
        let code2 = "export const a = 1;\nexport const b = 2;";
        assert!(!is_type_definition_file("src/stuff.ts", code2));
    }

    #[test]
    fn test_create_file_summary_extracts_imports() {
        let code = "import React from 'react';\nconst x = 1;\nexport default function App() { return null; }\n";
        let summary = create_file_summary(code, None);
        assert!(summary.contains("import React"));
        assert!(summary.contains("export default function App"));
        assert!(!summary.contains("const x = 1"));
    }

    #[test]
    fn test_create_file_summary_contract_max_lines() {
        let code: String = (0..50).map(|i| format!("export type T{} = number;\n", i)).collect();
        let summary = create_file_summary(&code, Some("src/types.ts"));
        assert!(summary.lines().count() <= 45);
    }

    #[test]
    fn test_build_cross_file_context_structure() {
        let summaries = vec![
            ("src/types.ts".into(), "export type Foo = string;".into()),
            ("src/App.tsx".into(), "export default function App() {}".into()),
        ];
        let context = build_cross_file_context(&summaries);
        assert!(context.contains("Type definitions"));
        assert!(context.contains("src/types.ts"));
        assert!(context.contains("Recent implementation files"));
        assert!(context.contains("src/App.tsx"));
    }

    #[test]
    fn test_build_cross_file_context_empty() {
        assert_eq!(build_cross_file_context(&[]), "");
    }
}
