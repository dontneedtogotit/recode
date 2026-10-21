//! Build Stub Generator (ported from devcode BuildStubGenerator.ts)
//!
//! Pure helpers for generating minimal stub files for missing imports and
//! adding missing exports to existing files. These prevent trivial
//! missing-import compile failures before the project ships to preview.
//! Pure module — no stores, no DI, no I/O.

#![allow(dead_code)]  // ROADMAP parity port: kept for planned wiring; prune when the Q-item closes.

use serde::{Deserialize, Serialize};

const DATA_FILE_EXTENSIONS: &[&str] = &[
    ".json", ".jsonc", ".json5", ".yaml", ".yml", ".toml",
    ".lock", ".md", ".txt", ".xml", ".csv",
];

const ASSET_EXTENSIONS: &[&str] = &[
    ".svg", ".png", ".jpg", ".jpeg", ".gif", ".webp",
    ".woff", ".woff2", ".ttf", ".eot",
];

const STYLE_EXTENSIONS: &[&str] = &[
    ".css", ".scss", ".less", ".sass", ".styl",
];

/// True when the path is a data/manifest file rather than a code module.
pub fn is_data_file_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    DATA_FILE_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
}

/// Normalize a path string: resolve `..` and `.` segments.
pub fn normalize_path(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    let mut normalized: Vec<&str> = Vec::new();
    for part in parts {
        if part == ".." {
            normalized.pop();
        } else if part != "." && !part.is_empty() {
            normalized.push(part);
        }
    }
    let result = normalized.join("/");
    if path.starts_with('/') && !result.starts_with('/') {
        format!("/{}", result)
    } else {
        result
    }
}

/// Resolve a relative import source to a concrete target file path.
/// Mirrors the extension-probing logic in devcode's SymbolRegistry.
pub fn resolve_target_path(import_source: &str, from_dir: &str) -> String {
    let normalized = if import_source.starts_with("@/") {
        normalize_path(&format!("src/{}", &import_source[2..]))
    } else {
        let root_relative = import_source.starts_with('/');
        let base = if root_relative { "" } else { from_dir };
        let source = if root_relative { &import_source[1..] } else { import_source };
        let joined = if base.is_empty() { source.to_string() } else { format!("{}/{}", base, source) };
        normalize_path(&joined)
    };

    let lower = normalized.to_ascii_lowercase();
    if STYLE_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
        || ASSET_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
    {
        return normalized;
    }

    if is_data_file_path(&normalized) {
        return normalized;
    }

    // Strip known code extensions
    let stripped = strip_code_extension(&normalized);

    // PascalCase final segment → component file (.tsx)
    let last_segment = stripped.rsplit('/').next().unwrap_or("");
    let is_pascal_case = last_segment.len() > 0
        && last_segment.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false)
        && last_segment.chars().all(|c| c.is_ascii_alphanumeric());

    if is_pascal_case {
        format!("{}.tsx", stripped)
    } else {
        format!("{}/index.ts", stripped)
    }
}

fn strip_code_extension(path: &str) -> String {
    for ext in &[".ts", ".tsx", ".js", ".jsx"] {
        if path.ends_with(ext) {
            return path[..path.len() - ext.len()].to_string();
        }
    }
    path.to_string()
}

/// Extract a display name from a file path for component stubs.
/// e.g. "src/components/Header.tsx" → "Header"
///      "src/stores/index.ts" → "Stores"
pub fn extract_component_name(file_path: &str) -> String {
    let stripped = strip_code_extension(file_path);
    let segments: Vec<&str> = stripped.split('/').collect();
    let mut name = segments.last().copied().unwrap_or("Module");
    if name == "index" && segments.len() > 1 {
        name = segments[segments.len() - 2];
    }
    let identifier: String = name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect();
    if identifier.is_empty() || identifier.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        format!("Module{}", identifier)
    } else {
        identifier
    }
}

/// Generate a minimal, runtime-safe placeholder export for a missing symbol.
pub fn generate_placeholder_export(symbol: &str) -> String {
    if symbol.ends_with("Error") {
        return format!(
            "export class {} extends Error {{\n  constructor(message?: string) {{\n    super(message);\n    this.name = '{}';\n  }}\n}}",
            symbol, symbol
        );
    }
    if symbol.starts_with("use")
        && symbol.len() > 3
        && symbol.chars().nth(3).map(|c| c.is_ascii_uppercase()).unwrap_or(false)
    {
        return format!(
            "export function {}(..._args: unknown[]): unknown {{\n  return undefined;\n}}",
            symbol
        );
    }
    if symbol.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false) {
        return format!("export class {} {{}}", symbol);
    }
    format!("export const {}: unknown = undefined;", symbol)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StubSymbol {
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedStub {
    pub path: String,
    pub content: String,
    pub language: String,
}

/// Build a minimal stub file for a missing import target.
pub fn build_stub(target_path: &str, symbols: &[StubSymbol]) -> GeneratedStub {
    let lower = target_path.to_ascii_lowercase();
    if STYLE_EXTENSIONS.iter().any(|ext| lower.ends_with(ext)) {
        return GeneratedStub {
            path: target_path.into(),
            content: "/* Auto-generated stub — replace with actual styles */\n".into(),
            language: "css".into(),
        };
    }
    if ASSET_EXTENSIONS.iter().any(|ext| lower.ends_with(ext)) {
        return GeneratedStub {
            path: target_path.into(),
            content: String::new(),
            language: "binary".into(),
        };
    }

    let is_tsx = target_path.ends_with(".tsx");
    let is_component = is_tsx
        && symbols.iter().any(|s| {
            s.is_default || s.name.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false)
        });

    let mut header_lines: Vec<String> = vec![
        "/**".into(),
        " * Auto-generated stub for missing file.".into(),
        format!(" * Imports from: expected at \"{}\"", target_path),
        " * Replace with actual implementation.".into(),
        " */".into(),
        String::new(),
    ];

    let mut export_lines: Vec<String> = Vec::new();

    if is_component {
        header_lines.push("import React from 'react';".into());
        header_lines.push(String::new());
        for sym in symbols {
            let display_name = if sym.is_default {
                extract_component_name(target_path)
            } else {
                sym.name.clone()
            };
            if sym.is_default || sym.name.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false) {
                export_lines.push(format!("export {}function {}() {{", if sym.is_default { "default " } else { "" }, display_name));
                export_lines.push(format!("  return <div>{} (stub)</div>;", display_name));
                export_lines.push("}".into());
                export_lines.push(String::new());
            } else {
                export_lines.push(format!("export function {}(): unknown {{", sym.name));
                export_lines.push("  return undefined;".into());
                export_lines.push("}".into());
                export_lines.push(String::new());
            }
        }
    } else {
        for sym in symbols {
            if sym.is_default {
                let name = extract_component_name(target_path);
                export_lines.push(format!("const {} = {{}};", name));
                export_lines.push(format!("export default {};", name));
                export_lines.push(String::new());
            } else if sym.name == "*" {
                // Wildcard re-export → skip
            } else if sym.name.starts_with("use")
                && sym.name.len() > 3
                && sym.name.chars().nth(3).map(|c| c.is_ascii_uppercase()).unwrap_or(false)
            {
                export_lines.push(format!("export function {}(): unknown {{", sym.name));
                export_lines.push("  return undefined;".into());
                export_lines.push("}".into());
                export_lines.push(String::new());
            } else if sym.name.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false) {
                export_lines.push(format!("export class {} {{}}", sym.name));
                export_lines.push(String::new());
            } else {
                export_lines.push(format!("export const {}: unknown = undefined;", sym.name));
                export_lines.push(String::new());
            }
        }
    }

    let content = header_lines.join("\n") + &export_lines.join("\n");
    GeneratedStub {
        path: target_path.into(),
        content,
        language: if is_tsx { "tsx".into() } else { "typescript".into() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_data_file_path() {
        assert!(is_data_file_path("package.json"));
        assert!(is_data_file_path("config.yaml"));
        assert!(is_data_file_path("README.md"));
        assert!(!is_data_file_path("App.tsx"));
    }

    #[test]
    fn test_normalize_path() {
        assert_eq!(normalize_path("src/../lib/utils"), "lib/utils");
        assert_eq!(normalize_path("src/./utils"), "src/utils");
        assert_eq!(normalize_path("/src/main"), "/src/main");
    }

    #[test]
    fn test_resolve_target_path_relative() {
        let result = resolve_target_path("./types", "src/components");
        assert_eq!(result, "src/components/types/index.ts");
    }

    #[test]
    fn test_resolve_target_path_pascal_case() {
        let result = resolve_target_path("./Button", "src/components");
        assert_eq!(result, "src/components/Button.tsx");
    }

    #[test]
    fn test_resolve_target_path_at_alias() {
        let result = resolve_target_path("@/types", "src/components");
        assert_eq!(result, "src/types/index.ts");
    }

    #[test]
    fn test_resolve_target_path_css() {
        let result = resolve_target_path("./styles.css", "src/components");
        assert_eq!(result, "src/components/styles.css");
    }

    #[test]
    fn test_extract_component_name() {
        assert_eq!(extract_component_name("src/components/Header.tsx"), "Header");
        assert_eq!(extract_component_name("src/stores/index.ts"), "Stores");
        assert_eq!(extract_component_name("my-widget.tsx"), "MyWidget");
        assert_eq!(extract_component_name("404.page.tsx"), "Module404Page");
    }

    #[test]
    fn test_generate_placeholder_export_error() {
        let result = generate_placeholder_export("AudioError");
        assert!(result.contains("class AudioError extends Error"));
    }

    #[test]
    fn test_generate_placeholder_export_hook() {
        let result = generate_placeholder_export("useAuth");
        assert!(result.contains("function useAuth"));
    }

    #[test]
    fn test_generate_placeholder_export_class() {
        let result = generate_placeholder_export("MyClass");
        assert!(result.contains("class MyClass"));
    }

    #[test]
    fn test_generate_placeholder_export_const() {
        let result = generate_placeholder_export("myConstant");
        assert!(result.contains("const myConstant"));
    }

    #[test]
    fn test_build_stub_css() {
        let stub = build_stub("src/styles.css", &[]);
        assert_eq!(stub.language, "css");
        assert!(stub.content.contains("stub"));
    }

    #[test]
    fn test_build_stub_component() {
        let symbols = vec![StubSymbol { name: "Header".into(), is_default: false }];
        let stub = build_stub("src/components/Header.tsx", &symbols);
        assert_eq!(stub.language, "tsx");
        assert!(stub.content.contains("import React"));
        assert!(stub.content.contains("function Header()"));
    }

    #[test]
    fn test_build_stub_barrel() {
        let symbols = vec![StubSymbol { name: "default".into(), is_default: true }];
        let stub = build_stub("src/utils/helpers.ts", &symbols);
        assert_eq!(stub.language, "typescript");
        assert!(stub.content.contains("export default"));
    }
}
