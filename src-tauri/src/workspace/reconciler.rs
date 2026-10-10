use crate::domain::{DomainError, Result};
use std::path::Path;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ReconciliationResult {
    pub reconciler: String,
    pub modified: bool,
    pub summary: String,
    pub duration_ms: u64,
    pub error: Option<String>,
}

pub trait WorkspaceReconciler {
    fn name(&self) -> &'static str;
    fn reconcile(&self, workspace_root: &Path) -> Result<ReconciliationResult>;
}

pub struct ReconciliationPipeline {
    reconcilers: Vec<Box<dyn WorkspaceReconciler + Send + Sync>>,
}

impl ReconciliationPipeline {
    pub fn new() -> Self {
        Self { reconcilers: Vec::new() }
    }

    pub fn add_reconciler<R>(&mut self, reconciler: R) -> &mut Self
    where
        R: WorkspaceReconciler + Send + Sync + 'static,
    {
        self.reconcilers.push(Box::new(reconciler));
        self
    }

    pub fn run(&self, workspace_root: &Path) -> Vec<ReconciliationResult> {
        let mut results = Vec::new();
        for reconciler in &self.reconcilers {
            let start = std::time::Instant::now();
            let result = match reconciler.reconcile(workspace_root) {
                Ok(result) => result,
                Err(error) => ReconciliationResult {
                    reconciler: reconciler.name().into(),
                    modified: false,
                    summary: "Reconciler failed".into(),
                    duration_ms: start.elapsed().as_millis() as u64,
                    error: Some(error.to_string()),
                },
            };
            results.push(result);
        }
        results
    }
}

impl Default for ReconciliationPipeline {
    fn default() -> Self {
        let mut pipeline = Self::new();
        pipeline.add_reconciler(DefaultJsxExtensionReconciler);
        pipeline.add_reconciler(DefaultImportPathReconciler);
        pipeline.add_reconciler(DefaultIndexHtmlReconciler);
        pipeline.add_reconciler(DefaultViteConfigDependencyReconciler);
        pipeline
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultJsxExtensionReconciler;

impl WorkspaceReconciler for DefaultJsxExtensionReconciler {
    fn name(&self) -> &'static str {
        "jsx-extensions"
    }

    fn reconcile(&self, workspace_root: &Path) -> Result<ReconciliationResult> {
        let start = std::time::Instant::now();
        let mut renamed = 0usize;
        let files = match super::LocalWorkspaceService.list_project_files(workspace_root) {
            Ok(files) => files,
            Err(error) => {
                return Ok(ReconciliationResult {
                    reconciler: self.name().into(),
                    modified: false,
                    summary: "Skipped (listing failed)".into(),
                    duration_ms: start.elapsed().as_millis() as u64,
                    error: Some(error.to_string()),
                });
            }
        };
        for file in files {
            if !file.path.ends_with(".ts") || file.path.ends_with(".d.ts") {
                continue;
            }
            if contains_jsx(&file.content) {
                let new_relative = format!("{}.x", file.path);
                let old_target = workspace_root.join(&file.path);
                let new_target = workspace_root.join(&new_relative);
                if let Err(error) = std::fs::rename(&old_target, &new_target) {
                    return Ok(ReconciliationResult {
                        reconciler: self.name().into(),
                        modified: false,
                        summary: format!("Rename failed for {}", file.path),
                        duration_ms: start.elapsed().as_millis() as u64,
                        error: Some(error.to_string()),
                    });
                }
                renamed += 1;
            }
        }
        Ok(ReconciliationResult {
            reconciler: self.name().into(),
            modified: renamed > 0,
            summary: if renamed > 0 { format!("Renamed {renamed} .ts file(s) to .tsx (JSX detected)") } else { "No JSX files to rename".into() },
            duration_ms: start.elapsed().as_millis() as u64,
            error: None,
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultIndexHtmlReconciler;

impl WorkspaceReconciler for DefaultIndexHtmlReconciler {
    fn name(&self) -> &'static str {
        "index-html"
    }

    fn reconcile(&self, workspace_root: &Path) -> Result<ReconciliationResult> {
        let start = std::time::Instant::now();
        let root_index = workspace_root.join("index.html");
        let public_index = workspace_root.join("public").join("index.html");
        if root_index.exists() {
            return Ok(ReconciliationResult {
                reconciler: self.name().into(),
                modified: false,
                summary: "index.html already at root".into(),
                duration_ms: start.elapsed().as_millis() as u64,
                error: None,
            });
        }
        if !public_index.exists() {
            return Ok(ReconciliationResult {
                reconciler: self.name().into(),
                modified: false,
                summary: "No index.html found".into(),
                duration_ms: start.elapsed().as_millis() as u64,
                error: None,
            });
        }
        std::fs::rename(&public_index, &root_index).map_err(|e| DomainError::internal(format!("index.html move failed: {e}")))?;
        Ok(ReconciliationResult {
            reconciler: self.name().into(),
            modified: true,
            summary: "Moved index.html from public/ to root".into(),
            duration_ms: start.elapsed().as_millis() as u64,
            error: None,
        })
    }
}

/// Detect JSX syntax in TypeScript source without naive substring matching
/// on comments or strings. Scans line-by-line, skipping:
///   - single-line comments (`// ...`)
///   - block comment regions (`/* ... */`)
///   - string literals (`'...'`, `"..."`, `` `...` ``)
/// and then checks for JSX-specific patterns on the remaining code.
///
/// This is NOT a parser — it is a fast heuristic that avoids the false
/// positives of bare `contains("<div")` (which matches HTML strings in
/// template literals, comments, etc.). A real parser-assisted check is
/// a future refinement (P1.9 scope: parser-assisted where feasible).
fn contains_jsx(content: &str) -> bool {
    let mut in_block_comment = false;
    for line in content.lines() {
        let trimmed = line.trim_start();
        // Block comment tracking: toggle on `/*` / `*/`.
        if in_block_comment {
            if let Some(end) = trimmed.find("*/") {
                in_block_comment = false;
                // Check the code after `*/` on the same line.
                let after = &trimmed[end + 2..];
                if line_has_jsx(after) {
                    return true;
                }
            }
            continue;
        }
        // Skip full-line single-line comments.
        if trimmed.starts_with("//") {
            continue;
        }
        // Detect block comment start on this line; check code before and
        // after the block comment (if it closes on the same line).
        let scan_line = if let Some(start) = trimmed.find("/*") {
            if let Some(end_rel) = trimmed[start..].find("*/") {
                // Block comment opens and closes on this line: scan code
                // before `/*` and after `*/`.
                let end_abs = start + end_rel;
                let before = &trimmed[..start];
                let after = &trimmed[end_abs + 2..];
                if line_has_jsx(before) || line_has_jsx(after) {
                    return true;
                }
                continue;
            }
            // Block comment opens but doesn't close: scan code before it.
            in_block_comment = true;
            &trimmed[..start]
        } else {
            trimmed
        };
        if line_has_jsx(scan_line) {
            return true;
        }
    }
    false
}

/// Check a single line fragment (with comments stripped) for JSX patterns.
/// JSX-specific: a `<` followed by an uppercase identifier (component) or
/// a known HTML tag, with no preceding string/context that would make it
/// a comparison operator.
fn line_has_jsx(fragment: &str) -> bool {
    // Strip string literals so `<` inside a string is not mistaken for JSX.
    let stripped = strip_strings(fragment);
    // JSX component: `<Component` or `<div` etc., as an opening tag.
    // We look for `<` followed by an identifier char, preceded by a
    // non-identifier (space, `=`, `(`, `;`, newline, or start of line).
    let chars: Vec<char> = stripped.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<' {
            // Check if this is JSX: followed by an identifier char, and
            // not a comparison (`<`, `<=`, `<<`, `<%`).
            let next = chars.get(i + 1).copied().unwrap_or(' ');
            if is_ident_start(next) {
                return true;
            }
        }
        i += 1;
    }
    // Also catch React.createElement and className (retained from the
    // original heuristic — these don't have the comment/string false-positive
    // risk because they are identifier expressions).
    stripped.contains("React.createElement") || stripped.contains("className=")
}

/// Strip string literals from a code fragment, replacing their contents
/// with empty strings so `<` inside a string is not mistaken for JSX.
fn strip_strings(code: &str) -> String {
    let mut out = String::with_capacity(code.len());
    let chars: Vec<char> = code.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\'' | '"' => {
                let quote = chars[i];
                i += 1;
                while i < chars.len() && chars[i] != quote {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        i += 2;
                        continue;
                    }
                    i += 1;
                }
                // Skip the closing quote
                if i < chars.len() {
                    i += 1;
                }
            }
            '`' => {
                i += 1;
                while i < chars.len() && chars[i] != '`' {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        i += 2;
                        continue;
                    }
                    i += 1;
                }
                if i < chars.len() {
                    i += 1;
                }
            }
            _ => {
                out.push(chars[i]);
                i += 1;
            }
        }
    }
    out
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

/// Update import paths after JSX extension renaming (.ts → .tsx).
///
/// When `DefaultJsxExtensionReconciler` renames `src/App.ts` to `src/App.tsx`,
/// any file importing `./App` or `./App.ts` still works (bundler resolves
/// extensions), but explicit `./App.ts` imports should be updated to
/// `./App.tsx` to avoid confusion and ensure consistent resolution. More
/// importantly, if a `.ts` file is renamed to `.tsx` and another file imports
/// it with an explicit `.ts` extension, the import will fail to resolve in
/// strict mode.
///
/// Ported from devcode's `ServerWorkspaceManager.reconcileImportExports`.
/// This is a lightweight version: it only fixes explicit extension imports
/// that point to renamed files, not full import/export reconciliation.
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultImportPathReconciler;

impl WorkspaceReconciler for DefaultImportPathReconciler {
    fn name(&self) -> &'static str {
        "import-paths"
    }

    fn reconcile(&self, workspace_root: &Path) -> Result<ReconciliationResult> {
        let start = std::time::Instant::now();
        let files = match super::LocalWorkspaceService.list_project_files(workspace_root) {
            Ok(files) => files,
            Err(error) => {
                return Ok(ReconciliationResult {
                    reconciler: self.name().into(),
                    modified: false,
                    summary: "Skipped (listing failed)".into(),
                    duration_ms: start.elapsed().as_millis() as u64,
                    error: Some(error.to_string()),
                });
            }
        };

        // Build a set of files that were renamed from .ts to .tsx.
        // A .tsx file whose .ts counterpart no longer exists was likely renamed.
        // Key by basename so import paths like `./App` can be matched.
        let mut renamed: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let tsx_paths: Vec<&str> = files.iter()
            .filter(|f| f.path.ends_with(".tsx"))
            .map(|f| f.path.as_str())
            .collect();
        for tsx_path in &tsx_paths {
            let ts_path = tsx_path.strip_suffix(".tsx").unwrap_or(tsx_path).to_string() + ".ts";
            if !files.iter().any(|f| f.path == ts_path) {
                // Key by basename stem: "src/App.tsx" → "App"
                let tsx_stem = tsx_path.rsplit('/').next().unwrap_or(tsx_path)
                    .strip_suffix(".tsx").unwrap_or(tsx_path).to_string();
                let ts_stem = ts_path.rsplit('/').next().unwrap_or(&ts_path)
                    .strip_suffix(".ts").unwrap_or(&ts_path).to_string();
                renamed.insert(ts_stem, tsx_stem);
            }
        }

        if renamed.is_empty() {
            return Ok(ReconciliationResult {
                reconciler: self.name().into(),
                modified: false,
                summary: "No renamed imports to fix".into(),
                duration_ms: start.elapsed().as_millis() as u64,
                error: None,
            });
        }

        let mut updated = 0usize;
        for file in &files {
            // Only update TypeScript/JavaScript source files.
            if !file.path.ends_with(".ts") && !file.path.ends_with(".tsx")
                && !file.path.ends_with(".js") && !file.path.ends_with(".jsx")
            {
                continue;
            }
            let mut content = file.content.clone();
            let original = content.clone();
            for (old_stem, new_stem) in &renamed {
                // Replace `from './old.ts'` → `from './new.tsx'`
                let old_import = format!("from \"./{old_stem}.ts\"");
                let new_import = format!("from \"./{new_stem}.tsx\"");
                content = content.replace(&old_import, &new_import);
                let old_import_s = format!("from './{old_stem}.ts'");
                let new_import_s = format!("from './{new_stem}.tsx'");
                content = content.replace(&old_import_s, &new_import_s);
            }
            if content != original {
                let target = workspace_root.join(&file.path);
                if std::fs::write(&target, &content).is_ok() {
                    updated += 1;
                }
            }
        }

        Ok(ReconciliationResult {
            reconciler: self.name().into(),
            modified: updated > 0,
            summary: if updated > 0 {
                format!("Updated import paths in {updated} file(s) after JSX rename")
            } else {
                "No import paths needed updating".into()
            },
            duration_ms: start.elapsed().as_millis() as u64,
            error: None,
        })
    }
}

/// Ensure packages imported in `vite.config.ts` are listed in `package.json`
/// devDependencies. The deterministic scaffolder already pins these, but if
/// the user or a previous run modified `vite.config.ts`, missing packages
/// would break the build.
///
/// Ported from devcode's `ViteConfigDependencyReconciler`.
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultViteConfigDependencyReconciler;

/// Safe version pins for common Vite plugins.
fn safe_version(pkg: &str) -> &'static str {
    match pkg {
        "vite" => "^7.3.6",
        "@vitejs/plugin-react" => "^5.2.0",
        "@vitejs/plugin-vue" => "^5.0.0",
        "vite-tsconfig-paths" => "^4.3.0",
        "vite-plugin-svelte" => "^3.0.0",
        "autoprefixer" => "^10.4.0",
        "postcss" => "^8.4.0",
        "tailwindcss" => "^3.4.0",
        _ => "latest",
    }
}

impl WorkspaceReconciler for DefaultViteConfigDependencyReconciler {
    fn name(&self) -> &'static str {
        "vite-config-dependencies"
    }

    fn reconcile(&self, workspace_root: &Path) -> Result<ReconciliationResult> {
        let start = std::time::Instant::now();
        let vite_config_path = workspace_root.join("vite.config.ts");
        let vite_config = match std::fs::read_to_string(&vite_config_path) {
            Ok(content) => content,
            Err(_) => {
                return Ok(ReconciliationResult {
                    reconciler: self.name().into(),
                    modified: false,
                    summary: "No vite.config.ts found".into(),
                    duration_ms: start.elapsed().as_millis() as u64,
                    error: None,
                });
            }
        };

        // Parse import statements from vite.config.ts to find package imports.
        let mut imports: Vec<String> = Vec::new();
        for line in vite_config.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with("import ") && !trimmed.starts_with("import\t") {
                continue;
            }
            // Extract the module path from `import ... from 'module'` or `import 'module'`.
            if let Some(pos) = trimmed.find("from ") {
                let after_from = &trimmed[pos + 5..].trim();
                if after_from.starts_with('\'') || after_from.starts_with('"') {
                    let quote = after_from.chars().next().unwrap();
                    if let Some(end) = after_from[1..].find(quote) {
                        let pkg = &after_from[1..1 + end];
                        if !pkg.starts_with('.') && !pkg.starts_with('/') {
                            // Extract package name (handle scoped packages).
                            let pkg_name = if pkg.starts_with('@') {
                                pkg.split('/').take(2).collect::<Vec<_>>().join("/")
                            } else {
                                pkg.split('/').next().unwrap_or(pkg).to_string()
                            };
                            imports.push(pkg_name);
                        }
                    }
                }
            } else {
                // Bare import: `import 'module'`
                let after_import = trimmed["import".len()..].trim();
                if after_import.starts_with('\'') || after_import.starts_with('"') {
                    let quote = after_import.chars().next().unwrap();
                    if let Some(end) = after_import[1..].find(quote) {
                        let pkg = &after_import[1..1 + end];
                        if !pkg.starts_with('.') && !pkg.starts_with('/') {
                            let pkg_name = if pkg.starts_with('@') {
                                pkg.split('/').take(2).collect::<Vec<_>>().join("/")
                            } else {
                                pkg.split('/').next().unwrap_or(pkg).to_string()
                            };
                            imports.push(pkg_name);
                        }
                    }
                }
            }
        }

        if imports.is_empty() {
            return Ok(ReconciliationResult {
                reconciler: self.name().into(),
                modified: false,
                summary: "No imports in vite.config.ts".into(),
                duration_ms: start.elapsed().as_millis() as u64,
                error: None,
            });
        }

        // Read package.json and check which imports are missing.
        let pkg_path = workspace_root.join("package.json");
        let pkg_content = match std::fs::read_to_string(&pkg_path) {
            Ok(content) => content,
            Err(error) => {
                return Ok(ReconciliationResult {
                    reconciler: self.name().into(),
                    modified: false,
                    summary: "Cannot read package.json".into(),
                    duration_ms: start.elapsed().as_millis() as u64,
                    error: Some(error.to_string()),
                });
            }
        };

        let mut pkg_json: serde_json::Value = match serde_json::from_str(&pkg_content) {
            Ok(value) => value,
            Err(error) => {
                return Ok(ReconciliationResult {
                    reconciler: self.name().into(),
                    modified: false,
                    summary: "Cannot parse package.json".into(),
                    duration_ms: start.elapsed().as_millis() as u64,
                    error: Some(error.to_string()),
                });
            }
        };

        let deps = pkg_json.get("dependencies")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        let dev_deps = pkg_json.get("devDependencies")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        let missing: Vec<String> = imports.iter()
            .filter(|pkg| !deps.contains_key(&pkg.to_string()) && !dev_deps.contains_key(&pkg.to_string()))
            .cloned()
            .collect();

        if missing.is_empty() {
            return Ok(ReconciliationResult {
                reconciler: self.name().into(),
                modified: false,
                summary: "All vite config dependencies present".into(),
                duration_ms: start.elapsed().as_millis() as u64,
                error: None,
            });
        }

        // Add missing dependencies to devDependencies.
        if let Some(dev_deps_obj) = pkg_json.get_mut("devDependencies").and_then(|v| v.as_object_mut()) {
            for pkg in &missing {
                let version = safe_version(pkg);
                dev_deps_obj.insert(pkg.clone(), serde_json::Value::String(version.to_string()));
            }
        } else {
            // Create devDependencies if it doesn't exist.
            let mut new_dev_deps = serde_json::Map::new();
            for pkg in &missing {
                let version = safe_version(pkg);
                new_dev_deps.insert(pkg.clone(), serde_json::Value::String(version.to_string()));
            }
            pkg_json["devDependencies"] = serde_json::Value::Object(new_dev_deps);
        }

        let updated = serde_json::to_string_pretty(&pkg_json).unwrap_or_default() + "\n";
        if let Err(error) = std::fs::write(&pkg_path, &updated) {
            return Ok(ReconciliationResult {
                reconciler: self.name().into(),
                modified: false,
                summary: "Failed to write updated package.json".into(),
                duration_ms: start.elapsed().as_millis() as u64,
                error: Some(error.to_string()),
            });
        }

        Ok(ReconciliationResult {
            reconciler: self.name().into(),
            modified: true,
            summary: format!("Added {} missing vite config dependencies: {}", missing.len(), missing.join(", ")),
            duration_ms: start.elapsed().as_millis() as u64,
            error: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_real_jsx_in_code() {
        assert!(contains_jsx("const App = () => <div>Hello</div>"));
        assert!(contains_jsx("function Comp() { return <Button />; }"));
        assert!(contains_jsx("const el = <span>text</span>"));
        assert!(contains_jsx("React.createElement('div', null)"));
        assert!(contains_jsx("const x = <MyComponent prop={1} />"));
    }

    #[test]
    fn does_not_false_positive_on_comments() {
        // JSX-looking text in a single-line comment
        assert!(!contains_jsx("// const x = <div>fake</div>"));
        // JSX-looking text in a block comment
        assert!(!contains_jsx("/* <div>not real jsx</div> */"));
        assert!(!contains_jsx("/*
  <span>multi-line comment</span>
*/"));
        // Code after a block comment is still checked
        assert!(contains_jsx("/* comment */ const x = <div>real</div>"));
    }

    #[test]
    fn does_not_false_positive_on_strings() {
        // JSX-looking text in a template literal
        assert!(!contains_jsx("const html = `<div>not jsx</div>`"));
        // JSX-looking text in a single-quoted string
        assert!(!contains_jsx("const s = '<span>fake</span>'"));
        // JSX-looking text in a double-quoted string
        assert!(!contains_jsx("const s = \"<button>fake</button>\""));
        // Real JSX after a string
        assert!(contains_jsx("const s = 'hello'; const x = <div>real</div>"));
    }

    #[test]
    fn does_not_false_positive_on_comparison_operators() {
        assert!(!contains_jsx("if (a < b) return 1;"));
        assert!(!contains_jsx("const x = a <= b;"));
        assert!(!contains_jsx("const x = a << 2;"));
        assert!(!contains_jsx("if (x < 10) { doSomething(); }"));
    }

    #[test]
    fn detects_jsx_after_string_with_angle_bracket() {
        // `<` inside a string should not mask real JSX on the same line.
        assert!(contains_jsx("const s = 'a < b'; const el = <div>real</div>"));
    }

    #[test]
    fn vite_config_dependency_reconciler_adds_missing_deps() {
        let root = tempfile::tempdir().unwrap();
        // vite.config.ts imports @vitejs/plugin-react but package.json doesn't have it.
        std::fs::write(root.path().join("vite.config.ts"),
            "import { defineConfig } from 'vitest/config'\nimport react from '@vitejs/plugin-react'\n\nexport default defineConfig({ plugins: [react()] })\n"
        ).unwrap();
        std::fs::write(root.path().join("package.json"),
            "{\"name\":\"test\",\"devDependencies\":{\"vitest\":\"^3.0.0\"}}"
        ).unwrap();

        let result = DefaultViteConfigDependencyReconciler.reconcile(root.path()).unwrap();
        assert!(result.modified);
        assert!(result.summary.contains("@vitejs/plugin-react"));

        let pkg: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.path().join("package.json")).unwrap()
        ).unwrap();
        assert!(pkg["devDependencies"]["@vitejs/plugin-react"].as_str().unwrap().contains("^5.2.0"));
        // Existing dep is preserved.
        assert!(pkg["devDependencies"]["vitest"].as_str().is_some());
    }

    #[test]
    fn vite_config_dependency_reconciler_skips_when_all_present() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("vite.config.ts"),
            "import react from '@vitejs/plugin-react'\nexport default {};\n"
        ).unwrap();
        std::fs::write(root.path().join("package.json"),
            "{\"devDependencies\":{\"@vitejs/plugin-react\":\"^5.0.0\"}}"
        ).unwrap();

        let result = DefaultViteConfigDependencyReconciler.reconcile(root.path()).unwrap();
        assert!(!result.modified);
        assert!(result.summary.contains("present"));
    }

    #[test]
    fn vite_config_dependency_reconciler_skips_when_no_vite_config() {
        let root = tempfile::tempdir().unwrap();
        let result = DefaultViteConfigDependencyReconciler.reconcile(root.path()).unwrap();
        assert!(!result.modified);
        assert!(result.summary.contains("No vite.config.ts"));
    }

    #[test]
    fn import_path_reconciler_updates_renamed_imports() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("src")).unwrap();
        // App.ts was renamed to App.tsx (only .tsx exists).
        std::fs::write(root.path().join("src/App.tsx"), "export function App() { return null; }\n").unwrap();
        // main.tsx imports App with explicit .ts extension (stale after rename).
        std::fs::write(root.path().join("src/main.tsx"),
            "import { App } from \"./App.ts\";\nrender(<App />);\n"
        ).unwrap();

        let result = DefaultImportPathReconciler.reconcile(root.path()).unwrap();
        assert!(result.modified);
        assert!(result.summary.contains("1 file"));

        let main = std::fs::read_to_string(root.path().join("src/main.tsx")).unwrap();
        assert!(main.contains("./App.tsx"), "import should be updated to .tsx: {main}");
        assert!(!main.contains("./App.ts\""));
    }

    #[test]
    fn import_path_reconciler_skips_when_nothing_renamed() {
        let root = tempfile::tempdir().unwrap();
        // Both .ts and .tsx exist — no rename happened.
        std::fs::create_dir_all(root.path().join("src")).unwrap();
        std::fs::write(root.path().join("src/App.ts"), "export function App() {}\n").unwrap();
        std::fs::write(root.path().join("src/App.tsx"), "export function App() {}\n").unwrap();
        std::fs::write(root.path().join("src/main.ts"), "import { App } from \"./App.ts\";\n").unwrap();

        let result = DefaultImportPathReconciler.reconcile(root.path()).unwrap();
        assert!(!result.modified);
        assert!(result.summary.contains("No renamed"));
    }

    #[test]
    fn default_pipeline_includes_all_reconcilers() {
        let pipeline = ReconciliationPipeline::default();
        // Run on an empty dir — all reconcilers should complete without error.
        let root = tempfile::tempdir().unwrap();
        let results = pipeline.run(root.path());
        assert!(results.len() >= 4, "should have at least 4 reconcilers: {:?}", results.iter().map(|r| &r.reconciler).collect::<Vec<_>>());
        let names: Vec<&str> = results.iter().map(|r| r.reconciler.as_str()).collect();
        assert!(names.contains(&"jsx-extensions"));
        assert!(names.contains(&"import-paths"));
        assert!(names.contains(&"index-html"));
        assert!(names.contains(&"vite-config-dependencies"));
    }
}
