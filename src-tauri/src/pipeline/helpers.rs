//! Build Pipeline Helpers (ported from devcode BuildPipelineHelpers.ts)
//!
//! Pure utility functions for the build pipeline: language inference, purpose
//! derivation, timeout estimation, JSON cleaning, token budget estimation,
//! and truncation detection.
//! Pure module — no stores, no DI, no I/O.

/// Derive a human-readable purpose string from a file path.
#[allow(dead_code)]  // parity helper, wiring pending
pub fn derive_purpose_from_path(file_path: &str) -> String {
    let parts: Vec<&str> = file_path.split('/').collect();
    let filename = parts.last().copied().unwrap_or(file_path);
    let name_without_ext = match filename.rfind('.') {
        Some(pos) => &filename[..pos],
        None => filename,
    };
    let dir = parts.get(parts.len().saturating_sub(2)).unwrap_or(&"").to_ascii_lowercase();

    let kind = match dir.as_str() {
        "components" => "component",
        "hooks" => "hook",
        "services" => "service",
        "utils" => "utility",
        "helpers" => "helper",
        "pages" => "page",
        "views" => "view",
        "models" => "model",
        "types" => "type definitions",
        "interfaces" => "interface definitions",
        "store" | "stores" => "state store",
        "api" => "API module",
        "routes" => "route handler",
        "middleware" => "middleware",
        "context" => "React context",
        "providers" => "provider",
        "lib" => "library module",
        "config" => "configuration",
        "constants" => "constants",
        _ => "module",
    };
    format!("{name_without_ext} {kind}")
}

/// Infer a language string from a file path extension.
#[allow(dead_code)]  // parity helper, wiring pending
pub fn infer_language(file_path: &str) -> String {
    let ext = file_path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "ts" | "tsx" => "typescript",
        "js" | "jsx" => "javascript",
        "py" => "python",
        "rs" => "rust",
        "go" => "go",
        "java" => "java",
        "css" | "scss" => "css",
        "html" | "htm" => "html",
        "json" => "json",
        "md" => "markdown",
        "sql" => "sql",
        "yaml" | "yml" => "yaml",
        _ => &ext,
    }.into()
}

/// Estimate per-file timeout based on file complexity heuristics.
#[allow(dead_code)]  // parity helper, wiring pending
pub fn estimate_file_timeout(
    path: &str,
    purpose: Option<&str>,
    base_timeout_ms: u64,
    max_timeout_ms: u64,
) -> u64 {
    let p = path.to_ascii_lowercase();
    let mut type_multiplier = 1.0_f64;
    if p.contains("/services/") || p.ends_with(".service.ts") {
        type_multiplier = 1.5;
    } else if p.contains("/pages/") || p.contains("/views/") {
        type_multiplier = 1.4;
    } else if p.contains("/components/") || p.ends_with(".tsx") {
        type_multiplier = 1.3;
    } else if p.contains("/hooks/") || p.contains("/stores/") || p.ends_with("store.ts") {
        type_multiplier = 1.2;
    }

    let purpose_len = purpose.map(|p| p.len()).unwrap_or(0);
    let purpose_bonus = std::cmp::min(60_000, (purpose_len / 100) * 1000) as u64;

    let estimated = (base_timeout_ms as f64 * type_multiplier).round() as u64 + purpose_bonus;
    std::cmp::max(base_timeout_ms, std::cmp::min(max_timeout_ms, estimated))
}

/// Calculate a dynamic timeout based on total message content size.
#[allow(dead_code)]  // parity helper, wiring pending
pub fn calculate_dynamic_timeout(
    total_content_chars: usize,
    fallback_ms: u64,
) -> u64 {
    if total_content_chars == 0 {
        return fallback_ms;
    }
    const MIN_TIMEOUT_MS: u64 = 60_000;
    const MAX_TIMEOUT_MS: u64 = 600_000;
    const BASE_TIMEOUT_MS: u64 = 60_000;
    const MS_PER_KB: f64 = 2_000.0;
    const BUFFER_MULTIPLIER: f64 = 1.5;

    let kb = total_content_chars as f64 / 1024.0;
    let estimated = BASE_TIMEOUT_MS as f64 + kb * MS_PER_KB * BUFFER_MULTIPLIER;
    std::cmp::max(MIN_TIMEOUT_MS, std::cmp::min(MAX_TIMEOUT_MS, estimated.round() as u64))
}

/// Strip markdown code fences from a raw LLM response.
#[allow(dead_code)]  // parity helper, wiring pending
pub fn clean_json_response(raw: &str) -> String {
    let mut cleaned = raw.trim().to_string();
    if cleaned.starts_with("```") {
        if let Some(end_first_line) = cleaned.find('\n') {
            cleaned = cleaned[end_first_line + 1..].to_string();
        }
        if cleaned.ends_with("```") {
            cleaned = cleaned[..cleaned.len() - 3].trim().to_string();
        }
    }
    cleaned.trim().to_string()
}

/// Budget-aware per-file token estimate.
#[allow(dead_code)]  // parity helper, wiring pending
pub fn estimate_file_max_tokens(content: &str, planning_context: Option<&str>) -> usize {
    let base_lines = content.lines().count();
    let by_lines = std::cmp::max(2_000, base_lines * 80);
    let spec_length = planning_context.map(|s| s.len()).unwrap_or(0);
    let by_spec = std::cmp::max(2_000, (spec_length as f64 * 1.5) as usize);
    std::cmp::min(16_000, std::cmp::max(by_lines, by_spec))
}

/// True when the provider signalled the response was cut off by a token cap.
pub fn is_truncated_response(finish_reason: Option<&str>) -> bool {
    match finish_reason.map(|r| r.to_ascii_lowercase()) {
        Some(ref r) if r == "length" || r == "max_tokens" || r == "content_filter" => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_purpose_from_path() {
        assert_eq!(derive_purpose_from_path("src/components/Button.tsx"), "Button component");
        assert_eq!(derive_purpose_from_path("src/hooks/useAuth.ts"), "useAuth hook");
        assert_eq!(derive_purpose_from_path("src/types.ts"), "types module");
        assert_eq!(derive_purpose_from_path("src/App.tsx"), "App module");
    }

    #[test]
    fn test_infer_language() {
        assert_eq!(infer_language("src/App.tsx"), "typescript");
        assert_eq!(infer_language("src/App.css"), "css");
        assert_eq!(infer_language("index.html"), "html");
        assert_eq!(infer_language("main.go"), "go");
        assert_eq!(infer_language("lib.rs"), "rust");
    }

    #[test]
    fn test_estimate_file_timeout() {
        let timeout = estimate_file_timeout("src/services/AuthService.ts", Some("auth"), 60_000, 300_000);
        assert!(timeout >= 90_000); // 1.5x multiplier
        let timeout2 = estimate_file_timeout("src/App.tsx", None, 60_000, 300_000);
        assert!(timeout2 >= 78_000); // 1.3x multiplier
    }

    #[test]
    fn test_clean_json_response() {
        let raw = "```json\n{\"key\": \"value\"}\n```";
        assert_eq!(clean_json_response(raw), "{\"key\": \"value\"}");
        let clean = "{\"key\": \"value\"}";
        assert_eq!(clean_json_response(clean), clean);
    }

    #[test]
    fn test_estimate_file_max_tokens() {
        let large = "x\n".repeat(200);
        let tokens = estimate_file_max_tokens(&large, None);
        assert!(tokens >= 2_000);
        assert!(tokens <= 16_000);
    }

    #[test]
    fn test_is_truncated_response() {
        assert!(is_truncated_response(Some("length")));
        assert!(is_truncated_response(Some("max_tokens")));
        assert!(!is_truncated_response(Some("stop")));
        assert!(!is_truncated_response(None));
    }
}
