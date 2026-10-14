use nanomgg_git_engine::GitError;
use serde::Serialize;
use specta::Type;

/// Error returned by every IPC command. Serialized as `{ kind, message }` so the
/// frontend can branch on `kind` and show `message` to the user.
#[derive(Debug, thiserror::Error, Serialize, Type)]
#[serde(tag = "kind", content = "message", rename_all = "camelCase")]
pub enum AppError {
    #[error("storage: {0}")]
    Storage(String),
    #[error("git: {0}")]
    Git(String),
    /// The folder exists but is not inside a git repository (the UI offers `git init`).
    #[error("{0}")]
    NotARepository(String),
    #[error("{0}")]
    PathNotFound(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("{0}")]
    Internal(String),
}

fn clean(message: String) -> String {
    nanomgg_logging::redact(&message).into_owned()
}

impl From<nanomgg_storage::StorageError> for AppError {
    fn from(e: nanomgg_storage::StorageError) -> Self {
        Self::Storage(clean(e.to_string()))
    }
}

impl From<GitError> for AppError {
    fn from(e: GitError) -> Self {
        let message = clean(e.to_string());
        match e {
            GitError::NotARepository(_) => Self::NotARepository(message),
            GitError::PathNotFound(_) => Self::PathNotFound(message),
            GitError::BareRepository(_) | GitError::DubiousOwnership(_) => {
                Self::Unsupported(message)
            }
            GitError::Io(_) | GitError::Failed { .. } => Self::Git(message),
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;
