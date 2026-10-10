use crate::domain::DomainError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum GenerationErrorKind {
    NotFound(String),
    WriteFailed(String),
    PromptFailed(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GenerationError {
    pub kind: GenerationErrorKind,
}

impl GenerationError {
    pub fn new(kind: GenerationErrorKind) -> Self {
        Self { kind }
    }
}

impl From<GenerationError> for DomainError {
    fn from(value: GenerationError) -> Self {
        DomainError::internal(value.to_string())
    }
}

impl std::fmt::Display for GenerationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            GenerationErrorKind::NotFound(path) => write!(formatter, "generation file not found: {path}"),
            GenerationErrorKind::WriteFailed(path) => write!(formatter, "generation write failed: {path}"),
            GenerationErrorKind::PromptFailed(message) => write!(formatter, "generation prompt failed: {message}"),
        }
    }
}

impl std::error::Error for GenerationError {}
