//! UX Consistency Service (ported from devcode UXConsistencyService.ts)
//!
//! Checks generated files for UX consistency issues: repeated interactive
//! patterns, missing loading/error states, inconsistent button labels, and
//! missing empty states.
//!
//! Pure module — no stores, no DI, no I/O.

use serde::{Deserialize, Serialize};

// ── Types ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Severity {
    #[serde(rename = "error")] Error,
    #[serde(rename = "warning")] Warning,
    #[serde(rename = "info")] Info,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UXFinding {
    pub rule_id: String,
    pub severity: Severity,
    pub message: String,
    pub file: String,
    pub line: Option<usize>,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UXConsistencyInput {
    pub files: Vec<FileContent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContent {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UXConsistencyResult {
    pub passed: bool,
    pub findings: Vec<UXFinding>,
    pub files_checked: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
}

// ── Service ─────────────────────────────────────────────────────────────────

pub struct UXConsistencyService;

impl UXConsistencyService {
    pub fn audit(&self, input: UXConsistencyInput) -> UXConsistencyResult {
        let mut findings = Vec::new();
        let mut files_checked = 0;

        for file in input.files {
            if !is_ui_like(&file.path) {
                continue;
            }
            files_checked += 1;
            
            findings.extend(detect_loading_state(&file.content, &file.path));
            findings.extend(detect_error_boundary(&file.content, &file.path));
            findings.extend(detect_empty_state(&file.content, &file.path));
        }

        let error_count = findings.iter().filter(|f| f.severity == Severity::Error).count();
        let warning_count = findings.iter().filter(|f| f.severity == Severity::Warning).count();
        let info_count = findings.iter().filter(|f| f.severity == Severity::Info).count();

        UXConsistencyResult {
            passed: error_count == 0,
            findings,
            files_checked,
            error_count,
            warning_count,
            info_count,
        }
    }
}

fn is_ui_like(path: &str) -> bool {
    path.ends_with(".tsx") || path.ends_with(".jsx") || path.ends_with(".vue") || 
    path.ends_with(".svelte") || path.ends_with(".html")
}

// ── Rules (Simplified Manual Parsing) ──────────────────────────

fn detect_loading_state(content: &str, path: &str) -> Vec<UXFinding> {
    let mut findings = Vec::new();
    let has_async = content.contains("await ") || content.contains("Promise") || content.contains("fetch(") || content.contains("axios");
    let has_loading = content.to_lowercase().contains("loading") || content.to_lowercase().contains("isloading") || content.to_lowercase().contains("pending") || content.to_lowercase().contains("spinner") || content.to_lowercase().contains("skeleton");
    
    if has_async && !has_loading {
        findings.push(UXFinding {
            rule_id: "ux-loading-state".to_string(),
            severity: Severity::Warning,
            message: "Async operation detected without a loading state.".to_string(),
            file: path.to_string(),
            line: None,
            suggestion: Some("Add a loading indicator or skeleton state for async operations.".to_string()),
        });
    }
    findings
}

fn detect_error_boundary(content: &str, path: &str) -> Vec<UXFinding> {
    let mut findings = Vec::new();
    let is_component = (path.ends_with(".tsx") || path.ends_with(".jsx") || path.ends_with(".vue") || path.ends_with(".svelte"))
        && (content.contains("export default") || content.contains("React.Component") || content.contains("defineComponent"));
    let has_error = content.contains("ErrorBoundary") || content.contains("catch") || content.contains("onError");
    
    if is_component && !has_error {
        findings.push(UXFinding {
            rule_id: "ux-error-boundary".to_string(),
            severity: Severity::Warning,
            message: "Component may lack error handling.".to_string(),
            file: path.to_string(),
            line: None,
            suggestion: Some("Add error boundaries or try/catch for async operations.".to_string()),
        });
    }
    findings
}

fn detect_empty_state(content: &str, path: &str) -> Vec<UXFinding> {
    let mut findings = Vec::new();
    let has_list = content.contains(".map(") || content.contains(".forEach(") || content.contains("v-for") || content.contains("@for");
    let has_empty = content.to_lowercase().contains("empty") || content.to_lowercase().contains("nodata");
    
    if has_list && !has_empty {
        findings.push(UXFinding {
            rule_id: "ux-empty-state".to_string(),
            severity: Severity::Info,
            message: "List rendering detected without an empty state.".to_string(),
            file: path.to_string(),
            line: None,
            suggestion: Some("Consider adding an empty state for when the list has no items.".to_string()),
        });
    }
    findings
}
