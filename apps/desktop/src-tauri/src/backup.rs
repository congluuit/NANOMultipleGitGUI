//! Phase 6.3 / 6.6: encrypted account-config export/import and full app backup/restore
//! (logic in `nanomgg_backup`).
//!
//! The frontend picks the file with the dialog plugin and sends its path; this module reads
//! and writes it (≤ 50 MB). Problems the user can fix — wrong password or modified file, not
//! a backup, wrong kind, newer app — come back as `problem`, not as errors.
//!
//! Restoring a full backup only *stages* it ([`nanomgg_backup::full::stage_restore`]); the app
//! then restarts and [`apply_pending_restore`] swaps the database in during setup, before
//! `Storage::open`. The outcome is shown once through [`backup_restore_notice`].

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use nanomgg_backup::accounts::{AccountsImportReport, AccountsPayload, AccountsPreview};
use nanomgg_backup::full::{FullPayload, FullPreview, StagedRestore};
use nanomgg_backup::{BackupError, BackupInfo, BackupKind, KdfCost, crypto};
use nanomgg_ssh_keys::KeyStore;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};
use zeroize::Zeroizing;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// What is wrong with a chosen file or password, for the dialog to explain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum BackupProblem {
    /// Wrong password, or the file was modified (cannot be told apart).
    WrongPassword,
    NotABackup,
    /// An account export where a full backup is expected, or the reverse.
    WrongKind,
    /// Made by a newer app version (file format or database schema).
    NewerVersion,
    TooLarge,
    /// Decrypted, but the content is damaged.
    Corrupt,
    WeakPassword,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupWriteResult {
    pub path: String,
    pub bytes: u64,
    /// Accounts in the file.
    pub accounts: u32,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountsFileResult {
    pub problem: Option<BackupProblem>,
    pub info: Option<BackupInfo>,
    pub preview: Option<AccountsPreview>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountsImportResult {
    pub problem: Option<BackupProblem>,
    pub report: Option<AccountsImportReport>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FullBackupFileResult {
    pub problem: Option<BackupProblem>,
    pub info: Option<BackupInfo>,
    pub preview: Option<FullPreview>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FullRestoreResult {
    pub problem: Option<BackupProblem>,
    /// Staged: restart the app to finish.
    pub staged: Option<StagedRestore>,
}

/// Outcome of a restore applied at this start (shown once).
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RestoreNotice {
    pub ok: bool,
    pub error: Option<String>,
    /// When the backup was made (Unix ms).
    pub created_at: Option<i64>,
    pub app_version: Option<String>,
    /// Copy of the replaced database.
    pub previous_copy: Option<String>,
}

/// Managed state: the startup restore outcome until the UI reads it.
pub struct RestoreStartup(Mutex<Option<RestoreNotice>>);

/// Startup hook (setup, after the data-folder move, **before** the database opens): swaps in a
/// database staged by "Restore from backup" in the previous session.
pub fn apply_pending_restore(data_dir: &Path) -> RestoreStartup {
    let notice = nanomgg_backup::full::apply_pending_restore(data_dir).map(|r| match r {
        Ok(applied) => {
            tracing::info!(created_at = ?applied.created_at, "app data restored from a backup");
            RestoreNotice {
                ok: true,
                error: None,
                created_at: applied.created_at,
                app_version: applied.app_version,
                previous_copy: applied.previous_copy.map(|p| p.display().to_string()),
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "restore from backup failed; current data kept");
            RestoreNotice {
                ok: false,
                error: Some(e),
                created_at: None,
                app_version: None,
                previous_copy: None,
            }
        }
    });
    RestoreStartup(Mutex::new(notice))
}

// ----- helpers -------------------------------------------------------------------------------

fn problem_of(e: &BackupError) -> Option<BackupProblem> {
    Some(match e {
        BackupError::WrongPassword => BackupProblem::WrongPassword,
        BackupError::NotABackup => BackupProblem::NotABackup,
        BackupError::WrongKind { .. } => BackupProblem::WrongKind,
        BackupError::NewerFormat(_) | BackupError::SchemaTooNew { .. } => {
            BackupProblem::NewerVersion
        }
        BackupError::TooLarge { .. } => BackupProblem::TooLarge,
        BackupError::Corrupt(_) => BackupProblem::Corrupt,
        BackupError::WeakPassword(_) => BackupProblem::WeakPassword,
        _ => return None,
    })
}

fn app_err(e: BackupError) -> AppError {
    match e {
        BackupError::Invalid(m) => AppError::InvalidInput(m),
        BackupError::Storage(e) => crate::accounts::storage_err(e),
        BackupError::SshKey(e) => crate::ssh_keys::ssh_err(e),
        BackupError::Io(e) if e.kind() == std::io::ErrorKind::NotFound => {
            AppError::PathNotFound(e.to_string())
        }
        other => AppError::Internal(nanomgg_logging::redact(&other.to_string()).into_owned()),
    }
}

/// `Ok(Err(problem))` for what the dialog explains, `Err` for real failures.
fn split<T>(r: nanomgg_backup::Result<T>) -> AppResult<Result<T, BackupProblem>> {
    match r {
        Ok(v) => Ok(Ok(v)),
        Err(e) => match problem_of(&e) {
            Some(p) => Ok(Err(p)),
            None => Err(app_err(e)),
        },
    }
}

fn app_version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

fn picked(path: &str) -> AppResult<PathBuf> {
    let path = PathBuf::from(path.trim());
    if path.as_os_str().is_empty() {
        return Err(AppError::InvalidInput("no file chosen".into()));
    }
    Ok(path)
}

/// Reads and decrypts a file of `kind`, then parses its JSON payload.
fn open_payload<T: serde::de::DeserializeOwned>(
    path: &Path,
    password: &str,
    kind: BackupKind,
) -> nanomgg_backup::Result<(BackupInfo, T)> {
    let bytes = crypto::read_file(path)?;
    let (info, plain) = crypto::open(&bytes, password, Some(kind))?;
    let payload =
        serde_json::from_slice(&plain).map_err(|e| BackupError::Corrupt(e.to_string()))?;
    Ok((info, payload))
}

fn seal_and_write(
    app: &AppHandle,
    path: &Path,
    kind: BackupKind,
    payload: &impl Serialize,
    password: &str,
) -> AppResult<u64> {
    let json =
        Zeroizing::new(serde_json::to_vec(payload).map_err(|e| AppError::Internal(e.to_string()))?);
    let file = crypto::seal(kind, &json, password, &app_version(app), KdfCost::DESKTOP)
        .map_err(app_err)?;
    nanomgg_backup::write_file_atomic(path, &file).map_err(app_err)?;
    Ok(file.len() as u64)
}

fn key_store(app: &AppHandle) -> AppResult<KeyStore> {
    Ok(KeyStore::new(&crate::app_paths::data_dir(app)?))
}

async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppHandle, &AppState) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(move || f(&app, &app.state::<AppState>()))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

// ----- Phase 6.3: account configuration ------------------------------------------------------

/// Writes the chosen accounts (all when `accountIds` is empty) to an encrypted file. Tokens and
/// the app's SSH keys only with `includeSecrets`.
#[tauri::command]
#[specta::specta]
pub async fn backup_accounts_export(
    app: AppHandle,
    path: String,
    password: String,
    account_ids: Vec<String>,
    include_secrets: bool,
) -> AppResult<BackupWriteResult> {
    let password = Zeroizing::new(password);
    blocking(app, move |app, st| {
        crypto::check_password(&password).map_err(app_err)?;
        let path = picked(&path)?;
        let payload = nanomgg_backup::accounts::export(
            &st.storage,
            st.accounts.secrets.as_ref(),
            &key_store(app)?,
            &account_ids,
            include_secrets,
        )
        .map_err(app_err)?;
        let bytes = seal_and_write(app, &path, BackupKind::Accounts, &payload, &password)?;
        tracing::info!(
            accounts = payload.accounts.len(),
            include_secrets,
            "account configuration exported"
        );
        Ok(BackupWriteResult {
            path: path.display().to_string(),
            bytes,
            accounts: u32::try_from(payload.accounts.len()).unwrap_or(u32::MAX),
        })
    })
    .await
}

/// Decrypts an account export and lists its accounts, marking those already on this machine.
#[tauri::command]
#[specta::specta]
pub async fn backup_accounts_open(
    app: AppHandle,
    path: String,
    password: String,
) -> AppResult<AccountsFileResult> {
    let password = Zeroizing::new(password);
    blocking(app, move |_, st| {
        let path = picked(&path)?;
        let opened = split(open_payload::<AccountsPayload>(
            &path,
            &password,
            BackupKind::Accounts,
        ))?;
        let (info, payload) = match opened {
            Ok(v) => v,
            Err(problem) => {
                return Ok(AccountsFileResult {
                    problem: Some(problem),
                    info: None,
                    preview: None,
                });
            }
        };
        let preview = split(nanomgg_backup::accounts::preview(&st.storage, &payload))?;
        Ok(match preview {
            Ok(preview) => AccountsFileResult {
                problem: None,
                info: Some(info),
                preview: Some(preview),
            },
            Err(problem) => AccountsFileResult {
                problem: Some(problem),
                info: Some(info),
                preview: None,
            },
        })
    })
    .await
}

/// Imports the accounts named by `keys` (ids inside the file). Existing accounts are skipped,
/// or get only their label/color when `updateExisting`. All or nothing.
#[tauri::command]
#[specta::specta]
pub async fn backup_accounts_import(
    app: AppHandle,
    path: String,
    password: String,
    keys: Vec<String>,
    update_existing: bool,
) -> AppResult<AccountsImportResult> {
    let password = Zeroizing::new(password);
    blocking(app, move |app, st| {
        let path = picked(&path)?;
        let opened = split(open_payload::<AccountsPayload>(
            &path,
            &password,
            BackupKind::Accounts,
        ))?;
        let payload = match opened {
            Ok((_, payload)) => payload,
            Err(problem) => {
                return Ok(AccountsImportResult {
                    problem: Some(problem),
                    report: None,
                });
            }
        };
        let report = split(nanomgg_backup::accounts::import(
            &st.storage,
            st.accounts.secrets.as_ref(),
            &key_store(app)?,
            &payload,
            &keys,
            update_existing,
        ))?;
        Ok(match report {
            Ok(report) => {
                tracing::info!(
                    added = report.added,
                    updated = report.updated,
                    skipped = report.skipped,
                    "account configuration imported"
                );
                crate::accounts::sync_open_repos(st);
                AccountsImportResult {
                    problem: None,
                    report: Some(report),
                }
            }
            Err(problem) => AccountsImportResult {
                problem: Some(problem),
                report: None,
            },
        })
    })
    .await
}

// ----- Phase 6.6: full backup / restore ------------------------------------------------------

/// Writes an encrypted snapshot of all app data; with `includeSecrets` also the tokens, SSH
/// passphrases, AI keys and the app's SSH key files.
#[tauri::command]
#[specta::specta]
pub async fn backup_full_create(
    app: AppHandle,
    path: String,
    password: String,
    include_secrets: bool,
) -> AppResult<BackupWriteResult> {
    let password = Zeroizing::new(password);
    blocking(app, move |app, st| {
        crypto::check_password(&password).map_err(app_err)?;
        let path = picked(&path)?;
        let data_dir = crate::app_paths::data_dir(app)?;
        let payload = nanomgg_backup::full::create(
            &st.storage,
            st.accounts.secrets.as_ref(),
            &data_dir,
            include_secrets,
        )
        .map_err(app_err)?;
        let bytes = seal_and_write(app, &path, BackupKind::Full, &payload, &password)?;
        tracing::info!(bytes, include_secrets, "full backup written");
        Ok(BackupWriteResult {
            path: path.display().to_string(),
            bytes,
            accounts: u32::try_from(st.storage.accounts().map_or(0, |a| a.len()))
                .unwrap_or(u32::MAX),
        })
    })
    .await
}

fn open_full(
    path: &str,
    password: &str,
) -> AppResult<Result<(BackupInfo, FullPayload), BackupProblem>> {
    split(open_payload::<FullPayload>(
        &picked(path)?,
        password,
        BackupKind::Full,
    ))
}

/// Decrypts a full backup and describes it (counts read from a throw-away copy).
#[tauri::command]
#[specta::specta]
pub async fn backup_full_open(
    app: AppHandle,
    path: String,
    password: String,
) -> AppResult<FullBackupFileResult> {
    let password = Zeroizing::new(password);
    blocking(app, move |_, _| {
        let (info, payload) = match open_full(&path, &password)? {
            Ok(v) => v,
            Err(problem) => {
                return Ok(FullBackupFileResult {
                    problem: Some(problem),
                    info: None,
                    preview: None,
                });
            }
        };
        Ok(match split(nanomgg_backup::full::inspect(&payload))? {
            Ok(preview) => FullBackupFileResult {
                problem: None,
                info: Some(info),
                preview: Some(preview),
            },
            Err(problem) => FullBackupFileResult {
                problem: Some(problem),
                info: Some(info),
                preview: None,
            },
        })
    })
    .await
}

/// Stages the restore (database next to the live one, secrets, key files). The caller then
/// restarts the app (`storage_restart`); the swap happens before the database opens.
#[tauri::command]
#[specta::specta]
pub async fn backup_full_restore(
    app: AppHandle,
    path: String,
    password: String,
) -> AppResult<FullRestoreResult> {
    let password = Zeroizing::new(password);
    blocking(app, move |app, st| {
        let (info, payload) = match open_full(&path, &password)? {
            Ok(v) => v,
            Err(problem) => {
                return Ok(FullRestoreResult {
                    problem: Some(problem),
                    staged: None,
                });
            }
        };
        let data_dir = crate::app_paths::data_dir(app)?;
        let staged = split(nanomgg_backup::full::stage_restore(
            &payload,
            &info,
            &data_dir,
            st.accounts.secrets.as_ref(),
        ))?;
        Ok(match staged {
            Ok(staged) => {
                tracing::info!(?staged, "restore staged; applied on the next start");
                FullRestoreResult {
                    problem: None,
                    staged: Some(staged),
                }
            }
            Err(problem) => FullRestoreResult {
                problem: Some(problem),
                staged: None,
            },
        })
    })
    .await
}

/// The restore applied at this start, once (later calls return `null`).
#[tauri::command]
#[specta::specta]
pub fn backup_restore_notice(app: AppHandle) -> Option<RestoreNotice> {
    app.try_state::<RestoreStartup>()
        .and_then(|s| s.0.lock().unwrap_or_else(|e| e.into_inner()).take())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn problems_are_separated_from_failures() {
        assert_eq!(
            split::<()>(Err(BackupError::WrongPassword)).unwrap(),
            Err(BackupProblem::WrongPassword)
        );
        assert_eq!(
            split::<()>(Err(BackupError::SchemaTooNew {
                found: 9,
                supported: 4
            }))
            .unwrap(),
            Err(BackupProblem::NewerVersion)
        );
        assert!(matches!(
            split::<()>(Err(BackupError::Invalid("x".into()))),
            Err(AppError::InvalidInput(_))
        ));
        let json = serde_json::to_value(BackupProblem::WrongKind).unwrap();
        assert_eq!(json, "wrongKind");
    }

    #[test]
    fn startup_notice_is_given_once() {
        let tmp = tempfile::tempdir().unwrap();
        let state = apply_pending_restore(tmp.path());
        assert!(state.0.lock().unwrap().is_none(), "nothing staged");
        std::fs::write(tmp.path().join(nanomgg_backup::full::PENDING_FILE), b"junk").unwrap();
        let state = apply_pending_restore(tmp.path());
        let notice = state.0.lock().unwrap().take().unwrap();
        assert!(!notice.ok && notice.error.is_some());
        assert!(state.0.lock().unwrap().is_none());
    }
}
