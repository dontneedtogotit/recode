//! Generated Module Paths (ported from devcode generatedModulePaths.ts)
//!
//! Path and identifier helpers for code the generator writes into a workspace.
//! Pure and total: given any string returns something safe to write to disk.

const ASSET_EXTENSIONS: &[&str] = &[
    ".css", ".scss", ".sass", ".less", ".styl", ".stylus",
    ".json", ".jsonc", ".svg", ".png", ".jpg", ".jpeg", ".gif", ".webp",
    ".avif", ".bmp", ".ico", ".woff", ".woff2", ".eot", ".ttf", ".otf",
    ".mp3", ".mp4", ".m4a", ".wav", ".ogg", ".webm", ".mov", ".pdf",
    ".txt", ".md", ".mdx", ".csv", ".yaml", ".yml", ".graphql", ".gql",
    ".wasm", ".glsl", ".frag", ".vert",
];

const CODE_EXTENSIONS: &[&str] = &[
    ".ts", ".tsx", ".js", ".jsx", ".mts", ".cts", ".mjs", ".cjs",
];

const TEST_MARKERS: &[&str] = &[".test", ".spec"];

fn strip_query_fragment(specifier: &str) -> &str {
    let q = specifier.find(|c| c == '?' || c == '#').unwrap_or(specifier.len());
    &specifier[..q]
}

fn ends_with_any_ci(haystack: &str, suffixes: &[&str]) -> bool {
    let lower = haystack.to_ascii_lowercase();
    suffixes.iter().any(|s| lower.ends_with(s))
}

/// True when an import specifier points at an asset rather than a module.
pub fn is_asset_import(specifier: &str) -> bool {
    let bare = strip_query_fragment(specifier);
    ends_with_any_ci(bare, ASSET_EXTENSIONS)
}

/// True when a path carries an extension the TS/JS toolchain compiles.
pub fn has_code_extension(path: &str) -> bool {
    ends_with_any_ci(path, CODE_EXTENSIONS)
}

/// True when a path is already a test or spec file.
pub fn is_test_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    for marker in TEST_MARKERS {
        if let Some(pos) = lower.rfind(marker) {
            let after = &path[pos + marker.len()..];
            if after.is_empty() || ends_with_any_ci(after, CODE_EXTENSIONS) {
                return true;
            }
        }
    }
    false
}

/// Convert an arbitrary file basename into a valid JavaScript identifier.
pub fn to_module_identifier(raw: &str, fallback: &str) -> String {
    let parts: Vec<&str> = raw
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '$')
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        return fallback.to_string();
    }
    let pascal: String = parts
        .iter()
        .map(|p| {
            let mut chars = p.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect();
    if pascal.is_empty() {
        return fallback.to_string();
    }
    if pascal.as_bytes()[0].is_ascii_digit() {
        format!("_{}", pascal)
    } else {
        pascal
    }
}

fn replace_code_extension_with_test(filename: &str) -> String {
    let lower = filename.to_ascii_lowercase();
    for ext in CODE_EXTENSIONS {
        if lower.ends_with(ext) {
            let pos = filename.len() - ext.len();
            return format!("{}.test{}", &filename[..pos], ext);
        }
    }
    format!("{}.test.ts", filename)
}

/// Derive the test file path for a source file. Idempotent.
pub fn derive_test_path(source_path: &str, colocate: bool) -> String {
    if is_test_path(source_path) {
        return source_path.to_string();
    }
    let segments: Vec<&str> = source_path.split('/').collect();
    let filename = segments.last().copied().unwrap_or(source_path);
    let dir = if segments.len() > 1 {
        segments[..segments.len() - 1].join("/")
    } else {
        String::new()
    };
    let test_filename = replace_code_extension_with_test(filename);
    if colocate {
        if dir.is_empty() {
            test_filename
        } else {
            format!("{}/{}", dir, test_filename)
        }
    } else if dir.is_empty() {
        format!("__tests__/{}", test_filename)
    } else {
        format!("{}/__tests__/{}", dir, test_filename)
    }
}

fn strip_test_marker(filename: &str) -> String {
    let lower = filename.to_ascii_lowercase();
    for marker in TEST_MARKERS {
        if let Some(pos) = lower.rfind(marker) {
            let after = &filename[pos + marker.len()..];
            if after.is_empty() || ends_with_any_ci(after, CODE_EXTENSIONS) {
                let before = &filename[..pos];
                return format!("{}{}", before, after);
            }
        }
    }
    filename.to_string()
}

/// The source path a test file covers.
pub fn source_path_for_test(test_path: &str) -> String {
    let segments: Vec<&str> = test_path.split('/').collect();
    let filename = segments.last().copied().unwrap_or(test_path);
    let dir_segments: Vec<&str> = segments
        .iter()
        .take(segments.len().saturating_sub(1))
        .filter(|s| **s != "__tests__")
        .copied()
        .collect();
    let source_name = strip_test_marker(filename);
    if dir_segments.is_empty() {
        source_name
    } else {
        format!("{}/{}", dir_segments.join("/"), source_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_asset_import() {
        assert!(is_asset_import("./index.css"));
        assert!(is_asset_import("./logo.svg?v=2"));
        assert!(!is_asset_import("./Button.tsx"));
    }

    #[test]
    fn test_has_code_extension() {
        assert!(has_code_extension("src/App.tsx"));
        assert!(has_code_extension("main.mjs"));
        assert!(!has_code_extension("index.css"));
    }

    #[test]
    fn test_is_test_path() {
        assert!(is_test_path("src/Button.test.tsx"));
        assert!(is_test_path("src/Button.spec.ts"));
        assert!(is_test_path("Button.test"));
        assert!(!is_test_path("src/Button.tsx"));
    }

    #[test]
    fn test_to_module_identifier() {
        assert_eq!(to_module_identifier("use-auth", "Gen"), "UseAuth");
        assert_eq!(to_module_identifier("index.css", "Gen"), "IndexCss");
        assert_eq!(to_module_identifier("404.page", "Gen"), "_404Page");
        assert_eq!(to_module_identifier("$$", "Fallback"), "$$");
        assert_eq!(to_module_identifier("---", "Fallback"), "Fallback");
        assert_eq!(to_module_identifier("", "Fallback"), "Fallback");
    }

    #[test]
    fn test_derive_test_path_colocate() {
        assert_eq!(derive_test_path("src/components/Button.tsx", true), "src/components/Button.test.tsx");
        assert_eq!(derive_test_path("src/components/Button.test.tsx", true), "src/components/Button.test.tsx");
    }

    #[test]
    fn test_derive_test_path_separate() {
        assert_eq!(derive_test_path("src/utils/helpers.ts", false), "src/utils/__tests__/helpers.test.ts");
    }

    #[test]
    fn test_source_path_for_test() {
        assert_eq!(source_path_for_test("src/components/Button.test.tsx"), "src/components/Button.tsx");
        assert_eq!(source_path_for_test("src/utils/__tests__/helpers.test.ts"), "src/utils/helpers.ts");
    }
}
