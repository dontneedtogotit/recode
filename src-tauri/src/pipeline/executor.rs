//! Pipeline Executor
//!
//! Orchestrates the build lifecycle, integrating generation, verification,
//! and progress tracking.

use crate::pipeline::types::{FileGenerationPrompt, GeneratedFile, LocalFileGenerationService};
use crate::pipeline::progress_tracker::{ProgressTracker, BuildProgress};
use crate::domain::Result;
use std::path::Path;
use std::sync::Arc;

pub struct PipelineExecutor {
    progress_tracker: Arc<ProgressTracker>,
    generation_service: LocalFileGenerationService,
}

impl PipelineExecutor {
    pub fn new(
        progress_tracker: Arc<ProgressTracker>,
        generation_service: LocalFileGenerationService,
    ) -> Self {
        Self {
            progress_tracker,
            generation_service,
        }
    }

    pub fn execute(
        &self,
        project_id: &str,
        workspace_root: Option<&Path>,
        prompts: &[FileGenerationPrompt],
        should_cancel: &dyn Fn() -> bool,
    ) -> Result<Vec<GeneratedFile>> {
        let cancel_token = self.progress_tracker.register_active_build(project_id.to_string());
        
        // Notify start
        self.progress_tracker.notify_progress(BuildProgress {
            project_id: project_id.to_string(),
            pipeline_id: None,
            progress: 0.0,
            stage: "initializing".to_string(),
            current_file: None,
        });

        // Delegate generation to generation service
        let result = self.generation_service.generate_and_write_cancellable(
            workspace_root.unwrap_or_else(|| Path::new("")),
            prompts,
            &|| should_cancel() || cancel_token.load(std::sync::atomic::Ordering::SeqCst),
        );

        self.progress_tracker.cleanup_active_build(project_id);
        
        result
    }
}
