//! File Path Validator (ported from devcode filePathValidator.ts)
//!
//! Pure validation logic for generated file paths. Catches unsafe paths
//! (traversal, absolute, control chars, forbidden OS chars) and common
//! AI-generated path mistakes (markdown fence leaks, double slashes, trailing
//! slashes, data files used as directories). Returns sanitized paths and
//! structured issues; no I/O, no logging side effects.
//!
//! Recode surfaces returned issues directly instead of logging.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PathIssueCode {
    EmptyPath,
    NullBytes,
    PathTraversal,
    AbsolutePath,
    ControlChars,
    ForbiddenChars,
    BackslashNormalized,
    LeadingSlashStripped,
    TrailingSlash,
    DuplicateSlashes,
    WhitespaceTrimmed,
    MarkdownFenceLeak,
    DataFileAsDirectory,
    TooLong,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathIssueSeverity {
    Error,
    Warning,
    AutoFixed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathIssue {
    pub code: PathIssueCode,
    pub message: String,
    pub severity: PathIssueSeverity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathValidationResult {
    pub valid: bool,
    pub sanitized_path: String,
    pub issues: Vec<PathIssue>,
    pub was_fixed: bool,
}

const MAX_PATH_LENGTH: usize = 512;

#[derive(Debug, Clone, Default)]
pub struct PathValidationOptions {
    pub auto_fix: bool,
}

impl PathValidationOptions {
    pub fn auto_fix() -> Self { Self { auto_fix: true } }
    pub fn no_fix() -> Self { Self { auto_fix: false } }
}

fn push_issue(issues: &mut Vec<PathIssue>, code: PathIssueCode, message: impl Into<String>, severity: PathIssueSeverity, original: Option<&str>) {
    issues.push(PathIssue {
        code,
        message: message.into(),
        severity,
        original: original.map(|s| s.to_string()),
    });
}

fn has_error(issues: &[PathIssue]) -> bool {
    issues.iter().any(|i| i.severity == PathIssueSeverity::Error)
}

fn was_fixed(issues: &[PathIssue]) -> bool {
    issues.iter().any(|i| i.severity == PathIssueSeverity::AutoFixed)
}

fn has_forbidden_chars(path: &str) -> bool {
    path.chars().any(|c| matches!(c, '<' | '>' | '|' | '?' | '*'))
}

fn has_control_chars(path: &str) -> bool {
    path.chars().any(|c| {
        matches!(c as u32,
            0x00..=0x08 | 0x0B | 0x0C | 0x0E..=0x1F | 0x7F
        )
    })
}

fn strip_control_chars(path: &str) -> String {
    path.chars()
        .filter(|c| !matches!(*c as u32, 0x00..=0x08 | 0x0B | 0x0C | 0x0E..=0x1F | 0x7F))
        .collect()
}

fn is_leaf_file_segment(segment: &str) -> bool {
    let lower = segment.to_ascii_lowercase();
    lower.ends_with(".json")
        || lower.ends_with(".jsonc")
        || lower.ends_with(".json5")
        || lower.ends_with(".yaml")
        || lower.ends_with(".yml")
        || lower.ends_with(".toml")
        || lower.ends_with(".lock")
        || lower.ends_with(".md")
        || lower.ends_with(".txt")
        || lower.ends_with(".xml")
        || lower.ends_with(".csv")
}

/// Validate and optionally auto-fix a generated file path.
pub fn validate_file_path(raw_path: &str, options: PathValidationOptions) -> PathValidationResult {
    let mut issues: Vec<PathIssue> = Vec::new();
    let path = raw_path.to_string();

    if raw_path.trim().is_empty() {
        push_issue(
            &mut issues,
            PathIssueCode::EmptyPath,
            "File path is empty or whitespace-only",
            PathIssueSeverity::Error,
            None,
        );
        return PathValidationResult {
            valid: false,
            sanitized_path: String::new(),
            issues,
            was_fixed: false,
        };
    }

    let mut path = path;

    // Markdown fence leak
    if path.starts_with("```") || path.contains("```") {
        let cleaned: String = path.lines()
            .filter(|line| !line.trim().starts_with("```"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
        if !cleaned.is_empty() && options.auto_fix {
            push_issue(
                &mut issues,
                PathIssueCode::MarkdownFenceLeak,
                "Path contained markdown code fence markers",
                PathIssueSeverity::AutoFixed,
                Some(&path),
            );
            path = cleaned;
        } else {
            push_issue(
                &mut issues,
                PathIssueCode::MarkdownFenceLeak,
                "Path contained markdown code fence markers",
                PathIssueSeverity::Error,
                Some(&path),
            );
        }
    }

    // Null bytes
    if path.contains('\0') {
        if options.auto_fix {
            push_issue(
                &mut issues,
                PathIssueCode::NullBytes,
                "Path contained null bytes",
                PathIssueSeverity::AutoFixed,
                Some(&path),
            );
            path = path.replace('\0', "");
        } else {
            push_issue(
                &mut issues,
                PathIssueCode::NullBytes,
                "Path contains null bytes",
                PathIssueSeverity::Error,
                None,
            );
            return early_result(path, issues);
        }
    }

    // Control characters
    if has_control_chars(&path) {
        if options.auto_fix {
            push_issue(
                &mut issues,
                PathIssueCode::ControlChars,
                "Path contained control characters",
                PathIssueSeverity::AutoFixed,
                Some(&path),
            );
            path = strip_control_chars(&path);
        } else {
            push_issue(
                &mut issues,
                PathIssueCode::ControlChars,
                "Path contains control characters",
                PathIssueSeverity::Error,
                None,
            );
            return early_result(path, issues);
        }
    }

    // Whitespace trim
    let trimmed = path.trim();
    if trimmed != path {
        if options.auto_fix {
            push_issue(
                &mut issues,
                PathIssueCode::WhitespaceTrimmed,
                "Path had leading/trailing whitespace",
                PathIssueSeverity::AutoFixed,
                Some(&path),
            );
            path = trimmed.to_string();
        } else {
            push_issue(
                &mut issues,
                PathIssueCode::WhitespaceTrimmed,
                "Path has leading/trailing whitespace",
                PathIssueSeverity::Warning,
                None,
            );
        }
    }

    // Path traversal
    let normalized = path.replace('\\', "/");
    let segments_slashy: Vec<&str> = normalized.split('/').collect();
    if segments_slashy.iter().any(|s| *s == "..") {
        push_issue(
            &mut issues,
            PathIssueCode::PathTraversal,
            "Path contains \"..\" traversal sequence",
            PathIssueSeverity::Error,
            None,
        );
        return early_result(path, issues);
    }

    // Comment prefix
    if path.starts_with("// ") {
        if options.auto_fix {
            push_issue(
                &mut issues,
                PathIssueCode::AbsolutePath,
                "Path had comment prefix (// ), stripped to relative",
                PathIssueSeverity::AutoFixed,
                Some(&path),
            );
            path = path.trim_start_matches("// ").trim_start_matches('/').to_string();
        } else {
            push_issue(
                &mut issues,
                PathIssueCode::AbsolutePath,
                "Path appears to be a comment-prefixed path",
                PathIssueSeverity::Warning,
                None,
            );
        }
    }

    // Absolute path
    let is_windows_abs = path.len() >= 3 && path.as_bytes()[1] == b':' && path.as_bytes()[2] == b'\\';
    if path.starts_with('/') || is_windows_abs {
        if options.auto_fix {
            push_issue(
                &mut issues,
                PathIssueCode::AbsolutePath,
                "Path was absolute, stripped to relative",
                PathIssueSeverity::AutoFixed,
                Some(&path),
            );
            if is_windows_abs {
                path = path.split_off(3);
            }
            path = path.trim_start_matches('/').to_string();
        } else {
            push_issue(
                &mut issues,
                PathIssueCode::AbsolutePath,
                "Path is absolute",
                PathIssueSeverity::Error,
                None,
            );
            return early_result(path, issues);
        }
    }

    // Backslash normalization
    if path.contains('\\') {
        if options.auto_fix {
            push_issue(
                &mut issues,
                PathIssueCode::BackslashNormalized,
                "Path contained backslashes, normalized to forward slashes",
                PathIssueSeverity::AutoFixed,
                Some(&path),
            );
            path = path.replace('\\', "/");
        } else {
            push_issue(
                &mut issues,
                PathIssueCode::BackslashNormalized,
                "Path contains backslashes",
                PathIssueSeverity::Warning,
                None,
            );
        }
    }

    // Leading slashes
    if path.starts_with('/') {
        if options.auto_fix {
            push_issue(
                &mut issues,
                PathIssueCode::LeadingSlashStripped,
                "Leading slash stripped for relative path",
                PathIssueSeverity::AutoFixed,
                Some(&path),
            );
            path = path.trim_start_matches('/').to_string();
        } else {
            push_issue(
                &mut issues,
                PathIssueCode::LeadingSlashStripped,
                "Path has leading slashes",
                PathIssueSeverity::Warning,
                None,
            );
        }
    }

    // Trailing slash -> directory
    if path.ends_with('/') {
        push_issue(
            &mut issues,
            PathIssueCode::TrailingSlash,
            "Path ends with \"/\" — appears to be a directory, not a file",
            PathIssueSeverity::Error,
            None,
        );
        return early_result(path, issues);
    }

    // Duplicate slashes
    if path.contains("//") {
        if options.auto_fix {
            push_issue(
                &mut issues,
                PathIssueCode::DuplicateSlashes,
                "Path contained duplicate slashes",
                PathIssueSeverity::AutoFixed,
                Some(&path),
            );
            while path.contains("//") {
                path = path.replace("//", "/");
            }
        } else {
            push_issue(
                &mut issues,
                PathIssueCode::DuplicateSlashes,
                "Path contains duplicate slashes",
                PathIssueSeverity::Warning,
                None,
            );
        }
    }

    // Forbidden characters
    if has_forbidden_chars(&path) {
        push_issue(
            &mut issues,
            PathIssueCode::ForbiddenChars,
            "Path contains forbidden characters (<>|?*)",
            PathIssueSeverity::Error,
            None,
        );
        return early_result(path, issues);
    }

    // Data file as directory
    let segs: Vec<&str> = path.split('/').collect();
    if segs.len() > 1 {
        if let Some(offending) = segs[..segs.len() - 1].iter().find(|s| is_leaf_file_segment(s)) {
            push_issue(
                &mut issues,
                PathIssueCode::DataFileAsDirectory,
                format!("Path uses data file \"{}\" as a directory", offending),
                PathIssueSeverity::Error,
                Some(raw_path),
            );
            return early_result(path, issues);
        }
    }

    // Too long
    if path.len() > MAX_PATH_LENGTH {
        push_issue(
            &mut issues,
            PathIssueCode::TooLong,
            format!("Path exceeds {} characters", MAX_PATH_LENGTH),
            PathIssueSeverity::Error,
            None,
        );
        return early_result(path, issues);
    }

    if path.is_empty() {
        push_issue(
            &mut issues,
            PathIssueCode::EmptyPath,
            "Path resolved to empty after sanitization",
            PathIssueSeverity::Error,
            None,
        );
        return early_result(String::new(), issues);
    }

    PathValidationResult {
        valid: !has_error(&issues),
        sanitized_path: path,
        was_fixed: was_fixed(&issues),
        issues,
    }
}

fn early_result(path: String, issues: Vec<PathIssue>) -> PathValidationResult {
    let was_fixed = was_fixed(&issues);
    PathValidationResult {
        valid: false,
        sanitized_path: path,
        issues,
        was_fixed,
    }
}

/// Validate a batch of file paths, returning valid files with sanitized paths.
pub fn validate_generated_file_paths(
    files: &[(String, String)], // (path, content)
) -> (Vec<(String, String)>, usize, usize, Vec<String>) {
    let mut valid_files = Vec::new();
    let mut rejected = 0;
    let mut auto_fixed = 0;
    let mut warnings = Vec::new();

    for (path, content) in files {
        let result = validate_file_path(path, PathValidationOptions::auto_fix());
        if !result.valid {
            rejected += 1;
            let error_msg = format!(
                "Rejected file: \"{}\" — {}",
                path,
                result
                    .issues
                    .iter()
                    .filter(|i| i.severity == PathIssueSeverity::Error)
                    .map(|i| i.message.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            warnings.push(error_msg);
            continue;
        }
        if result.was_fixed {
            auto_fixed += 1;
        }
        valid_files.push((result.sanitized_path, content.clone()));
    }

    (valid_files, rejected, auto_fixed, warnings)
}

/// Returns the canonical prompt rules for generated file paths.
pub fn get_file_path_rules_prompt() -> String {
    [
        "",
        "## File Path Rules (STRICTLY ENFORCED)",
        "When generating files, use ONLY valid relative file paths. The following rules are enforced:",
        "- DO NOT generate platform configuration files (`package.json`, `vite.config.ts`, `tsconfig.json`, `tailwind.config.js`). These are generated automatically by the platform.",
        "- Paths MUST be relative (no leading \"/\" or drive letters like \"C:\\\")",
        "- Paths MUST NOT contain \"..\" (no directory traversal)",
        "- Paths MUST NOT contain: < > | ? * (forbidden OS characters)",
        "- Paths MUST NOT end with \"/\" (that indicates a directory, not a file)",
        "- Paths MUST NOT contain null bytes or control characters",
        "- Paths MUST be 512 characters or fewer",
        "- Use forward slashes \"/\" as directory separators (never backslashes)",
        "",
        "Framework-specific characters ARE allowed:",
        "- \"$\" for TanStack Router dynamic routes (e.g., \"src/routes/cameras/$cameraId.tsx\")",
        "- \"[]\" for Next.js dynamic routes (e.g., \"pages/users/[id].tsx\")",
        "- \"()\" for Next.js route groups (e.g., \"app/(auth)/login.tsx\")",
        "",
        "DO NOT wrap file paths in markdown code fences. Provide the raw path directly.",
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_relative_path() {
        let r = validate_file_path("src/components/Button.tsx", PathValidationOptions::auto_fix());
        assert!(r.valid);
        assert_eq!(r.sanitized_path, "src/components/Button.tsx");
        assert!(!r.was_fixed);
    }

    #[test]
    fn test_traversal_rejected() {
        let r = validate_file_path("../etc/passwd", PathValidationOptions::auto_fix());
        assert!(!r.valid);
        assert!(r.issues.iter().any(|i| i.code == PathIssueCode::PathTraversal));
    }

    #[test]
    fn test_absolute_stripped_in_auto_fix() {
        let r = validate_file_path("/src/App.tsx", PathValidationOptions::auto_fix());
        assert!(r.valid);
        assert_eq!(r.sanitized_path, "src/App.tsx");
        assert!(r.was_fixed);
    }

    #[test]
    fn test_absolute_rejected_without_auto_fix() {
        let r = validate_file_path("/src/App.tsx", PathValidationOptions::no_fix());
        assert!(!r.valid);
    }

    #[test]
    fn test_markdown_fence_leak() {
        let r = validate_file_path("```tsx\nsrc/App.tsx\n```", PathValidationOptions::auto_fix());
        assert!(r.valid);
        assert_eq!(r.sanitized_path, "src/App.tsx");
        assert!(r.issues.iter().any(|i| i.code == PathIssueCode::MarkdownFenceLeak));
    }

    #[test]
    fn test_backslash_normalization() {
        let r = validate_file_path("src\\components\\Button.tsx", PathValidationOptions::auto_fix());
        assert!(r.valid);
        assert_eq!(r.sanitized_path, "src/components/Button.tsx");
    }

    #[test]
    fn test_trailing_slash_rejected() {
        let r = validate_file_path("src/components/", PathValidationOptions::auto_fix());
        assert!(!r.valid);
        assert!(r.issues.iter().any(|i| i.code == PathIssueCode::TrailingSlash));
    }

    #[test]
    fn test_data_file_as_directory() {
        let r = validate_file_path("package.json/index.ts", PathValidationOptions::auto_fix());
        assert!(!r.valid);
        assert!(r.issues.iter().any(|i| i.code == PathIssueCode::DataFileAsDirectory));
    }

    #[test]
    fn test_forbidden_chars() {
        let r = validate_file_path("src/<bad>.tsx", PathValidationOptions::auto_fix());
        assert!(!r.valid);
        assert!(r.issues.iter().any(|i| i.code == PathIssueCode::ForbiddenChars));
    }

    #[test]
    fn test_batch_validation_counts() {
        let files = vec![
            ("src/A.tsx".into(), "a".into()),
            ("../bad.ts".into(), "b".into()),
            ("  src/spaces.tsx  ".into(), "c".into()),
        ];
        let (valid, rejected, fixed, _) = validate_generated_file_paths(&files);
        assert_eq!(valid.len(), 2);
        assert_eq!(rejected, 1);
        assert_eq!(fixed, 1);
    }

    #[test]
    fn test_rules_prompt_keywords() {
        let prompt = get_file_path_rules_prompt();
        assert!(prompt.contains("STRICTLY ENFORCED"));
        assert!(prompt.contains("directory traversal"));
    }
}
