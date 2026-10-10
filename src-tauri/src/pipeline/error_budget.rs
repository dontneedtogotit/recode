//! Generation Error Handler (ported from devcode GenerationErrorHandler.ts)
//! and Auxiliary File Budget (from AuxiliaryFileBudget.ts).
//!
//! Pure failure classification and budget enforcement for the build pipeline.
//! No stores, no DI, no I/O.

use serde::{Deserialize, Serialize};

/// Structured reason for a failed file generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureReason {
    Truncated,
    Timeout,
    EmptyResponse,
    ParseError,
    AiError,
}

/// Classify a failure error message into a structured failure reason.
pub fn classify_failure_reason(error: &str) -> FailureReason {
    let lower = error.to_lowercase();
    if lower.contains("truncat")
        || lower.contains("max_tokens")
        || lower.contains("length")
        || lower.contains("unclosed")
    {
        return FailureReason::Truncated;
    }
    if lower.contains("timeout") || lower.contains("timed out") {
        return FailureReason::Timeout;
    }
    if lower.contains("no files") || lower.contains("empty") || lower.contains("no content") {
        return FailureReason::EmptyResponse;
    }
    if lower.contains("parse") || lower.contains("invalid json") || lower.contains("unexpected token") {
        return FailureReason::ParseError;
    }
    FailureReason::AiError
}

/// A failure pattern: the same reason recurring across multiple files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailurePattern {
    #[serde(rename = "type")]
    pub pattern_type: String,
    pub count: usize,
    pub affected_paths: Vec<String>,
}

/// Detect a failure pattern: ≥2 files failing with the same reason.
/// Returns the first such reason in first-seen order.
pub fn detect_failure_pattern(failures: &[FailureReason]) -> Option<FailurePattern> {
    if failures.len() < 2 {
        return None;
    }
    let mut order: Vec<String> = Vec::new();
    let mut counts: Vec<(String, usize)> = Vec::new();
    for reason in failures {
        let key = match reason {
            FailureReason::Truncated => "truncated",
            FailureReason::Timeout => "timeout",
            FailureReason::EmptyResponse => "empty_response",
            FailureReason::ParseError => "parse_error",
            FailureReason::AiError => "ai_error",
        }
        .to_string();
        if let Some(entry) = counts.iter_mut().find(|(k, _)| *k == key) {
            entry.1 += 1;
        } else {
            counts.push((key.clone(), 1));
            order.push(key);
        }
    }
    for key in order {
        if let Some((_, count)) = counts.iter().find(|(k, _)| *k == key) {
            let count = *count;
            if count >= 2 {
                let affected: Vec<String> = failures
                    .iter()
                    .filter(|r| {
                        matches!(
                            (r, key.as_str()),
                            (FailureReason::Truncated, "truncated")
                                | (FailureReason::Timeout, "timeout")
                                | (FailureReason::EmptyResponse, "empty_response")
                                | (FailureReason::ParseError, "parse_error")
                                | (FailureReason::AiError, "ai_error")
                        )
                    })
                    .map(|_| String::new())
                    .collect();
                return Some(FailurePattern {
                    pattern_type: key,
                    count,
                    // Paths are tracked by the caller; this function only sees reasons.
                    affected_paths: affected,
                });
            }
        }
    }
    None
}

/// Minimum auxiliary file slack even for tiny plans.
const MIN_AUXILIARY_FILES: usize = 3;
/// Auxiliary slack ratio over the planned file count.
const AUXILIARY_FILE_RATIO: f64 = 0.25;

/// Files that are part of the deterministic scaffold/manifest: never dropped
/// by the auxiliary budget, because dropping them breaks the project entirely.
fn is_required_manifest(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let filename = lower.rsplit('/').next().unwrap_or(&lower);
    let is_manifest_name = filename == "package.json"
        || filename == "package-lock.json"
        || (filename.starts_with("tsconfig") && filename.ends_with(".json"))
        || filename.starts_with("vite.config.")
        || filename.starts_with("vitest.config.")
        || filename.starts_with("eslint")
        || filename.starts_with(".eslintrc")
        || filename.starts_with("postcss.config.")
        || filename.starts_with("tailwind.config.")
        || filename.starts_with("prettier.config.")
        || filename == "index.html"
        || filename == ".gitignore"
        || filename == "readme.md";
    is_manifest_name || lower.contains("public/")
}

/// Compute the auxiliary budget and identify excess files to drop.
/// Returns the list of paths exceeding the budget (to be dropped by the caller).
/// Planned files and required manifest files are never dropped.
pub fn enforce_auxiliary_file_budget(
    generated_paths: &[String],
    planned_files: &[String],
) -> Vec<String> {
    let planned_count = planned_files.len();
    if planned_count == 0 {
        return Vec::new();
    }
    let auxiliary_budget = (MIN_AUXILIARY_FILES).max(
        ((planned_count as f64) * AUXILIARY_FILE_RATIO).ceil() as usize,
    );
    let planned: std::collections::HashSet<&String> = planned_files.iter().collect();
    let auxiliary: Vec<&String> = generated_paths
        .iter()
        .filter(|p| !planned.contains(p) && !is_required_manifest(p))
        .collect();
    let excess_start = auxiliary_budget;
    auxiliary
        .iter()
        .skip(excess_start)
        .map(|p| (*p).clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_truncated() {
        assert_eq!(classify_failure_reason("Response was truncated mid-token"), FailureReason::Truncated);
        assert_eq!(classify_failure_reason("max_tokens limit hit"), FailureReason::Truncated);
        assert_eq!(classify_failure_reason("Unclosed JSX tag"), FailureReason::Truncated);
    }

    #[test]
    fn test_classify_timeout() {
        assert_eq!(classify_failure_reason("Request timeout after 30s"), FailureReason::Timeout);
        assert_eq!(classify_failure_reason("provider timed out"), FailureReason::Timeout);
    }

    #[test]
    fn test_classify_empty() {
        assert_eq!(classify_failure_reason("provider returned empty content"), FailureReason::EmptyResponse);
        assert_eq!(classify_failure_reason("no files produced"), FailureReason::EmptyResponse);
    }

    #[test]
    fn test_classify_parse() {
        assert_eq!(classify_failure_reason("failed to parse JSON response"), FailureReason::ParseError);
        assert_eq!(classify_failure_reason("unexpected token in JSON"), FailureReason::ParseError);
    }

    #[test]
    fn test_classify_ai_error_default() {
        assert_eq!(classify_failure_reason("something went wrong"), FailureReason::AiError);
    }

    #[test]
    fn test_detect_failure_pattern_two_same() {
        let pattern = detect_failure_pattern(&[
            FailureReason::Truncated,
            FailureReason::Truncated,
        ]).unwrap();
        assert_eq!(pattern.pattern_type, "truncated");
        assert_eq!(pattern.count, 2);
    }

    #[test]
    fn test_detect_failure_pattern_none() {
        assert!(detect_failure_pattern(&[FailureReason::Truncated]).is_none());
        assert!(detect_failure_pattern(&[
            FailureReason::Truncated,
            FailureReason::Timeout,
        ]).is_none());
    }

    #[test]
    fn test_enforce_auxiliary_file_budget_zero_plan() {
        let generated = vec!["src/extra.ts".to_string()];
        assert!(enforce_auxiliary_file_budget(&generated, &[]).is_empty());
    }

    #[test]
    fn test_enforce_auxiliary_file_budget_under() {
        let planned = vec!["src/App.tsx".to_string(), "src/main.tsx".to_string()];
        let generated = vec![
            "src/App.tsx".to_string(),
            "src/main.tsx".to_string(),
            "src/__tests__/App.test.tsx".to_string(),
        ];
        assert!(enforce_auxiliary_file_budget(&generated, &planned).is_empty());
    }

    #[test]
    fn test_enforce_auxiliary_file_budget_excess_dropped() {
        let planned = vec!["src/App.tsx".to_string()];
        let generated = vec![
            "src/App.tsx".to_string(),
            "src/a.test.tsx".to_string(),
            "src/b.test.tsx".to_string(),
            "src/c.test.tsx".to_string(),
            "src/d.test.tsx".to_string(),
        ];
        let excess = enforce_auxiliary_file_budget(&generated, &planned);
        assert_eq!(excess, vec!["src/d.test.tsx".to_string()]);
    }

    #[test]
    fn test_enforce_auxiliary_file_budget_manifest_never_dropped() {
        let planned = vec!["src/App.tsx".to_string()];
        let generated = vec![
            "src/App.tsx".to_string(),
            "package.json".to_string(),
            "index.html".to_string(),
            "src/a.test.tsx".to_string(),
            "src/b.test.tsx".to_string(),
            "src/c.test.tsx".to_string(),
            "src/d.test.tsx".to_string(),
        ];
        let excess = enforce_auxiliary_file_budget(&generated, &planned);
        assert!(excess.iter().all(|p| *p != "package.json" && *p != "index.html"));
    }

    #[test]
    fn test_enforce_auxiliary_file_budget_public_dir_protected() {
        let planned = vec!["src/App.tsx".to_string()];
        let generated = vec![
            "public/logo.svg".to_string(),
            "src/a.test.tsx".to_string(),
            "src/b.test.tsx".to_string(),
            "src/c.test.tsx".to_string(),
            "src/d.test.tsx".to_string(),
        ];
        let excess = enforce_auxiliary_file_budget(&generated, &planned);
        assert!(!excess.contains(&"public/logo.svg".to_string()));
    }
}
