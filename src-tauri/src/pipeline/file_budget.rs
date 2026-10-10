//! Auxiliary File Budget (ported from devcode AuxiliaryFileBudget.ts)
//!
//! Bounds auxiliary files (tests, stubs, router repairs) against the plan.
//! Planned files and deterministic scaffold/manifest files are never dropped.
//! Pure module — no stores, no DI, no I/O.

use std::collections::HashSet;

/// Minimum auxiliary files allowed even if the plan is small.
const MIN_AUXILIARY_FILES: usize = 3;
/// Ratio of auxiliary files to planned files.
const AUXILIARY_FILE_RATIO: f64 = 0.25;

/// Pattern matching required manifest/scaffold files that must never be dropped.
/// Matches: package.json, tsconfig*.json, vite.config.*, vitest.config.*,
/// eslint*.json, .eslintrc*, postcss.config.*, tailwind.config.*,
/// prettier.config.*, index.html, .gitignore, README.md, public/
fn is_required_manifest(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let filename = lower.rsplit('/').next().unwrap_or(&lower);
    matches!(filename,
        "package.json" | "package-lock.json"
        | "tsconfig.json" | "tsconfig.app.json" | "tsconfig.node.json"
        | "vite.config.ts" | "vite.config.js" | "vite.config.mts" | "vite.config.mjs"
        | "vitest.config.ts" | "vitest.config.js" | "vitest.config.mts" | "vitest.config.mjs"
        | "index.html" | ".gitignore" | "readme.md"
    ) || filename.starts_with("tsconfig.")
    || filename.starts_with(".eslintrc")
    || filename.starts_with("eslint.")
    || filename.starts_with("postcss.config.")
    || filename.starts_with("tailwind.config.")
    || filename.starts_with("prettier.config.")
    || lower.starts_with("public/")
}

/// Enforce the auxiliary file budget: drop non-planned, non-manifest files
/// that exceed the slack. Returns the filtered file list and the number dropped.
pub fn enforce_auxiliary_file_budget(
    generated_paths: Vec<String>,
    planned_paths: &HashSet<String>,
) -> (Vec<String>, usize) {
    let planned_count = planned_paths.len();
    if planned_count == 0 {
        return (generated_paths, 0);
    }

    let auxiliary_budget = std::cmp::max(
        MIN_AUXILIARY_FILES,
        (planned_count as f64 * AUXILIARY_FILE_RATIO).ceil() as usize,
    );

    // Collect auxiliary paths (non-planned, non-manifest)
    let auxiliary_paths: Vec<&String> = generated_paths
        .iter()
        .filter(|p| !planned_paths.contains(*p) && !is_required_manifest(p))
        .collect();

    if auxiliary_paths.len() <= auxiliary_budget {
        return (generated_paths, 0);
    }

    // Mark the excess auxiliary paths for dropping
    let excess: HashSet<String> = auxiliary_paths[auxiliary_budget..]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let dropped = excess.len();

    let filtered: Vec<String> = generated_paths
        .into_iter()
        .filter(|p| planned_paths.contains(p) || is_required_manifest(p) || !excess.contains(p))
        .collect();

    (filtered, dropped)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn planned() -> HashSet<String> {
        ["src/main.tsx", "src/App.tsx", "src/App.css"]
            .iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn planned_and_manifest_files_are_never_dropped() {
        let planned = planned();
        let files: Vec<String> = vec![
            "src/main.tsx".into(), "src/App.tsx".into(), "src/App.css".into(),
            "package.json".into(), "index.html".into(),
        ];
        let (kept, dropped) = enforce_auxiliary_file_budget(files, &planned);
        assert_eq!(dropped, 0);
        assert_eq!(kept.len(), 5);
    }

    #[test]
    fn excess_auxiliary_files_are_dropped() {
        let planned = planned();
        let mut files: Vec<String> = planned.iter().cloned().collect();
        // Add many auxiliary files beyond the budget (3 planned * 0.25 = 1, max(3, 1) = 3)
        for i in 0..10 {
            files.push(format!("src/test/helper{i}.ts"));
        }
        let (kept, dropped) = enforce_auxiliary_file_budget(files, &planned);
        assert!(dropped > 0);
        // All planned files survive
        for p in &planned {
            assert!(kept.contains(p));
        }
        // At most 3 auxiliary files survive
        let aux_surviving: usize = kept.iter()
            .filter(|p| !planned.contains(*p) && !is_required_manifest(p))
            .count();
        assert!(aux_surviving <= 3);
    }

    #[test]
    fn empty_plan_skips_enforcement() {
        let planned = HashSet::new();
        let files: Vec<String> = vec!["a.ts".into(), "b.ts".into()];
        let (kept, dropped) = enforce_auxiliary_file_budget(files, &planned);
        assert_eq!(dropped, 0);
        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn manifest_files_recognized() {
        assert!(is_required_manifest("package.json"));
        assert!(is_required_manifest("tsconfig.json"));
        assert!(is_required_manifest("tsconfig.app.json"));
        assert!(is_required_manifest("vite.config.ts"));
        assert!(is_required_manifest("index.html"));
        assert!(is_required_manifest(".gitignore"));
        assert!(is_required_manifest("README.md"));
        assert!(is_required_manifest("public/favicon.ico"));
        assert!(!is_required_manifest("src/App.tsx"));
        assert!(!is_required_manifest("src/test/helper.ts"));
    }
}
