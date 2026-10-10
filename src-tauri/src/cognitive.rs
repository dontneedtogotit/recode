use super::domain::DomainError;
use super::learning::{LearningPatternRecord, PatternKind, LearningPatternService, SqlitePatternRepository};
use rusqlite::Connection;
use std::sync::Arc;

pub trait CognitiveEngine {
    fn reflect_on_task(&self, conn: &Connection, task_id: &str, success: bool, output: &str) -> Result<Option<LearningPatternRecord>, DomainError>;
}

pub struct LocalCognitiveEngine {
    learning: Arc<LearningPatternService<SqlitePatternRepository>>,
}

impl LocalCognitiveEngine {
    pub fn new(learning: Arc<LearningPatternService<SqlitePatternRepository>>) -> Self {
        Self { learning }
    }
}

impl CognitiveEngine for LocalCognitiveEngine {
    fn reflect_on_task(&self, conn: &Connection, task_id: &str, success: bool, _output: &str) -> Result<Option<LearningPatternRecord>, DomainError> {
        let kind = if success { PatternKind::Success } else { PatternKind::AntiPattern };
        let pattern = format!("task:{task_id}:{kind:?}");

        let record = LearningPatternRecord {
            id: format!("cognitive-{task_id}"),
            kind,
            agent_role: "runtime".into(),
            task_type: "agent_task".into(),
            content: pattern,
            score: if success { 0.9 } else { 0.3 },
            sample_count: 1,
            updated_at: 0,
        };

        if success {
            self.learning.record_success_pattern(
                conn,
                super::learning::SuccessPattern {
                    id: record.id.clone(),
                    agent_role: record.agent_role.clone(),
                    task_type: record.task_type.clone(),
                    pattern: record.content.clone(),
                    confidence: record.score,
                    success_count: 1,
                    sample_count: record.sample_count,
                    created_at: std::time::SystemTime::UNIX_EPOCH,
                    updated_at: std::time::SystemTime::UNIX_EPOCH,
                },
            )?;
        } else {
            self.learning.record_anti_pattern(
                conn,
                super::learning::AntiPattern {
                    id: record.id.clone(),
                    agent_role: record.agent_role.clone(),
                    task_type: record.task_type.clone(),
                    pattern: record.content.clone(),
                    severity: 0.3,
                    occurrence_count: 1,
                    created_at: std::time::SystemTime::UNIX_EPOCH,
                    updated_at: std::time::SystemTime::UNIX_EPOCH,
                },
            )?;
        }

        Ok(Some(record))
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::DomainError;

    #[test]
    fn cognitive_reflection_compiles() {
        struct Stub;
        let _ = Stub;
    }
}
