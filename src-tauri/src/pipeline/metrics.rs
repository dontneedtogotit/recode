//! Pipeline Metrics Collector (ported from devcode PipelineMetricsCollector.ts)
//!
//! Collects metrics specific to pipeline execution: stage durations,
//! success/failure counts, file generation counts, and overall pipeline
//! health. Pure module — no stores, no DI, no I/O.

#![allow(dead_code)]  // ROADMAP parity port: kept for planned wiring; prune when the Q-item closes.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const MAX_HISTORY: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StageMetrics {
    pub name: String,
    pub status: String,
    pub duration_ms: u64,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PipelineMetrics {
    pub pipeline_id: String,
    pub project_id: String,
    pub start_time: u64,
    pub end_time: u64,
    pub duration_ms: u64,
    pub success: bool,
    pub stages: Vec<StageMetrics>,
    pub files_generated: usize,
    pub files_failed: usize,
    pub retry_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AggregatedPipelineMetrics {
    pub total_pipelines: usize,
    pub successful_pipelines: usize,
    pub failed_pipelines: usize,
    pub average_duration_ms: f32,
    pub total_files_generated: usize,
    pub total_files_failed: usize,
    pub stage_success_rate: BTreeMap<String, f32>,
}

pub struct PipelineMetricsCollector {
    history: Vec<PipelineMetrics>,
}

impl PipelineMetricsCollector {
    pub fn new() -> Self {
        Self { history: Vec::new() }
    }

    pub fn record_pipeline(&mut self, metrics: PipelineMetrics) {
        self.history.push(metrics);
        if self.history.len() > MAX_HISTORY {
            self.history.remove(0);
        }
    }

    pub fn get_history(&self, limit: usize) -> &[PipelineMetrics] {
        let start = self.history.len().saturating_sub(limit);
        &self.history[start..]
    }

    pub fn get_latest(&self) -> Option<&PipelineMetrics> {
        self.history.last()
    }

    pub fn get_aggregated(&self) -> AggregatedPipelineMetrics {
        let total = self.history.len();
        let successful = self.history.iter().filter(|p| p.success).count();
        let failed = total - successful;
        let total_duration: u64 = self.history.iter().map(|p| p.duration_ms).sum();
        let total_generated: usize = self.history.iter().map(|p| p.files_generated).sum();
        let total_failed: usize = self.history.iter().map(|p| p.files_failed).sum();

        let mut stage_stats: BTreeMap<String, (usize, usize)> = BTreeMap::new(); // (success, total)
        for pipeline in &self.history {
            for stage in &pipeline.stages {
                let entry = stage_stats.entry(stage.name.clone()).or_insert((0, 0));
                entry.1 += 1;
                if stage.success {
                    entry.0 += 1;
                }
            }
        }

        let stage_success_rate: BTreeMap<String, f32> = stage_stats
            .iter()
            .map(|(name, (success, total))| {
                (name.clone(), if *total > 0 { *success as f32 / *total as f32 } else { 0.0 })
            })
            .collect();

        AggregatedPipelineMetrics {
            total_pipelines: total,
            successful_pipelines: successful,
            failed_pipelines: failed,
            average_duration_ms: if total > 0 { total_duration as f32 / total as f32 } else { 0.0 },
            total_files_generated: total_generated,
            total_files_failed: total_failed,
            stage_success_rate,
        }
    }

    pub fn clear(&mut self) {
        self.history.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_metrics(id: &str, success: bool) -> PipelineMetrics {
        PipelineMetrics {
            pipeline_id: id.into(),
            project_id: "proj-1".into(),
            start_time: 1000,
            end_time: 2000,
            duration_ms: 1000,
            success,
            stages: vec![StageMetrics {
                name: "generate".into(),
                status: if success { "completed" } else { "failed" }.into(),
                duration_ms: 500,
                success,
                error: if success { None } else { Some("error".into()) },
            }],
            files_generated: if success { 9 } else { 0 },
            files_failed: if success { 0 } else { 9 },
            retry_count: 0,
        }
    }

    #[test]
    fn test_record_and_aggregate() {
        let mut collector = PipelineMetricsCollector::new();
        collector.record_pipeline(sample_metrics("run-1", true));
        collector.record_pipeline(sample_metrics("run-2", false));

        let agg = collector.get_aggregated();
        assert_eq!(agg.total_pipelines, 2);
        assert_eq!(agg.successful_pipelines, 1);
        assert_eq!(agg.failed_pipelines, 1);
        assert_eq!(agg.total_files_generated, 9);
        assert_eq!(agg.total_files_failed, 9);
        assert!((agg.average_duration_ms - 1000.0).abs() < 0.1);
        let rate = agg.stage_success_rate.get("generate").unwrap();
        assert!((rate - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_history_limit() {
        let mut collector = PipelineMetricsCollector::new();
        for i in 0..10 {
            collector.record_pipeline(sample_metrics(&format!("run-{i}"), true));
        }
        let history = collector.get_history(5);
        assert_eq!(history.len(), 5);
        assert_eq!(history[0].pipeline_id, "run-5");
    }

    #[test]
    fn test_latest() {
        let mut collector = PipelineMetricsCollector::new();
        assert!(collector.get_latest().is_none());
        collector.record_pipeline(sample_metrics("run-1", true));
        assert_eq!(collector.get_latest().unwrap().pipeline_id, "run-1");
    }
}
