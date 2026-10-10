use std::sync::atomic::{AtomicBool, Ordering};
use crate::domain::Result;
use crate::event_bus::{DomainEvent, EventBus, now_millis};
use crate::planner::{Plan, PlanStep, Planner, StepProgress};
use crate::pipeline::types::{BuildOutcome, FileGenerationPrompt, GeneratedFile, GeneratedFileStatus, LocalFileGenerationService};
use crate::pipeline::verify::{verify_workspace, DefaultVerificationStage, GateStatus, VerificationResult};
use super::outcome::{evaluate_required_gates, GateDecision, REQUIRED_GATES};
use crate::pipeline::{PipelineInput, PipelineRun};
use crate::self_heal::{RepairAction, SelfHealService};
use crate::runtime_monitor::MonitorEvent;
use crate::pipeline::ledger::LEDGER_PROJECT_ID;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

fn apply_verification(
    outcome: &mut BuildOutcome,
    results: &[crate::pipeline::verify::VerificationResult],
) {
    let assessment = evaluate_required_gates(results);
    if assessment.decision != GateDecision::Satisfied {
        outcome.success = false;
        outcome.degraded = true;
        outcome.summary.push_str(if assessment.decision == GateDecision::Failed {
            "; verification failed: "
        } else {
            "; verification blocked: "
        });
        outcome.summary.push_str(&assessment.blockers.join("; "));
    }
}

fn failed_prompts(prompts: &[FileGenerationPrompt], error: &str) -> Vec<GeneratedFile> {
    prompts.iter().map(|prompt| GeneratedFile {
        path: prompt.file_path.clone(),
        content: String::new(),
        language: Some(prompt.language.clone()),
        status: GeneratedFileStatus::Failed,
        error: Some(error.to_owned()),
    }).collect()
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PipelineStageResult {
    pub stage: String,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PipelineHandle {
    pub run_id: String,
    pub active: bool,
    pub state: PipelineState,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum PipelineState {
    Idle,
    Running,
    Paused,
    Cancelled,
    Completed,
    Failed,
}

pub struct LocalPipelineEngine<'a> {
    pub event_bus: &'a EventBus,
    pub self_heal: &'a dyn SelfHealService,
}

impl<'a> LocalPipelineEngine<'a> {
    pub fn new(event_bus: &'a EventBus, self_heal: &'a dyn SelfHealService) -> Self {
        Self { event_bus, self_heal }
    }

    pub fn run_with_cancellation(
        &self,
        input: PipelineInput,
        cancel: Option<&AtomicBool>,
        planner: &dyn Fn(&str) -> Result<Plan>,
        runtime: &dyn Fn(
            &mut Plan,
            &mut dyn FnMut(&PlanStep, StepProgress) -> bool,
            &AtomicBool,
        ) -> Result<(), String>,
    ) -> Result<PipelineRun> {
        self.run_internal(input, cancel, planner, runtime)
    }

    fn run_internal(
        &self,
        input: PipelineInput,
        cancel: Option<&AtomicBool>,
        planner: &dyn Fn(&str) -> Result<Plan>,
        runtime: &dyn Fn(
            &mut Plan,
            &mut dyn FnMut(&PlanStep, StepProgress) -> bool,
            &AtomicBool,
        ) -> Result<(), String>,
    ) -> Result<PipelineRun> {
        let mut plan = planner(&input.goal).map_err(|e| {
            if input.emit_events {
                self.event_bus.publish(DomainEvent {
                    topic: "plan.failed".into(),
                    payload: serde_json::json!({ "goal": input.goal, "error": e.to_string() }),
                });
            }
            e
        })?;

        if input.emit_events {
            self.event_bus.publish(DomainEvent {
                topic: "plan.created".into(),
                payload: serde_json::json!({ "plan_id": plan.id.as_str(), "goal": plan.goal }),
            });
        }

        let mut events = Vec::new();
        let mut report_step = |step: &PlanStep, progress: StepProgress| -> bool {
            if cancel.map(|t| t.load(Ordering::SeqCst)).unwrap_or(false) {
                return false;
            }
            if !input.emit_events {
                return true;
            }
            let topic = match progress {
                StepProgress::Started => "step.started",
                StepProgress::Completed => "step.completed",
            };
            self.event_bus.publish(DomainEvent {
                topic: topic.into(),
                payload: serde_json::json!({
                    "step_id": step.id.as_str(),
                    "description": step.description,
                }),
            });
            events.push(MonitorEvent {
                ts: now_millis().to_string(),
                severity: "info".into(),
                message: format!("{}: {}", topic, step.description),
            });
            true
        };

        let runtime_result = runtime(
            &mut plan,
            &mut report_step,
            cancel.unwrap_or(&AtomicBool::new(false)),
        );
        if runtime_result.is_err() {
            plan.status = crate::planner::PlanStatus::Failed;
            return Err(crate::domain::DomainError::internal(
                "pipeline execution failed",
            ));
        }

        let manifest_service = crate::pipeline::manifest::LocalGenerationManifestService::new();
        let mut manifest = manifest_service.build(&input.run_id, &input)?;

        let prompt_service = crate::pipeline::prompts::LocalPromptBuilderService::new();
        let prompts = prompt_service.build(&manifest)?;

        let generator = crate::pipeline::generator::LocalFileGeneratorService::new();
        for prompt in prompts {
            if let Err(error) = generator.generate(&mut manifest, &prompt) {
                manifest_service.mark_failed(&mut manifest, &prompt.file_path, error.to_string());
                if input.emit_events {
                    self.event_bus.publish(DomainEvent {
                        topic: "plan.failed".into(),
                        payload: serde_json::json!({ "file": prompt.file_path, "error": error.to_string() }),
                    });
                }
            }
        }

        plan.status = crate::planner::PlanStatus::Completed;
        if input.emit_events {
            self.event_bus.publish(DomainEvent {
                topic: "plan.completed".into(),
                payload: serde_json::json!({ "plan_id": plan.id.as_str(), "steps": plan.steps.len() }),
            });
            events.push(MonitorEvent {
                ts: now_millis().to_string(),
                severity: "info".into(),
                message: format!("plan {} completed", plan.id),
            });
        }

        let repair_actions = self
            .self_heal
            .attempt_repair(&events)
            .map_err(|e| crate::domain::DomainError::internal(e.to_string()))?;

        let (generated_files, failed_files, outcome) =
            self.generate_project_files(None, input.goal.as_str());
        let verification_results = outcome.verification_results.clone();
        let repair_results = outcome.repair_results.clone();
        let verification_summary = outcome.verification_summary.clone();
        Ok(PipelineRun {
            plan,
            repair_actions,
            events,
            run_id: input.run_id,
            execution_context: None,
            generated_files,
            failed_files,
            outcome: Some(outcome),
            verification_results,
            repair_results,
            verification_summary,
        })
    }

    pub fn generate_project_files(
        &self,
        workspace_root: Option<&Path>,
        goal: &str,
    ) -> (
        Vec<GeneratedFile>,
        Vec<GeneratedFile>,
        BuildOutcome,
    ) {
        self.generate_project_files_cancellable(workspace_root, goal, &|| false)
    }

    pub fn generate_project_files_cancellable(
        &self,
        workspace_root: Option<&Path>,
        goal: &str,
        should_cancel: &dyn Fn() -> bool,
    ) -> (Vec<GeneratedFile>, Vec<GeneratedFile>, BuildOutcome) {
        let provider = crate::gateway::OpenAICompatibleAdapter::from_env().map(|adapter| {
            let gateway: Arc<dyn crate::gateway::GatewayService> =
                Arc::new(crate::gateway::DefaultGatewayService::new(
                    Arc::new(adapter),
                    crate::gateway::CircuitBreakerConfig::default(),
                ));
            let model = std::env::var("RECODE_OPENAI_MODEL").unwrap_or_else(|_| "gpt-6-sol".into());
            (gateway, model)
        });
        self.generate_project_files_with_provider_cancellable(workspace_root, goal, provider, should_cancel)
    }

    fn generate_project_files_with_provider(
        &self,
        workspace_root: Option<&Path>,
        goal: &str,
        provider: Result<(Arc<dyn crate::gateway::GatewayService>, String)>,
    ) -> (Vec<GeneratedFile>, Vec<GeneratedFile>, BuildOutcome) {
        self.generate_project_files_with_provider_cancellable(workspace_root, goal, provider, &|| false)
    }

    fn generate_project_files_with_provider_cancellable(
        &self,
        workspace_root: Option<&Path>,
        goal: &str,
        provider: Result<(Arc<dyn crate::gateway::GatewayService>, String)>,
        should_cancel: &dyn Fn() -> bool,
    ) -> (Vec<GeneratedFile>, Vec<GeneratedFile>, BuildOutcome) {
        let manifest_input = crate::pipeline::PipelineInput {
            goal: goal.to_string(),
            emit_events: false,
            run_id: "manifest".into(),
            project_id: None,
        };
        let manifest_service = crate::pipeline::manifest::LocalGenerationManifestService::new();
        let manifest = match manifest_service.build(&manifest_input.run_id, &manifest_input) {
            Ok(manifest) => manifest,
            Err(error) => {
                let failed = vec![GeneratedFile {
                    path: String::new(),
                    content: String::new(),
                    language: None,
                    status: GeneratedFileStatus::Failed,
                    error: Some(error.to_string()),
                }];
                let outcome = BuildOutcome {
                    success: false,
                    degraded: true,
                    generated_file_count: 0,
                    failed_file_count: failed.len(),
                    failed_files: failed.clone(),
                    summary: format!("manifest build failed: {error}"),
                    verification_results: None,
                    repair_results: None,
                    verification_summary: Some(String::new()),
                    analysis: None,
                    report: None,
                    static_analysis: None,
                    mental_simulation: None,
                };
                return (Vec::new(), failed, outcome);
            }
        };

        let prompt_service = crate::pipeline::prompts::LocalPromptBuilderService::new();
        let prompts = match prompt_service.build_file_prompts(&manifest, goal) {
            Ok(prompts) => prompts,
            Err(error) => {
                let error = error.to_string();
                let failed: Vec<_> = manifest
                    .files
                    .iter()
                    .map(|file| GeneratedFile {
                        path: file.path.clone(),
                        content: String::new(),
                        language: Some(file.language.clone()),
                        status: GeneratedFileStatus::Failed,
                        error: Some(error.clone()),
                    })
                    .collect();
                let outcome = BuildOutcome {
                    success: false,
                    degraded: true,
                    generated_file_count: 0,
                    failed_file_count: failed.len(),
                    failed_files: failed.clone(),
                    summary: format!("prompt build failed: {error}"),
                    verification_results: None,
                    repair_results: None,
                    verification_summary: Some(String::new()),
                    analysis: None,
                    report: None,
                    static_analysis: None,
                    mental_simulation: None,
                };
                return (Vec::new(), failed, outcome);
            }
        };

        // Generation ledger (ported from devcode): reuse files whose on-disk
        // bytes still match the last successful generation for these inputs.
        let model = provider.as_ref().map(|(_, model)| model.clone()).unwrap_or_default();
        let file_paths: Vec<String> = prompts.iter().map(|prompt| prompt.file_path.clone()).collect();
        let inputs_hash = crate::pipeline::ledger::compute_generation_inputs_hash(goal, &model, &file_paths);

        let mut generated = Vec::new();
        let mut failed = Vec::new();

        if let Some(workspace_root) = workspace_root {
            let ledger = crate::pipeline::ledger::load_ledger(workspace_root, LEDGER_PROJECT_ID);
            let (reuse_paths, regenerate_paths) = crate::pipeline::ledger::plan_regeneration(
                ledger.as_ref(),
                &inputs_hash,
                workspace_root,
                &file_paths,
            );
            // Checkpoint resume (Q8, ported from devcode's CheckpointManager):
            // files a cancelled/failed attempt already wrote whose bytes still
            // match are resumed without re-billing the provider. The old
            // attempt's checkpoint must be read before the new attempt takes
            // over the checkpoint file.
            let previous_checkpoint =
                crate::pipeline::checkpoint::load_checkpoint(workspace_root, LEDGER_PROJECT_ID);
            let checkpoint_resume_paths: Vec<String> = previous_checkpoint
                .as_ref()
                .map(|checkpoint| {
                    crate::pipeline::checkpoint::resume_paths(
                        checkpoint,
                        &inputs_hash,
                        workspace_root,
                        &file_paths,
                    )
                })
                .unwrap_or_default()
                .into_iter()
                .filter(|path| !reuse_paths.contains(path))
                .collect();
            let attempt_id = uuid::Uuid::new_v4().to_string();
            crate::pipeline::checkpoint::start_attempt(
                workspace_root,
                LEDGER_PROJECT_ID,
                &attempt_id,
                &inputs_hash,
            );
            let mut checkpoint_state =
                crate::pipeline::checkpoint::GenerationCheckpoint::running(
                    LEDGER_PROJECT_ID,
                    attempt_id.clone(),
                    inputs_hash.clone(),
                );
            // Resumed files are this attempt's progress too (devcode's
            // checkpoint carries completed stage data forward): inherit them
            // so a later failure still resumes from this point.
            for path in &checkpoint_resume_paths {
                if let Some(sha256) =
                    crate::pipeline::ledger::file_sha256(&workspace_root.join(path))
                {
                    let _ = checkpoint_state.record(path, sha256);
                }
            }
            let _ = crate::pipeline::checkpoint::persist_checkpoint(
                workspace_root,
                &checkpoint_state,
            );
            let regenerate_prompts: Vec<_> = prompts
                .iter()
                .filter(|prompt| {
                    regenerate_paths.contains(&prompt.file_path)
                        && !checkpoint_resume_paths.contains(&prompt.file_path)
                })
                .cloned()
                .collect();
            // Reused files are first-class results: report them with their
            // on-disk content so verification and the frontend can see them.
            // Their summaries also seed the cross-file context (Q29) so newly
            // generated files stay consistent with files not being regenerated.
            let mut reused_summaries: Vec<(String, String)> = Vec::new();
            for prompt in prompts.iter().filter(|prompt| {
                reuse_paths.contains(&prompt.file_path)
                    || checkpoint_resume_paths.contains(&prompt.file_path)
            }) {
                let content = std::fs::read_to_string(workspace_root.join(&prompt.file_path))
                    .unwrap_or_default();
                if !content.is_empty() {
                    reused_summaries.push((
                        prompt.file_path.clone(),
                        crate::pipeline::cross_file_context::create_file_summary(
                            &content,
                            Some(&prompt.file_path),
                        ),
                    ));
                }
                generated.push(GeneratedFile {
                    path: prompt.file_path.clone(),
                    content,
                    language: Some(prompt.language.clone()),
                    status: GeneratedFileStatus::Reused,
                    error: None,
                });
            }

            match provider {
                Ok((gateway, model)) => {
                    let generator = LocalFileGenerationService::new(
                        crate::workspace::LocalWorkspaceService, gateway, model,
                    );
                    let mut persist_progress =
                        |file_path: &str, content: &str| {
                            let sha256 = crate::pipeline::ledger::content_sha256(content);
                            let _ = checkpoint_state.record(file_path, sha256);
                            let _ = crate::pipeline::checkpoint::persist_checkpoint(
                                workspace_root,
                                &checkpoint_state,
                            );
                        };
                    match generator.generate_and_write_tracked(
                        workspace_root,
                        &regenerate_prompts,
                        should_cancel,
                        &reused_summaries,
                        &mut persist_progress,
                    ) {
                        Ok(results) => {
                            for file in results {
                                if matches!(file.status, GeneratedFileStatus::Written) {
                                    LocalFileGenerationService::upsert_generated_file(&mut generated, file);
                                } else {
                                    failed.push(file);
                                }
                            }
                        }
                        Err(error) => failed.extend(failed_prompts(&regenerate_prompts, &error.to_string())),
                    }
                }
                Err(error) => failed.extend(failed_prompts(&regenerate_prompts, &error.to_string())),
            }

            // Persist checksums of every successfully written file so future
            // runs can reuse them. Ledger IO is fail-soft.
            if failed.is_empty() {
                let mut written: BTreeMap<String, String> = BTreeMap::new();
                for file in generated.iter().filter(|file| matches!(file.status, GeneratedFileStatus::Written | GeneratedFileStatus::Reused)) {
                    if let Some(sha256) = crate::pipeline::ledger::file_sha256(
                        &workspace_root.join(&file.path),
                    ) {
                        written.insert(file.path.clone(), sha256);
                    }
                }
                let _ = crate::pipeline::ledger::store_ledger(
                    workspace_root,
                    LEDGER_PROJECT_ID,
                    &inputs_hash,
                    &written,
                );
                // Clean run: the ledger now owns reuse, so the checkpoint is
                // cleared (devcode clears the checkpoint after successful
                // completion) and this attempt is marked terminal so zombie
                // writes from it are rejected.
                crate::pipeline::checkpoint::finalize_attempt(
                    workspace_root,
                    LEDGER_PROJECT_ID,
                    &attempt_id,
                    crate::pipeline::checkpoint::CheckpointStatus::Completed,
                );
                crate::pipeline::checkpoint::clear_checkpoint(workspace_root);
            } else {
                // Partial run: keep the checkpoint on disk so the next run can
                // resume the files this attempt already wrote, and mark this
                // attempt terminal so no further progress writes are accepted.
                let status = if should_cancel() {
                    crate::pipeline::checkpoint::CheckpointStatus::Cancelled
                } else {
                    crate::pipeline::checkpoint::CheckpointStatus::Failed
                };
                crate::pipeline::checkpoint::finalize_attempt(
                    workspace_root,
                    LEDGER_PROJECT_ID,
                    &attempt_id,
                    status,
                );
            }
        }

        let success = generated.len() == prompts.len();
        let reused_count = generated
            .iter()
            .filter(|file| matches!(file.status, GeneratedFileStatus::Reused))
            .count();
        let summary = if success {
            if reused_count > 0 {
                format!(
                    "generated {} files, reused {} unchanged from ledger/checkpoint",
                    generated.len() - reused_count,
                    reused_count
                )
            } else {
                format!("generated {} files", generated.len())
            }
        } else if generated.is_empty() {
            if let Some(error) = failed.first().and_then(|file| file.error.as_deref()) {
                format!("generated 0 of {} files, {} failed: {error}", prompts.len(), failed.len())
            } else {
                format!("generated 0 of {} files", prompts.len())
            }
        } else {
            format!(
                "generated {} of {} files, {} failed",
                generated.len(),
                prompts.len(),
                failed.len()
            )
        };
        // Failure pattern detection (ported from devcode GenerationErrorHandler):
        // when ≥2 files fail with the same classified reason, surface the
        // pattern in the summary so users see "4 files truncated" instead of
        // having to infer it from individual errors.
        let failure_pattern = {
            let reasons: Vec<_> = failed
                .iter()
                .filter_map(|file| file.error.as_deref())
                .map(crate::pipeline::error_budget::classify_failure_reason)
                .collect();
            crate::pipeline::error_budget::detect_failure_pattern(&reasons)
        };
        let summary = if let Some(pattern) = failure_pattern {
            format!(
                "{summary}; detected pattern: {} files failed with {} errors",
                pattern.count, pattern.pattern_type
            )
        } else {
            summary
        };
        let (verification_results, verification_summary) = self
            .run_verification(workspace_root, &generated);

        // Adaptive repair service: suggest repairs based on failed checks and
        // generated file content (ported from devcode AdaptiveRepairService).
        let repair_result = {
            let repair_service = crate::pipeline::repair::AdaptiveRepairService;
            let failed_checks: Vec<String> = verification_results
                .iter()
                .filter(|r| !r.passed)
                .map(|r| r.gate.clone())
                .collect();
            let generated_files: Vec<crate::pipeline::repair::FileContentSnapshot> = generated
                .iter()
                .map(|f| crate::pipeline::repair::FileContentSnapshot {
                    path: f.path.clone(),
                    content: f.content.clone(),
                })
                .collect();
            repair_service.suggest(&crate::pipeline::repair::AdaptiveRepairInput {
                failed_checks,
                generated_files,
                diagnostics: Vec::new(),
                compiler_output: None,
            })
        };

        // Build outcome analyzer: determine verdict, failure mode, root cause,
        // and aggregate metrics (ported from devcode BuildOutcomeAnalyzer).
        // Mental execution simulator (ported from devcode
        // MentalExecutionSimulator): zero-cost symbolic dry-run over the
        // generated module set — circular dependencies and test-spec
        // conformance checks that surface before any expensive repair loop.
        let mental_simulation = {
            let modules: Vec<crate::pipeline::mental_simulator::SimulatedModule> = generated
                .iter()
                .map(|f| crate::pipeline::mental_simulator::SimulatedModule {
                    path: f.path.clone(),
                    source_code: f.content.clone(),
                    test_code: if f.path.contains("test") || f.path.contains("spec") {
                        Some(f.content.clone())
                    } else {
                        None
                    },
                    dependencies: Vec::new(),
                })
                .collect();
            crate::pipeline::mental_simulator::simulate(&modules)
        };
        let analysis = {
            let outcome_summary = summary.clone();
            let failed_checks: Vec<String> = verification_results
                .iter()
                .filter(|r| !r.passed)
                .map(|r| r.gate.clone())
                .collect();
            let skipped_checks: Vec<String> = verification_results
                .iter()
                .filter(|r| r.evidence.as_ref().is_some_and(|e| e.status == crate::pipeline::verify::GateStatus::Skipped))
                .map(|r| r.gate.clone())
                .collect();
            let mut analyzer = crate::pipeline::outcome_analyzer::BuildOutcomeAnalyzer::new();
            analyzer.analyze(crate::pipeline::outcome_analyzer::BuildOutcomeAnalysisInput {
                outcome: crate::pipeline::outcome_analyzer::OutcomeAnalysisOutcome {
                    success,
                    degraded: !failed.is_empty(),
                    failed_checks,
                    skipped_checks,
                    summary: outcome_summary,
                    stage: "verification".into(),
                },
                generated_file_count: generated.len(),
                persisted_file_count: generated.len(),
                duration_ms: 0,
                verification_results: None,
            })
        };

        // Verification report service: generate comprehensive report (ported
        // from devcode VerificationReportService).
        let report = {
            let report_service = crate::pipeline::report::VerificationReportService;
            let outcome_success = success;
            let outcome_degraded = !failed.is_empty();
            let failed_checks: Vec<String> = verification_results
                .iter()
                .filter(|r| !r.passed)
                .map(|r| r.gate.clone())
                .collect();
            let skipped_checks: Vec<String> = verification_results
                .iter()
                .filter(|r| r.evidence.as_ref().is_some_and(|e| e.status == crate::pipeline::verify::GateStatus::Skipped))
                .map(|r| r.gate.clone())
                .collect();
            report_service.generate_report(&crate::pipeline::report::VerificationReportInput {
                outcome: crate::pipeline::report::VerificationReportOutcome {
                    success: outcome_success,
                    degraded: outcome_degraded,
                    failed_checks,
                    skipped_checks,
                    summary: summary.clone(),
                    stage: "verification".into(),
                },
                gate_results: None,
                generated_file_count: generated.len(),
                persisted_file_count: generated.len(),
                duration_ms: 0,
            })
        };

        // Static analysis (ported from devcode StaticAnalysisService): run
        // lint-level checks (console statements, TODO/FIXME comments) on
        // generated file content and surface findings in the outcome.
        let static_analysis_result = {
            let files_for_analysis: Vec<(String, String)> = generated
                .iter()
                .map(|f| (f.path.clone(), f.content.clone()))
                .collect();
            crate::pipeline::static_analysis::StaticAnalysisService::analyze_files(
                &files_for_analysis,
                &crate::pipeline::static_analysis::StaticAnalysisOptions::default(),
            )
        };

        let mut outcome = BuildOutcome {
            success,
            degraded: !failed.is_empty(),
            generated_file_count: generated.len(),
            failed_file_count: failed.len(),
            failed_files: failed.clone(),
            summary,
            verification_results: None,
            repair_results: Some(repair_result),
            verification_summary: Some(verification_summary),
            analysis: Some(analysis),
            report: Some(report),
            static_analysis: Some(static_analysis_result),
            mental_simulation: Some(mental_simulation),
        };
        // Plan-only simulation has no build gates. It must not acquire fake
        // failures for operations it never requested.
        if workspace_root.is_some() {
            apply_verification(&mut outcome, &verification_results);
        }
        outcome.verification_results = Some(verification_results);
        (generated, failed, outcome)
    }

    pub fn run_verification(
        &self,
        workspace_root: Option<&Path>,
        files: &[GeneratedFile],
    ) -> (Vec<crate::pipeline::verify::VerificationResult>, String) {
        let root = match workspace_root {
            Some(root) => root,
            None => return (Vec::new(), String::new()),
        };
        let stages: Vec<Box<dyn crate::pipeline::verify::VerificationStage + Send + Sync>> =
            vec![Box::new(DefaultVerificationStage)];
        let mut results: Vec<_> = verify_workspace(&stages, root, files).into_iter().map(|result| {
            let status = if result.passed { GateStatus::Passed } else { GateStatus::Failed };
            result.with_required_evidence(status, Some(true))
        }).collect();
        // Do not spawn generated code until scoped consent and a bounded
        // supervisor exist. Missing runners are unknown, never executed failures.
        for gate in REQUIRED_GATES.iter().filter(|gate| **gate != "persistence") {
            results.push(VerificationResult::unknown(
                gate,
                "Not executed: approved manifest, scoped execution consent and supervised verifier are not yet available.",
                None,
            ));
        }
        let summary = results
            .iter()
            .map(|result| format!("{}: {}", result.gate, result.detail))
            .collect::<Vec<_>>()
            .join("; ");
        (results, summary)
    }

}

#[cfg(test)]
mod verification_outcome_tests {
    use super::*;
    use crate::gateway::{GatewayRequest, GatewayResponse, GatewayService};
    use crate::pipeline::verify::VerificationResult;
    use std::sync::atomic::AtomicBool;

    struct CancellingGateway(Arc<AtomicBool>);

    impl GatewayService for CancellingGateway {
        fn send(&self, _request: GatewayRequest) -> Result<GatewayResponse> {
            self.0.store(true, Ordering::SeqCst);
            Ok(GatewayResponse {
                status: 200,
                body: serde_json::json!({"choices": [{"message": {"content": "new content"}, "finish_reason": "stop"}]}),
            })
        }
    }

    #[test]
    fn engine_passes_generation_cancellation_before_writing_first_file() {
        let (bus, _receiver) = EventBus::new();
        let heal = crate::self_heal::DefaultSelfHealService;
        let engine = LocalPipelineEngine::new(&bus, &heal);
        let root = tempfile::tempdir().unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let (generated, failed, outcome) = engine.generate_project_files_with_provider_cancellable(
            Some(root.path()), "build a page",
            Ok((Arc::new(CancellingGateway(cancelled.clone())), "test-model".into())),
            &|| cancelled.load(Ordering::SeqCst),
        );
        assert!(generated.is_empty());
        assert_eq!(failed.len(), crate::pipeline::manifest::VITE_PROJECT_FILES.len());
        assert!(!outcome.success);
        assert!(!root.path().join("README.md").exists());
        assert!(!root.path().join("src/main.tsx").exists());
    }

    #[test]
    fn unavailable_provider_marks_every_requested_workspace_file_failed() {
        let (bus, _receiver) = EventBus::new();
        let heal = crate::self_heal::DefaultSelfHealService;
        let engine = LocalPipelineEngine::new(&bus, &heal);
        let root = tempfile::tempdir().unwrap();
        let failure = || crate::domain::DomainError::unavailable("RECODE_OPENAI_BASE_URL is not configured");

        let (generated, failed, outcome) = engine.generate_project_files_with_provider(
            Some(root.path()), "build a page", Err(failure()),
        );
        assert!(generated.is_empty());
        assert_eq!(failed.len(), crate::pipeline::manifest::VITE_PROJECT_FILES.len());
        assert!(failed.iter().all(|file| {
            file.status == GeneratedFileStatus::Failed
                && file.error.as_deref().is_some_and(|error| error.contains("RECODE_OPENAI_BASE_URL"))
        }));
        assert_eq!(outcome.failed_file_count, crate::pipeline::manifest::VITE_PROJECT_FILES.len());
        assert!(!outcome.success);
        assert!(outcome.degraded);
        assert!(outcome.summary.contains("RECODE_OPENAI_BASE_URL"));
        assert!(!root.path().join("README.md").exists());

        let (_, simulated_failures, _) = engine.generate_project_files_with_provider(
            None, "build a page", Err(failure()),
        );
        assert!(simulated_failures.is_empty());
    }

    #[test]
    fn failed_persistence_gate_downgrades_build_outcome() {
        let mut outcome = BuildOutcome {
            success: true,
            degraded: false,
            generated_file_count: 1,
            failed_file_count: 0,
            failed_files: Vec::new(),
            summary: "generated 1 files".into(),
            verification_results: None,
            repair_results: None,
            verification_summary: None,
            analysis: None,
            report: None,
            static_analysis: None,
            mental_simulation: None,
        };
        apply_verification(&mut outcome, &[VerificationResult::failed("persistence", "missing file")]);
        assert!(!outcome.success);
        assert!(outcome.degraded);
        assert!(outcome.summary.contains("verification failed"));
    }
}

#[cfg(test)]
mod http_generation_tests {
    use super::*;
    use crate::gateway::{CircuitBreakerConfig, DefaultGatewayService, OpenAICompatibleAdapter};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    #[test]
    fn loopback_provider_generates_full_vite_manifest_and_passes_disk_verification() {
        use std::collections::HashMap;

        let expected_files: HashMap<&str, &str> = [
            ("package.json", "{\"name\":\"generated\",\"version\":\"0.0.1\"}\n"),
            ("tsconfig.json", "{\"compilerOptions\":{}}\n"),
            ("tsconfig.app.json", "{\"compilerOptions\":{}}\n"),
            ("vite.config.ts", "import { defineConfig } from 'vite'; export default defineConfig({});\n"),
            ("index.html", "<!doctype html><html><body><div id=root></div></body></html>\n"),
            ("README.md", "# Built project\n"),
            ("src/main.tsx", "import React from 'react';\nimport { createRoot } from 'react-dom/client';\nimport App from './App';\n\ncreateRoot(document.getElementById('root')!).render(<App />);\n"),
            ("src/App.tsx", "export default function App() { return <div>Hello</div>; }\n"),
            ("src/App.css", ".app { color: black; }\n"),
        ].into_iter().collect();

        let file_order: Vec<&str> = crate::pipeline::manifest::VITE_PROJECT_FILES
            .iter()
            .map(|(path, _, _, _, _, _)| *path)
            .collect();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let server_expected_files = expected_files.clone();
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            for path in file_order {
                let content = server_expected_files[path];
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(error) => panic!("provider request for {path} missing: {error}"),
                    }
                };
                socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut bytes = Vec::new();
                let header_end = loop {
                    let mut chunk = [0u8; 4096];
                    let count = socket.read(&mut chunk).unwrap();
                    assert!(count > 0, "request ended before headers");
                    bytes.extend_from_slice(&chunk[..count]);
                    if let Some(position) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                        break position + 4;
                    }
                };
                let headers = String::from_utf8_lossy(&bytes[..header_end]);
                assert!(headers.starts_with("POST /v1/chat/completions "));
                let length: usize = headers.lines().find_map(|line| {
                    line.to_ascii_lowercase().strip_prefix("content-length: ")?.trim().parse().ok()
                }).unwrap();
                while bytes.len() - header_end < length {
                    let mut chunk = [0u8; 4096];
                    let count = socket.read(&mut chunk).unwrap();
                    assert!(count > 0, "request ended before body");
                    bytes.extend_from_slice(&chunk[..count]);
                }
                let request: serde_json::Value = serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap();
                assert_eq!(request["model"], "selected-model");
                assert_eq!(request["stream"], false);
                assert!(request["messages"][0]["content"].as_str().unwrap().contains(path), "request must mention {path}");

                let body = serde_json::json!({"choices": [{"message": {"content": content}, "finish_reason": "stop"}]}).to_string();
                let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                socket.write_all(response.as_bytes()).unwrap();
            }
        });

        let adapter = Arc::new(OpenAICompatibleAdapter::new(&url, "selected-model", None).unwrap());
        let gateway = Arc::new(DefaultGatewayService::new(adapter, CircuitBreakerConfig::default()));
        let (bus, _receiver) = EventBus::new();
        let heal = crate::self_heal::DefaultSelfHealService;
        let engine = LocalPipelineEngine::new(&bus, &heal);
        let root = tempfile::tempdir().unwrap();
        let (generated, failed, outcome) = engine.generate_project_files_with_provider(
            Some(root.path()), "build a project", Ok((gateway, "selected-model".into())),
        );
        server.join().unwrap();
        assert!(failed.is_empty(), "{failed:?}");
        assert_eq!(generated.len(), crate::pipeline::manifest::VITE_PROJECT_FILES.len(), "all manifest files should be generated");
        assert!(!outcome.success, "disk persistence alone is not a verified build: {outcome:?}");
        assert!(outcome.summary.contains("verification blocked"));
        let gates = outcome.verification_results.as_ref().unwrap();
        assert_eq!(gates.len(), REQUIRED_GATES.len());
        assert!(gates[0].passed);
        assert_eq!(gates[0].gate, "persistence");
        assert!(gates[1..].iter().all(|result| {
            let evidence = result.evidence.as_ref().unwrap();
            evidence.status == GateStatus::Unknown && evidence.execution.is_none()
        }));

        for (path, expected) in expected_files {
            let actual = std::fs::read_to_string(root.path().join(path)).unwrap();
            assert_eq!(actual, expected, "mismatch for {path}");
        }
    }

    struct RejectingGateway;

    impl crate::gateway::GatewayService for RejectingGateway {
        fn send(
            &self,
            _request: crate::gateway::GatewayRequest,
        ) -> crate::domain::Result<crate::gateway::GatewayResponse> {
            panic!("provider must not be called when the ledger reuses every file");
        }
    }

    #[test]
    fn second_run_reuses_ledger_entries_and_skips_the_provider() {
        let (bus, _receiver) = EventBus::new();
        let heal = crate::self_heal::DefaultSelfHealService;
        let engine = LocalPipelineEngine::new(&bus, &heal);
        let root = tempfile::tempdir().unwrap();

        // Seed the ledger exactly as a successful first run would: write the
        // manifest files, hash them, and persist under the run's inputs hash.
        // package.json gets valid JSON: the post-generation validation gate
        // refuses to reuse malformed manifests.
        let paths: Vec<String> = crate::pipeline::manifest::VITE_PROJECT_FILES
            .iter()
            .map(|(path, _, _, _, _, _)| path.to_string())
            .collect();
        std::fs::create_dir_all(root.path().join("src")).unwrap();
        let seed_content = |path: &str| -> String {
            if path == "package.json" {
                format!("{{\"name\":\"seed\",\"path\":\"{path}\"}}\n")
            } else {
                format!("content of {path}\n")
            }
        };
        for path in &paths {
            std::fs::write(root.path().join(path), seed_content(path)).unwrap();
        }
        let inputs_hash = crate::pipeline::ledger::compute_generation_inputs_hash(
            "build a project",
            "selected-model",
            &paths,
        );
        let written: std::collections::BTreeMap<String, String> = paths
            .iter()
            .map(|path| {
                let sha = crate::pipeline::ledger::file_sha256(&root.path().join(path)).unwrap();
                (path.clone(), sha)
            })
            .collect();
        crate::pipeline::ledger::store_ledger(root.path(), LEDGER_PROJECT_ID, &inputs_hash, &written)
            .unwrap();

        // Second run: every file must be reused without contacting the
        // provider, which would panic inside RejectingGateway.
        let (generated, failed, outcome) = engine.generate_project_files_with_provider(
            Some(root.path()),
            "build a project",
            Ok((Arc::new(RejectingGateway), "selected-model".into())),
        );
        assert!(failed.is_empty(), "{failed:?}");
        assert_eq!(
            generated.len(),
            crate::pipeline::manifest::VITE_PROJECT_FILES.len(),
            "every manifest file should be reused"
        );
        assert!(generated
            .iter()
            .all(|file| file.status == GeneratedFileStatus::Reused));
        assert!(generated
            .iter()
            .all(|file| file.content == seed_content(&file.path)));
        assert!(!outcome.success, "persistence gate alone still does not make a verified build");
        assert!(outcome.summary.contains("verification blocked"));

        // A drifted file must regenerate: tamper with one file and rerun with
        // a gateway that only serves that file.
        std::fs::write(root.path().join("src/App.css"), "tampered\n").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("regeneration request missing: {error}"),
                }
            };
            socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut bytes = Vec::new();
            let header_end = loop {
                let mut chunk = [0u8; 4096];
                let count = socket.read(&mut chunk).unwrap();
                assert!(count > 0, "request ended before headers");
                bytes.extend_from_slice(&chunk[..count]);
                if let Some(position) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    break position + 4;
                }
            };
            let _header_end = header_end;
            let body = serde_json::json!({"choices": [{"message": {"content": ".regenerated {}"}, "finish_reason": "stop"}]}).to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).unwrap();
        });
        let adapter = Arc::new(OpenAICompatibleAdapter::new(&url, "selected-model", None).unwrap());
        let gateway = Arc::new(DefaultGatewayService::new(adapter, CircuitBreakerConfig::default()));
        let (generated, failed, _outcome) = engine.generate_project_files_with_provider(
            Some(root.path()),
            "build a project",
            Ok((gateway, "selected-model".into())),
        );
        server.join().unwrap();
        assert!(failed.is_empty(), "{failed:?}");
        let reused: Vec<_> = generated
            .iter()
            .filter(|file| file.status == GeneratedFileStatus::Reused)
            .map(|file| file.path.as_str())
            .collect();
        let regenerated: Vec<_> = generated
            .iter()
            .filter(|file| file.status == GeneratedFileStatus::Written)
            .map(|file| file.path.as_str())
            .collect();
        assert_eq!(regenerated, vec!["src/App.css"]);
        assert_eq!(reused.len(), crate::pipeline::manifest::VITE_PROJECT_FILES.len() - 1);
        assert_eq!(
            std::fs::read_to_string(root.path().join("src/App.css")).unwrap(),
            ".regenerated {}"
        );
    }
}

#[cfg(test)]
mod checkpoint_resume_tests {
    use super::*;
    use crate::gateway::{GatewayRequest, GatewayResponse, GatewayService};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Serves one bounded-successful response per request, counts requests,
    /// and panics on any request beyond the expected budget.
    struct CountingGateway {
        remaining: AtomicUsize,
        requests: Arc<AtomicUsize>,
    }

    impl GatewayService for CountingGateway {
        fn send(&self, _request: GatewayRequest) -> Result<GatewayResponse> {
            let served = self.remaining.fetch_sub(1, Ordering::SeqCst);
            assert!(served > 0, "unexpected extra provider request");
            let served_so_far = self.requests.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(GatewayResponse {
                status: 200,
                // Quoted so the content is also valid JSON when it lands on
                // package.json (post-generation validation gates that file).
                body: serde_json::json!({"choices": [{"message": {"content": format!("\"generated content {served_so_far}\"")}, "finish_reason": "stop"}]}),
            })
        }
    }

    #[test]
    fn cancelled_partial_run_resumes_from_checkpoint_without_rebilling() {
        let (bus, _receiver) = EventBus::new();
        let heal = crate::self_heal::DefaultSelfHealService;
        let engine = LocalPipelineEngine::new(&bus, &heal);
        let root = tempfile::tempdir().unwrap();

        let file_count = crate::pipeline::manifest::VITE_PROJECT_FILES.len();
        // The provider phase runs 4 cancellation checks per prompt (loop head,
        // attempt head, after send, after generation returns). Cancelling at
        // check 4N+2 aborts the write loop after exactly one file has landed
        // on disk: check 4N+1 passes and writes file 1, check 4N+2 aborts
        // before file 2.
        let cancel_at = 4 * file_count + 2;
        let checks = Arc::new(AtomicUsize::new(0));
        let first_requests = Arc::new(AtomicUsize::new(0));
        let first_gateway = Arc::new(CountingGateway {
            remaining: AtomicUsize::new(file_count),
            requests: first_requests.clone(),
        });
        let should_cancel = move || {
            checks.fetch_add(1, Ordering::SeqCst) + 1 >= cancel_at
        };

        let (generated, failed, outcome) = engine.generate_project_files_with_provider_cancellable(
            Some(root.path()),
            "build a project",
            Ok((first_gateway, "selected-model".into())),
            &should_cancel,
        );
        // Exactly one file was written before the cancellation landed.
        assert!(generated.is_empty(), "{generated:?}");
        assert_eq!(failed.len(), file_count);
        assert!(!outcome.success);
        assert_eq!(first_requests.load(Ordering::SeqCst), file_count);

        // The checkpoint durably records the partial progress and the
        // cancellation, so the next run can resume from it.
        let checkpoint = crate::pipeline::checkpoint::load_checkpoint(
            root.path(),
            LEDGER_PROJECT_ID,
        )
        .expect("checkpoint must persist after a cancelled partial run");
        assert_eq!(checkpoint.status, crate::pipeline::checkpoint::CheckpointStatus::Cancelled);
        assert_eq!(checkpoint.files.len(), 1, "exactly one file should be checkpointed");
        let resumed_path = checkpoint.files[0].path.clone();
        assert!(root.path().join(&resumed_path).exists(), "checkpointed file must be on disk");

        // Resume run: the checkpointed file must not re-bill the provider, so
        // the gateway only budgets the remaining files.
        let second_requests = Arc::new(AtomicUsize::new(0));
        let second_gateway = Arc::new(CountingGateway {
            remaining: AtomicUsize::new(file_count - 1),
            requests: second_requests.clone(),
        });
        let (generated, failed, outcome) = engine.generate_project_files_with_provider_cancellable(
            Some(root.path()),
            "build a project",
            Ok((second_gateway, "selected-model".into())),
            &|| false,
        );
        assert!(failed.is_empty(), "{failed:?}");
        assert_eq!(
            second_requests.load(Ordering::SeqCst),
            file_count - 1,
            "the checkpointed file must not be re-billed"
        );
        assert_eq!(generated.len(), file_count);
        let resumed = generated
            .iter()
            .find(|file| file.status == GeneratedFileStatus::Reused)
            .expect("checkpointed file must be resumed as a first-class result");
        assert_eq!(resumed.path, resumed_path);
        assert_eq!(
            generated
                .iter()
                .filter(|file| file.status == GeneratedFileStatus::Written)
                .count(),
            file_count - 1
        );
        assert_eq!(outcome.generated_file_count, file_count);

        // A clean resumed run hands reuse back to the ledger (checkpoint
        // cleared, every manifest file — resumed file included — promoted).
        assert!(
            crate::pipeline::checkpoint::load_checkpoint(root.path(), LEDGER_PROJECT_ID).is_none(),
            "checkpoint must be cleared after a clean run"
        );
        let ledger = crate::pipeline::ledger::load_ledger(root.path(), LEDGER_PROJECT_ID)
            .expect("clean resumed run must store the ledger");
        assert_eq!(ledger.files.len(), file_count);
        assert!(ledger.checksum(&resumed_path).is_some());
    }
}

#[cfg(test)]
mod post_generation_validation_tests {
    use super::*;
    use crate::gateway::{GatewayRequest, GatewayResponse, GatewayService};
    use std::sync::Mutex;

    /// Serves scripted per-file content with a valid default for everything
    /// else, mirroring devcode's post-generation fixture: a malformed
    /// package.json alongside valid siblings.
    struct ScriptedGateway {
        contents: Mutex<std::collections::HashMap<String, String>>,
        default: String,
    }

    impl ScriptedGateway {
        fn new(contents: Vec<(String, String)>, default: &str) -> Self {
            Self {
                contents: Mutex::new(contents.into_iter().collect()),
                default: default.to_string(),
            }
        }
    }

    impl GatewayService for ScriptedGateway {
        fn send(&self, request: GatewayRequest) -> Result<GatewayResponse> {
            let messages = request.payload["messages"]
                .as_array()
                .expect("messages");
            // The system prompt names the file: "File: {path}\nPurpose: ..."
            let system = messages[0]["content"].as_str().expect("system content");
            let file_path = system
                .split("File: ")
                .nth(1)
                .and_then(|rest| rest.split('\n').next())
                .expect("file path in system prompt")
                .to_string();
            let content = self
                .contents
                .lock()
                .expect("contents")
                .get(&file_path)
                .cloned()
                .unwrap_or_else(|| self.default.clone());
            Ok(GatewayResponse {
                status: 200,
                body: serde_json::json!({"choices": [{"message": {"content": content}, "finish_reason": "stop"}]}),
            })
        }
    }

    /// The provider returns a malformed package.json; the hard gate must fail
    /// that file before any write, preserve prior bytes on disk, and leave
    /// no ledger entry behind. Valid siblings still generate.
    #[test]
    fn malformed_package_json_fails_the_file_and_is_never_persisted() {
        let (bus, _receiver) = EventBus::new();
        let heal = crate::self_heal::DefaultSelfHealService;
        let engine = LocalPipelineEngine::new(&bus, &heal);
        let root = tempfile::tempdir().unwrap();

        let file_count = crate::pipeline::manifest::VITE_PROJECT_FILES.len();
        std::fs::write(root.path().join("package.json"), "{\"name\":\"previous\"}").unwrap();
        let gateway = Arc::new(ScriptedGateway::new(
            vec![("package.json".into(), "{ not valid json".into())],
            "export const generated = true;\n",
        ));

        let (generated, failed, outcome) = engine.generate_project_files_with_provider(
            Some(root.path()),
            "build a project",
            Ok((gateway, "selected-model".into())),
        );
        // Exactly the malformed file failed; every sibling still generated.
        assert_eq!(failed.len(), 1, "{failed:?}");
        assert_eq!(failed[0].path, "package.json");
        assert_eq!(failed[0].status, GeneratedFileStatus::Failed);
        assert!(
            failed[0]
                .error
                .as_deref()
                .is_some_and(|error| error.contains("package.json is malformed JSON:")),
            "unexpected failure reason: {:?}",
            failed[0].error
        );
        assert_eq!(generated.len(), file_count - 1, "{generated:?}");
        // Prior bytes are preserved: the malformed content was never written.
        assert_eq!(
            std::fs::read_to_string(root.path().join("package.json")).unwrap(),
            "{\"name\":\"previous\"}"
        );
        // A run with a failed file must not be promoted into the reuse ledger.
        assert!(
            crate::pipeline::ledger::load_ledger(root.path(), LEDGER_PROJECT_ID).is_none(),
            "a run with a failed file must not store a ledger"
        );
        assert!(!outcome.success);
        assert!(outcome.degraded);
    }

    /// A clean run whose package.json is valid must behave exactly as before:
    /// written, persisted, ledger stored, checkpoint cleared.
    #[test]
    fn valid_package_json_still_persists_and_promotes_into_the_ledger() {
        let (bus, _receiver) = EventBus::new();
        let heal = crate::self_heal::DefaultSelfHealService;
        let engine = LocalPipelineEngine::new(&bus, &heal);
        let root = tempfile::tempdir().unwrap();

        let file_count = crate::pipeline::manifest::VITE_PROJECT_FILES.len();
        let gateway = Arc::new(ScriptedGateway::new(
            vec![(
                "package.json".into(),
                "{\"name\":\"generated\",\"version\":\"0.0.1\"}\n".into(),
            )],
            "export const generated = true;\n",
        ));

        let (generated, failed, outcome) = engine.generate_project_files_with_provider(
            Some(root.path()),
            "build a project",
            Ok((gateway, "selected-model".into())),
        );
        assert!(failed.is_empty(), "{failed:?}");
        assert_eq!(generated.len(), file_count);
        assert!(
            generated
                .iter()
                .all(|file| matches!(file.status, GeneratedFileStatus::Written))
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join("package.json")).unwrap(),
            "{\"name\":\"generated\",\"version\":\"0.0.1\"}\n"
        );
        let ledger = crate::pipeline::ledger::load_ledger(root.path(), LEDGER_PROJECT_ID)
            .expect("clean run must store the ledger");
        assert_eq!(ledger.files.len(), file_count);
        assert!(ledger.checksum("package.json").is_some());
        assert!(
            crate::pipeline::checkpoint::load_checkpoint(root.path(), LEDGER_PROJECT_ID)
                .map_or(true, |checkpoint| checkpoint.files.is_empty()),
            "clean run must leave no checkpoint progress"
        );
        // Generation succeeded; the build still fails native gate policy
        // (only persistence runs), exactly as before the gate existed.
        assert!(!outcome.success);
        assert!(outcome.summary.contains("verification blocked"));
    }
}
