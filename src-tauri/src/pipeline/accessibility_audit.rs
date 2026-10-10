//! Accessibility Audit Service (ported from devcode AccessibilityAuditService.ts)
//!
//! Analyzes generated files for accessibility issues and produces
//! actionable findings. 
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
pub struct AccessibilityFinding {
    pub rule_id: String,
    pub severity: Severity,
    pub message: String,
    pub file: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilityAuditInput {
    pub files: Vec<FileContent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContent {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccessibilityAuditResult {
    pub passed: bool,
    pub findings: Vec<AccessibilityFinding>,
    pub files_checked: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
}

// ── Service ─────────────────────────────────────────────────────────────────

pub struct AccessibilityAuditService;

impl AccessibilityAuditService {
    pub fn audit(&self, input: AccessibilityAuditInput) -> AccessibilityAuditResult {
        let mut findings = Vec::new();
        let mut files_checked = 0;

        for file in input.files {
            if !is_html_like(&file.path) {
                continue;
            }
            files_checked += 1;
            
            // Simple manual detection to avoid `regex` crate
            findings.extend(detect_img_alt(&file.content, &file.path));
            findings.extend(detect_btn_text(&file.content, &file.path));
        }

        let error_count = findings.iter().filter(|f| f.severity == Severity::Error).count();
        let warning_count = findings.iter().filter(|f| f.severity == Severity::Warning).count();
        let info_count = findings.iter().filter(|f| f.severity == Severity::Info).count();

        AccessibilityAuditResult {
            passed: error_count == 0,
            findings,
            files_checked,
            error_count,
            warning_count,
            info_count,
        }
    }
}

fn is_html_like(path: &str) -> bool {
    path.ends_with(".html") || path.ends_with(".jsx") || path.ends_with(".tsx") || 
    path.ends_with(".vue") || path.ends_with(".svelte")
}

// ── Simple Rule Implementations (Manual Parsing) ──────────────────────────

fn detect_img_alt(content: &str, path: &str) -> Vec<AccessibilityFinding> {
    let mut findings = Vec::new();
    let content_lower = content.to_lowercase();
    
    // Find all <img tags
    let mut pos = 0;
    while let Some(start) = content_lower[pos..].find("<img") {
        let img_start = pos + start;
        if let Some(end) = content_lower[img_start..].find('>') {
            let img_tag = &content_lower[img_start..img_start + end + 1];
            
            // Check for alt=
            if !img_tag.contains("alt=") {
                findings.push(AccessibilityFinding {
                    rule_id: "a11y-img-alt".to_string(),
                    severity: Severity::Error,
                    message: "Image missing alt attribute.".to_string(),
                    file: path.to_string(),
                    line: None,
                    column: None,
                    suggestion: Some("Add a descriptive alt attribute or alt=\"\" for decorative images.".to_string()),
                });
            }
            pos = img_start + end + 1;
        } else {
            break;
        }
    }
    findings
}

fn detect_btn_text(content: &str, path: &str) -> Vec<AccessibilityFinding> {
    let mut findings = Vec::new();
    let content_lower = content.to_lowercase();
    
    let mut pos = 0;
    while let Some(start) = content_lower[pos..].find("<button") {
        let btn_start = pos + start;
        if let Some(end) = content_lower[btn_start..].find("</button>") {
            let btn_tag = &content_lower[btn_start..btn_start + end + 9]; // include </button>
            let tag_content = &content[btn_start..btn_start + end];
            
            // Simple check: does it have content and no aria-label?
            let has_text = !tag_content.contains(">") || !tag_content.split('>').last().unwrap_or("").trim().is_empty();
            let has_aria = tag_content.contains("aria-label=");
            
            if !has_text && !has_aria {
                findings.push(AccessibilityFinding {
                    rule_id: "a11y-btn-text".to_string(),
                    severity: Severity::Error,
                    message: "Button has no accessible name.".to_string(),
                    file: path.to_string(),
                    line: None,
                    column: None,
                    suggestion: Some("Add visible text or an aria-label to the button.".to_string()),
                });
            }
            pos = btn_start + end + 9;
        } else {
            break;
        }
    }
    findings
}
