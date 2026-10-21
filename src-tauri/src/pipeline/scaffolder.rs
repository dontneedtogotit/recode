//! Deterministic project scaffolding (ported from devcode `ProjectScaffolder` /
//! `scaffoldPartition` / config generators).
//!
//! Configuration files for a given stack have exactly one correct answer.
//! Asking an LLM for `package.json`, `tsconfig.json`, or `vite.config.ts`
//! is where invented dependency versions, missing peer dependencies, and
//! wrong `moduleResolution` values come from. This module generates them
//! deterministically from curated, pinned manifests — no LLM involved.
//!
//! Pure module — no I/O, no DI. The engine calls `scaffold_files()` to produce
//! `GeneratedFile`s for scaffold paths, then only sends the remaining source
//! paths to the provider.
//!
//! Source mapping:
//! - `devcode/src/application/services/templates/scaffoldPartition.ts` → `is_scaffold_path` / `partition_scaffold_paths`
//! - `devcode/src/application/services/templates/generators/PackageJsonGenerator.ts` → `generate_package_json`
//! - `devcode/src/application/services/templates/generators/TsconfigGenerator.ts` → `generate_tsconfig` / `generate_tsconfig_app` / `generate_tsconfig_node`
//! - `devcode/src/application/services/templates/generators/ViteConfigGenerator.ts` → `generate_vite_config`
//! - `devcode/src/application/services/templates/generators/GitignoreGenerator.ts` → `generate_gitignore`
//! - `devcode/src/application/services/templates/manifest/curated-packages.json` → `REACT_SPA_PACKAGES`

use super::types::{GeneratedFile, GeneratedFileStatus};

/// Root-level files the deterministic generators own.
///
/// `index.html` is deliberately absent: it carries the root element id, the
/// page title and the entry script the generated app actually depends on, so
/// the model still writes it.
pub const SCAFFOLD_PATHS: &[&str] = &[
    "package.json",
    "tsconfig.json",
    "tsconfig.app.json",
    "tsconfig.node.json",
    "vite.config.ts",
    ".gitignore",
];

const SCAFFOLD_SET: &[&str] = SCAFFOLD_PATHS;

/// Strip a leading `./` or `/` so both spellings of a root path compare equal.
fn normalize_project_path(path: &str) -> &str {
    let path = path.strip_prefix("./").unwrap_or(path);
    path.strip_prefix('/').unwrap_or(path)
}

/// True when this path is a root-level configuration file the generators own.
/// Nested copies (`packages/ui/tsconfig.json`, a test fixture) are source.
pub fn is_scaffold_path(path: &str) -> bool {
    let normalized = normalize_project_path(path);
    // Only root-level config files are scaffold. Nested paths are source.
    if normalized.contains('/') {
        return false;
    }
    SCAFFOLD_SET.contains(&normalized)
}

/// Split a planned file list into scaffold (deterministic) and source (LLM).
pub fn partition_scaffold_paths(paths: &[String]) -> (Vec<String>, Vec<String>) {
    let mut scaffold = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut source = Vec::new();
    for path in paths {
        if is_scaffold_path(path) {
            let normalized = normalize_project_path(path).to_string();
            if !seen.contains(&normalized) {
                seen.insert(normalized.clone());
                scaffold.push(normalized);
            }
        } else {
            source.push(path.clone());
        }
    }
    (scaffold, source)
}

// ─── Curated packages (react-spa) ───────────────────────────────────────────
// Pinned, peer-compatible versions from devcode's curated-packages.json.

/// Dependencies for a Vite + React + TypeScript SPA.
fn react_spa_dependencies() -> Vec<(&'static str, &'static str)> {
    vec![("react", "19.2.7"), ("react-dom", "19.2.7")]
}

/// Dev dependencies for a Vite + React + TypeScript SPA.
fn react_spa_dev_dependencies() -> Vec<(&'static str, &'static str)> {
    vec![
        ("@types/react", "19.2.17"),
        ("@types/react-dom", "19.2.3"),
        ("@vitejs/plugin-react", "5.2.0"),
        ("typescript", "5.9.3"),
        ("vite", "7.3.6"),
        ("vitest", "3.2.6"),
        ("@testing-library/react", "16.3.2"),
        ("eslint", "9.39.4"),
        ("typescript-eslint", "8.68.0"),
    ]
}

// ─── Package.json generator ─────────────────────────────────────────────────

/// Generate a deterministic `package.json` for a Vite + React + TypeScript SPA.
pub fn generate_package_json(project_name: &str, description: &str) -> String {
    generate_package_json_for_stack(
        project_name,
        description,
        &super::stack::ProjectStack::ReactVite,
    )
}

/// Generate a deterministic `package.json` for the project's stack. The
/// vanilla stack omits React and its types/plugins entirely.
pub fn generate_package_json_for_stack(
    project_name: &str,
    description: &str,
    stack: &super::stack::ProjectStack,
) -> String {
    let (deps, dev_deps): (
        serde_json::Map<String, serde_json::Value>,
        serde_json::Map<String, serde_json::Value>,
    ) = match stack {
        super::stack::ProjectStack::ReactVite => {
            let mut deps = serde_json::Map::new();
            for (name, version) in react_spa_dependencies() {
                deps.insert(
                    name.to_string(),
                    serde_json::Value::String(format!("^{version}")),
                );
            }
            let mut dev_deps = serde_json::Map::new();
            for (name, version) in react_spa_dev_dependencies() {
                dev_deps.insert(
                    name.to_string(),
                    serde_json::Value::String(format!("^{version}")),
                );
            }
            (deps, dev_deps)
        }
        super::stack::ProjectStack::VanillaVite => {
            let deps = serde_json::Map::new();
            let mut dev_deps = serde_json::Map::new();
            for (name, version) in vanilla_dev_dependencies() {
                dev_deps.insert(
                    name.to_string(),
                    serde_json::Value::String(format!("^{version}")),
                );
            }
            (deps, dev_deps)
        }
    };

    let mut scripts = serde_json::Map::new();
    scripts.insert(
        "dev".to_string(),
        serde_json::Value::String("vite".to_string()),
    );
    scripts.insert(
        "build".to_string(),
        serde_json::Value::String("tsc -b && vite build".to_string()),
    );
    scripts.insert(
        "preview".to_string(),
        serde_json::Value::String("vite preview".to_string()),
    );
    scripts.insert(
        "test".to_string(),
        serde_json::Value::String("vitest".to_string()),
    );
    scripts.insert(
        "test:run".to_string(),
        serde_json::Value::String("vitest run".to_string()),
    );
    scripts.insert(
        "lint".to_string(),
        serde_json::Value::String("eslint src".to_string()),
    );
    scripts.insert(
        "type-check".to_string(),
        serde_json::Value::String("tsc --noEmit".to_string()),
    );

    let package_json = serde_json::json!({
        "name": slugify(project_name),
        "version": "1.0.0",
        "private": true,
        "type": "module",
        "description": description,
        "scripts": scripts,
        "dependencies": deps,
        "devDependencies": dev_deps,
        "engines": { "node": ">=20.0.0" }
    });
    serde_json::to_string_pretty(&package_json).unwrap_or_default() + "\n"
}

/// Dev dependencies for a Vite + vanilla TypeScript static site. No UI
/// framework, no React types/plugins.
fn vanilla_dev_dependencies() -> Vec<(&'static str, &'static str)> {
    vec![
        ("typescript", "5.9.3"),
        ("vite", "7.3.6"),
        ("vitest", "3.2.6"),
        ("eslint", "9.39.4"),
        ("typescript-eslint", "8.68.0"),
    ]
}

/// Generate a deterministic root `tsconfig.json` for a Vite + React project.
pub fn generate_tsconfig() -> String {
    let config = serde_json::json!({
        "files": [],
        "references": [
            { "path": "./tsconfig.app.json" },
            { "path": "./tsconfig.node.json" }
        ]
    });
    serde_json::to_string_pretty(&config).unwrap_or_default() + "\n"
}

/// Generate `tsconfig.app.json` — the app-facing compiler options.
pub fn generate_tsconfig_app() -> String {
    let config = serde_json::json!({
        "compilerOptions": {
            "target": "ES2022",
            "useDefineForClassFields": true,
            "lib": ["ES2022", "DOM", "DOM.Iterable"],
            "module": "ESNext",
            "skipLibCheck": true,
            "moduleResolution": "bundler",
            "allowImportingTsExtensions": true,
            "verbatimModuleSyntax": true,
            "resolveJsonModule": true,
            "isolatedModules": true,
            "moduleDetection": "force",
            "noEmit": true,
            "jsx": "react-jsx",
            "strict": true,
            "noUnusedLocals": true,
            "noUnusedParameters": true,
            "erasableSyntaxOnly": true,
            "noFallthroughCasesInSwitch": true,
            "noUncheckedSideEffectImports": true
        },
        "include": ["src"]
    });
    serde_json::to_string_pretty(&config).unwrap_or_default() + "\n"
}

/// Generate `tsconfig.app.json` for a vanilla TypeScript project — same
/// strictness, no JSX compiler option.
pub fn generate_tsconfig_app_vanilla() -> String {
    let config = serde_json::json!({
        "compilerOptions": {
            "target": "ES2022",
            "useDefineForClassFields": true,
            "lib": ["ES2022", "DOM", "DOM.Iterable"],
            "module": "ESNext",
            "skipLibCheck": true,
            "moduleResolution": "bundler",
            "allowImportingTsExtensions": true,
            "verbatimModuleSyntax": true,
            "resolveJsonModule": true,
            "isolatedModules": true,
            "moduleDetection": "force",
            "noEmit": true,
            "strict": true,
            "noUnusedLocals": true,
            "noUnusedParameters": true,
            "erasableSyntaxOnly": true,
            "noFallthroughCasesInSwitch": true,
            "noUncheckedSideEffectImports": true
        },
        "include": ["src"]
    });
    serde_json::to_string_pretty(&config).unwrap_or_default() + "\n"
}

/// Generate `tsconfig.node.json` — the build-tool-facing compiler options.
pub fn generate_tsconfig_node() -> String {
    let config = serde_json::json!({
        "compilerOptions": {
            "target": "ES2023",
            "lib": ["ES2023"],
            "module": "ESNext",
            "skipLibCheck": true,
            "moduleResolution": "bundler",
            "allowImportingTsExtensions": true,
            "verbatimModuleSyntax": true,
            "moduleDetection": "force",
            "noEmit": true,
            "strict": true,
            "noUnusedLocals": true,
            "noUnusedParameters": true,
            "erasableSyntaxOnly": true,
            "noFallthroughCasesInSwitch": true,
            "noUncheckedSideEffectImports": true
        },
        "include": ["vite.config.ts"]
    });
    serde_json::to_string_pretty(&config).unwrap_or_default() + "\n"
}

/// Generate a deterministic `vite.config.ts` for a React + Vitest project.
pub fn generate_vite_config() -> String {
    r#"import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: { '@': './src' },
  },
  server: {
    port: 5173,
    open: true,
  },
  build: {
    outDir: 'dist',
    sourcemap: true,
    target: 'es2020',
  },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
  },
})
"#
    .to_string()
}

/// Generate a deterministic `vite.config.ts` for a vanilla TypeScript
/// project — no framework plugin.
pub fn generate_vite_config_vanilla() -> String {
    r#"import { defineConfig } from 'vitest/config'

export default defineConfig({
  resolve: {
    alias: { '@': './src' },
  },
  server: {
    port: 5173,
    open: true,
  },
  build: {
    outDir: 'dist',
    sourcemap: true,
    target: 'es2020',
  },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
  },
})
"#
    .to_string()
}

/// Generate a deterministic `.gitignore` for a Node/Vite project.
pub fn generate_gitignore() -> String {
    r#"# Dependencies
node_modules/
.pnp
.pnp.js

# Build output
dist/
build/
out/
*.tsbuildinfo

# Environment
.env
.env.local
.env.*.local

# IDE
.vscode/
.idea/
*.swp
*.swo
*~

# OS
.DS_Store
Thumbs.db

# Logs
logs/
*.log
npm-debug.log*

# Coverage
coverage/
.nyc_output/

# Cache
.cache/
tmp/
temp/
"#
    .to_string()
}

// ─── Scaffolder entry point ─────────────────────────────────────────────────

/// Produce deterministic `GeneratedFile`s for all scaffold paths in the given
/// list. Files not in `SCAFFOLD_PATHS` are ignored (the caller partitions first
/// or passes the full list; either way only scaffold files are produced).
///
/// Returns `(scaffold_files, scaffold_paths_set)` — the generated files and
/// the set of paths that were scaffolded, so the engine can exclude them from
/// the provider prompt list.
pub fn scaffold_files(
    paths: &[String],
    project_name: &str,
    description: &str,
) -> Vec<GeneratedFile> {
    let (scaffold_paths, _source) = partition_scaffold_paths(paths);
    // The deterministic generators must match the project's stack: a vanilla
    // goal must not receive React dependencies, the React vite plugin, or a
    // JSX tsconfig. `description` is the pipeline goal.
    let stack = super::stack::ProjectStack::detect(description);
    let mut files = Vec::new();
    for path in &scaffold_paths {
        let content = match path.as_str() {
            "package.json" => generate_package_json_for_stack(project_name, description, &stack),
            "tsconfig.json" => generate_tsconfig(),
            "tsconfig.app.json" => match stack {
                super::stack::ProjectStack::ReactVite => generate_tsconfig_app(),
                super::stack::ProjectStack::VanillaVite => generate_tsconfig_app_vanilla(),
            },
            "tsconfig.node.json" => generate_tsconfig_node(),
            "vite.config.ts" => match stack {
                super::stack::ProjectStack::ReactVite => generate_vite_config(),
                super::stack::ProjectStack::VanillaVite => generate_vite_config_vanilla(),
            },
            ".gitignore" => generate_gitignore(),
            _ => continue,
        };
        files.push(GeneratedFile {
            path: path.clone(),
            content,
            language: Some(language_for_path(path)),
            status: GeneratedFileStatus::Written,
            error: None,
        });
    }
    files
}

fn language_for_path(path: &str) -> String {
    if path.ends_with(".json") {
        "json".to_string()
    } else if path.ends_with(".ts") {
        "typescript".to_string()
    } else if path == ".gitignore" {
        "text".to_string()
    } else {
        "text".to_string()
    }
}

/// Convert a project name into a valid npm package name slug.
fn slugify(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "generated-app".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partition_separates_scaffold_from_source() {
        let paths: Vec<String> = vec![
            "package.json".into(),
            "src/main.tsx".into(),
            "tsconfig.json".into(),
            "src/App.tsx".into(),
            "vite.config.ts".into(),
            "index.html".into(),
        ];
        let (scaffold, source) = partition_scaffold_paths(&paths);
        assert_eq!(scaffold.len(), 3);
        assert!(scaffold.contains(&"package.json".to_string()));
        assert!(scaffold.contains(&"tsconfig.json".to_string()));
        assert!(scaffold.contains(&"vite.config.ts".to_string()));
        assert_eq!(source.len(), 3);
        assert!(source.contains(&"src/main.tsx".to_string()));
        assert!(source.contains(&"index.html".into()));
    }

    #[test]
    fn nested_config_is_not_scaffold() {
        assert!(!is_scaffold_path("packages/ui/tsconfig.json"));
        assert!(!is_scaffold_path("src/lib/vite.config.ts"));
        assert!(!is_scaffold_path("tests/package.json"));
    }

    #[test]
    fn leading_slash_and_dot_slash_normalized() {
        assert!(is_scaffold_path("./package.json"));
        assert!(is_scaffold_path("/package.json"));
        assert!(is_scaffold_path("package.json"));
    }

    #[test]
    fn partition_deduplicates() {
        let paths: Vec<String> = vec![
            "package.json".into(),
            "./package.json".into(),
            "/package.json".into(),
        ];
        let (scaffold, _source) = partition_scaffold_paths(&paths);
        assert_eq!(scaffold.len(), 1);
    }

    #[test]
    fn package_json_is_valid_json_with_pinned_deps() {
        let content = generate_package_json("My Todo App!", "A todo app");
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["name"], "my-todo-app");
        assert_eq!(parsed["private"], true);
        assert_eq!(parsed["type"], "module");
        assert_eq!(parsed["dependencies"]["react"], "^19.2.7");
        assert_eq!(parsed["dependencies"]["react-dom"], "^19.2.7");
        assert_eq!(parsed["devDependencies"]["typescript"], "^5.9.3");
        assert_eq!(parsed["devDependencies"]["vite"], "^7.3.6");
        assert_eq!(parsed["devDependencies"]["@vitejs/plugin-react"], "^5.2.0");
        assert_eq!(parsed["scripts"]["dev"], "vite");
        assert_eq!(parsed["scripts"]["build"], "tsc -b && vite build");
        assert_eq!(parsed["scripts"]["type-check"], "tsc --noEmit");
        assert_eq!(parsed["engines"]["node"], ">=20.0.0");
    }

    #[test]
    fn package_json_slug_fallback() {
        let content = generate_package_json("!!!", "");
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["name"], "generated-app");
    }

    #[test]
    fn tsconfig_json_has_references() {
        let content = generate_tsconfig();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let refs = parsed["references"].as_array().unwrap();
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0]["path"], "./tsconfig.app.json");
        assert_eq!(refs[1]["path"], "./tsconfig.node.json");
    }

    #[test]
    fn tsconfig_app_has_react_jsx_and_strict() {
        let content = generate_tsconfig_app();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["compilerOptions"]["jsx"], "react-jsx");
        assert_eq!(parsed["compilerOptions"]["strict"], true);
        assert_eq!(parsed["compilerOptions"]["moduleResolution"], "bundler");
        assert_eq!(parsed["include"][0], "src");
    }

    #[test]
    fn tsconfig_node_includes_vite_config() {
        let content = generate_tsconfig_node();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["include"][0], "vite.config.ts");
        assert_eq!(parsed["compilerOptions"]["module"], "ESNext");
    }

    #[test]
    fn vite_config_imports_react_plugin() {
        let content = generate_vite_config();
        assert!(content.contains("import { defineConfig } from 'vitest/config'"));
        assert!(content.contains("import react from '@vitejs/plugin-react'"));
        assert!(content.contains("plugins: [react()]"));
        assert!(content.contains("alias: { '@': './src' }"));
        assert!(content.contains("environment: 'jsdom'"));
    }

    #[test]
    fn gitignore_covers_node_modules_and_dist() {
        let content = generate_gitignore();
        assert!(content.contains("node_modules/"));
        assert!(content.contains("dist/"));
        assert!(content.contains(".env"));
        assert!(content.contains("coverage/"));
    }

    #[test]
    fn scaffold_files_produces_all_configs() {
        let paths: Vec<String> = vec![
            "package.json".into(),
            "tsconfig.json".into(),
            "tsconfig.app.json".into(),
            "tsconfig.node.json".into(),
            "vite.config.ts".into(),
            ".gitignore".into(),
            "src/main.tsx".into(),
            "src/App.tsx".into(),
        ];
        let files = scaffold_files(&paths, "test-app", "A test app");
        assert_eq!(files.len(), 6);
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"package.json"));
        assert!(paths.contains(&"tsconfig.json"));
        assert!(paths.contains(&"tsconfig.app.json"));
        assert!(paths.contains(&"tsconfig.node.json"));
        assert!(paths.contains(&"vite.config.ts"));
        assert!(paths.contains(&".gitignore"));
        for file in &files {
            assert_eq!(file.status, GeneratedFileStatus::Written);
            assert!(file.error.is_none());
        }
    }

    #[test]
    fn scaffold_files_with_only_source_returns_empty() {
        let paths: Vec<String> = vec!["src/main.tsx".into(), "src/App.tsx".into()];
        let files = scaffold_files(&paths, "test", "");
        assert!(files.is_empty());
    }

    #[test]
    fn vanilla_package_json_has_no_react_dependencies() {
        let content = generate_package_json_for_stack(
            "Vanilla Site",
            "a vanilla typescript site",
            &super::super::stack::ProjectStack::VanillaVite,
        );
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["name"], "vanilla-site");
        // No React runtime deps, no React types/plugins in devDependencies.
        assert!(parsed["dependencies"].as_object().unwrap().is_empty());
        let dev = parsed["devDependencies"].as_object().unwrap();
        assert!(!dev.contains_key("react"));
        assert!(!dev.contains_key("@types/react"));
        assert!(!dev.contains_key("@vitejs/plugin-react"));
        // Vite toolchain still present.
        assert_eq!(dev["vite"], "^7.3.6");
        assert_eq!(dev["typescript"], "^5.9.3");
    }

    #[test]
    fn vanilla_vite_config_has_no_react_plugin() {
        let content = generate_vite_config_vanilla();
        assert!(!content.contains("@vitejs/plugin-react"));
        assert!(!content.contains("react()"));
        assert!(content.contains("defineConfig"));
    }

    #[test]
    fn vanilla_tsconfig_app_has_no_jsx_option() {
        let content = generate_tsconfig_app_vanilla();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(parsed["compilerOptions"].get("jsx").is_none());
        assert_eq!(parsed["compilerOptions"]["strict"], true);
    }

    #[test]
    fn scaffold_files_matches_vanilla_stack_from_goal() {
        let paths: Vec<String> = vec![
            "package.json".into(),
            "tsconfig.app.json".into(),
            "vite.config.ts".into(),
        ];
        let files = scaffold_files(&paths, "vanilla-app", "build a vanilla typescript site");
        let by_path = |path: &str| {
            files
                .iter()
                .find(|f| f.path == path)
                .unwrap_or_else(|| panic!("{path} missing"))
                .content
                .clone()
        };
        let pkg: serde_json::Value = serde_json::from_str(&by_path("package.json")).unwrap();
        assert!(pkg["dependencies"].as_object().unwrap().is_empty());
        let tsconfig: serde_json::Value =
            serde_json::from_str(&by_path("tsconfig.app.json")).unwrap();
        assert!(tsconfig["compilerOptions"].get("jsx").is_none());
        assert!(!by_path("vite.config.ts").contains("@vitejs/plugin-react"));
    }

    #[test]
    fn scaffold_files_keeps_react_defaults_for_ordinary_goals() {
        let paths: Vec<String> = vec!["package.json".into()];
        let files = scaffold_files(&paths, "react-app", "build a landing page");
        let pkg: serde_json::Value = serde_json::from_str(&files[0].content).unwrap();
        assert_eq!(pkg["dependencies"]["react"], "^19.2.7");
    }

    #[test]
    fn scaffold_package_json_content_is_valid() {
        let files = scaffold_files(&["package.json".into()], "my-app", "desc");
        assert_eq!(files.len(), 1);
        let parsed: serde_json::Value = serde_json::from_str(&files[0].content).unwrap();
        assert_eq!(parsed["name"], "my-app");
    }
}
