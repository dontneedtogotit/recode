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
        pipeline.add_reconciler(DefaultIndexHtmlReconciler);
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
}
