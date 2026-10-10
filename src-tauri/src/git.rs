use super::domain::DomainError;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GitStatus {
    pub branch: String,
    pub changed: Vec<String>,
    pub staged: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GitInitOutput {
    pub initialized: bool,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GitCommitOutput {
    pub committed: bool,
    pub message: String,
}

pub trait GitService {
    fn status(&self, root: &Path) -> Result<GitStatus, DomainError>;
    fn init(&self, root: &Path) -> Result<GitInitOutput, DomainError>;
    fn add_all(&self, root: &Path) -> Result<(), DomainError>;
    fn commit(&self, root: &Path, message: &str) -> Result<GitCommitOutput, DomainError>;
}

pub struct LocalGitService;

impl GitService for LocalGitService {
    fn status(&self, root: &Path) -> Result<GitStatus, DomainError> {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .arg("status")
            .arg("--porcelain")
            .arg("-b")
            .output()
            .map_err(|error| DomainError::unavailable(error.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DomainError::unavailable(format!(
                "git status failed: {}",
                stderr.trim()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut branch = String::new();
        let mut changed = Vec::new();

        for line in stdout.lines() {
            if line.starts_with("## ") {
                branch = line.trim_start_matches("## ").to_string();
                if branch.starts_with("No commits yet on ") {
                    branch = branch.trim_start_matches("No commits yet on ").to_string();
                } else {
                    branch = branch
                        .split_once("...")
                        .map(|(b, _)| b)
                        .unwrap_or(&branch)
                        .to_string();
                }
                continue;
            }

            if line.len() < 4 {
                continue;
            }

            let path = line[3..].trim().to_string();
            match &line[0..2] {
                "??" | " M" | "D " | "A " | "M " | "R " | "C " | "U " => changed.push(path),
                _ => {}
            }
        }

        Ok(GitStatus {
            branch,
            changed,
            staged: vec![],
        })
    }

    fn init(&self, root: &Path) -> Result<GitInitOutput, DomainError> {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .arg("init")
            .output()
            .map_err(|error| DomainError::unavailable(error.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DomainError::unavailable(format!(
                "git init failed: {}",
                stderr.trim()
            )));
        }

        Ok(GitInitOutput {
            initialized: true,
            message: "initialized git repository".into(),
        })
    }

    fn add_all(&self, root: &Path) -> Result<(), DomainError> {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .arg("add")
            .arg(".")
            .output()
            .map_err(|error| DomainError::unavailable(error.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DomainError::unavailable(format!(
                "git add failed: {}",
                stderr.trim()
            )));
        }

        Ok(())
    }

    fn commit(&self, root: &Path, message: &str) -> Result<GitCommitOutput, DomainError> {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .arg("commit")
            .arg("-m")
            .arg(message)
            .output()
            .map_err(|error| DomainError::unavailable(error.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DomainError::unavailable(format!(
                "git commit failed: {}",
                stderr.trim()
            )));
        }

        Ok(GitCommitOutput {
            committed: true,
            message: "committed changes".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    struct FakeGitService {
        status: GitStatus,
        init_error: Option<DomainError>,
        commit_output: GitCommitOutput,
    }

    impl GitService for FakeGitService {
        fn status(&self, _root: &Path) -> Result<GitStatus, DomainError> {
            Ok(self.status.clone())
        }

        fn init(&self, _root: &Path) -> Result<GitInitOutput, DomainError> {
            self.init_error.clone().map_or(
                Ok(GitInitOutput {
                    initialized: true,
                    message: "fake init".into(),
                }),
                Err,
            )
        }

        fn add_all(&self, _root: &Path) -> Result<(), DomainError> {
            Ok(())
        }

        fn commit(&self, _root: &Path, _message: &str) -> Result<GitCommitOutput, DomainError> {
            Ok(self.commit_output.clone())
        }
    }

    #[test]
    fn status_reports_branch_and_changed_files() {
        let service = FakeGitService {
            status: GitStatus {
                branch: "main".into(),
                changed: vec!["src/main.rs".into()],
                staged: vec![],
            },
            init_error: None,
            commit_output: GitCommitOutput {
                committed: true,
                message: "ok".into(),
            },
        };

        let status = service.status(Path::new("/tmp")).expect("git status");
        assert_eq!(status.branch, "main");
        assert_eq!(status.changed, vec!["src/main.rs"]);
    }

    #[test]
    fn init_returns_error_when_git_is_unavailable() {
        let service = FakeGitService {
            status: GitStatus {
                branch: String::new(),
                changed: Vec::new(),
                staged: Vec::new(),
            },
            init_error: Some(DomainError::unavailable("git binary missing")),
            commit_output: GitCommitOutput {
                committed: false,
                message: String::new(),
            },
        };

        let result = service.init(Path::new("/tmp"));
        assert!(result.is_err());
    }

    #[test]
    fn commit_returns_output_on_success() {
        let service = FakeGitService {
            status: GitStatus {
                branch: String::new(),
                changed: Vec::new(),
                staged: Vec::new(),
            },
            init_error: None,
            commit_output: GitCommitOutput {
                committed: true,
                message: "committed changes".into(),
            },
        };

        let result = service
            .commit(Path::new("/tmp"), "chore: init")
            .expect("commit");
        assert!(result.committed);
        assert_eq!(result.message, "committed changes");
    }

    #[test]
    fn local_service_status_parses_porcelain_output() {
        let service = LocalGitService;
        let repo = tempfile::tempdir().expect("temp repo");
        let _ = std::process::Command::new("git")
            .arg("-C")
            .arg(repo.path())
            .arg("init")
            .arg("-b")
            .arg("main")
            .output();
        std::fs::write(repo.path().join("README.md"), "hello").expect("write readme");
        std::fs::write(repo.path().join("main.rs"), "fn main() {}").expect("write source");

        let status = service.status(repo.path()).expect("git status");
        assert_eq!(status.branch, "main");
        assert_eq!(status.changed, vec!["README.md", "main.rs"]);
    }

    #[test]
    fn local_service_commit_fails_when_git_is_unavailable() {
        let service = LocalGitService;
        let repo = tempfile::tempdir().expect("temp repo");
        let result = service.commit(repo.path(), "no-git");
        assert!(result.is_err());
    }
}
