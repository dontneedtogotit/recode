//! Native all-pass policy for the initial generated web-app workflow.
//! This is not a subprocess runner or an authorization boundary. Only native
//! verifiers may produce receipts; serialized history is not fresh evidence.
use super::verify::{GateStatus, VerificationResult};

pub const REQUIRED_GATES: &[&str] = &["persistence", "typecheck", "bundle", "tests", "startup"];

#[derive(Debug, PartialEq, Eq)]
pub enum GateDecision {
    Satisfied,
    Failed,
    Blocked,
}

pub struct GateAssessment {
    pub decision: GateDecision,
    pub blockers: Vec<String>,
}

pub fn evaluate_required_gates(results: &[VerificationResult]) -> GateAssessment {
    let mut blockers = Vec::new();
    let mut failed = false;
    // Preserve any explicitly reported failure, even outside the required set.
    for result in results {
        let known_failure = match &result.evidence {
            Some(evidence) => evidence.status == GateStatus::Failed
                || evidence.execution.as_ref().is_some_and(|execution| execution.exit_code != 0)
                || (evidence.status == GateStatus::Passed && !result.passed),
            None => !result.passed,
        };
        if known_failure {
            failed = true;
            blockers.push(format!("{}: failed", result.gate));
        }
        if result.evidence.as_ref().is_some_and(|evidence| evidence.required)
            && !REQUIRED_GATES.contains(&result.gate.as_str())
        {
            blockers.push(format!("{}: unsupported required gate", result.gate));
        }
    }
    for gate in REQUIRED_GATES {
        let matches: Vec<_> = results.iter().filter(|result| result.gate == *gate).collect();
        if matches.len() != 1 {
            blockers.push(format!("{gate}: missing or duplicate evidence"));
            continue;
        }
        let result = matches[0];
        let Some(evidence) = &result.evidence else {
            blockers.push(format!("{gate}: legacy evidence lacks applicability and execution metadata"));
            continue;
        };
        // Result-controlled flags cannot waive native requirements. Waivers need
        // a future reviewed plan policy, not `required: false` in a gate record.
        if evidence.schema_version != 1 || !evidence.required || evidence.applicable != Some(true) {
            blockers.push(format!("{gate}: unsupported version or unresolved applicability"));
            continue;
        }
        if evidence.status != GateStatus::Passed || !result.passed || result.degraded {
            blockers.push(format!("{gate}: not an unqualified pass"));
            continue;
        }
        if *gate != "persistence" {
            let has_receipt = evidence.execution.as_ref().is_some_and(|execution| {
                execution.exit_code == 0
                    && execution.command.first().is_some_and(|command| !command.trim().is_empty())
                    && !execution.tool_version.trim().is_empty()
            });
            if !has_receipt {
                blockers.push(format!("{gate}: executed command receipt missing"));
            }
        }
    }
    let decision = if failed {
        GateDecision::Failed
    } else if blockers.is_empty() {
        GateDecision::Satisfied
    } else {
        GateDecision::Blocked
    };
    GateAssessment { decision, blockers }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::verify::ExecutionEvidence;

    // Synthetic evidence fixtures, not proof that any command executed.
    fn passes() -> Vec<VerificationResult> {
        REQUIRED_GATES.iter().map(|gate| {
            let mut result = VerificationResult::ok(*gate, "fixture")
                .with_required_evidence(GateStatus::Passed, Some(true));
            if *gate != "persistence" {
                result.evidence.as_mut().unwrap().execution = Some(ExecutionEvidence {
                    command: vec!["fixture-verifier".into(), gate.to_string()],
                    exit_code: 0,
                    duration_ms: 1,
                    tool_version: "fixture-v1".into(),
                });
            }
            result
        }).collect()
    }

    #[test]
    fn requires_every_gate_not_just_persisted_files() {
        assert_eq!(evaluate_required_gates(&[]).decision, GateDecision::Blocked);
        assert_eq!(evaluate_required_gates(&passes()[..1]).decision, GateDecision::Blocked);
        assert_eq!(evaluate_required_gates(&passes()).decision, GateDecision::Satisfied);
    }

    #[test]
    fn unknown_skipped_and_degraded_are_not_success_or_measured_failure() {
        for status in [GateStatus::Unknown, GateStatus::Skipped] {
            let mut results = passes();
            results[1].passed = false;
            results[1].evidence.as_mut().unwrap().status = status;
            assert_eq!(evaluate_required_gates(&results).decision, GateDecision::Blocked);
        }
        let mut results = passes();
        results[1].degraded = true;
        assert_eq!(evaluate_required_gates(&results).decision, GateDecision::Blocked);
    }

    #[test]
    fn flags_versions_and_duplicates_cannot_bypass_native_policy() {
        for variant in 0..4 {
            let mut results = passes();
            let evidence = results[1].evidence.as_mut().unwrap();
            match variant {
                0 => evidence.required = false,
                1 => evidence.applicable = Some(false),
                2 => evidence.applicable = None,
                _ => evidence.schema_version = 2,
            }
            assert_eq!(evaluate_required_gates(&results).decision, GateDecision::Blocked);
        }
        let mut results = passes();
        results.push(results[0].clone());
        assert_eq!(evaluate_required_gates(&results).decision, GateDecision::Blocked);
    }

    #[test]
    fn subprocess_pass_needs_a_receipt_and_nonzero_exit_overrides_pass() {
        let mut results = passes();
        results[1].evidence.as_mut().unwrap().execution = None;
        assert_eq!(evaluate_required_gates(&results).decision, GateDecision::Blocked);
        let mut results = passes();
        results[1].evidence.as_mut().unwrap().execution.as_mut().unwrap().exit_code = 1;
        assert_eq!(evaluate_required_gates(&results).decision, GateDecision::Failed);
    }

    #[test]
    fn explicit_failure_wins_over_unknown_and_degradation() {
        let mut results = passes();
        results[1] = VerificationResult::unknown("typecheck", "not run", Some(true));
        results[2] = VerificationResult::failed("bundle", "bad import")
            .with_required_evidence(GateStatus::Failed, Some(true));
        results[2].degraded = true;
        assert_eq!(evaluate_required_gates(&results).decision, GateDecision::Failed);
    }

    #[test]
    fn historical_json_still_loads_but_does_not_become_new_evidence() {
        let result: VerificationResult = serde_json::from_value(serde_json::json!({
            "gate": "persistence", "passed": true, "degraded": false, "detail": "legacy"
        })).unwrap();
        assert!(result.evidence.is_none());
        assert_eq!(evaluate_required_gates(&[result]).decision, GateDecision::Blocked);
    }

    #[test]
    fn versioned_unknown_round_trips_without_fabricated_execution() {
        let result = VerificationResult::unknown("startup", "not executed", None);
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["evidence"]["status"], "unknown");
        assert!(json["evidence"]["execution"].is_null());
        assert_eq!(serde_json::from_value::<VerificationResult>(json).unwrap(), result);
    }
}
