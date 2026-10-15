//! Full app backup / restore (Phase 6.6, `.nanomgg-backup`).
//!
//! A backup is a consistent snapshot of the whole SQLite database (`VACUUM INTO`: settings,
//! themes, notes, AI settings, repos/tabs, accounts, rules, bindings…) plus, on request, the
//! keychain secrets (tokens, refresh tokens, SSH passphrases, AI keys) and the SSH key files of
//! the app key store. Not included: `locations.json` (per machine), logs, cache, WebView data.
//!
//! Restoring happens in two steps because the live database is open while the app runs:
//! 1. [`stage_restore`] (running app): writes the database to `<data>/nanomgg.db.restore`
//!    (migrated, SSH key paths rewritten for this data folder, account secret flags matched to
//!    the keychain), the secrets to the keychain and the key files to the key store.
//! 2. [`apply_pending_restore`] (next start, before the database opens): copies the current
//!    database to `<data>/backups/pre-restore-<time>.db` and swaps the staged one in. Any
//!    failure keeps the current database and is reported to the user.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use nanomgg_accounts::AccountSecret;
use nanomgg_credentials::{SecretKey, SecretKind, SecretStore};
use nanomgg_ssh_keys::{KeyStore, SshKeyError, validate_key_id};
use nanomgg_storage::{Storage, StorageError};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::crypto::BackupInfo;
use crate::error::{BackupError, Result};

pub const DB_FILE: &str = "nanomgg.db";
/// Staged database, swapped in at the next start.
pub const PENDING_FILE: &str = "nanomgg.db.restore";
const PENDING_META: &str = "nanomgg.db.restore.json";
/// A staged database that could not be used (kept for support, replaced by the next one).
const FAILED_FILE: &str = "nanomgg.db.restore-failed";
/// Copies of the database taken right before a restore.
pub const BACKUPS_DIR: &str = "backups";
const PRE_RESTORE_PREFIX: &str = "pre-restore-";
const KEEP_PRE_RESTORE: usize = 3;
/// Keychain owners of the AI provider keys (Phase 6.4, kind `token`).
pub const AI_SECRET_OWNERS: &[&str] = &["ai_openai", "ai_anthropic", "ai_gemini", "ai_compatible"];
const SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FullPayload {
    /// App data folder on the source machine (to rewrite stored key paths).
    pub data_dir: String,
    /// The database file, base64.
    pub db: String,
    #[serde(default)]
    pub includes_secrets: bool,
    #[serde(default)]
    pub secrets: Vec<SecretEntry>,
    /// Files relative to the data folder (`ssh/<key id>`, `ssh/<key id>.pub`), `/`-separated.
    #[serde(default)]
    pub files: Vec<FileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretEntry {
    pub owner: String,
    /// `token`, `refresh_token` or `ssh_passphrase`.
    pub kind: String,
    pub value: Zeroizing<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub content: Zeroizing<String>,
}

fn kind_of(name: &str) -> Option<SecretKind> {
    [
        SecretKind::Token,
        SecretKind::RefreshToken,
        SecretKind::SshPassphrase,
    ]
    .into_iter()
    .find(|k| k.as_str() == name)
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Snapshot of the live database (a consistent copy, WAL included).
fn snapshot(storage: &Storage, work: &Path) -> Result<Vec<u8>> {
    let path = work.join("snapshot.db");
    storage.snapshot_to(&path)?;
    Ok(std::fs::read(&path)?)
}

/// Collects everything to back up.
pub fn create(
    storage: &Storage,
    secrets: &dyn SecretStore,
    data_dir: &Path,
    include_secrets: bool,
) -> Result<FullPayload> {
    std::fs::create_dir_all(data_dir)?;
    let work = tempfile::Builder::new()
        .prefix(".backup-")
        .tempdir_in(data_dir)?;
    let db = snapshot(storage, work.path())?;
    let mut payload = FullPayload {
        data_dir: data_dir.display().to_string(),
        db: B64.encode(&db),
        includes_secrets: include_secrets,
        secrets: Vec::new(),
        files: Vec::new(),
    };
    if !include_secrets {
        return Ok(payload);
    }
    let mut wanted: Vec<SecretKey> = Vec::new();
    for account in storage.accounts()? {
        for kind in [SecretKind::Token, SecretKind::RefreshToken] {
            wanted.push(SecretKey::account(&account.id, kind));
        }
    }
    for key in storage.ssh_keys(None)? {
        if key.has_passphrase {
            wanted.push(SecretKey::ssh_passphrase(&key.id));
        }
    }
    for owner in AI_SECRET_OWNERS {
        wanted.push(SecretKey::new(*owner, SecretKind::Token));
    }
    for key in wanted {
        if let Some(value) = secrets.get(&key)? {
            payload.secrets.push(SecretEntry {
                owner: key.owner,
                kind: key.kind.as_str().into(),
                value,
            });
        }
    }
    let store = KeyStore::new(data_dir);
    for id in store.list()? {
        payload.files.push(FileEntry {
            path: format!("ssh/{id}"),
            content: store.read_private_key(&id)?,
        });
        if let Ok(text) = std::fs::read_to_string(store.public_key_path(&id)?) {
            payload.files.push(FileEntry {
                path: format!("ssh/{id}.pub"),
                content: Zeroizing::new(text),
            });
        }
    }
    Ok(payload)
}

// ----- preview -------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FullPreview {
    pub accounts: u32,
    pub repos: u32,
    /// `None` when the backup has no such setting.
    pub notes: Option<u32>,
    pub custom_themes: Option<u32>,
    pub includes_secrets: bool,
    pub secrets: u32,
    pub ssh_key_files: u32,
    /// Data folder of the source machine.
    pub data_dir: String,
}

fn decode_db(payload: &FullPayload) -> Result<Zeroizing<Vec<u8>>> {
    let db = Zeroizing::new(
        B64.decode(&payload.db)
            .map_err(|_| BackupError::Corrupt("invalid database data".into()))?,
    );
    if !db.starts_with(SQLITE_MAGIC) {
        return Err(BackupError::Corrupt("the backup holds no database".into()));
    }
    Ok(db)
}

/// Opens (and migrates) a database file of a backup; a newer schema is refused.
fn open_backup_db(path: &Path) -> Result<Storage> {
    Storage::open(path).map_err(|e| match e {
        StorageError::SchemaTooNew { found, supported } => {
            BackupError::SchemaTooNew { found, supported }
        }
        other => BackupError::Corrupt(format!("the database cannot be read: {other}")),
    })
}

fn array_len(storage: &Storage, key: &str) -> Option<u32> {
    storage
        .get_setting::<serde_json::Value>(key)
        .ok()
        .flatten()
        .and_then(|v| v.as_array().map(|a| count(a.len())))
}

/// Reads the counts shown before restoring (from a throw-away copy of the database).
pub fn inspect(payload: &FullPayload) -> Result<FullPreview> {
    let db = decode_db(payload)?;
    let work = tempfile::tempdir()?;
    let path = work.path().join(DB_FILE);
    std::fs::write(&path, db.as_slice())?;
    let storage = open_backup_db(&path)?;
    let preview = FullPreview {
        accounts: count(storage.accounts()?.len()),
        repos: count(storage.recent_repos(u32::MAX)?.len()),
        notes: array_len(&storage, "notes.items"),
        custom_themes: array_len(&storage, "theme.custom"),
        includes_secrets: payload.includes_secrets,
        secrets: count(payload.secrets.len()),
        ssh_key_files: count(
            payload
                .files
                .iter()
                .filter(|f| !f.path.ends_with(".pub"))
                .count(),
        ),
        data_dir: payload.data_dir.clone(),
    };
    drop(storage);
    Ok(preview)
}

// ----- stage ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingMeta {
    created_at: i64,
    app_version: String,
    staged_at: i64,
}

/// `value` moved from below `from` to below `to` (separators and ASCII case ignored, so a
/// backup from another OS or drive letter still matches).
pub fn rebase_path(value: &str, from: &str, to: &Path) -> Option<String> {
    let norm = |s: &str| s.trim().replace('\\', "/");
    let (v, f) = (norm(value), norm(from));
    let f = f.trim_end_matches('/');
    if f.is_empty() || v.len() <= f.len() + 1 {
        return None;
    }
    let (head, rest) = (v.get(..f.len())?, v.get(f.len()..)?);
    if !head.eq_ignore_ascii_case(f) || !rest.starts_with('/') {
        return None;
    }
    let mut out = to.to_path_buf();
    for part in rest.split('/').filter(|p| !p.is_empty()) {
        out.push(part);
    }
    Some(out.display().to_string())
}

fn remove_db_files(db: &Path) {
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let mut name = db.as_os_str().to_owned();
        name.push(suffix);
        let path = PathBuf::from(name);
        if path.exists()
            && let Err(e) = std::fs::remove_file(&path)
        {
            tracing::warn!(path = %path.display(), error = %e, "could not remove a database file");
        }
    }
}

fn sidecar(db: &Path, suffix: &str) -> PathBuf {
    let mut name = db.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// What [`stage_restore`] wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct StagedRestore {
    pub secrets: u32,
    pub ssh_key_files: u32,
    /// Key files skipped because a different key with the same id exists here.
    pub ssh_key_conflicts: u32,
}

/// Prepares the restore of `payload` into `data_dir` (see the module docs). Nothing replaces
/// the live database until the next start.
pub fn stage_restore(
    payload: &FullPayload,
    info: &BackupInfo,
    data_dir: &Path,
    secrets: &dyn SecretStore,
) -> Result<StagedRestore> {
    let db = decode_db(payload)?;
    std::fs::create_dir_all(data_dir)?;
    let tmp = data_dir.join(format!("{PENDING_FILE}.tmp"));
    remove_db_files(&tmp);
    std::fs::write(&tmp, db.as_slice())?;
    let result = stage_into(payload, &tmp, data_dir, secrets);
    let staged = match result {
        Ok(staged) => staged,
        Err(e) => {
            remove_db_files(&tmp);
            return Err(e);
        }
    };
    let meta = PendingMeta {
        created_at: info.created_at,
        app_version: info.app_version.clone(),
        staged_at: crate::now_ms(),
    };
    let meta = serde_json::to_vec(&meta).map_err(|e| BackupError::Corrupt(e.to_string()))?;
    crate::write_file_atomic(&data_dir.join(PENDING_META), &meta)?;
    let pending = data_dir.join(PENDING_FILE);
    remove_db_files(&pending);
    // The rename is the commit point: the next start only sees a complete staged database.
    std::fs::rename(&tmp, &pending)?;
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(sidecar(&tmp, suffix));
    }
    Ok(staged)
}

fn stage_into(
    payload: &FullPayload,
    tmp: &Path,
    data_dir: &Path,
    secrets: &dyn SecretStore,
) -> Result<StagedRestore> {
    let storage = open_backup_db(tmp)?;
    let mut staged = StagedRestore::default();

    // Key files first: their rows are pointed at them below.
    let store = KeyStore::new(data_dir);
    let mut restored_keys = Vec::new();
    for file in &payload.files {
        let Some(id) = file.path.strip_prefix("ssh/") else {
            tracing::warn!(path = %file.path, "unknown file in the backup skipped");
            continue;
        };
        if id.ends_with(".pub") || validate_key_id(id).is_err() {
            continue;
        }
        let public = payload
            .files
            .iter()
            .find(|f| f.path == format!("ssh/{id}.pub"))
            .map(|f| f.content.trim().to_owned());
        match store.save(id, &file.content, public.as_deref()) {
            Ok(_) => {
                staged.ssh_key_files += 1;
                restored_keys.push(id.to_owned());
            }
            Err(SshKeyError::KeyExists(path)) => {
                tracing::warn!(path = %path.display(), "a different SSH key with the same id exists; kept");
                staged.ssh_key_conflicts += 1;
            }
            Err(e) => return Err(e.into()),
        }
    }

    let from = payload.data_dir.as_str();
    for mut key in storage.ssh_keys(None)? {
        let managed = restored_keys.contains(&key.id).then(|| {
            store
                .private_key_path(&key.id)
                .map(|p| p.display().to_string())
        });
        let next = match managed {
            Some(Ok(path)) => Some(path),
            _ => key
                .private_key_path
                .as_deref()
                .and_then(|p| rebase_path(p, from, data_dir)),
        };
        if let Some(path) = next
            && key.private_key_path.as_deref() != Some(path.as_str())
        {
            key.private_key_path = Some(path);
            storage.update_ssh_key(&key)?;
        }
    }
    for mut identity in storage.identities(None)? {
        if let Some(path) = identity
            .signing_key_ref
            .as_deref()
            .and_then(|p| rebase_path(p, from, data_dir))
        {
            identity.signing_key_ref = Some(path);
            storage.update_identity(&identity)?;
        }
    }

    for entry in &payload.secrets {
        let Some(kind) = kind_of(&entry.kind) else {
            continue;
        };
        secrets.set(&SecretKey::new(entry.owner.clone(), kind), &entry.value)?;
        staged.secrets += 1;
    }

    // `accounts.secrets` lists the keychain entries that exist: match it to this keychain so
    // an account restored without its token shows "sign in again".
    for account in storage.accounts()? {
        let mut present = Vec::new();
        for kind in [AccountSecret::Token, AccountSecret::RefreshToken] {
            let secret = match kind {
                AccountSecret::Token => SecretKind::Token,
                AccountSecret::RefreshToken => SecretKind::RefreshToken,
            };
            if secrets
                .get(&SecretKey::account(&account.id, secret))?
                .is_some()
            {
                present.push(kind);
            }
        }
        let mut current = account.secrets.clone();
        current.sort();
        if current != present {
            storage.set_account_secrets(&account.id, &present)?;
        }
    }
    drop(storage);
    Ok(staged)
}

/// A restore was staged and waits for the next start.
pub fn has_pending_restore(data_dir: &Path) -> bool {
    data_dir.join(PENDING_FILE).is_file()
}

/// Drops a staged restore (database file only; keychain entries and key files stay).
pub fn cancel_pending_restore(data_dir: &Path) {
    remove_db_files(&data_dir.join(PENDING_FILE));
    let _ = std::fs::remove_file(data_dir.join(PENDING_META));
}

// ----- apply at startup ----------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreApplied {
    /// When the backup was made (Unix ms), if known.
    pub created_at: Option<i64>,
    pub app_version: Option<String>,
    /// Copy of the database that was replaced.
    pub previous_copy: Option<PathBuf>,
}

/// `YYYYMMDD-HHMMSS` (UTC) of Unix milliseconds.
fn utc_stamp(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Copies the current database into `backups/` (a consistent copy when it opens).
fn keep_previous(db: &Path, data_dir: &Path) -> std::result::Result<Option<PathBuf>, String> {
    if !db.is_file() {
        return Ok(None);
    }
    let dir = data_dir.join(BACKUPS_DIR);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    let stamp = utc_stamp(crate::now_ms());
    let mut target = dir.join(format!("{PRE_RESTORE_PREFIX}{stamp}.db"));
    let mut n = 1;
    while target.exists() {
        target = dir.join(format!("{PRE_RESTORE_PREFIX}{stamp}-{n}.db"));
        n += 1;
    }
    let copied = Storage::open(db).and_then(|s| s.snapshot_to(&target));
    if let Err(e) = copied {
        tracing::warn!(error = %e, "could not snapshot the current database; copying the file");
        let _ = std::fs::remove_file(&target);
        std::fs::copy(db, &target)
            .map_err(|e| format!("could not copy the current database: {e}"))?;
    }
    prune_previous(&dir);
    Ok(Some(target))
}

fn prune_previous(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut copies: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(PRE_RESTORE_PREFIX) && n.ends_with(".db"))
        })
        .collect();
    copies.sort();
    let extra = copies.len().saturating_sub(KEEP_PRE_RESTORE);
    for old in copies.into_iter().take(extra) {
        if let Err(e) = std::fs::remove_file(&old) {
            tracing::warn!(path = %old.display(), error = %e, "could not remove an old pre-restore copy");
        }
    }
}

fn read_meta(data_dir: &Path) -> Option<PendingMeta> {
    let bytes = std::fs::read(data_dir.join(PENDING_META)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Call at startup **before** opening the database. `None`: nothing staged. `Some(Err)`: the
/// staged database was refused or could not be swapped in; the current one is kept.
pub fn apply_pending_restore(
    data_dir: &Path,
) -> Option<std::result::Result<RestoreApplied, String>> {
    let pending = data_dir.join(PENDING_FILE);
    if !pending.is_file() {
        return None;
    }
    let meta = read_meta(data_dir);
    let result = apply(data_dir, &pending).map(|previous_copy| RestoreApplied {
        created_at: meta.as_ref().map(|m| m.created_at),
        app_version: meta.as_ref().map(|m| m.app_version.clone()),
        previous_copy,
    });
    if result.is_err() && pending.is_file() {
        let failed = data_dir.join(FAILED_FILE);
        remove_db_files(&failed);
        if std::fs::rename(&pending, &failed).is_err() {
            remove_db_files(&pending);
        }
    }
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(sidecar(&pending, suffix));
    }
    let _ = std::fs::remove_file(data_dir.join(PENDING_META));
    Some(result)
}

fn apply(data_dir: &Path, pending: &Path) -> std::result::Result<Option<PathBuf>, String> {
    // Valid, not from a newer app; migrated now so the swap cannot fail on open.
    match open_backup_db(pending) {
        Ok(storage) => drop(storage),
        Err(e) => return Err(e.to_string()),
    }
    let db = data_dir.join(DB_FILE);
    let previous = keep_previous(&db, data_dir)?;
    // The previous copy was taken through SQLite, which checkpointed the WAL on close; stale
    // `-wal`/`-shm` files must not be replayed onto the restored database.
    for suffix in ["-wal", "-shm"] {
        let path = sidecar(&db, suffix);
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|e| format!("could not remove {}: {e}", path.display()))?;
        }
    }
    std::fs::rename(pending, &db).map_err(|e| format!("could not replace the database: {e}"))?;
    Ok(previous)
}

#[cfg(test)]
mod tests;
