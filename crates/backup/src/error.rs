use crate::BackupKind;

#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("the file is not a NANOMultipleGitGUI backup")]
    NotABackup,
    #[error("the file was made by a newer version of the app (format v{0})")]
    NewerFormat(u32),
    #[error("this file holds {found:?} data, not {expected:?}")]
    WrongKind {
        found: BackupKind,
        expected: BackupKind,
    },
    #[error("the file is larger than {limit} bytes")]
    TooLarge { limit: u64 },
    /// Wrong password and a modified file cannot be told apart (authenticated encryption).
    #[error("wrong password, or the file was modified")]
    WrongPassword,
    #[error("the password must have at least {0} characters")]
    WeakPassword(usize),
    #[error("the backup database (schema v{found}) is newer than this app (v{supported})")]
    SchemaTooNew { found: u32, supported: u32 },
    #[error("invalid backup content: {0}")]
    Corrupt(String),
    /// A request the user can correct (nothing selected…).
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Storage(#[from] nanomgg_storage::StorageError),
    #[error("{0}")]
    Keychain(#[from] nanomgg_credentials::KeychainError),
    #[error("{0}")]
    SshKey(#[from] nanomgg_ssh_keys::SshKeyError),
}

pub type Result<T, E = BackupError> = std::result::Result<T, E>;
