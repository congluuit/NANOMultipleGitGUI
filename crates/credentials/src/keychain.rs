//! Secrets in the OS keychain (Windows Credential Manager, macOS Keychain, Linux Secret
//! Service) and the [`KeychainSource`] that serves them to the credential helper.
//!
//! Entry names follow `nanomgg:<owner>:<kind>` where `owner` is an account id
//! (`nanomgg:acc_3f9c…:token`) or, for SSH key passphrases, the key id
//! (`nanomgg:key_81ab…:ssh_passphrase`) so a key keeps its passphrase when reassigned.
//! Values are only ever held in [`Zeroizing`] buffers and never logged.
//!
//! The OS backend ([`OsKeychain`]) is behind the `os-keychain` feature so the sidecar binary does
//! not link it; tests use [`MemorySecretStore`].

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use zeroize::Zeroizing;

use crate::server::{AccountCredential, CredentialSource};

/// Service name used for every keychain entry.
pub const KEYCHAIN_SERVICE: &str = "nanomgg";

/// What a secret is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecretKind {
    /// HTTPS token (OAuth access token or PAT).
    Token,
    RefreshToken,
    /// Passphrase of an SSH private key (owner = key id).
    SshPassphrase,
}

impl SecretKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Token => "token",
            Self::RefreshToken => "refresh_token",
            Self::SshPassphrase => "ssh_passphrase",
        }
    }
}

/// Name of one keychain entry.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SecretKey {
    pub owner: String,
    pub kind: SecretKind,
}

impl SecretKey {
    pub fn new(owner: impl Into<String>, kind: SecretKind) -> Self {
        Self {
            owner: owner.into(),
            kind,
        }
    }

    pub fn account(account_id: &str, kind: SecretKind) -> Self {
        Self::new(account_id, kind)
    }

    pub fn ssh_passphrase(key_id: &str) -> Self {
        Self::new(key_id, SecretKind::SshPassphrase)
    }

    /// `nanomgg:<owner>:<kind>`.
    pub fn entry_name(&self) -> String {
        format!("{KEYCHAIN_SERVICE}:{}:{}", self.owner, self.kind.as_str())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum KeychainError {
    #[error("invalid secret name `{0}`")]
    InvalidKey(String),
    #[error("the secret is empty")]
    EmptySecret,
    #[error("OS keychain unavailable: {0}")]
    Unavailable(String),
    #[error("OS keychain error: {0}")]
    Platform(String),
}

/// Storage for secrets. Implementations must never log values.
pub trait SecretStore: Send + Sync + 'static {
    fn get(&self, key: &SecretKey) -> Result<Option<Zeroizing<String>>, KeychainError>;
    fn set(&self, key: &SecretKey, value: &str) -> Result<(), KeychainError>;
    /// Returns whether an entry existed.
    fn delete(&self, key: &SecretKey) -> Result<bool, KeychainError>;
}

fn check_key(key: &SecretKey) -> Result<(), KeychainError> {
    let ok = !key.owner.is_empty()
        && key
            .owner
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if ok {
        Ok(())
    } else {
        Err(KeychainError::InvalidKey(key.owner.clone()))
    }
}

/// In-memory store for tests and as a fallback when no OS keychain is available
/// (secrets then last only for this run).
#[derive(Default)]
pub struct MemorySecretStore {
    entries: RwLock<HashMap<String, Zeroizing<String>>>,
}

impl MemorySecretStore {
    pub fn len(&self) -> usize {
        self.entries.read().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl SecretStore for MemorySecretStore {
    fn get(&self, key: &SecretKey) -> Result<Option<Zeroizing<String>>, KeychainError> {
        check_key(key)?;
        Ok(self
            .entries
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key.entry_name())
            .cloned())
    }

    fn set(&self, key: &SecretKey, value: &str) -> Result<(), KeychainError> {
        check_key(key)?;
        if value.is_empty() {
            return Err(KeychainError::EmptySecret);
        }
        self.entries
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key.entry_name(), Zeroizing::new(value.to_owned()));
        Ok(())
    }

    fn delete(&self, key: &SecretKey) -> Result<bool, KeychainError> {
        check_key(key)?;
        Ok(self
            .entries
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&key.entry_name())
            .is_some())
    }
}

/// The real OS keychain through the `keyring` crate (native backends only, no OpenSSL:
/// Windows Credential Manager, macOS Keychain Services, Linux Secret Service over zbus with
/// pure-Rust crypto).
#[cfg(feature = "os-keychain")]
#[derive(Debug, Default, Clone, Copy)]
pub struct OsKeychain;

#[cfg(feature = "os-keychain")]
impl OsKeychain {
    fn entry(key: &SecretKey) -> Result<keyring::Entry, KeychainError> {
        check_key(key)?;
        let name = key.entry_name();
        // Target = full entry name (what Windows Credential Manager shows); service/user are
        // what macOS and Secret Service index on.
        keyring::Entry::new_with_target(
            &name,
            KEYCHAIN_SERVICE,
            &format!("{}:{}", key.owner, key.kind.as_str()),
        )
        .map_err(map_keyring_error)
    }
}

#[cfg(feature = "os-keychain")]
fn map_keyring_error(e: keyring::Error) -> KeychainError {
    match e {
        keyring::Error::NoStorageAccess(inner) => KeychainError::Unavailable(inner.to_string()),
        keyring::Error::PlatformFailure(inner) => KeychainError::Platform(inner.to_string()),
        other => KeychainError::Platform(other.to_string()),
    }
}

#[cfg(feature = "os-keychain")]
impl SecretStore for OsKeychain {
    fn get(&self, key: &SecretKey) -> Result<Option<Zeroizing<String>>, KeychainError> {
        match Self::entry(key)?.get_password() {
            Ok(value) => Ok(Some(Zeroizing::new(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_keyring_error(e)),
        }
    }

    fn set(&self, key: &SecretKey, value: &str) -> Result<(), KeychainError> {
        if value.is_empty() {
            return Err(KeychainError::EmptySecret);
        }
        Self::entry(key)?
            .set_password(value)
            .map_err(map_keyring_error)
    }

    fn delete(&self, key: &SecretKey) -> Result<bool, KeychainError> {
        match Self::entry(key)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(map_keyring_error(e)),
        }
    }
}

/// Non-secret account facts the credential server needs (from the app database).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountRef {
    /// Provider host (`github.com`, `gitlab.acme.dev:8443`) — tokens go to this host only.
    pub host: String,
    pub username: String,
}

/// An SSH private key of an account, for answering passphrase prompts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshKeyRef {
    pub key_id: String,
    pub private_key_path: String,
}

/// Looks up account metadata; implemented by the app over SQLite.
pub trait AccountDirectory: Send + Sync + 'static {
    fn account(&self, account_id: &str) -> Option<AccountRef>;
    fn ssh_keys(&self, account_id: &str) -> Vec<SshKeyRef>;
}

/// [`CredentialSource`] backed by the account directory and a [`SecretStore`] (the keychain).
pub struct KeychainSource {
    directory: Arc<dyn AccountDirectory>,
    secrets: Arc<dyn SecretStore>,
}

impl KeychainSource {
    pub fn new(directory: Arc<dyn AccountDirectory>, secrets: Arc<dyn SecretStore>) -> Self {
        Self { directory, secrets }
    }
}

/// Canonical form of a key path for comparison: `/` separators, MSYS `/c/...` and `C:/...`
/// both → `c:/...`, and lower case where file systems ignore case.
pub fn normalize_key_path(path: &str) -> String {
    let mut p = path.trim().replace('\\', "/");
    let b = p.as_bytes();
    if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b'/' {
        p = format!("{}:{}", p[1..2].to_ascii_lowercase(), &p[2..]);
    } else if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        p = format!("{}{}", p[..1].to_ascii_lowercase(), &p[1..]);
    }
    if cfg!(any(windows, target_os = "macos")) {
        p.to_lowercase()
    } else {
        p
    }
}

impl CredentialSource for KeychainSource {
    fn credential(&self, account_id: &str) -> Option<AccountCredential> {
        let account = self.directory.account(account_id)?;
        // A keychain failure is reported to git as "unknown account", never with details.
        let token = self
            .secrets
            .get(&SecretKey::account(account_id, SecretKind::Token))
            .ok()
            .flatten()?;
        Some(AccountCredential {
            host: account.host,
            username: account.username,
            token,
        })
    }

    fn ssh_passphrase(&self, account_id: &str, key_path: &str) -> Option<Zeroizing<String>> {
        let wanted = normalize_key_path(key_path);
        let key = self
            .directory
            .ssh_keys(account_id)
            .into_iter()
            .find(|k| normalize_key_path(&k.private_key_path) == wanted)?;
        self.secrets
            .get(&SecretKey::ssh_passphrase(&key.key_id))
            .ok()
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dir;

    impl AccountDirectory for Dir {
        fn account(&self, account_id: &str) -> Option<AccountRef> {
            (account_id == "acc_a").then(|| AccountRef {
                host: "github.com".into(),
                username: "alice".into(),
            })
        }

        fn ssh_keys(&self, account_id: &str) -> Vec<SshKeyRef> {
            if account_id == "acc_a" {
                vec![SshKeyRef {
                    key_id: "key_1".into(),
                    private_key_path: "C:\\Users\\Me\\.ssh\\work_ed25519".into(),
                }]
            } else {
                vec![]
            }
        }
    }

    fn source() -> (KeychainSource, Arc<MemorySecretStore>) {
        let store = Arc::new(MemorySecretStore::default());
        (KeychainSource::new(Arc::new(Dir), store.clone()), store)
    }

    #[test]
    fn entry_names_follow_the_documented_format() {
        assert_eq!(
            SecretKey::account("acc_1", SecretKind::Token).entry_name(),
            "nanomgg:acc_1:token"
        );
        assert_eq!(
            SecretKey::account("acc_1", SecretKind::RefreshToken).entry_name(),
            "nanomgg:acc_1:refresh_token"
        );
        assert_eq!(
            SecretKey::ssh_passphrase("key_9").entry_name(),
            "nanomgg:key_9:ssh_passphrase"
        );
    }

    #[test]
    fn memory_store_round_trip_and_key_checks() {
        let store = MemorySecretStore::default();
        let key = SecretKey::account("acc_1", SecretKind::Token);
        assert!(store.get(&key).unwrap().is_none());
        store.set(&key, "ghp_x").unwrap();
        assert_eq!(store.get(&key).unwrap().unwrap().as_str(), "ghp_x");
        store.set(&key, "ghp_y").unwrap();
        assert_eq!(store.len(), 1);
        assert!(store.delete(&key).unwrap());
        assert!(!store.delete(&key).unwrap());
        assert!(matches!(
            store.set(&key, ""),
            Err(KeychainError::EmptySecret)
        ));
        for bad in ["", "a:b", "acc 1", "../x"] {
            assert!(
                matches!(
                    store.get(&SecretKey::account(bad, SecretKind::Token)),
                    Err(KeychainError::InvalidKey(_))
                ),
                "{bad}"
            );
        }
    }

    #[test]
    fn source_combines_directory_and_keychain() {
        let (src, store) = source();
        assert!(src.credential("acc_a").is_none(), "no token stored yet");
        store
            .set(&SecretKey::account("acc_a", SecretKind::Token), "ghp_alice")
            .unwrap();
        let c = src.credential("acc_a").unwrap();
        assert_eq!(
            (c.host.as_str(), c.username.as_str()),
            ("github.com", "alice")
        );
        assert_eq!(c.token.as_str(), "ghp_alice");
        assert!(src.credential("acc_unknown").is_none());
    }

    #[test]
    fn passphrase_only_for_the_accounts_own_key() {
        let (src, store) = source();
        store
            .set(&SecretKey::ssh_passphrase("key_1"), "hunter2")
            .unwrap();
        for path in [
            "C:/Users/Me/.ssh/work_ed25519",
            "c:/Users/Me/.ssh/work_ed25519",
            "/c/Users/Me/.ssh/work_ed25519",
            "C:\\Users\\Me\\.ssh\\work_ed25519",
        ] {
            let got = src.ssh_passphrase("acc_a", path);
            assert_eq!(
                got.as_deref().map(String::as_str),
                Some("hunter2"),
                "{path}"
            );
        }
        assert!(
            src.ssh_passphrase("acc_a", "C:/Users/Me/.ssh/other")
                .is_none()
        );
        assert!(
            src.ssh_passphrase("acc_b", "C:/Users/Me/.ssh/work_ed25519")
                .is_none()
        );
    }

    /// Writes, reads and deletes one throw-away entry in the real OS keychain.
    /// Opt-in: `cargo test -p nanomgg-credentials --features os-keychain -- --ignored`.
    #[cfg(feature = "os-keychain")]
    #[test]
    #[ignore]
    fn os_keychain_round_trip() {
        let mut nonce = [0u8; 6];
        getrandom::fill(&mut nonce).unwrap();
        let owner = format!(
            "nanomgg-test-{}",
            nonce.iter().map(|b| format!("{b:02x}")).collect::<String>()
        );
        let key = SecretKey::new(owner, SecretKind::Token);
        let kc = OsKeychain;
        assert!(kc.get(&key).unwrap().is_none());
        kc.set(&key, "test-secret-value").unwrap();
        let read = kc.get(&key);
        let deleted = kc.delete(&key);
        assert_eq!(read.unwrap().unwrap().as_str(), "test-secret-value");
        assert!(deleted.unwrap());
        assert!(kc.get(&key).unwrap().is_none());
    }
}
