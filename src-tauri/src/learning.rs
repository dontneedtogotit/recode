use super::domain::DomainError;
use rusqlite::Connection;
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum PatternKind {
    Success,
    AntiPattern,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SuccessPattern {
    pub id: String,
    pub agent_role: String,
    pub task_type: String,
    pub pattern: String,
    pub confidence: f64,
    pub success_count: u64,
    pub sample_count: u64,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AntiPattern {
    pub id: String,
    pub agent_role: String,
    pub task_type: String,
    pub pattern: String,
    pub severity: f64,
    pub occurrence_count: u64,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LearningPatternRecord {
    pub id: String,
    pub kind: PatternKind,
    pub agent_role: String,
    pub task_type: String,
    pub content: String,
    pub score: f64,
    pub sample_count: u64,
    pub updated_at: i64,
}

pub trait PatternRepository {
    fn ensure_schema(&self, conn: &Connection) -> Result<(), DomainError>;
    fn insert_pattern(
        &self,
        conn: &Connection,
        record: &LearningPatternRecord,
    ) -> Result<(), DomainError>;
    fn list_patterns(
        &self,
        conn: &Connection,
        kind: PatternKind,
    ) -> Result<Vec<LearningPatternRecord>, DomainError>;
    fn upsert_pattern(
        &self,
        conn: &Connection,
        record: &LearningPatternRecord,
    ) -> Result<(), DomainError>;
    fn delete_pattern(&self, conn: &Connection, id: &str) -> Result<(), DomainError>;
}

pub struct SqlitePatternRepository;

impl PatternRepository for SqlitePatternRepository {
    fn ensure_schema(&self, conn: &Connection) -> Result<(), DomainError> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS learning_patterns (\n             id TEXT PRIMARY KEY,\n             kind TEXT NOT NULL,\n             agent_role TEXT NOT NULL,\n             task_type TEXT NOT NULL,\n             content TEXT NOT NULL,\n             score REAL NOT NULL,\n             sample_count INTEGER NOT NULL DEFAULT 1,\n             updated_at INTEGER NOT NULL\n             )",
            [],
        )
        .map(|_| ())
        .map_err(DomainError::from)
    }

    fn insert_pattern(
        &self,
        conn: &Connection,
        record: &LearningPatternRecord,
    ) -> Result<(), DomainError> {
        conn.execute(
            "INSERT INTO learning_patterns (id, kind, agent_role, task_type, content, score, sample_count, updated_at)\n             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                record.id,
                match record.kind {
                    PatternKind::Success => "success",
                    PatternKind::AntiPattern => "anti",
                },
                record.agent_role,
                record.task_type,
                record.content,
                record.score,
                record.sample_count,
                record.updated_at,
            ],
        )
        .map(|_| ())
        .map_err(DomainError::from)
    }

    fn list_patterns(
        &self,
        conn: &Connection,
        kind: PatternKind,
    ) -> Result<Vec<LearningPatternRecord>, DomainError> {
        let kind_str = match kind {
            PatternKind::Success => "success",
            PatternKind::AntiPattern => "anti",
        };
        let mut stmt = conn
            .prepare("SELECT id, kind, agent_role, task_type, content, score, sample_count, updated_at FROM learning_patterns WHERE kind = ?1 ORDER BY score DESC")
            .map_err(DomainError::from)?;
        let rows = stmt
            .query_map([kind_str], |row| {
                Ok(LearningPatternRecord {
                    id: row.get(0)?,
                    kind: if row.get::<_, String>(1)? == "success" {
                        PatternKind::Success
                    } else {
                        PatternKind::AntiPattern
                    },
                    agent_role: row.get(2)?,
                    task_type: row.get(3)?,
                    content: row.get(4)?,
                    score: row.get(5)?,
                    sample_count: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            })
            .map_err(DomainError::from)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(DomainError::from)
    }

    fn upsert_pattern(
        &self,
        conn: &Connection,
        record: &LearningPatternRecord,
    ) -> Result<(), DomainError> {
        let kind_str = match record.kind {
            PatternKind::Success => "success",
            PatternKind::AntiPattern => "anti",
        };
        conn.execute(
            "INSERT INTO learning_patterns (id, kind, agent_role, task_type, content, score, sample_count, updated_at)\n             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)\n             ON CONFLICT(id) DO UPDATE SET score = excluded.score, sample_count = excluded.sample_count, updated_at = excluded.updated_at",
            rusqlite::params![
                record.id,
                kind_str,
                record.agent_role,
                record.task_type,
                record.content,
                record.score,
                record.sample_count,
                record.updated_at,
            ],
        )
        .map(|_| ())
        .map_err(DomainError::from)
    }

    fn delete_pattern(&self, conn: &Connection, id: &str) -> Result<(), DomainError> {
        conn.execute("DELETE FROM learning_patterns WHERE id = ?1", [id])
            .map(|_| ())
            .map_err(DomainError::from)
    }
}

#[derive(Debug, Clone)]
pub struct LearningPatternService<R: PatternRepository> {
    repository: R,
    min_confidence: f64,
    max_patterns: usize,
}

impl<R: PatternRepository> LearningPatternService<R> {
    pub fn new(repository: R) -> Self {
        Self {
            repository,
            min_confidence: 0.5,
            max_patterns: 50,
        }
    }

    pub fn with_thresholds(
        repository: R,
        min_confidence: f64,
        max_patterns: usize,
    ) -> Self {
        Self {
            repository,
            min_confidence,
            max_patterns,
        }
    }

    pub fn record_success_pattern(
        &self,
        conn: &Connection,
        pattern: SuccessPattern,
    ) -> Result<(), DomainError> {
        let score = self.score_success_pattern(&pattern);
        if score < self.min_confidence {
            return Ok(());
        }

        let record = LearningPatternRecord {
            id: pattern.id,
            kind: PatternKind::Success,
            agent_role: pattern.agent_role,
            task_type: pattern.task_type,
            content: pattern.pattern,
            score,
            sample_count: pattern.sample_count,
            updated_at: self.to_unix_ms(pattern.updated_at),
        };

        self.repository.upsert_pattern(conn, &record)
    }

    pub fn record_anti_pattern(
        &self,
        conn: &Connection,
        pattern: AntiPattern,
    ) -> Result<(), DomainError> {
        if pattern.severity <= 0.0 {
            return Ok(());
        }

        let record = LearningPatternRecord {
            id: pattern.id,
            kind: PatternKind::AntiPattern,
            agent_role: pattern.agent_role,
            task_type: pattern.task_type,
            content: pattern.pattern,
            score: pattern.severity,
            sample_count: pattern.occurrence_count,
            updated_at: self.to_unix_ms(pattern.updated_at),
        };

        self.repository.upsert_pattern(conn, &record)
    }

    pub fn top_success_patterns(
        &self,
        conn: &Connection,
        agent_role: &str,
        task_type: &str,
    ) -> Result<Vec<LearningPatternRecord>, DomainError> {
        let all = self.repository.list_patterns(conn, PatternKind::Success)?;
        let mut filtered: Vec<_> = all
            .into_iter()
            .filter(|pattern| {
                pattern.score >= self.min_confidence
                    && pattern.agent_role == agent_role
                    && pattern.task_type == task_type
            })
            .collect();
        filtered.truncate(self.max_patterns);
        Ok(filtered)
    }

    pub fn top_anti_patterns(
        &self,
        conn: &Connection,
        agent_role: &str,
        task_type: &str,
    ) -> Result<Vec<LearningPatternRecord>, DomainError> {
        let all = self
            .repository
            .list_patterns(conn, PatternKind::AntiPattern)?;
        let mut filtered: Vec<_> = all
            .into_iter()
            .filter(|pattern| {
                pattern.score > 0.0
                    && pattern.agent_role == agent_role
                    && pattern.task_type == task_type
            })
            .collect();
        filtered.truncate(self.max_patterns);
        Ok(filtered)
    }

    pub fn evict_if_over_limit(&self, conn: &Connection) -> Result<(), DomainError> {
        let all = self.repository.list_patterns(conn, PatternKind::Success)?;
        if all.len() <= self.max_patterns {
            return Ok(());
        }

        let mut sorted = all;
        sorted.sort_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let evict_count = sorted.len() - self.max_patterns;
        for pattern in sorted.into_iter().take(evict_count) {
            self.repository.delete_pattern(conn, &pattern.id)?;
        }
        Ok(())
    }

    fn score_success_pattern(&self, pattern: &SuccessPattern) -> f64 {
        if pattern.sample_count == 0 {
            return 0.0;
        }
        let success_rate = pattern.success_count as f64 / pattern.sample_count as f64;
        let recency = self.recency_score(pattern.updated_at);
        (success_rate * 0.7 + recency * 0.3).clamp(0.0, 1.0)
    }

    fn recency_score(&self, updated_at: SystemTime) -> f64 {
        let elapsed = updated_at
            .elapsed()
            .unwrap_or(Duration::from_secs(0))
            .as_secs_f64();
        let half_life = Duration::from_secs(60 * 60 * 24 * 30).as_secs_f64();
        let decay = 0.5_f64.powf(elapsed / half_life.max(1.0));
        decay.clamp(0.0, 1.0)
    }

    fn to_unix_ms(&self, time: SystemTime) -> i64 {
        time.duration_since(SystemTime::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or(0)
    }
}

use std::sync::Arc;

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    struct InMemoryPatternRepository {
        store: Arc<std::sync::Mutex<Vec<LearningPatternRecord>>>,
    }

    impl InMemoryPatternRepository {
        fn new() -> Self {
            Self {
                store: Arc::new(std::sync::Mutex::new(Vec::new())),
            }
        }
    }

    impl PatternRepository for InMemoryPatternRepository {
        fn ensure_schema(&self, _conn: &Connection) -> Result<(), DomainError> {
            Ok(())
        }

        fn insert_pattern(
            &self,
            _conn: &Connection,
            record: &LearningPatternRecord,
        ) -> Result<(), DomainError> {
            self.store.lock().expect("lock store").push(record.clone());
            Ok(())
        }

        fn list_patterns(
            &self,
            _conn: &Connection,
            kind: PatternKind,
        ) -> Result<Vec<LearningPatternRecord>, DomainError> {
            let store = self.store.lock().expect("lock store");
            Ok(store
                .iter()
                .filter(|record| record.kind == kind)
                .cloned()
                .collect())
        }

        fn upsert_pattern(
            &self,
            _conn: &Connection,
            record: &LearningPatternRecord,
        ) -> Result<(), DomainError> {
            let mut store = self.store.lock().expect("lock store");
            if let Some(existing) = store.iter_mut().find(|item| item.id == record.id) {
                *existing = record.clone();
            } else {
                store.push(record.clone());
            }
            Ok(())
        }

        fn delete_pattern(&self, _conn: &Connection, id: &str) -> Result<(), DomainError> {
            let mut store = self.store.lock().expect("lock store");
            store.retain(|record| record.id != id);
            Ok(())
        }
    }

    #[test]
    fn records_success_pattern_above_threshold() {
        let repository = InMemoryPatternRepository::new();
        let service = LearningPatternService::new(repository.clone());

        let conn = Connection::open_in_memory().expect("in-memory connection");
        let _ = service.record_success_pattern(
            &conn,
            SuccessPattern {
                id: "success-1".into(),
                agent_role: "coder".into(),
                task_type: "refactor".into(),
                pattern: "always run tests before committing".into(),
                confidence: 0.95,
                success_count: 9,
                sample_count: 10,
                created_at: SystemTime::now(),
                updated_at: SystemTime::now(),
            },
        );

        let patterns = repository
            .list_patterns(&conn, PatternKind::Success)
            .expect("list patterns");
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].content, "always run tests before committing");
        assert_eq!(patterns[0].kind, PatternKind::Success);
    }

    #[test]
    fn ignores_low_confidence_success_patterns() {
        let repository = InMemoryPatternRepository::new();
        let service = LearningPatternService::with_thresholds(repository.clone(), 0.9, 10);

        let conn = Connection::open_in_memory().expect("in-memory connection");
        let _ = service.record_success_pattern(
            &conn,
            SuccessPattern {
                id: "success-2".into(),
                agent_role: "coder".into(),
                task_type: "refactor".into(),
                pattern: "low confidence pattern".into(),
                confidence: 0.2,
                success_count: 2,
                sample_count: 10,
                created_at: SystemTime::now(),
                updated_at: SystemTime::now(),
            },
        );

        let patterns = repository
            .list_patterns(&conn, PatternKind::Success)
            .expect("list patterns");
        assert!(patterns.is_empty());
    }

    #[test]
    fn records_anti_patterns() {
        let repository = InMemoryPatternRepository::new();
        let service = LearningPatternService::new(repository.clone());

        let conn = Connection::open_in_memory().expect("in-memory connection");
        let _ = service.record_anti_pattern(
            &conn,
            AntiPattern {
                id: "anti-1".into(),
                agent_role: "reviewer".into(),
                task_type: "security".into(),
                pattern: "skip auth checks when under time pressure".into(),
                severity: 0.85,
                occurrence_count: 3,
                created_at: SystemTime::now(),
                updated_at: SystemTime::now(),
            },
        );

        let patterns = repository
            .list_patterns(&conn, PatternKind::AntiPattern)
            .expect("list patterns");
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].kind, PatternKind::AntiPattern);
        assert_eq!(
            patterns[0].content,
            "skip auth checks when under time pressure"
        );
    }

    #[test]
    fn eviction_removes_lowest_scored_patterns_when_over_limit() {
        let repository = InMemoryPatternRepository::new();
        let service = LearningPatternService::with_thresholds(repository.clone(), 0.0, 2);

        let conn = Connection::open_in_memory().expect("in-memory connection");
        for index in 0..3 {
            let _ = service.record_success_pattern(
                &conn,
                SuccessPattern {
                    id: format!("success-{}", index),
                    agent_role: "coder".into(),
                    task_type: "refactor".into(),
                    pattern: format!("pattern {}", index),
                    confidence: 1.0,
                    success_count: 1,
                    sample_count: 1,
                    created_at: SystemTime::now(),
                    updated_at: SystemTime::now(),
                },
            );
        }

        let _ = service.evict_if_over_limit(&conn);
        let patterns = repository
            .list_patterns(&conn, PatternKind::Success)
            .expect("list patterns");
        assert_eq!(patterns.len(), 2);
        assert!(
            patterns.iter().any(|pattern| pattern.id == "success-2"),
            "expected highest-scored pattern to remain"
        );
    }

    #[test]
    fn filters_patterns_by_agent_role_and_task_type() {
        let repository = InMemoryPatternRepository::new();
        let service = LearningPatternService::new(repository.clone());

        let conn = Connection::open_in_memory().expect("in-memory connection");
        let _ = service.record_success_pattern(
            &conn,
            SuccessPattern {
                id: "success-filter".into(),
                agent_role: "planner".into(),
                task_type: "planning".into(),
                pattern: "break large goals into acceptance criteria".into(),
                confidence: 0.9,
                success_count: 8,
                sample_count: 10,
                created_at: SystemTime::now(),
                updated_at: SystemTime::now(),
            },
        );

        let patterns = service
            .top_success_patterns(&conn, "planner", "planning")
            .expect("top patterns");
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].agent_role, "planner");
        assert_eq!(patterns[0].task_type, "planning");
    }
}
