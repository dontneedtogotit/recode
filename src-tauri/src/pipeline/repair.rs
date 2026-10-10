//! Adaptive Repair Service (ported from devcode AdaptiveRepairService.ts)
//!
//! Suggests and applies repairs for common build failures based on learned patterns.
//! Pure module — no stores, no DI, no I/O.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RepairSuggestion {
    pub rule_id: String,
    pub description: String,
    pub confidence: f32,
    pub files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompilerDiagnostic {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveRepairInput {
    pub failed_checks: Vec<String>,
    pub generated_files: Vec<FileContentSnapshot>,
    #[serde(default)]
    pub diagnostics: Vec<CompilerDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compiler_output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContentSnapshot {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdaptiveRepairResult {
    pub suggestions: Vec<RepairSuggestion>,
    pub applied: Vec<RepairSuggestion>,
    pub skipped: Vec<RepairSuggestion>,
}

pub struct AdaptiveRepairService;

impl AdaptiveRepairService {
    pub fn suggest(&self, input: &AdaptiveRepairInput) -> AdaptiveRepairResult {
        let mut suggestions = Vec::new();
        let mut applied = Vec::new();
        let mut skipped = Vec::new();

        // 1. missing-import
        let has_compile_failure = input.failed_checks.iter().any(|c| c == "compile");
        let has_diagnostic_import = input.diagnostics.iter().any(|d| {
            let code_str = d.code.as_deref().unwrap_or("");
            code_str == "2307" || code_str == "2304" ||
                contains_any(&d.message, &["Cannot find module", "Module not found", "TS2307", "TS2304"])
        });
        let has_output_import = input.compiler_output.as_ref().is_some_and(|o| {
            contains_any(o, &["Cannot find module", "Module not found", "TS2307", "TS2304"])
        });
        let has_content_import = input.generated_files.iter().any(|f| {
            contains_any(&f.content, &["Cannot find module", "Module not found", "TS2307"])
        });

        if has_compile_failure && (has_diagnostic_import || has_output_import || has_content_import) {
            let mut files: Vec<String> = input.diagnostics.iter()
                .filter(|d| {
                    let code_str = d.code.as_deref().unwrap_or("");
                    code_str == "2307" || code_str == "2304" ||
                        contains_any(&d.message, &["Cannot find module", "Module not found", "TS2307", "TS2304"])
                })
                .filter_map(|d| d.file.clone())
                .collect();

            for f in &input.generated_files {
                if contains_any(&f.content, &["Cannot find module", "Module not found", "TS2307"]) {
                    if !files.contains(&f.path) {
                        files.push(f.path.clone());
                    }
                }
            }

            let target_files = if files.is_empty() {
                input.generated_files.iter().map(|f| f.path.clone()).collect()
            } else {
                files
            };

            let suggestion = RepairSuggestion {
                rule_id: "missing-import".into(),
                description: "Add missing import statements for unresolved modules.".into(),
                confidence: 0.8,
                files: target_files.clone(),
                patch: Some(format!("# Review import paths in: {}", target_files.join(", "))),
            };
            suggestions.push(suggestion.clone());
            applied.push(suggestion);
        }

        // 2. missing-env-config
        let has_prod_failure = input.failed_checks.iter().any(|c| c == "production");
        let has_env_content = input.generated_files.iter().any(|f| {
            f.content.contains("process.env") || f.content.contains("import.meta.env")
        });
        if has_prod_failure && has_env_content {
            let files: Vec<String> = input.generated_files.iter()
                .filter(|f| f.content.contains("process.env") || f.content.contains("import.meta.env"))
                .map(|f| f.path.clone())
                .collect();
            let suggestion = RepairSuggestion {
                rule_id: "missing-env-config".into(),
                description: "Add environment variable declarations or .env.example.".into(),
                confidence: 0.7,
                files: files.clone(),
                patch: Some(format!("# Add env config for: {}", files.join(", "))),
            };
            suggestions.push(suggestion.clone());
            skipped.push(suggestion);
        }

        // 3. missing-error-boundary
        let has_browser_failure = input.failed_checks.iter().any(|c| c == "browser");
        let has_error_content = input.generated_files.iter().any(|f| {
            f.content.contains("console.error") || f.content.contains("Uncaught")
        });
        if has_browser_failure && has_error_content {
            let files: Vec<String> = input.generated_files.iter()
                .filter(|f| f.path.ends_with(".tsx") || f.path.ends_with(".jsx") || f.path.ends_with(".vue") || f.path.ends_with(".svelte"))
                .map(|f| f.path.clone())
                .collect();
            let suggestion = RepairSuggestion {
                rule_id: "missing-error-boundary".into(),
                description: "Add error boundaries or global error handlers.".into(),
                confidence: 0.6,
                files: files.clone(),
                patch: Some(format!("# Wrap UI roots in error boundaries: {}", files.join(", "))),
            };
            suggestions.push(suggestion.clone());
            skipped.push(suggestion);
        }

        AdaptiveRepairResult { suggestions, applied, skipped }
    }

    pub fn apply_repair(
        &self,
        suggestion: &RepairSuggestion,
        files: &[FileContentSnapshot],
    ) -> Vec<FileContentSnapshot> {
        let _patch = suggestion.patch.as_ref();
        let patch_comment = format!("/* REPAIR: {} */\n", suggestion.description);
        files.iter().map(|file| {
            if suggestion.files.contains(&file.path) && !file.content.contains(patch_comment.trim()) {
                FileContentSnapshot {
                    path: file.path.clone(),
                    content: patch_comment.clone() + &file.content,
                }
            } else {
                file.clone()
            }
        }).collect()
    }
}

fn contains_any(text: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|&kw| text.contains(kw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adaptive_repair_missing_import() {
        let service = AdaptiveRepairService;
        let input = AdaptiveRepairInput {
            failed_checks: vec!["compile".into()],
            generated_files: vec![
                FileContentSnapshot {
                    path: "src/App.tsx".into(),
                    content: "import React from 'react';\nTS2307 Cannot find module './missing'".into(),
                },
            ],
            diagnostics: vec![
                CompilerDiagnostic {
                    file: Some("src/App.tsx".into()),
                    line: Some(1),
                    column: Some(1),
                    code: Some("2307".into()),
                    message: "Cannot find module './missing'".into(),
                },
            ],
            compiler_output: None,
        };
        let result = service.suggest(&input);
        assert_eq!(result.applied.len(), 1);
        assert_eq!(result.applied[0].rule_id, "missing-import");

        let patched = service.apply_repair(&result.applied[0], &input.generated_files);
        assert!(patched[0].content.contains("REPAIR:"));
    }
}
