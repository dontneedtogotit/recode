//! Content Type Validator (ported from devcode contentTypeValidator.ts)
//!
//! Validates that AI-generated file content matches the expected file type.
//! Prevents the "file-content misalignment" bug where the AI generates
//! TypeScript code for index.html.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContentValidationAction {
    Accept,
    ReplaceWithFallback,
    WarnAndAccept,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentValidationResult {
    pub valid: bool,
    pub expected_type: String,
    pub detected_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub action: ContentValidationAction,
    pub confidence: f32, // Note: no Eq derive
}

pub fn detect_content_type(content: &str) -> String {
    let trimmed = content.trim_start();
    
    // HTML detection
    if trimmed.to_lowercase().starts_with("<!doctype html") || trimmed.to_lowercase().starts_with("<html") {
        return "html".into();
    }
    
    // CSS detection (but not CSS-in-JS)
    if (content.contains("{") && std::primitive::str::contains(content, "@media ")) ||
       content.contains(":root {") || content.contains(":root{") {
        if !content.contains("import ") && !content.contains("export ") {
            return "css".into();
        }
    }
    
    // TypeScript/TSX detection
    if content.contains("import ") || content.contains("export ") || content.contains("interface ") || content.contains("type ") {
        if content.contains("className") || content.contains("</div>") || content.contains("</span>") {
            return "tsx".into();
        }
        return "typescript".into();
    }
    
    // Plain JS
    if content.contains("function ") || content.contains("const ") || content.contains("class ") {
        return "javascript".into();
    }

    "unknown".into()
}

pub fn normalize_type(raw_type: &str) -> String {
    let lower = raw_type.to_lowercase();
    let lower: String = lower.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    if lower.contains("html") { return "html".into(); }
    if lower.contains("css") { return "css".into(); }
    if lower.contains("tsx") || lower.contains("typescriptreact") { return "tsx".into(); }
    if lower.contains("ts") || lower.contains("typescript") { return "typescript".into(); }
    if lower.contains("js") || lower.contains("javascript") { return "javascript".into(); }
    lower
}

pub fn types_compatible(expected: &str, detected: &str) -> bool {
    let norm_expected = normalize_type(expected);
    let norm_detected = normalize_type(detected);
    
    if norm_expected == norm_detected { return true; }
    
    let ts_family = ["typescript", "tsx", "javascript", "jsx"];
    if ts_family.contains(&norm_expected.as_str()) && ts_family.contains(&norm_detected.as_str()) {
        return true;
    }
    
    if norm_detected == "unknown" { return true; }
    
    false
}

pub fn infer_type_from_path(file_path: &str) -> String {
    let lower = file_path.to_lowercase();
    if lower.ends_with(".html") || lower.ends_with(".htm") { return "html".into(); }
    if lower.ends_with(".css") { return "css".into(); }
    if lower.ends_with(".tsx") { return "tsx".into(); }
    if lower.ends_with(".ts") { return "typescript".into(); }
    if lower.ends_with(".jsx") || lower.ends_with(".js") { return "javascript".into(); }
    if lower.ends_with(".json") { return "json".into(); }
    if lower.ends_with(".md") { return "markdown".into(); }
    "unknown".into()
}

pub fn validate_file_content_matches(content: &str, file_path: &str, expected_language: Option<&str>) -> ContentValidationResult {
    let inferred = expected_language.map(String::from).unwrap_or_else(|| infer_type_from_path(file_path));
    let expected_type = normalize_type(&inferred);
    let detected_type = detect_content_type(content);
    
    let confidence = if detected_type == "unknown" { 0.3 } else { 0.9 };
    
    if types_compatible(&expected_type, &detected_type) {
        return ContentValidationResult {
            valid: true,
            expected_type,
            detected_type,
            reason: None,
            action: ContentValidationAction::Accept,
            confidence,
        };
    }
    
    let reason = format!("Expected {} content for {}, but detected {}", expected_type, file_path, detected_type);
    
    let is_critical = 
        (expected_type == "html" && (detected_type == "typescript" || detected_type == "tsx" || detected_type == "javascript")) ||
        (expected_type == "css"  && (detected_type == "typescript" || detected_type == "tsx" || detected_type == "javascript"));
        
    if is_critical {
        return ContentValidationResult {
            valid: false,
            expected_type,
            detected_type,
            reason: Some(reason),
            action: ContentValidationAction::ReplaceWithFallback,
            confidence,
        };
    }
    
    ContentValidationResult {
        valid: false,
        expected_type,
        detected_type,
        reason: Some(reason),
        action: ContentValidationAction::WarnAndAccept,
        confidence,
    }
}

pub fn get_fallback_content(file_path: &str, language: Option<&str>) -> Option<String> {
    let raw = language.map(String::from).unwrap_or_else(|| infer_type_from_path(file_path));
    let t = normalize_type(&raw);
    if t == "html" {
        return Some(r#"<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>App</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
"#.into());
    }
    if t == "css" {
        return Some(r#"/* Generated CSS file */
:root {
  --color-primary: #1976D2;
  --color-text: #212121;
  --spacing-md: 1rem;
}

body {
  margin: 0;
  font-family: system-ui, -apple-system, sans-serif;
}
"#.into());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_html() {
        let content = "<!DOCTYPE html><html><body></body></html>";
        let res = validate_file_content_matches(content, "index.html", None);
        assert!(res.valid);
        assert_eq!(res.action, ContentValidationAction::Accept);
        assert_eq!(res.detected_type, "html");
    }

    #[test]
    fn test_critical_mismatch_html_vs_ts() {
        let content = "export function App() { return <div></div>; }";
        let res = validate_file_content_matches(content, "index.html", None);
        assert!(!res.valid);
        assert_eq!(res.action, ContentValidationAction::ReplaceWithFallback);
        assert_eq!(res.detected_type, "tsx");
        
        let fallback = get_fallback_content("index.html", None).unwrap();
        assert!(fallback.starts_with("<!DOCTYPE html>"));
    }

    #[test]
    fn test_compatibility_ts_tsx() {
        let content = "export function App() { return <div></div>; }";
        let res = validate_file_content_matches(content, "components/App.ts", None);
        // expected: typescript, detected: tsx -> compatible
        assert!(res.valid);
        assert_eq!(res.action, ContentValidationAction::Accept);
    }
}
