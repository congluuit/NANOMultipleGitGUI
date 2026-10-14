use std::path::PathBuf;

/// Everything that can go wrong while generating, importing or storing an SSH key.
#[derive(Debug, thiserror::Error)]
pub enum SshKeyError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// The file is a legacy PEM key (`-----BEGIN RSA PRIVATE KEY-----`, `EC`, `DSA`, PKCS#8).
    /// Convert it with `ssh-keygen -p -f <file>` (OpenSSH ≥ 7.8 rewrites it in the OpenSSH
    /// format) and import again.
    #[error(
        "legacy PEM private key ({label}) is not supported; convert it to the OpenSSH format with `ssh-keygen -p -f <file>`"
    )]
    LegacyPem { label: String },
    /// PuTTY `.ppk` file; export it as an OpenSSH key from PuTTYgen first.
    #[error(
        "PuTTY .ppk keys are not supported; export the key in the OpenSSH format from PuTTYgen"
    )]
    PuttyFormat,
    /// The text is not a private key at all (a public key, a certificate, random text…).
    #[error("not an OpenSSH private key")]
    NotAPrivateKey,
    /// The text looks like an OpenSSH key but could not be decoded.
    #[error("invalid OpenSSH key: {0}")]
    InvalidKey(String),
    /// The key is encrypted and no passphrase was given.
    #[error("the private key is protected by a passphrase")]
    PassphraseRequired,
    #[error("wrong passphrase")]
    WrongPassphrase,
    /// Comments must fit on the public key line.
    #[error("the key comment must not contain line breaks")]
    InvalidComment,
    /// Key ids become file names: `[A-Za-z0-9._-]`, 1–64 chars, not starting with `.`.
    #[error("invalid key id {0:?}")]
    InvalidKeyId(String),
    /// A different key is already stored under this id; keys are never overwritten.
    #[error("a different key already exists at {}", .0.display())]
    KeyExists(PathBuf),
    #[error("no key stored at {}", .0.display())]
    NotFound(PathBuf),
    /// Restricting or checking the file permissions failed.
    #[error("cannot restrict permissions of {}: {message}", .path.display())]
    Permissions { path: PathBuf, message: String },
    /// `ssh-keyscan` (or another helper tool) failed.
    #[error("{0}")]
    Tool(String),
}

impl SshKeyError {
    pub(crate) fn invalid(err: impl std::fmt::Display) -> Self {
        SshKeyError::InvalidKey(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, SshKeyError>;
