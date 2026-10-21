//! Generation Error Handler (ported from devcode GenerationErrorHandler.ts)
//!
//! Classifies failure error messages and detects common failure patterns across generated files.
//! Pure module — no stores, no DI, no I/O.

#![allow(dead_code)]  // ROADMAP parity port: kept for planned wiring; prune when the Q-item closes.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FailedFileSummary {
    pub path: String,
    pub error: String,
    pub failure_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FailurePattern {
    pub pattern_type: String,
    pub count: usize,
    pub affected_paths: Vec<String>,
}

/// Classify a failure error message into a structured failure reason.
pub fn classify_failure_reason(error: &str) -> String {
    let lower = error.to_lowercase();
    if lower.contains("truncat")
        || lower.contains("max_tokens")
        || lower.contains("length")
        || lower.contains("unclosed")
    {
        return "truncated".into();
    }
    if lower.contains("timeout") || lower.contains("timed out") {
        return "timeout".into();
    }
    if lower.contains("no files") || lower.contains("empty") || lower.contains("no content") {
        return "empty_response".into();
    }
    if lower.contains("parse")
        || lower.contains("invalid json")
        || lower.contains("unexpected token")
    {
        return "parse_error".into();
    }
    "ai_error".into()
}

/// Detect repeating failure patterns across multiple files.
pub fn detect_failure_pattern(failures: &[FailedFileSummary]) -> Option<FailurePattern> {
    if failures.len() < 2 {
        return None;
    }

    let mut by_reason: std::collections::BTreeMap<String, (usize, Vec<String>)> = std::collections::BTreeMap::new();
    for f in failures {
        let entry = by_reason.entry(f.failure_reason.clone()).or_insert((0, Vec::new()));
        entry.0 += 1;
        entry.1.push(f.path.clone());
    }

    for (reason, (count, paths)) in by_reason {
        if count >= 2 {
            return Some(FailurePattern {
                pattern_type: reason,
                count,
                affected_paths: paths,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_failure_reason() {
        assert_eq!(classify_failure_reason("Error: output length exceeded max_tokens"), "truncated");
        assert_eq!(classify_failure_reason("Request timed out after 30s"), "timeout");
        assert_eq!(classify_failure_reason("Invalid JSON at line 1"), "parse_error");
        assert_eq!(classify_failure_reason("Unknown provider error"), "ai_error");
    }

    #[test]
    fn test_detect_failure_pattern() {
        let failures = vec![
            FailedFileSummary { path: "a.ts".into(), error: "JSON parse failed".into(), failure_reason: "parse_error".into() },
            FailedFileSummary { path: "b.ts".into(), error: "JSON syntax error".into(), failure_reason: "parse_error".into() },
        ];
        let pattern = detect_failure_pattern(&failures).unwrap();
        assert_eq!(pattern.pattern_type, "parse_error");
        assert_eq!(pattern.count, 2);
        assert_eq!(pattern.affected_paths, vec!["a.ts".to_string(), "b.ts".to_string()]);
    }
}
