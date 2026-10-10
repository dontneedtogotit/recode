use std::path::Path;

use crate::workspace::{LocalWorkspaceService, WorkspaceService};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateWorkspaceInput {
    pub id: String,
    pub root: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateWorkspaceOutput {
    pub id: String,
    pub root: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("internal: {0}")]
    Internal(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

impl serde::Serialize for CommandError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub trait CreateWorkspaceCommand {
    fn create_workspace(
        &self,
        input: CreateWorkspaceInput,
    ) -> Result<CreateWorkspaceOutput, CommandError>;
}

pub struct DefaultCreateWorkspaceCommand {
    pub service: LocalWorkspaceService,
}

impl CreateWorkspaceCommand for DefaultCreateWorkspaceCommand {
    fn create_workspace(
        &self,
        input: CreateWorkspaceInput,
    ) -> Result<CreateWorkspaceOutput, CommandError> {
        let info = self
            .service
            .create_workspace(&input.id, Path::new(&input.root))
            .map_err(|e| CommandError::Internal(e.to_string()))?;
        Ok(CreateWorkspaceOutput {
            id: info.id,
            root: info.root.to_string_lossy().to_string(),
        })
    }
}
