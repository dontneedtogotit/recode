//! Verification Policy Service (ported from devcode VerificationPolicyService.ts)
//!
//! Maps a build verification policy to the set of required gates and their
//! enforcement rules. Single source of truth for what "verified" means at each policy level.

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuildVerificationPolicy {
    PreviewOnly,
    Standard,
    Strict,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VerificationPolicyRule {
    pub policy: BuildVerificationPolicy,
    pub label: String,
    pub description: String,
    pub required_gates: Vec<String>,
    pub skip_degrades: bool,
    pub preview_available: bool,
}

pub fn get_verification_policy_rule(policy: BuildVerificationPolicy) -> VerificationPolicyRule {
    match policy {
        BuildVerificationPolicy::PreviewOnly => VerificationPolicyRule {
            policy: BuildVerificationPolicy::PreviewOnly,
            label: "Preview available".into(),
            description: "Only compile and boot are verified. The app is previewable but not fully verified.".into(),
            required_gates: vec!["compile".into(), "boot".into()],
            skip_degrades: true,
            preview_available: true,
        },
        BuildVerificationPolicy::Standard => VerificationPolicyRule {
            policy: BuildVerificationPolicy::Standard,
            label: "Standard verification".into(),
            description: "Compile, boot, test, lint, and browser smoke are verified. The app is fully verified when all pass.".into(),
            required_gates: vec!["compile".into(), "boot".into(), "test".into(), "lint".into(), "browser".into()],
            skip_degrades: true,
            preview_available: false,
        },
        BuildVerificationPolicy::Strict => VerificationPolicyRule {
            policy: BuildVerificationPolicy::Strict,
            label: "Strict verification".into(),
            description: "All gates including persistence are verified. The app is fully verified only when every check passes.".into(),
            required_gates: vec!["compile".into(), "boot".into(), "test".into(), "lint".into(), "browser".into(), "persistence".into()],
            skip_degrades: false,
            preview_available: false,
        },
    }
}

pub fn is_gate_required(policy: BuildVerificationPolicy, gate_id: &str) -> bool {
    get_verification_policy_rule(policy).required_gates.iter().any(|g| g == gate_id)
}

pub fn does_skip_degrade(policy: BuildVerificationPolicy) -> bool {
    get_verification_policy_rule(policy).skip_degrades
}

pub fn is_preview_available(policy: BuildVerificationPolicy) -> bool {
    get_verification_policy_rule(policy).preview_available
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectRiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VerificationPolicyInput {
    pub risk_level: Option<ProjectRiskLevel>,
    pub has_tests: Option<bool>,
    pub is_production: Option<bool>,
    pub is_preview: Option<bool>,
}

pub fn determine_verification_policy(input: &VerificationPolicyInput) -> BuildVerificationPolicy {
    let risk_level = input.risk_level.unwrap_or(ProjectRiskLevel::Medium);
    let has_tests = input.has_tests.unwrap_or(true);
    let is_production = input.is_production.unwrap_or(false);
    let is_preview = input.is_preview.unwrap_or(false);

    if is_preview {
        return BuildVerificationPolicy::PreviewOnly;
    }
    if is_production || matches!(risk_level, ProjectRiskLevel::Critical | ProjectRiskLevel::High) {
        return BuildVerificationPolicy::Strict;
    }
    if matches!(risk_level, ProjectRiskLevel::Low) && !has_tests {
        return BuildVerificationPolicy::PreviewOnly;
    }
    BuildVerificationPolicy::Standard
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_selection() {
        assert_eq!(
            determine_verification_policy(&VerificationPolicyInput {
                risk_level: None,
                has_tests: None,
                is_production: Some(true),
                is_preview: None,
            }),
            BuildVerificationPolicy::Strict
        );
        assert_eq!(
            determine_verification_policy(&VerificationPolicyInput {
                risk_level: Some(ProjectRiskLevel::Low),
                has_tests: Some(false),
                is_production: None,
                is_preview: None,
            }),
            BuildVerificationPolicy::PreviewOnly
        );
        assert_eq!(
            determine_verification_policy(&VerificationPolicyInput {
                risk_level: None,
                has_tests: None,
                is_production: None,
                is_preview: Some(true),
            }),
            BuildVerificationPolicy::PreviewOnly
        );
    }
}
