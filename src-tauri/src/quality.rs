use super::domain::DomainError;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QualityBaseline {
    pub file_path: String,
    pub line_count: usize,
    pub generated_at: SystemTime,
    pub checksum: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QualityFinding {
    pub file_path: String,
    pub rule: String,
    pub severity: FindingSeverity,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum FindingSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QualityReport {
    pub findings: Vec<QualityFinding>,
    pub critical_count: usize,
    pub warning_count: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScanOptions {
    pub root: PathBuf,
    pub include_tests: bool,
    pub max_critical_findings: usize,
    pub max_warning_findings: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            include_tests: true,
            max_critical_findings: 0,
            max_warning_findings: 10,
        }
    }
}

pub trait QualityRepository {
    fn ensure_schema(&self, conn: &Connection) -> Result<(), DomainError>;
    fn save_baseline(
        &self,
        conn: &Connection,
        baseline: &QualityBaseline,
    ) -> Result<(), DomainError>;
    fn list_baselines(
        &self,
        conn: &Connection,
    ) -> Result<Vec<QualityBaseline>, DomainError>;
    fn delete_baseline(&self, conn: &Connection, file_path: &str) -> Result<(), DomainError>;
}

#[derive(Debug, Clone)]
pub struct SqliteQualityRepository;

impl QualityRepository for SqliteQualityRepository {
    fn ensure_schema(&self, conn: &Connection) -> Result<(), DomainError> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS quality_baselines (\n             file_path TEXT PRIMARY KEY,\n             line_count INTEGER NOT NULL,\n             generated_at INTEGER NOT NULL,\n             checksum TEXT NOT NULL\n             )",
            [],
        )
        .map(|_| ())
        .map_err(DomainError::from)
    }

    fn save_baseline(
        &self,
        conn: &Connection,
        baseline: &QualityBaseline,
    ) -> Result<(), DomainError> {
        conn.execute(
            "INSERT INTO quality_baselines (file_path, line_count, generated_at, checksum)\n             VALUES (?1, ?2, ?3, ?4)\n             ON CONFLICT(file_path) DO UPDATE SET line_count = excluded.line_count, generated_at = excluded.generated_at, checksum = excluded.checksum",
            rusqlite::params![
                baseline.file_path,
                baseline.line_count as i64,
                baseline.generated_at.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0),
                baseline.checksum,
            ],
        )
        .map(|_| ())
        .map_err(DomainError::from)
    }

    fn list_baselines(
        &self,
        conn: &Connection,
    ) -> Result<Vec<QualityBaseline>, DomainError> {
        let mut stmt = conn
            .prepare("SELECT file_path, line_count, generated_at, checksum FROM quality_baselines")
            .map_err(DomainError::from)?;
        let rows = stmt
            .query_map([], |row| {
                Ok(QualityBaseline {
                    file_path: row.get(0)?,
                    line_count: row.get::<_, i64>(1)? as usize,
                    generated_at: SystemTime::UNIX_EPOCH
                        + std::time::Duration::from_millis(row.get::<_, i64>(2)? as u64),
                    checksum: row.get(3)?,
                })
            })
            .map_err(DomainError::from)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(DomainError::from)
    }

    fn delete_baseline(&self, conn: &Connection, file_path: &str) -> Result<(), DomainError> {
        conn.execute(
            "DELETE FROM quality_baselines WHERE file_path = ?1",
            [file_path],
        )
        .map(|_| ())
        .map_err(DomainError::from)
    }
}

#[derive(Debug, Clone)]
pub struct QualityScanner<R: QualityRepository> {
    repository: R,
}

impl<R: QualityRepository> QualityScanner<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub fn record_baseline(
        &self,
        conn: &Connection,
        file_path: &str,
        content: &str,
    ) -> Result<(), DomainError> {
        let baseline = QualityBaseline {
            file_path: file_path.to_string(),
            line_count: content.lines().count(),
            generated_at: SystemTime::now(),
            checksum: Self::content_checksum(content),
        };
        self.repository.save_baseline(conn, &baseline)
    }

    pub fn list_baselines(
        &self,
        conn: &Connection,
    ) -> Result<Vec<QualityBaseline>, DomainError> {
        self.repository.list_baselines(conn)
    }

    pub fn delete_baseline(
        &self,
        conn: &Connection,
        file_path: &str,
    ) -> Result<(), DomainError> {
        self.repository.delete_baseline(conn, file_path)
    }

    pub fn scan(
        &self,
        conn: &Connection,
        root: &Path,
        options: &ScanOptions,
    ) -> Result<QualityReport, DomainError> {
        let mut findings = Vec::new();
        let entries = Self::collect_files(root, options.include_tests)?;

        for path in entries {
            let content = std::fs::read_to_string(&path)
                .map_err(|error| DomainError::internal(error.to_string()))?;
            let file_path = path.to_string_lossy().to_string();

            findings.extend(Self::check_empty_file(&file_path, &content));
            findings.extend(Self::check_todos(&file_path, &content));
            findings.extend(Self::check_placeholder_comments(&file_path, &content));
            findings.extend(Self::check_line_length(&file_path, &content));

            let baselines = self.repository.list_baselines(conn)?;
            findings.extend(Self::check_baseline_drift(&file_path, &content, baselines));
        }

        let critical_count = findings
            .iter()
            .filter(|finding| finding.severity == FindingSeverity::Critical)
            .count();
        let warning_count = findings
            .iter()
            .filter(|finding| finding.severity == FindingSeverity::Warning)
            .count();

        Ok(QualityReport {
            findings,
            critical_count,
            warning_count,
        })
    }

    pub fn baseline_drift(
        &self,
        conn: &Connection,
        root: &Path,
    ) -> Result<Vec<QualityFinding>, DomainError> {
        let mut findings = Vec::new();
        let entries = Self::collect_files(root, true)?;
        let baselines = self.repository.list_baselines(conn)?;
        let baseline_map: std::collections::HashMap<_, _> = baselines
            .into_iter()
            .map(|baseline| (baseline.file_path.clone(), baseline))
            .collect();

        for path in entries {
            let content = std::fs::read_to_string(&path)
                .map_err(|error| DomainError::internal(error.to_string()))?;
            let file_path = path.to_string_lossy().to_string();

            if let Some(baseline) = baseline_map.get(file_path.as_str()) {
                let current_lines = content.lines().count();
                if current_lines != baseline.line_count {
                    findings.push(QualityFinding {
                        file_path,
                        rule: "baseline_line_drift".into(),
                        severity: FindingSeverity::Warning,
                        message: format!(
                            "line count changed from {} to {}",
                            baseline.line_count, current_lines
                        ),
                    });
                }
            }
        }

        Ok(findings)
    }

    pub fn update_baselines(
        &self,
        conn: &Connection,
        root: &Path,
        include_tests: bool,
    ) -> Result<(), DomainError> {
        let entries = Self::collect_files(root, include_tests)?;
        for path in entries {
            let content = std::fs::read_to_string(&path)
                .map_err(|error| DomainError::internal(error.to_string()))?;
            self.record_baseline(conn, &path.to_string_lossy(), &content)?;
        }
        Ok(())
    }

    fn collect_files(root: &Path, include_tests: bool) -> Result<Vec<PathBuf>, DomainError> {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(root)
            .map_err(|error| DomainError::internal(error.to_string()))?
        {
            let entry = entry.map_err(|error| DomainError::internal(error.to_string()))?;
            let path = entry.path();
            if path.is_dir() {
                entries.extend(Self::collect_files(&path, include_tests)?);
            } else if let Some(extension) = path.extension() {
                if matches!(
                    extension.to_string_lossy().as_ref(),
                    "rs" | "ts" | "tsx" | "json" | "md"
                ) {
                    if !include_tests && path.to_string_lossy().contains("test") {
                        continue;
                    }
                    entries.push(path);
                }
            }
        }
        entries.sort();
        Ok(entries)
    }

    fn check_empty_file(file_path: &str, content: &str) -> Vec<QualityFinding> {
        if content.trim().is_empty() {
            return vec![QualityFinding {
                file_path: file_path.to_string(),
                rule: "empty_file".into(),
                severity: FindingSeverity::Critical,
                message: "file is empty".into(),
            }];
        }
        Vec::new()
    }

    fn check_todos(file_path: &str, content: &str) -> Vec<QualityFinding> {
        let mut findings = Vec::new();
        for (index, line) in content.lines().enumerate() {
            let trimmed = line.trim().to_lowercase();
            if trimmed.contains("todo:") || trimmed.contains("fixme:") {
                findings.push(QualityFinding {
                    file_path: file_path.to_string(),
                    rule: "todo_comment".into(),
                    severity: FindingSeverity::Warning,
                    message: format!("line {} contains TODO/FIXME: {}", index + 1, line.trim()),
                });
            }
        }
        findings
    }

    fn check_placeholder_comments(file_path: &str, content: &str) -> Vec<QualityFinding> {
        let placeholders = [
            "placeholder",
            "todo: implement",
            "fixme: implement",
            "not implemented",
            "stub",
        ];
        let mut findings = Vec::new();
        for (index, line) in content.lines().enumerate() {
            let lowered = line.trim().to_lowercase();
            if placeholders.iter().any(|token| lowered.contains(token)) {
                findings.push(QualityFinding {
                    file_path: file_path.to_string(),
                    rule: "placeholder_comment".into(),
                    severity: FindingSeverity::Critical,
                    message: format!("line {} looks like placeholder: {}", index + 1, line.trim()),
                });
            }
        }
        findings
    }

    fn check_line_length(file_path: &str, content: &str) -> Vec<QualityFinding> {
        let mut findings = Vec::new();
        for (index, line) in content.lines().enumerate() {
            if line.len() > 120 {
                findings.push(QualityFinding {
                    file_path: file_path.to_string(),
                    rule: "line_length".into(),
                    severity: FindingSeverity::Warning,
                    message: format!(
                        "line {} exceeds 120 characters: {} chars",
                        index + 1,
                        line.len()
                    ),
                });
            }
        }
        findings
    }

    fn check_baseline_drift(
        file_path: &str,
        content: &str,
        baselines: Vec<QualityBaseline>,
    ) -> Vec<QualityFinding> {
        let mut findings = Vec::new();
        let current_lines = content.lines().count();
        for baseline in baselines {
            if baseline.file_path == file_path && baseline.line_count != current_lines {
                findings.push(QualityFinding {
                    file_path: file_path.to_string(),
                    rule: "baseline_drift".into(),
                    severity: FindingSeverity::Warning,
                    message: format!(
                        "baseline line count {} does not match current {}",
                        baseline.line_count, current_lines
                    ),
                });
            }
        }
        findings
    }

    fn content_checksum(content: &str) -> String {
        use std::hash::{DefaultHasher, Hasher};
        let mut hasher = DefaultHasher::new();
        hasher.write(content.as_bytes());
        format!("{:x}", hasher.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Debug, Clone)]
    struct InMemoryQualityRepository {
        store: Arc<std::sync::Mutex<HashMap<String, QualityBaseline>>>,
    }

    impl InMemoryQualityRepository {
        fn new() -> Self {
            Self {
                store: Arc::new(std::sync::Mutex::new(HashMap::new())),
            }
        }
    }

    impl QualityRepository for InMemoryQualityRepository {
        fn ensure_schema(&self, _conn: &Connection) -> Result<(), DomainError> {
            Ok(())
        }

        fn save_baseline(
            &self,
            _conn: &Connection,
            baseline: &QualityBaseline,
        ) -> Result<(), DomainError> {
            self.store
                .lock()
                .expect("lock store")
                .insert(baseline.file_path.clone(), baseline.clone());
            Ok(())
        }

        fn list_baselines(
            &self,
            _conn: &Connection,
        ) -> Result<Vec<QualityBaseline>, DomainError> {
            let store = self.store.lock().expect("lock store");
            Ok(store.values().cloned().collect())
        }

        fn delete_baseline(&self, _conn: &Connection, file_path: &str) -> Result<(), DomainError> {
            self.store.lock().expect("lock store").remove(file_path);
            Ok(())
        }
    }

    #[test]
    fn empty_file_emits_critical_finding() {
        let findings =
            QualityScanner::<InMemoryQualityRepository>::check_empty_file("empty.rs", "");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, FindingSeverity::Critical);
    }

    #[test]
    fn todo_comments_emit_warning_findings() {
        let content = "// TODO: wire IPC\nfn main() {}\n";
        let findings =
            QualityScanner::<InMemoryQualityRepository>::check_todos("app.rs", content);
        assert!(findings
            .iter()
            .any(|finding| finding.rule == "todo_comment"));
    }

    #[test]
    fn placeholder_comments_emit_critical_findings() {
        let content = "// stub implementation\nfn main() {}\n";
        let findings =
            QualityScanner::<InMemoryQualityRepository>::check_placeholder_comments("app.rs", content);
        assert!(findings
            .iter()
            .any(|finding| finding.rule == "placeholder_comment"));
    }

    #[test]
    fn long_lines_emit_warning_findings() {
        let long_line = "let x = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\";";
        let content = format!("fn main() {{\n{}\n}}\n", long_line);
        let findings =
            QualityScanner::<InMemoryQualityRepository>::check_line_length("app.rs", &content);
        assert!(findings.iter().any(|finding| finding.rule == "line_length"));
    }

    #[test]
    fn baseline_drift_detects_line_count_change() {
        let repository = InMemoryQualityRepository::new();
        let scanner = QualityScanner::new(repository.clone());
        let conn = Connection::open_in_memory().expect("in-memory connection");

        let _ = scanner.record_baseline(&conn, "app.rs", "line1\nline2\n");
        let findings = QualityScanner::<InMemoryQualityRepository>::check_baseline_drift(
            "app.rs",
            "line1\nline2\nline3\n",
            repository.list_baselines(&conn).expect("baselines"),
        );
        assert!(findings
            .iter()
            .any(|finding| finding.rule == "baseline_drift"));
    }
}
