use std::borrow::Cow;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DetectedErrorType {
    Typescript,
    Syntax,
    Build,
    Runtime,
    Network,
    Permission,
    Unknown,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DetectedError {
    pub error_type: DetectedErrorType,
    pub severity: String,
    pub message: String,
    pub file_path: Option<String>,
    pub line_number: Option<u32>,
    pub column_number: Option<u32>,
    pub suggested_fix: Option<String>,
}

const BENIGN_PREFIXES: &[&str] = &["[vite]", "[hmr]", "VITE v"];

const ERROR_SIGNATURES: &[(&str, DetectedErrorType, &str)] = &[
    ("SyntaxError:", DetectedErrorType::Syntax, "syntax error"),
    ("error TS", DetectedErrorType::Typescript, "typescript error"),
    ("Failed to resolve", DetectedErrorType::Build, "build resolution error"),
    ("Missing script:", DetectedErrorType::Build, "missing npm script"),
    ("[esbuild]", DetectedErrorType::Build, "esbuild error"),
    ("[rollup]", DetectedErrorType::Build, "rollup error"),
    ("Plugin error:", DetectedErrorType::Build, "plugin error"),
    (
        "Unhandled promise rejection:",
        DetectedErrorType::Runtime,
        "unhandled promise rejection",
    ),
    (
        "Unhandled Rejection at:",
        DetectedErrorType::Runtime,
        "unhandled rejection",
    ),
    ("exited with code", DetectedErrorType::Runtime, "process exited"),
    ("EACCES", DetectedErrorType::Permission, "permission denied"),
    (
        "permission denied",
        DetectedErrorType::Permission,
        "permission denied",
    ),
    (
        "ENOENT",
        DetectedErrorType::Permission,
        "missing file or directory",
    ),
    (
        "no such file or directory",
        DetectedErrorType::Permission,
        "missing file or directory",
    ),
    ("timed out", DetectedErrorType::Network, "network timeout"),
    (
        "ECONNREFUSED",
        DetectedErrorType::Network,
        "connection refused",
    ),
    (
        "connection refused",
        DetectedErrorType::Network,
        "connection refused",
    ),
];

pub struct ConsoleErrorDetector;

impl ConsoleErrorDetector {
    pub fn detect(line: &str) -> Option<DetectedError> {
        let trimmed = line.trim();
        if Self::is_benign_line(trimmed) {
            return None;
        }

        for (signature, error_type, message_hint) in ERROR_SIGNATURES {
            if trimmed.contains(signature) {
                let _message: String = trimmed.into();
                let mut file_path = Self::extract_file_path(trimmed);
                let line_number = Self::extract_line_number(trimmed);
                let column_number = Self::extract_column_number(trimmed);

                let message = if matches!(error_type, DetectedErrorType::Typescript)
                    && message_hint.starts_with("typescript")
                {
                    Self::normalize_typescript_message(trimmed)
                } else if matches!(error_type, DetectedErrorType::Build)
                    && signature == &"Failed to resolve"
                {
                    file_path = Self::resolve_source_file(trimmed);
                    Self::normalize_failed_resolve_message(trimmed)
                } else if matches!(error_type, DetectedErrorType::Build)
                    && signature == &"Missing script:"
                {
                    file_path = Some("package.json".into());
                    Self::normalize_missing_script_message(trimmed)
                } else if matches!(error_type, DetectedErrorType::Syntax)
                    && signature == &"SyntaxError:"
                {
                    Self::normalize_syntax_message(trimmed)
                } else if matches!(error_type, DetectedErrorType::Network)
                    && (signature == &"ECONNREFUSED"
                        || signature == &"connection refused")
                {
                    Self::normalize_connection_refused_message(trimmed)
                } else {
                    (*message_hint).into()
                };

                return Some(DetectedError {
                    error_type: error_type.clone(),
                    severity: "error".into(),
                    message,
                    file_path,
                    line_number,
                    column_number,
                    suggested_fix: None,
                });
            }
        }

        if trimmed.to_lowercase().contains("error") {
            return Some(DetectedError {
                error_type: DetectedErrorType::Unknown,
                severity: "error".into(),
                message: trimmed.into(),
                file_path: None,
                line_number: None,
                column_number: None,
                suggested_fix: None,
            });
        }

        None
    }

    pub fn is_benign_line(line: &str) -> bool {
        BENIGN_PREFIXES
            .iter()
            .any(|prefix| line.starts_with(prefix))
            || line.contains("[vite] hmr update")
            || line.contains("[hmr] hot updated")
            || line.contains("[hmr] page reload")
            || line.contains("ready in")
            || line.contains("Local:  ")
            || line.contains("Network: ")
    }

    fn normalize_typescript_message(line: &str) -> String {
        if let Some(marker) = line.find(" - error TS") {
            let rest = &line[marker + 2..];
            if let Some(code_end) = rest.find(':') {
                let code = rest[..code_end].trim();
                let message = rest[code_end + 1..].trim();
                return format!("TypeScript error {}: {}", code, message);
            }
        }
        "TypeScript compilation error".into()
    }

    fn normalize_failed_resolve_message(line: &str) -> String {
        let lower = line.to_lowercase();
        let marker = "failed to resolve ";
        if let Some(start) = lower.find(marker) {
            let rest = &line[start + marker.len()..];
            let trimmed = rest.trim_start();
            let mut source = trimmed;
            let mut import_path = None;
            if let Some(from) = trimmed.find(" from ") {
                import_path = Some(
                    trimmed[..from]
                        .trim_matches('\'')
                        .trim_matches('"')
                        .to_string(),
                );
                source = &trimmed[from + 5..];
            }
            let from = source
                .trim_start()
                .trim_matches('\'')
                .trim_matches('"')
                .trim_end_matches('.')
                .trim_end_matches(' ')
                .to_string();
            if let Some(import_path) = import_path {
                format!("Failed to resolve '{import_path}' from '{from}'")
            } else {
                format!("Failed to resolve from '{from}'")
            }
        } else {
            "Build resolution error".into()
        }
    }

    fn normalize_missing_script_message(line: &str) -> String {
        let marker = "Missing script:";
        if let Some(start) = line.find(marker) {
            let script = line[start + marker.len()..]
                .trim()
                .trim_matches('"')
                .trim_matches('\'');
            format!("Missing script: \"{script}\"")
        } else {
            "Missing npm script".into()
        }
    }

    fn normalize_syntax_message(line: &str) -> String {
        let marker = "SyntaxError:";
        if let Some(start) = line.find(marker) {
            let message = line[start + marker.len()..].trim();
            if message.is_empty() {
                "syntax error".into()
            } else {
                format!("SyntaxError: {message}")
            }
        } else {
            "syntax error".into()
        }
    }

    fn normalize_connection_refused_message(_line: &str) -> String {
        "Connection refused".into()
    }

    fn resolve_source_file(line: &str) -> Option<String> {
        let marker = " from ";
        if let Some(index) = line.find(marker) {
            let after = &line[index + marker.len()..];
            let candidate = after
                .trim_start()
                .trim_matches('\'')
                .trim_matches('"')
                .trim_end_matches('.')
                .trim_end_matches(' ')
                .to_string();
            if candidate.starts_with('.')
                || candidate.starts_with('/')
                || candidate.contains('/')
                || candidate == "package.json"
            {
                return Some(candidate);
            }
        }
        None
    }

    fn extract_file_path(line: &str) -> Option<String> {
        let candidates = Self::split_potential_paths(line);
        candidates.into_iter().find(|candidate| {
            candidate.starts_with('.')
                || candidate.starts_with('/')
                || candidate.contains('/')
                || (candidate.len() > 2 && candidate.ends_with(".ts"))
                || (candidate.len() > 2 && candidate.ends_with(".tsx"))
                || (candidate.len() > 2 && candidate.ends_with(".js"))
                || (candidate.len() > 2 && candidate.ends_with(".jsx"))
                || candidate == "package.json"
        })
    }

    fn extract_line_number(line: &str) -> Option<u32> {
        Self::first_number_after_colon(line, 1)
    }

    fn extract_column_number(line: &str) -> Option<u32> {
        Self::first_number_after_colon(line, 2)
    }

    fn first_number_after_colon(line: &str, occurrence: usize) -> Option<u32> {
        let mut count = 0;
        for segment in line.split(':') {
            count += 1;
            if count <= occurrence {
                continue;
            }
            let digits = segment.trim().trim_end_matches([' ', '-']);
            if let Some(number) = digits.split_whitespace().next() {
                if let Ok(value) = number.parse::<u32>() {
                    if value > 0 {
                        return Some(value);
                    }
                }
            }
        }
        None
    }

    fn split_potential_paths(line: &str) -> Vec<String> {
        let mut values = Vec::new();
        for segment in line.split([' ', '(', ')', '[', ']', ',', '\'', '"']) {
            let trimmed = segment.trim();
            if trimmed.len() > 1 {
                values.push(trimmed.to_string());
            }
        }
        values
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_benign_vite_status_lines() {
        assert!(ConsoleErrorDetector::is_benign_line("[vite] hmr update /src/main.tsx"));
        assert!(ConsoleErrorDetector::is_benign_line("[hmr] hot updated"));
        assert!(ConsoleErrorDetector::is_benign_line("VITE v6.0.0  ready in 234ms"));
        assert!(ConsoleErrorDetector::is_benign_line("Local:   http://localhost:9666/"));
        assert!(!ConsoleErrorDetector::is_benign_line("Error: failed to start dev server"));
    }

    #[test]
    fn detects_typescript_errors() {
        let error = ConsoleErrorDetector::detect(
            "src/main.tsx:10:5 - error TS2304: Cannot find name 'foo'.",
        );
        assert!(error.is_some());
        let error = error.unwrap();
        assert_eq!(error.error_type, DetectedErrorType::Typescript);
        assert_eq!(error.file_path.as_deref(), Some("src/main.tsx:10:5"));
        assert_eq!(error.line_number, Some(10));
        assert_eq!(error.column_number, Some(5));
        assert!(error.message.contains("TS2304"));
    }

    #[test]
    fn detects_missing_script() {
        let error = ConsoleErrorDetector::detect(r#"npm error Missing script: "dev""#);
        assert!(error.is_some());
        let error = error.unwrap();
        assert_eq!(error.error_type, DetectedErrorType::Build);
        assert!(error.message.contains("Missing script"));
        assert_eq!(error.file_path.as_deref(), Some("package.json"));
    }

    #[test]
    fn detects_failed_to_resolve() {
        let error = ConsoleErrorDetector::detect("Failed to resolve './missing' from './App.tsx'");
        assert!(error.is_some());
        let error = error.unwrap();
        assert_eq!(error.error_type, DetectedErrorType::Build);
        assert!(error.message.contains("Failed to resolve"));
        assert_eq!(error.file_path.as_deref(), Some("./App.tsx"));
    }

    #[test]
    fn detects_syntax_error() {
        let error = ConsoleErrorDetector::detect("SyntaxError: Unexpected token '<'");
        assert!(error.is_some());
        let error = error.unwrap();
        assert_eq!(error.error_type, DetectedErrorType::Syntax);
        assert!(error.message.contains("Unexpected token '<'"));
    }

    #[test]
    fn detects_network_timeout() {
        let error = ConsoleErrorDetector::detect("Request timed out after 30s");
        assert!(error.is_some());
        let error = error.unwrap();
        assert_eq!(error.error_type, DetectedErrorType::Network);
        assert!(error.message.contains("timeout"));
    }

    #[test]
    fn detects_connection_refused() {
        let error = ConsoleErrorDetector::detect("connect ECONNREFUSED 127.0.0.1:9666");
        assert!(error.is_some());
        let error = error.unwrap();
        assert_eq!(error.error_type, DetectedErrorType::Network);
        assert!(error.message.contains("Connection refused"));
    }

    #[test]
    fn detects_permission_error() {
        let error = ConsoleErrorDetector::detect("EACCES: permission denied, open '/tmp/output'");
        assert!(error.is_some());
        let error = error.unwrap();
        assert_eq!(error.error_type, DetectedErrorType::Permission);
    }

    #[test]
    fn falls_back_to_unknown_error_when_line_contains_error() {
        let error = ConsoleErrorDetector::detect("Something went wrong: error state");
        assert!(error.is_some());
        let error = error.unwrap();
        assert_eq!(error.error_type, DetectedErrorType::Unknown);
        assert_eq!(error.severity, "error");
    }

    #[test]
    fn returns_none_for_benign_hmr() {
        let error = ConsoleErrorDetector::detect("[hmr] page reload /src/main.tsx");
        assert!(error.is_none());
    }
}
