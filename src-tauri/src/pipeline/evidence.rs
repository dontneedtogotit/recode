//! Evidence Collector (ported from devcode EvidenceCollector.ts)
//!
//! Collects and organizes evidence from the build pipeline for audit,
//! compliance, and debugging purposes. Evidence includes generated files,
//! verification results, logs, and build artifacts.
//! Pure module — no stores, no DI, no I/O.

#![allow(dead_code)]  // ROADMAP parity port: kept for planned wiring; prune when the Q-item closes.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceItem {
    pub id: String,
    pub evidence_type: String, // 'file' | 'log' | 'verification' | 'artifact' | 'metric'
    pub category: String,
    pub description: String,
    pub data: serde_json::Value,
    pub timestamp: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceCollectionSummary {
    pub total_items: usize,
    pub file_count: usize,
    pub log_count: usize,
    pub verification_count: usize,
    pub artifact_count: usize,
    pub metric_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceCollection {
    pub id: String,
    pub build_id: String,
    pub items: Vec<EvidenceItem>,
    pub summary: EvidenceCollectionSummary,
    pub collected_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceCollectionInput {
    pub generated_files: Vec<EvidenceFile>,
    #[serde(default)]
    pub verification_results: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub logs: Vec<EvidenceLog>,
    #[serde(default)]
    pub artifacts: Vec<EvidenceArtifact>,
    #[serde(default)]
    pub metrics: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceFile {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceLog {
    pub level: String,
    pub message: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceArtifact {
    pub name: String,
    pub path: String,
    pub size: usize,
}

pub struct EvidenceCollector {
    collections: BTreeMap<String, EvidenceCollection>,
}

impl EvidenceCollector {
    pub fn new() -> Self {
        Self { collections: BTreeMap::new() }
    }

    pub fn collect(&mut self, input: &EvidenceCollectionInput) -> EvidenceCollection {
        let mut items = Vec::new();
        let build_id = uuid_v4();
        let now = now_iso();

        // Collect file evidence
        for file in &input.generated_files {
            let ext = file.path.rsplit('.').next().unwrap_or("unknown");
            items.push(EvidenceItem {
                id: uuid_v4(),
                evidence_type: "file".into(),
                category: "generated".into(),
                description: format!("Generated file: {}", file.path),
                data: serde_json::json!({ "path": file.path, "size": file.content.len() }),
                timestamp: now.clone(),
                tags: vec!["generated".into(), ext.into()],
            });
        }

        // Collect verification evidence
        for (check_id, result) in &input.verification_results {
            items.push(EvidenceItem {
                id: uuid_v4(),
                evidence_type: "verification".into(),
                category: check_id.clone(),
                description: format!("Verification result: {check_id}"),
                data: result.clone(),
                timestamp: now.clone(),
                tags: vec!["verification".into(), check_id.clone()],
            });
        }

        // Collect log evidence
        for log in &input.logs {
            items.push(EvidenceItem {
                id: uuid_v4(),
                evidence_type: "log".into(),
                category: log.level.clone(),
                description: log.message.clone(),
                data: serde_json::json!({ "level": log.level, "message": log.message }),
                timestamp: log.timestamp.clone(),
                tags: vec!["log".into(), log.level.clone()],
            });
        }

        // Collect artifact evidence
        for artifact in &input.artifacts {
            items.push(EvidenceItem {
                id: uuid_v4(),
                evidence_type: "artifact".into(),
                category: "build-artifact".into(),
                description: format!("Build artifact: {}", artifact.name),
                data: serde_json::json!({ "name": artifact.name, "path": artifact.path, "size": artifact.size }),
                timestamp: now.clone(),
                tags: vec!["artifact".into(), artifact.name.clone()],
            });
        }

        // Collect metric evidence
        for (name, value) in &input.metrics {
            items.push(EvidenceItem {
                id: uuid_v4(),
                evidence_type: "metric".into(),
                category: "performance".into(),
                description: format!("Metric: {name}"),
                data: serde_json::json!({ "name": name, "value": value }),
                timestamp: now.clone(),
                tags: vec!["metric".into(), name.clone()],
            });
        }

        let summary = EvidenceCollectionSummary {
            total_items: items.len(),
            file_count: items.iter().filter(|i| i.evidence_type == "file").count(),
            log_count: items.iter().filter(|i| i.evidence_type == "log").count(),
            verification_count: items.iter().filter(|i| i.evidence_type == "verification").count(),
            artifact_count: items.iter().filter(|i| i.evidence_type == "artifact").count(),
            metric_count: items.iter().filter(|i| i.evidence_type == "metric").count(),
        };

        let collection = EvidenceCollection {
            id: uuid_v4(),
            build_id,
            items,
            summary,
            collected_at: now,
        };
        self.collections.insert(collection.id.clone(), collection.clone());
        collection
    }

    pub fn get_collection(&self, id: &str) -> Option<&EvidenceCollection> {
        self.collections.get(id)
    }

    pub fn get_all_collections(&self) -> Vec<EvidenceCollection> {
        self.collections.values().cloned().collect()
    }

    pub fn clear(&mut self) {
        self.collections.clear();
    }
}

fn uuid_v4() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("t{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_files_and_verification() {
        let mut collector = EvidenceCollector::new();
        let input = EvidenceCollectionInput {
            generated_files: vec![
                EvidenceFile { path: "src/App.tsx".into(), content: "export const App = () => null;".into() },
                EvidenceFile { path: "index.html".into(), content: "<!DOCTYPE html>".into() },
            ],
            verification_results: {
                let mut m = BTreeMap::new();
                m.insert("compile".into(), serde_json::json!({ "status": "passed" }));
                m
            },
            logs: vec![],
            artifacts: vec![],
            metrics: BTreeMap::new(),
        };
        let collection = collector.collect(&input);
        assert_eq!(collection.summary.file_count, 2);
        assert_eq!(collection.summary.verification_count, 1);
        assert_eq!(collection.summary.total_items, 3);
    }

    #[test]
    fn test_collect_logs_and_metrics() {
        let mut collector = EvidenceCollector::new();
        let input = EvidenceCollectionInput {
            generated_files: vec![],
            verification_results: BTreeMap::new(),
            logs: vec![
                EvidenceLog { level: "warn".into(), message: "test warning".into(), timestamp: "t1".into() },
            ],
            artifacts: vec![],
            metrics: {
                let mut m = BTreeMap::new();
                m.insert("duration_ms".into(), serde_json::json!(1500));
                m
            },
        };
        let collection = collector.collect(&input);
        assert_eq!(collection.summary.log_count, 1);
        assert_eq!(collection.summary.metric_count, 1);
    }

    #[test]
    fn test_get_collection_by_id() {
        let mut collector = EvidenceCollector::new();
        let input = EvidenceCollectionInput {
            generated_files: vec![EvidenceFile { path: "a.ts".into(), content: "x".into() }],
            verification_results: BTreeMap::new(),
            logs: vec![],
            artifacts: vec![],
            metrics: BTreeMap::new(),
        };
        let collection = collector.collect(&input);
        let id = collection.id.clone();
        assert!(collector.get_collection(&id).is_some());
        collector.clear();
        assert!(collector.get_collection(&id).is_none());
    }
}
