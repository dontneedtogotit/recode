//! Mental Execution Simulator (ported from devcode MentalExecutionSimulator.ts).
//!
//! Pre-execution symbolic dry-run: validates that a generated module set has
//! no circular dependencies and that generated test files reference only
//! symbols the subject modules actually export. Zero-cost simulation before
//! expensive generation/repair loops. No stores, no DI, no I/O.

use std::collections::{BTreeMap, BTreeSet};

/// Minimal contract input for simulation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SimulatedModule {
    /// Module file path, e.g. `src/utils/format.ts`.
    pub path: String,
    /// Source code of the module (not the test).
    pub source_code: String,
    /// Test code for the module, if any.
    pub test_code: Option<String>,
    /// Paths of modules this module imports (dependency edges).
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SimulationMetrics {
    pub total_modules_simulated: usize,
    pub total_assertions_checked: usize,
    pub cycles_detected: usize,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SimulationResult {
    pub is_valid: bool,
    pub graph_valid: bool,
    pub test_baseline_valid: bool,
    pub type_conformance_valid: bool,
    pub cycles: Vec<Vec<String>>,
    pub diagnostics: Vec<String>,
    pub metrics: SimulationMetrics,
}

/// Test-framework globals that never count as conformance violations.
const TEST_FRAMEWORK_KEYWORDS: [&str; 9] = [
    "describe", "it", "test", "expect", "vi", "beforeEach", "afterEach", "beforeAll", "afterAll",
];

/// Run the mental simulation over a module set.
pub fn simulate(modules: &[SimulatedModule]) -> SimulationResult {
    let mut diagnostics: Vec<String> = Vec::new();
    let mut cycles = Vec::new();
    let mut total_assertions = 0usize;

    // 1. Dependency graph cycle detection (DFS with recursion stack).
    let mut adjacency: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for module in modules {
        adjacency.insert(module.path.clone(), module.dependencies.clone());
    }
    let mut visited: BTreeSet<String> = BTreeSet::new();
    let mut rec_stack: BTreeSet<String> = BTreeSet::new();
    let mut current_path: Vec<String> = Vec::new();
    for module in modules {
        let path = module.path.clone();
        if !visited.contains(&path) {
            dfs_cycle(
                &path,
                &adjacency,
                &mut visited,
                &mut rec_stack,
                &mut current_path,
                &mut cycles,
            );
        }
    }
    let graph_valid = cycles.is_empty();
    if !graph_valid {
        for cycle in &cycles {
            diagnostics.push(format!("Circular dependency detected: {}", cycle.join(" -> ")));
        }
    }

    // 2. Test specification validation + cross-symbol conformance.
    let mut test_baseline_valid = true;
    let mut type_conformance_valid = true;
    for module in modules {
        let test_code = match &module.test_code {
            Some(code) if !code.trim().is_empty() => code,
            _ => continue, // no test spec: not a violation in recode's model
        };

        // Valid red baseline: test must contain assertions and test cases.
        let assertions = count_occurrences(test_code, "expect(");
        let describe_blocks = count_occurrences(test_code, "describe(");
        let it_blocks = count_occurrences(test_code, "it(") + count_occurrences(test_code, "test(");
        total_assertions += assertions;
        if assertions == 0 || describe_blocks == 0 || it_blocks == 0 {
            test_baseline_valid = false;
            diagnostics.push(format!(
                "Test specification for '{}' contains no valid assertions or test cases.",
                module.path
            ));
        }

        // Conformance: symbols imported from the subject module must be exported.
        let exported = extract_exported_symbols(&module.source_code);
        let imported = extract_imported_symbols(test_code);
        for symbol in imported {
            if !exported.contains(&symbol) {
                type_conformance_valid = false;
                diagnostics.push(format!(
                    "Symbol '{}' in test specification was not found in exported signatures or types for '{}'.",
                    symbol, module.path
                ));
            }
        }
    }

    let is_valid = graph_valid && test_baseline_valid && type_conformance_valid;
    SimulationResult {
        is_valid,
        graph_valid,
        test_baseline_valid,
        type_conformance_valid,
        cycles,
        diagnostics,
        metrics: SimulationMetrics {
            total_modules_simulated: modules.len(),
            total_assertions_checked: total_assertions,
            cycles_detected: 0,
        },
    }
}

fn dfs_cycle(
    node: &str,
    adjacency: &BTreeMap<String, Vec<String>>,
    visited: &mut BTreeSet<String>,
    rec_stack: &mut BTreeSet<String>,
    current_path: &mut Vec<String>,
    cycles: &mut Vec<Vec<String>>,
) -> bool {
    visited.insert(node.to_string());
    rec_stack.insert(node.to_string());
    current_path.push(node.to_string());

    if let Some(neighbors) = adjacency.get(node) {
        for neighbor in neighbors {
            if !visited.contains(neighbor) {
                if dfs_cycle(neighbor, adjacency, visited, rec_stack, current_path, cycles) {
                    return true;
                }
            } else if rec_stack.contains(neighbor) {
                if let Some(start) = current_path.iter().position(|n| n == neighbor) {
                    let mut cycle = current_path[start..].to_vec();
                    cycle.push(neighbor.clone());
                    cycles.push(cycle);
                }
                return true;
            }
        }
    }

    current_path.pop();
    rec_stack.remove(node);
    false
}

fn count_occurrences(haystack: &str, needle: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }
    let mut count = 0;
    let mut rest = haystack;
    while let Some(pos) = rest.find(needle) {
        count += 1;
        rest = &rest[pos + needle.len()..];
    }
    count
}

/// Extract exported symbol names from source code (function/class/const/type/interface).
fn extract_exported_symbols(source: &str) -> BTreeSet<String> {
    let mut symbols = BTreeSet::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("export ") {
            continue;
        }
        // export function foo / export const foo / export type Foo / export interface Foo
        let rest = trimmed
            .strip_prefix("export ")
            .and_then(|r| r.strip_prefix("async "))
            .unwrap_or(trimmed);
        let after = rest.strip_prefix("export ").unwrap_or(rest);
        if let Some(rest) = after.strip_prefix("function ") {
            push_identifier(&mut symbols, rest);
        } else if let Some(rest) = after.strip_prefix("class ") {
            push_identifier(&mut symbols, rest);
        } else if let Some(rest) = after.strip_prefix("const ") {
            push_identifier(&mut symbols, rest);
        } else if let Some(rest) = after.strip_prefix("let ") {
            push_identifier(&mut symbols, rest);
        } else if let Some(rest) = after.strip_prefix("var ") {
            push_identifier(&mut symbols, rest);
        } else if let Some(rest) = after.strip_prefix("type ") {
            push_identifier(&mut symbols, rest);
        } else if let Some(rest) = after.strip_prefix("interface ") {
            push_identifier(&mut symbols, rest);
        } else if let Some(rest) = after.strip_prefix("default ") {
            push_identifier(&mut symbols, rest);
        }
    }
    symbols
}

fn push_identifier(out: &mut BTreeSet<String>, rest: &str) {
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
        .collect();
    if !name.is_empty() {
        out.insert(name);
    }
}

/// Extract symbols imported in test code via `import { X, Y } from '...'`.
fn extract_imported_symbols(test_code: &str) -> Vec<String> {
    let mut symbols = Vec::new();
    for line in test_code.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("import ") || !trimmed.contains('{') || !trimmed.contains('}') {
            continue;
        }
        let start = match trimmed.find('{') {
            Some(pos) => pos + 1,
            None => continue,
        };
        let end = match trimmed.find('}') {
            Some(pos) => pos,
            None => continue,
        };
        if end <= start {
            continue;
        }
        for name in trimmed[start..end].split(',') {
            let name = name.trim();
            // handle `foo as bar` — the local name is what's referenced
            let local = name.split_whitespace().next().unwrap_or("");
            if !local.is_empty() && !TEST_FRAMEWORK_KEYWORDS.contains(&local) {
                symbols.push(local.to_string());
            }
        }
    }
    symbols
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(path: &str, source: &str, test: Option<&str>, deps: &[&str]) -> SimulatedModule {
        SimulatedModule {
            path: path.to_string(),
            source_code: source.to_string(),
            test_code: test.map(|t| t.to_string()),
            dependencies: deps.iter().map(|d| d.to_string()).collect(),
        }
    }

    #[test]
    fn test_valid_modules_pass() {
        let modules = vec![
            module(
                "src/utils/format.ts",
                "export function format(x: string): string {\n  return x;\n}",
                Some("import { format } from './format';\ndescribe('format', () => {\n  it('works', () => {\n    expect(format('a')).toBe('a');\n  });\n});"),
                &[],
            ),
        ];
        let result = simulate(&modules);
        assert!(result.is_valid, "{:?}", result.diagnostics);
        assert!(result.graph_valid);
        assert!(result.test_baseline_valid);
        assert!(result.type_conformance_valid);
    }

    #[test]
    fn test_circular_dependency_detected() {
        let modules = vec![
            module("src/a.ts", "export const a = 1;", None, &["src/b.ts"]),
            module("src/b.ts", "export const b = 1;", None, &["src/a.ts"]),
        ];
        let result = simulate(&modules);
        assert!(!result.graph_valid);
        assert!(!result.is_valid);
        assert_eq!(result.cycles.len(), 1);
        assert_eq!(result.cycles[0], vec!["src/a.ts", "src/b.ts", "src/a.ts"]);
        assert!(result.diagnostics.iter().any(|d| d.contains("Circular dependency")));
    }

    #[test]
    fn test_missing_assertions_flagged() {
        let modules = vec![
            module(
                "src/x.ts",
                "export const x = 1;",
                Some("describe('x', () => {\n  it('exists', () => {\n    console.log('no assertion');\n  });\n});"),
                &[],
            ),
        ];
        let result = simulate(&modules);
        assert!(!result.test_baseline_valid);
        assert!(result.diagnostics.iter().any(|d| d.contains("no valid assertions")));
    }

    #[test]
    fn test_conformance_violation_detected() {
        let modules = vec![
            module(
                "src/y.ts",
                "export const y = 1;",
                Some("import { y, nonexistent } from './y';\ndescribe('y', () => {\n  it('works', () => {\n    expect(y).toBe(1);\n  });\n});"),
                &[],
            ),
        ];
        let result = simulate(&modules);
        assert!(!result.type_conformance_valid);
        assert!(result.diagnostics.iter().any(|d| d.contains("nonexistent")));
    }

    #[test]
    fn test_framework_imports_not_flagged() {
        let modules = vec![
            module(
                "src/z.ts",
                "export const z = 1;",
                Some("import { describe, it, expect, vi, z } from './z';\ndescribe('z', () => {\n  it('works', () => {\n    expect(z).toBe(1);\n  });\n});"),
                &[],
            ),
        ];
        let result = simulate(&modules);
        assert!(result.type_conformance_valid, "{:?}", result.diagnostics);
    }

    #[test]
    fn test_no_test_spec_is_not_violation() {
        let modules = vec![module("src/w.ts", "export const w = 1;", None, &[])];
        let result = simulate(&modules);
        assert!(result.is_valid);
    }

    #[test]
    fn test_self_cycle() {
        let modules = vec![module("src/self.ts", "export const s = 1;", None, &["src/self.ts"])];
        let result = simulate(&modules);
        assert!(!result.graph_valid);
        assert_eq!(result.cycles.len(), 1);
        assert_eq!(result.cycles[0], vec!["src/self.ts", "src/self.ts"]);
    }

    #[test]
    fn test_metrics_populated() {
        let modules = vec![
            module("src/m.ts", "export const m = 1;", Some("import { m } from './m';\ndescribe('m', () => {\n  it('a', () => {\n    expect(m).toBe(1);\n    expect(m).toBe(2);\n  });\n});"), &[]),
        ];
        let result = simulate(&modules);
        assert_eq!(result.metrics.total_modules_simulated, 1);
        assert_eq!(result.metrics.total_assertions_checked, 2);
    }
}
