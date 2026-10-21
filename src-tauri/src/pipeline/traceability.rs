//! Requirements Traceability Service (ported from devcode RequirementsTraceabilityService.ts)
//!
//! Tracks requirements through the build pipeline, from initial planning
//! through implementation to verification. Ensures every requirement has
//! corresponding implementation and test coverage.
//! Pure module — no stores, no DI, no I/O.

#![allow(dead_code)]  // ROADMAP parity port: kept for planned wiring; prune when the Q-item closes.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Requirement {
    pub id: String,
    pub description: String,
    pub priority: String, // 'high' | 'medium' | 'low'
    pub acceptance_criteria: Vec<String>,
    pub status: String, // 'planned' | 'implemented' | 'tested' | 'verified' | 'failed'
    pub implementation_files: Vec<String>,
    pub test_files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_result: Option<String>, // 'passed' | 'failed' | 'skipped'
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceabilityInput {
    pub requirements: Vec<TraceabilityRequirement>,
    pub generated_files: Vec<TraceabilityFile>,
    pub test_files: Vec<TraceabilityFile>,
    #[serde(default)]
    pub verification_results: BTreeMap<String, TraceabilityVerification>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceabilityRequirement {
    pub id: String,
    pub description: String,
    pub priority: String,
    pub acceptance_criteria: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceabilityFile {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceabilityVerification {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TraceabilityReport {
    pub total_requirements: usize,
    pub implemented: usize,
    pub tested: usize,
    pub verified: usize,
    pub failed: usize,
    pub coverage: f32,
    pub requirements: Vec<Requirement>,
    pub gaps: Vec<String>,
}

pub struct RequirementsTraceabilityService {
    requirements: BTreeMap<String, Requirement>,
}

impl RequirementsTraceabilityService {
    pub fn new() -> Self {
        Self { requirements: BTreeMap::new() }
    }

    pub fn track(&mut self, input: &TraceabilityInput) -> TraceabilityReport {
        let mut requirements = Vec::new();
        let mut gaps = Vec::new();

        for req in &input.requirements {
            let mut requirement = Requirement {
                id: req.id.clone(),
                description: req.description.clone(),
                priority: req.priority.clone(),
                acceptance_criteria: req.acceptance_criteria.clone(),
                status: "planned".into(),
                implementation_files: Vec::new(),
                test_files: Vec::new(),
                verification_result: None,
            };

            // Find implementation files
            for file in &input.generated_files {
                if matches_requirement(&file.path, &req.description) {
                    requirement.implementation_files.push(file.path.clone());
                }
            }

            // Find test files
            for file in &input.test_files {
                if matches_requirement(&file.path, &req.description) {
                    requirement.test_files.push(file.path.clone());
                }
            }

            // Update status based on findings
            if !requirement.implementation_files.is_empty() {
                requirement.status = "implemented".into();
            }
            if !requirement.test_files.is_empty() {
                requirement.status = "tested".into();
            }
            if let Some(verification) = input.verification_results.get(&req.id) {
                if verification.status == "passed" {
                    requirement.verification_result = Some("passed".into());
                    requirement.status = "verified".into();
                } else {
                    requirement.verification_result = Some("failed".into());
                    requirement.status = "failed".into();
                }
            }

            // Track gaps
            if requirement.implementation_files.is_empty() {
                gaps.push(format!("Requirement \"{}\" has no implementation files.", req.id));
            }
            if requirement.test_files.is_empty() && requirement.status == "implemented" {
                gaps.push(format!("Requirement \"{}\" has implementation but no tests.", req.id));
            }

            self.requirements.insert(req.id.clone(), requirement.clone());
            requirements.push(requirement);
        }

        let implemented = requirements.iter()
            .filter(|r| r.status == "implemented" || r.status == "tested" || r.status == "verified")
            .count();
        let tested = requirements.iter()
            .filter(|r| r.status == "tested" || r.status == "verified")
            .count();
        let verified = requirements.iter()
            .filter(|r| r.status == "verified")
            .count();
        let failed = requirements.iter()
            .filter(|r| r.status == "failed")
            .count();
        let coverage = if requirements.is_empty() { 0.0 } else { implemented as f32 / requirements.len() as f32 };

        TraceabilityReport {
            total_requirements: requirements.len(),
            implemented,
            tested,
            verified,
            failed,
            coverage,
            requirements,
            gaps,
        }
    }

    pub fn get_requirement(&self, id: &str) -> Option<&Requirement> {
        self.requirements.get(id)
    }

    pub fn get_all_requirements(&self) -> Vec<Requirement> {
        self.requirements.values().cloned().collect()
    }

    pub fn clear(&mut self) {
        self.requirements.clear();
    }
}

/// Check if a file path matches a requirement by keyword overlap.
fn matches_requirement(file_path: &str, requirement_description: &str) -> bool {
    let desc_lower = requirement_description.to_lowercase();
    let keywords: Vec<&str> = desc_lower
        .split_whitespace()
        .filter(|w| w.len() > 3)
        .collect();

    let file_lower = file_path.to_lowercase();
    keywords.iter().any(|kw| file_lower.contains(kw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_finds_implementation_files() {
        let mut service = RequirementsTraceabilityService::new();
        let input = TraceabilityInput {
            requirements: vec![TraceabilityRequirement {
                id: "req-1".into(),
                description: "user authentication component".into(),
                priority: "high".into(),
                acceptance_criteria: vec![],
            }],
            generated_files: vec![
                TraceabilityFile { path: "src/components/auth/AuthComponent.tsx".into() },
                TraceabilityFile { path: "src/App.tsx".into() },
            ],
            test_files: vec![],
            verification_results: BTreeMap::new(),
        };
        let report = service.track(&input);
        assert_eq!(report.total_requirements, 1);
        assert_eq!(report.implemented, 1);
        assert!(report.coverage > 0.0);
        assert_eq!(report.requirements[0].implementation_files.len(), 1);
    }

    #[test]
    fn test_track_identifies_gaps() {
        let mut service = RequirementsTraceabilityService::new();
        let input = TraceabilityInput {
            requirements: vec![TraceabilityRequirement {
                id: "req-1".into(),
                description: "database migration".into(),
                priority: "high".into(),
                acceptance_criteria: vec![],
            }],
            generated_files: vec![],
            test_files: vec![],
            verification_results: BTreeMap::new(),
        };
        let report = service.track(&input);
        assert_eq!(report.implemented, 0);
        assert!(!report.gaps.is_empty());
        assert!(report.gaps.iter().any(|g| g.contains("no implementation files")));
    }

    #[test]
    fn test_track_with_verification() {
        let mut service = RequirementsTraceabilityService::new();
        let mut verification = BTreeMap::new();
        verification.insert("req-1".into(), TraceabilityVerification { status: "passed".into() });
        let input = TraceabilityInput {
            requirements: vec![TraceabilityRequirement {
                id: "req-1".into(),
                description: "authentication component".into(),
                priority: "high".into(),
                acceptance_criteria: vec![],
            }],
            generated_files: vec![TraceabilityFile { path: "src/auth/AuthComponent.tsx".into() }],
            test_files: vec![TraceabilityFile { path: "src/auth/AuthComponent.test.ts".into() }],
            verification_results: verification,
        };
        let report = service.track(&input);
        assert_eq!(report.verified, 1);
        assert_eq!(report.requirements[0].status, "verified");
    }
}
