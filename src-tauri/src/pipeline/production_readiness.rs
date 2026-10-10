//! Production Readiness Service (ported from devcode ProductionReadinessService.ts)
//!
//! Evaluates generated projects for production readiness: environment config,
//! error handling, logging, security headers, and deployment basics.
//!
//! Pure module — no stores, no DI, no I/O. Checks are heuristic-based (regex/
//! substring matching), not a parser — they catch the absence of common
//! production patterns, not subtle misconfigurations.

use serde::{Deserialize, Serialize};

// ── Types ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductionFinding {
    pub rule_id: String,
    pub severity: String,
    pub message: String,
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductionReadinessResult {
    pub passed: bool,
    pub findings: Vec<ProductionFinding>,
    pub files_checked: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
}

/// Input file for readiness evaluation.
pub struct ProductionFile {
    pub path: String,
    pub content: String,
}

// ── Service ───────────────────────────────────────────────────────────────────

pub struct ProductionReadinessService;

impl ProductionReadinessService {
    /// Evaluate generated files for production readiness.
    pub fn evaluate(&self, files: &[ProductionFile]) -> ProductionReadinessResult {
        let mut findings = Vec::new();
        let files_checked = files.len();

        // Rule: prod-env-config
        findings.extend(detect_env_config(files));
        // Rule: prod-error-handling
        findings.extend(detect_error_handling(files));
        // Rule: prod-logging
        findings.extend(detect_logging(files));
        // Rule: prod-security-headers
        findings.extend(detect_security_headers(files));
        // Rule: prod-build-config
        findings.extend(detect_build_config(files));

        let error_count = findings.iter().filter(|f| f.severity == "error").count();
        let warning_count = findings.iter().filter(|f| f.severity == "warning").count();
        let info_count = findings.iter().filter(|f| f.severity == "info").count();

        ProductionReadinessResult {
            passed: error_count == 0,
            findings,
            files_checked,
            error_count,
            warning_count,
            info_count,
        }
    }
}

// ── Rules ──────────────────────────────────────────────────────────────────────

fn is_source_file(path: &str) -> bool {
    path.ends_with(".ts") || path.ends_with(".tsx") || path.ends_with(".js") || path.ends_with(".jsx")
}

fn is_source_or_json(path: &str) -> bool {
    is_source_file(path) || path.ends_with(".json")
}

fn first_source_path<'a>(files: &'a [ProductionFile]) -> &'a str {
    files
        .iter()
        .find(|f| is_source_file(&f.path))
        .map(|f| f.path.as_str())
        .or_else(|| files.first().map(|f| f.path.as_str()))
        .unwrap_or("project")
}

fn detect_env_config(files: &[ProductionFile]) -> Vec<ProductionFinding> {
    let mut results = Vec::new();
    let source_files: Vec<&ProductionFile> = files.iter().filter(|f| is_source_or_json(&f.path)).collect();
    if source_files.is_empty() {
        return results;
    }
    let has_env = source_files.iter().any(|f| {
        f.path.contains(".env")
            || f.content.contains("process.env.")
            || f.content.contains("import.meta.env.")
    });
    let has_env_example = files.iter().any(|f| f.path.contains(".env.example"));
    if !has_env {
        let first = source_files[0];
        results.push(ProductionFinding {
            rule_id: "prod-env-config".into(),
            severity: "error".into(),
            message: "No environment configuration detected.".into(),
            file: first.path.clone(),
            line: None,
            suggestion: Some("Add environment variable handling for secrets and config.".into()),
        });
    }
    if !has_env_example {
        let first = &source_files[0];
        results.push(ProductionFinding {
            rule_id: "prod-env-config".into(),
            severity: "warning".into(),
            message: "Missing .env.example or equivalent.".into(),
            file: first.path.clone(),
            line: None,
            suggestion: Some("Add an example env file to document required variables.".into()),
        });
    }
    results
}

fn detect_error_handling(files: &[ProductionFile]) -> Vec<ProductionFinding> {
    let mut results = Vec::new();
    let source_files: Vec<&ProductionFile> = files.iter().filter(|f| is_source_file(&f.path)).collect();
    if source_files.is_empty() {
        return results;
    }
    let has_global_error = source_files.iter().any(|f| {
        f.content.contains("ErrorBoundary")
            || f.content.contains("window.addEventListener(\"error\"")
            || f.content.contains("window.addEventListener('error'")
    }) || files.iter().any(|f| {
        f.path.to_lowercase().contains("error-handling")
            || f.path.to_lowercase().contains("errorhandler")
    });
    if !has_global_error {
        let first = &source_files[0];
        results.push(ProductionFinding {
            rule_id: "prod-error-handling".into(),
            severity: "error".into(),
            message: "No global error handling detected.".into(),
            file: first.path.clone(),
            line: None,
            suggestion: Some("Add global error handling or an error boundary.".into()),
        });
    }
    results
}

fn detect_logging(files: &[ProductionFile]) -> Vec<ProductionFinding> {
    let mut results = Vec::new();
    let source_files: Vec<&ProductionFile> = files.iter().filter(|f| is_source_file(&f.path)).collect();
    if source_files.is_empty() {
        return results;
    }
    let has_logging = source_files.iter().any(|f| {
        f.content.contains("Logger")
            || f.content.contains("console.log")
            || f.content.contains("console.warn")
            || f.content.contains("console.error")
            || f.content.contains("logging")
            || f.content.contains("winston")
            || f.content.contains("pino")
    });
    if !has_logging {
        let first = &source_files[0];
        results.push(ProductionFinding {
            rule_id: "prod-logging".into(),
            severity: "warning".into(),
            message: "No logging detected in source files.".into(),
            file: first.path.clone(),
            line: None,
            suggestion: Some("Add structured logging for production observability.".into()),
        });
    }
    results
}

fn detect_security_headers(files: &[ProductionFile]) -> Vec<ProductionFinding> {
    let mut results = Vec::new();
    let server_files: Vec<&ProductionFile> = files.iter().filter(|f| {
        let p = f.path.to_lowercase();
        p.contains("server") || p.contains("middleware") || p.contains("config")
            || p.contains("vite.config") || p.contains("webpack")
    }).collect();
    if server_files.is_empty() {
        return results;
    }
    let has_security = server_files.iter().any(|f| {
        let c = f.content.to_lowercase();
        c.contains("helmet") || c.contains("content-security-policy")
            || c.contains("x-frame-options") || c.contains("cors") || c.contains("security")
    });
    if !has_security {
        let first = &server_files[0];
        results.push(ProductionFinding {
            rule_id: "prod-security-headers".into(),
            severity: "warning".into(),
            message: "No security headers detected.".into(),
            file: first.path.clone(),
            line: None,
            suggestion: Some("Add security headers (CSP, X-Frame-Options, etc.).".into()),
        });
    }
    results
}

fn detect_build_config(files: &[ProductionFile]) -> Vec<ProductionFinding> {
    let mut results = Vec::new();
    let has_build_config = files.iter().any(|f| {
        let p = f.path.to_lowercase();
        p.contains("vite.config") || p.contains("webpack.config")
            || p.contains("rollup.config") || p.contains("tsconfig.json")
            || p.contains("package.json")
    });
    if !has_build_config {
        let file = files.first().map(|f| f.path.as_str()).unwrap_or("project");
        results.push(ProductionFinding {
            rule_id: "prod-build-config".into(),
            severity: "info".into(),
            message: "No build configuration detected.".into(),
            file: file.into(),
            line: None,
            suggestion: Some("Add a build configuration for production bundling.".into()),
        });
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, content: &str) -> ProductionFile {
        ProductionFile { path: path.into(), content: content.into() }
    }

    #[test]
    fn passes_when_all_production_patterns_present() {
        let files = vec![
            file("src/App.tsx", "import { ErrorBoundary } from './ErrorBoundary'; console.log('hi');"),
            file(".env.example", "API_KEY=your-key-here"),
            file("src/config.ts", "const apiKey = process.env.API_KEY;"),
            file("vite.config.ts", "export default { server: { headers: { 'X-Frame-Options': 'DENY' } } }"),
        ];
        let result = ProductionReadinessService.evaluate(&files);
        assert!(result.passed, "findings: {:?}", result.findings);
        assert_eq!(result.error_count, 0);
    }

    #[test]
    fn detects_missing_env_config() {
        let files = vec![file("src/App.tsx", "export function App() { return null; }")];
        let result = ProductionReadinessService.evaluate(&files);
        assert!(!result.passed);
        assert!(result.findings.iter().any(|f| f.rule_id == "prod-env-config" && f.severity == "error"));
    }

    #[test]
    fn detects_missing_error_handling() {
        let files = vec![file("src/App.tsx", "export function App() { return null; }")];
        let result = ProductionReadinessService.evaluate(&files);
        assert!(!result.passed);
        assert!(result.findings.iter().any(|f| f.rule_id == "prod-error-handling" && f.severity == "error"));
    }

    #[test]
    fn detects_missing_logging() {
        let files = vec![file("src/App.tsx", "export function App() { return null; }")];
        let result = ProductionReadinessService.evaluate(&files);
        // Missing logging is a warning, not an error — the build still passes.
        assert!(result.passed);
        assert!(result.warning_count > 0);
        assert!(result.findings.iter().any(|f| f.rule_id == "prod-logging"));
    }

    #[test]
    fn detects_missing_security_headers() {
        let files = vec![file("vite.config.ts", "export default { plugins: [] };")];
        let result = ProductionReadinessService.evaluate(&files);
        assert!(result.findings.iter().any(|f| f.rule_id == "prod-security-headers"));
    }

    #[test]
    fn detects_missing_build_config() {
        let files = vec![file("src/App.tsx", "export function App() {}")];
        let result = ProductionReadinessService.evaluate(&files);
        assert!(result.findings.iter().any(|f| f.rule_id == "prod-build-config" && f.severity == "info"));
    }

    #[test]
    fn empty_files_returns_no_errors() {
        let result = ProductionReadinessService.evaluate(&[]);
        assert!(result.findings.is_empty());
        assert_eq!(result.files_checked, 0);
    }

    #[test]
    fn env_example_satisfies_env_warning() {
        let files = vec![
            file("src/App.tsx", "export function App() { return null; }"),
            file(".env.example", "API_KEY=example"),
        ];
        let result = ProductionReadinessService.evaluate(&files);
        // env.example present satisfies the warning; error handling is still missing.
        let env_findings: Vec<_> = result.findings.iter().filter(|f| f.rule_id == "prod-env-config").collect();
        assert!(env_findings.iter().all(|f| f.severity != "warning"), "should not have env-config warning");
    }
}
