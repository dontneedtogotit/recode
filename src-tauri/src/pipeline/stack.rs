//! Goal-driven project stack selection (ported in spirit from devcode's
//! `ProjectTemplateRegistry`, bounded to the stacks recode can actually
//! scaffold deterministically).
//!
//! The manifest must not be hard-wired to React: a goal asking for a
//! static/vanilla TypeScript site gets the vanilla Vite file set instead of
//! `src/main.tsx` + `src/App.tsx`. Detection is keyword-based and
//! conservative — anything unrecognized falls back to the React SPA stack,
//! which is the pipeline's default and the one the deterministic scaffold
//! generators were originally built for.

use crate::pipeline::manifest::{VANILLA_PROJECT_FILES, VITE_PROJECT_FILES};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectStack {
    /// Vite + React + TypeScript SPA (default; matches the pinned
    /// curated-packages versions in `scaffolder.rs`).
    ReactVite,
    /// Vite + vanilla TypeScript static site — no UI framework.
    VanillaVite,
}

impl Default for ProjectStack {
    fn default() -> Self {
        Self::ReactVite
    }
}

impl ProjectStack {
    /// Conservative keyword detection. Goals that never mention a stack (or
    /// mention React) keep the React default; an explicit vanilla/static ask
    /// opts into the vanilla file set.
    pub fn detect(goal: &str) -> Self {
        let goal = goal.to_lowercase();
        const VANILLA_MARKERS: &[&str] = &[
            "vanilla",
            "no framework",
            "without a framework",
            "plain typescript",
            "static site",
        ];
        if VANILLA_MARKERS.iter().any(|marker| goal.contains(marker)) {
            Self::VanillaVite
        } else {
            Self::ReactVite
        }
    }

    /// The manifest file tuples for this stack:
    /// `(path, language, purpose, wave, max_bytes, expected_loc)`.
    pub fn project_files(
        &self,
    ) -> &'static [(&'static str, &'static str, &'static str, u8, usize, usize)] {
        match self {
            Self::ReactVite => VITE_PROJECT_FILES,
            Self::VanillaVite => VANILLA_PROJECT_FILES,
        }
    }

    /// Entry module path whose generated content imports the app module.
    pub fn entry_source_path(&self) -> &'static str {
        match self {
            Self::ReactVite => "src/main.tsx",
            Self::VanillaVite => "src/main.ts",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn react_is_the_default_for_ordinary_goals() {
        assert_eq!(
            ProjectStack::detect("build a landing page"),
            ProjectStack::ReactVite
        );
        assert_eq!(
            ProjectStack::detect("create a react dashboard"),
            ProjectStack::ReactVite
        );
        assert_eq!(ProjectStack::detect(""), ProjectStack::ReactVite);
    }

    #[test]
    fn vanilla_markers_select_the_vanilla_stack() {
        for goal in [
            "build a vanilla typescript site",
            "static site with a pricing page",
            "no framework, just dom manipulation",
            "plain typescript app",
        ] {
            assert_eq!(
                ProjectStack::detect(goal),
                ProjectStack::VanillaVite,
                "goal should select the vanilla stack: {goal}"
            );
        }
    }

    #[test]
    fn detection_is_case_insensitive() {
        assert_eq!(
            ProjectStack::detect("Build a VANILLA TypeScript site"),
            ProjectStack::VanillaVite
        );
    }

    #[test]
    fn each_stack_has_a_distinct_file_set() {
        let react = ProjectStack::ReactVite.project_files();
        let vanilla = ProjectStack::VanillaVite.project_files();
        assert!(react.iter().any(|&(path, ..)| path == "src/main.tsx"));
        assert!(!react.iter().any(|&(path, ..)| path == "src/main.ts"));
        assert!(vanilla.iter().any(|&(path, ..)| path == "src/main.ts"));
        assert!(!vanilla.iter().any(|&(path, ..)| path == "src/main.tsx"));
    }

    #[test]
    fn entry_paths_match_the_stack_file_set() {
        for stack in [ProjectStack::ReactVite, ProjectStack::VanillaVite] {
            let files = stack.project_files();
            assert!(
                files
                    .iter()
                    .any(|&(path, ..)| path == stack.entry_source_path()),
                "entry path {} must exist in the {:?} file set",
                stack.entry_source_path(),
                stack
            );
        }
    }
}
