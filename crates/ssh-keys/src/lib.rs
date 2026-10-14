//! SSH keys for NANOMultipleGitGUI accounts.
//!
//! - [`key`]: generate Ed25519 keys (optionally passphrase-protected, OpenSSH bcrypt-pbkdf
//!   format), import existing OpenSSH private keys, fingerprints and public key lines.
//! - [`store`]: keep private keys under `<app data>/ssh/<key_id>` with permissions OpenSSH
//!   accepts (`0600` / user-only ACL on Windows), written atomically and never overwritten.
//! - [`command`]: the `GIT_SSH_COMMAND` that makes git (and git-lfs) use exactly one key.
//! - [`known_hosts`]: pinned github.com / gitlab.com host keys, the app's `known_hosts` file
//!   and `ssh-keyscan` verification.
//!
//! Secrets (private key text) are held in [`zeroize::Zeroizing`] buffers and redacted from
//! `Debug` output. Passphrases are the caller's to store (keychain).

pub mod command;
pub mod error;
pub mod key;
pub mod known_hosts;
pub mod perms;
pub mod store;

pub use command::{HostKeyPolicy, SshCommandOptions, git_ssh_command, ssh_args};
pub use error::{Result, SshKeyError};
pub use key::{
    GeneratedKey, ImportedKey, KeyInfo, KeyKind, PrivateKeyFormat, PublicFileCheck, detect_format,
    fingerprint, generate_ed25519, import_private_key, import_private_key_file,
    inspect_private_key, parse_public_key, verify_passphrase,
};
pub use known_hosts::{
    HostKeyVerdict, KnownHostEntry, PinnedHostKey, ScannedHostKey, SeedReport, scan_host_keys,
    seed_known_hosts, verify_host_key,
};
pub use perms::{PermissionReport, check_permissions, restrict_to_current_user};
pub use store::{KeyStore, StoredKey, validate_key_id};
