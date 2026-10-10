//! Progress Tracker (ported from devcode ProgressTracker.ts)
//!
//! Manages active build state, cancellation, and progress subscriptions.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, serde::Serialize)]
pub struct BuildProgress {
    pub project_id: String,
    pub pipeline_id: Option<String>,
    pub progress: f32, // 0-100
    pub stage: String,
    pub current_file: Option<String>,
}

impl std::fmt::Debug for ProgressTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProgressTracker")
         .field("active_builds", &self.active_builds)
         .finish()
    }
}
pub struct ProgressTracker {
    active_builds: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    // Callback subscription management: Simple closure-based subscriber
    subscribers: Arc<Mutex<HashMap<String, Vec<Box<dyn Fn(BuildProgress) + Send + Sync>>>>>,
}

impl ProgressTracker {
    pub fn new() -> Self {
        Self {
            active_builds: Arc::new(Mutex::new(HashMap::new())),
            subscribers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn register_active_build(&self, project_id: String) -> Arc<AtomicBool> {
        let cancel_token = Arc::new(AtomicBool::new(false));
        self.active_builds.lock().unwrap().insert(project_id, cancel_token.clone());
        cancel_token
    }

    pub fn has_active_build(&self, project_id: &str) -> bool {
        self.active_builds.lock().unwrap().contains_key(project_id)
    }

    pub fn cancel_build(&self, project_id: &str) {
        if let Some(token) = self.active_builds.lock().unwrap().remove(project_id) {
            token.store(true, Ordering::SeqCst);
        }
    }

    pub fn cleanup_active_build(&self, project_id: &str) {
        self.active_builds.lock().unwrap().remove(project_id);
    }

    pub fn subscribe(&self, project_id: String, callback: Box<dyn Fn(BuildProgress) + Send + Sync>) {
        self.subscribers.lock().unwrap().entry(project_id).or_default().push(callback);
    }

    pub fn notify_progress(&self, progress: BuildProgress) {
        if let Some(callbacks) = self.subscribers.lock().unwrap().get(&progress.project_id) {
            for cb in callbacks {
                cb(progress.clone());
            }
        }
    }
}
