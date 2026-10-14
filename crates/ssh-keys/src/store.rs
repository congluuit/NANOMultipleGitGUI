//! Private keys stored by the app under `<app data>/ssh/<key_id>` (+ `<key_id>.pub`).
//!
//! Writes are atomic (temp file in the same directory → restrict permissions → write →
//! `fsync` → no-clobber rename), so a crash never leaves a half-written or world-readable key,
//! and an existing different key is never overwritten.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use zeroize::Zeroizing;

use crate::error::{Result, SshKeyError};
use crate::key::{self, GeneratedKey, ImportedKey, KeyInfo};
use crate::perms::{self, PermissionReport};

/// Where a key was stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct StoredKey {
    pub key_id: String,
    /// Absolute path of the private key (pass to `ssh -i`).
    pub private_key_path: String,
    pub public_key_path: String,
    pub info: KeyInfo,
}

/// Key directory of the app.
#[derive(Debug, Clone)]
pub struct KeyStore {
    dir: PathBuf,
}

impl KeyStore {
    /// Store in `<app_data_dir>/ssh`.
    pub fn new(app_data_dir: &Path) -> Self {
        Self {
            dir: app_data_dir.join("ssh"),
        }
    }

    /// Store in exactly `dir`.
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn private_key_path(&self, key_id: &str) -> Result<PathBuf> {
        validate_key_id(key_id)?;
        Ok(self.dir.join(key_id))
    }

    pub fn public_key_path(&self, key_id: &str) -> Result<PathBuf> {
        validate_key_id(key_id)?;
        Ok(self.dir.join(format!("{key_id}.pub")))
    }

    pub fn save_generated(&self, key_id: &str, key: &GeneratedKey) -> Result<StoredKey> {
        self.save(key_id, &key.private_key, Some(&key.info.public_key))
    }

    pub fn save_imported(&self, key_id: &str, key: &ImportedKey) -> Result<StoredKey> {
        self.save(key_id, &key.private_key, Some(&key.info.public_key))
    }

    /// Saves an OpenSSH private key (encrypted or not) and its `.pub`.
    ///
    /// `public_key` (optional) supplies the comment for encrypted keys, whose comment is not
    /// readable without the passphrase; it must belong to the same key. Idempotent for the
    /// same key; [`SshKeyError::KeyExists`] if another key already uses `key_id`. The private
    /// key file is restricted to the current user and verified before any secret byte is
    /// written.
    pub fn save(
        &self,
        key_id: &str,
        private_key: &str,
        public_key: Option<&str>,
    ) -> Result<StoredKey> {
        let private_path = self.private_key_path(key_id)?;
        let public_path = self.public_key_path(key_id)?;
        let parsed = key::parse_private(private_key)?;
        let mut info = key::key_info(parsed.public_key(), parsed.is_encrypted());
        if let Some(line) = public_key
            && !key::merge_public_comment(&mut info, line)
        {
            return Err(SshKeyError::InvalidKey(
                "the public key does not belong to the private key".into(),
            ));
        }
        let pem = parsed
            .to_openssh(ssh_key::LineEnding::LF)
            .map_err(SshKeyError::invalid)?;
        drop(parsed);

        self.ensure_dir()?;
        if private_path.exists() {
            let existing = key::read_key_file(&private_path)?;
            if existing.trim() != pem.trim() {
                return Err(SshKeyError::KeyExists(private_path));
            }
            perms::restrict_and_verify(&private_path)?;
        } else {
            let mut tmp = tempfile::Builder::new()
                .prefix(".tmp-key-")
                .tempfile_in(&self.dir)?;
            perms::restrict_and_verify(tmp.path())?;
            tmp.write_all(pem.as_bytes())?;
            tmp.as_file().sync_all()?;
            tmp.persist_noclobber(&private_path).map_err(|e| {
                if e.error.kind() == std::io::ErrorKind::AlreadyExists {
                    SshKeyError::KeyExists(private_path.clone())
                } else {
                    SshKeyError::Io(e.error)
                }
            })?;
        }

        let public_line = format!("{}\n", info.public_key);
        let current_pub = std::fs::read_to_string(&public_path).ok();
        if current_pub.as_deref() != Some(public_line.as_str()) {
            let mut tmp = tempfile::Builder::new()
                .prefix(".tmp-pub-")
                .tempfile_in(&self.dir)?;
            tmp.write_all(public_line.as_bytes())?;
            tmp.as_file().sync_all()?;
            tmp.persist(&public_path)
                .map_err(|e| SshKeyError::Io(e.error))?;
        }

        Ok(StoredKey {
            key_id: key_id.to_string(),
            private_key_path: private_path.to_string_lossy().into_owned(),
            public_key_path: public_path.to_string_lossy().into_owned(),
            info,
        })
    }

    /// Public facts of a stored key (no passphrase needed).
    pub fn load(&self, key_id: &str) -> Result<StoredKey> {
        let private_path = self.private_key_path(key_id)?;
        if !private_path.exists() {
            return Err(SshKeyError::NotFound(private_path));
        }
        let text = key::read_key_file(&private_path)?;
        let mut info = key::inspect_private_key(&text)?;
        let public_path = self.public_key_path(key_id)?;
        if let Ok(line) = std::fs::read_to_string(&public_path) {
            key::merge_public_comment(&mut info, line.trim());
        }
        Ok(StoredKey {
            key_id: key_id.to_string(),
            private_key_path: private_path.to_string_lossy().into_owned(),
            public_key_path: public_path.to_string_lossy().into_owned(),
            info,
        })
    }

    /// The private key text (e.g. to verify a passphrase or export it).
    pub fn read_private_key(&self, key_id: &str) -> Result<Zeroizing<String>> {
        let path = self.private_key_path(key_id)?;
        if !path.exists() {
            return Err(SshKeyError::NotFound(path));
        }
        key::read_key_file(&path)
    }

    /// Ids of the stored keys, sorted.
    pub fn list(&self) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(ids),
            Err(e) => return Err(e.into()),
        };
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type()?.is_file()
                && !name.ends_with(".pub")
                && validate_key_id(&name).is_ok()
            {
                ids.push(name);
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// Deletes the key pair; `Ok(false)` when nothing was stored.
    pub fn remove(&self, key_id: &str) -> Result<bool> {
        let private_path = self.private_key_path(key_id)?;
        let public_path = self.public_key_path(key_id)?;
        let existed = remove_if_exists(&private_path)?;
        remove_if_exists(&public_path)?;
        Ok(existed)
    }

    /// Checks that the stored private key is still private to the current user.
    pub fn check_permissions(&self, key_id: &str) -> Result<PermissionReport> {
        perms::check_permissions(&self.private_key_path(key_id)?)
    }

    /// Re-applies the restrictive permissions (e.g. after a restore from backup).
    pub fn repair_permissions(&self, key_id: &str) -> Result<()> {
        perms::restrict_and_verify(&self.private_key_path(key_id)?)
    }

    fn ensure_dir(&self) -> Result<()> {
        if !self.dir.is_dir() {
            std::fs::create_dir_all(&self.dir)?;
            perms::restrict_to_current_user(&self.dir, true)?;
        }
        Ok(())
    }
}

fn remove_if_exists(path: &Path) -> Result<bool> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// Key ids become file names: `[A-Za-z0-9._-]`, 1–64 chars, not starting with `.`, no `.pub`
/// suffix, and not a reserved Windows device name.
pub fn validate_key_id(key_id: &str) -> Result<()> {
    let valid_chars = key_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    let stem = key_id.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit());
    if key_id.is_empty()
        || key_id.len() > 64
        || !valid_chars
        || key_id.starts_with('.')
        || key_id.ends_with('.')
        || key_id.ends_with(".pub")
        || reserved
    {
        return Err(SshKeyError::InvalidKeyId(key_id.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_key_id;

    #[test]
    fn key_ids() {
        for ok in ["acc-1_ed25519", "a", "k.2026"] {
            assert!(validate_key_id(ok).is_ok(), "{ok}");
        }
        for bad in [
            "", ".hidden", "a/b", "a\\b", "x.pub", "con", "COM1", "a b", "trail.", "a*",
        ] {
            assert!(validate_key_id(bad).is_err(), "{bad}");
        }
        assert!(validate_key_id(&"a".repeat(65)).is_err());
    }
}
