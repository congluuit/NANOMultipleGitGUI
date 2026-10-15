//! Encrypted files of NANOMultipleGitGUI (Phase 6.3 / 6.6).
//!
//! - [`crypto`]: the shared envelope — JSON header (format, version, kind, KDF parameters…)
//!   authenticated as AAD, payload encrypted with XChaCha20-Poly1305 under a key derived from
//!   the user's password with Argon2id.
//! - [`accounts`]: account configuration export/import between machines (`.nanomgg-accounts`):
//!   providers, accounts, identities, rules; tokens and SSH keys only on request.
//! - [`full`]: full app backup (`.nanomgg-backup`): a snapshot of the whole SQLite database plus,
//!   on request, keychain secrets and the SSH key files of the app. Restoring stages a database
//!   next to the live one; [`full::apply_pending_restore`] swaps it in at the next start,
//!   before the database is opened.

pub mod accounts;
pub mod crypto;
mod error;
pub mod full;

pub use crypto::{BackupInfo, BackupKind, KdfCost};
pub use error::{BackupError, Result};

/// Milliseconds since the Unix epoch.
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Writes `bytes` to `path` through a temp file in the same folder (never a half-written file).
pub fn write_file_atomic(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let dir = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let mut tmp = tempfile::Builder::new()
        .prefix(".nanomgg-tmp-")
        .tempfile_in(dir)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| BackupError::Io(e.error))?;
    Ok(())
}
