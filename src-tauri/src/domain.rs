use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;
use uuid::Uuid;

pub type Result<T, E = DomainError> = std::result::Result<T, E>;

#[derive(Debug, Error, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DomainError {
    #[error("not found: {message}")]
    NotFound { message: String },
    #[error("invalid input: {message}")]
    InvalidInput { message: String },
    #[error("internal: {message}")]
    Internal { message: String },
    #[error("unauthorized")]
    Unauthorized,
    #[error("conflict: {message}")]
    Conflict { message: String },
    #[error("unavailable: {message}")]
    Unavailable { message: String },
}

impl From<rusqlite::Error> for DomainError {
    fn from(error: rusqlite::Error) -> Self {
        DomainError::internal(error.to_string())
    }
}

impl From<std::io::Error> for DomainError {
    fn from(error: std::io::Error) -> Self {
        DomainError::internal(error.to_string())
    }
}

impl From<serde_json::Error> for DomainError {
    fn from(error: serde_json::Error) -> Self {
        DomainError::internal(error.to_string())
    }
}

impl DomainError {
    pub fn not_found(message: impl Into<String>) -> Self {
        DomainError::NotFound {
            message: message.into(),
        }
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        DomainError::InvalidInput {
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        DomainError::Internal {
            message: message.into(),
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        DomainError::Conflict {
            message: message.into(),
        }
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        DomainError::Unavailable {
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EntityId(String);

impl EntityId {
    pub fn new() -> Self {
        EntityId(Uuid::new_v4().to_string())
    }

    pub fn from_string(value: impl Into<String>) -> Self {
        EntityId(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for EntityId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_wraps_data() {
        let result: Result<i32> = Ok(42);
        assert!(result.is_ok());
    }

    #[test]
    fn failure_wraps_error() {
        let result: Result<i32> = Err(DomainError::not_found("x"));
        assert!(result.is_err());
    }

    #[test]
    fn entity_id_generates_unique_values() {
        let a = EntityId::new();
        let b = EntityId::new();
        assert_ne!(a.as_str(), b.as_str());
    }
}
