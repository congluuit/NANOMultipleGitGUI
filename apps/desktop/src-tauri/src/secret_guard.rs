//! Secret Guard (before a commit): scans the added lines of the staged diff for credentials
//! with the rules of [`nanomgg_git_engine::secrets`] and reports them masked.
//!
//! State kept by the app, never in the repository:
//! - `secretGuard.enabled` (on by default);
//! - `secretGuard.ignored`: per repository (keyed by its root path), the fingerprints of
//!   findings the user chose to always ignore. A fingerprint is a salted hash, not the secret.
//!
//! Only counts are logged; a secret never reaches a log line.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use nanomgg_git_engine::secrets::{self, SecretReport};
use nanomgg_storage::Storage;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};
use crate::repos::repo_root;
use crate::state::AppState;

const KEY_ENABLED: &str = "secretGuard.enabled";
const KEY_IGNORED: &str = "secretGuard.ignored";
/// Stored fingerprints per repository at most (oldest dropped first).
const MAX_IGNORED_PER_REPO: usize = 2000;

type IgnoredMap = BTreeMap<String, Vec<String>>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SecretGuardPrefs {
    pub enabled: bool,
    /// Findings always ignored, over every repository.
    pub ignored_count: u32,
}

/// Result of the pre-commit check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SecretGuardCheck {
    /// `false`: the guard is turned off in Settings and nothing was scanned.
    pub enabled: bool,
    pub report: SecretReport,
}

/// Key of a repository in [`KEY_IGNORED`]: its root path with `/` separators (lower case on
/// Windows, where paths are case-insensitive).
pub(crate) fn repo_key(root: &Path) -> String {
    let s = root.to_string_lossy().replace('\\', "/");
    let s = s.trim_end_matches('/').to_owned();
    if cfg!(windows) { s.to_lowercase() } else { s }
}

fn load_ignored(db: &Storage) -> AppResult<IgnoredMap> {
    Ok(db.get_setting(KEY_IGNORED)?.unwrap_or_default())
}

pub(crate) fn enabled(db: &Storage) -> AppResult<bool> {
    Ok(db.get_setting(KEY_ENABLED)?.unwrap_or(true))
}

pub(crate) fn prefs(db: &Storage) -> AppResult<SecretGuardPrefs> {
    let ignored = load_ignored(db)?;
    Ok(SecretGuardPrefs {
        enabled: enabled(db)?,
        ignored_count: ignored.values().map(Vec::len).sum::<usize>() as u32,
    })
}

pub(crate) fn ignored_for(db: &Storage, root: &Path) -> AppResult<HashSet<String>> {
    Ok(load_ignored(db)?
        .remove(&repo_key(root))
        .unwrap_or_default()
        .into_iter()
        .collect())
}

/// Fingerprints are 32 lower-case hex characters (see `secrets::fingerprint`).
fn valid_fingerprint(fp: &str) -> bool {
    fp.len() == 32
        && fp
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(crate) fn add_ignored(db: &Storage, root: &Path, fingerprints: &[String]) -> AppResult<()> {
    if let Some(bad) = fingerprints.iter().find(|f| !valid_fingerprint(f)) {
        return Err(AppError::InvalidInput(format!(
            "invalid finding fingerprint: {bad:?}"
        )));
    }
    let mut map = load_ignored(db)?;
    let list = map.entry(repo_key(root)).or_default();
    for fp in fingerprints {
        if !list.contains(fp) {
            list.push(fp.clone());
        }
    }
    if list.len() > MAX_IGNORED_PER_REPO {
        let extra = list.len() - MAX_IGNORED_PER_REPO;
        list.drain(..extra);
    }
    db.set_setting(KEY_IGNORED, &map)?;
    Ok(())
}

pub(crate) fn check(st: &AppState, root: &Path) -> AppResult<SecretGuardCheck> {
    if !enabled(&st.storage)? {
        return Ok(SecretGuardCheck {
            enabled: false,
            report: SecretReport::default(),
        });
    }
    let ignored = ignored_for(&st.storage, root)?;
    let report = secrets::scan_staged(&st.git, root, ignored)?;
    tracing::info!(
        findings = report.findings.len(),
        ignored = report.ignored,
        files = report.files_scanned,
        truncated = report.truncated,
        "secret guard: staged changes scanned"
    );
    Ok(SecretGuardCheck {
        enabled: true,
        report,
    })
}

// ----- commands ------------------------------------------------------------------------------

/// Scans the staged changes of `repo_id` for secrets (nothing when the guard is off).
#[tauri::command]
#[specta::specta]
pub async fn secret_guard_check(app: AppHandle, repo_id: i64) -> AppResult<SecretGuardCheck> {
    let root = repo_root(&app.state::<AppState>(), repo_id)?;
    tauri::async_runtime::spawn_blocking(move || check(&app.state::<AppState>(), &root))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

/// "Always ignore in this repository": remembers the findings' fingerprints in app storage.
#[tauri::command]
#[specta::specta]
pub fn secret_guard_ignore(
    app: AppHandle,
    repo_id: i64,
    fingerprints: Vec<String>,
) -> AppResult<SecretGuardPrefs> {
    let st = app.state::<AppState>();
    let root = repo_root(&st, repo_id)?;
    add_ignored(&st.storage, &root, &fingerprints)?;
    prefs(&st.storage)
}

#[tauri::command]
#[specta::specta]
pub fn secret_guard_prefs(app: AppHandle) -> AppResult<SecretGuardPrefs> {
    prefs(&app.state::<AppState>().storage)
}

#[tauri::command]
#[specta::specta]
pub fn secret_guard_set_enabled(app: AppHandle, enabled: bool) -> AppResult<SecretGuardPrefs> {
    let db = &app.state::<AppState>().storage;
    db.set_setting(KEY_ENABLED, &enabled)?;
    prefs(db)
}

/// Forgets every "always ignore" choice (all repositories).
#[tauri::command]
#[specta::specta]
pub fn secret_guard_clear_ignored(app: AppHandle) -> AppResult<SecretGuardPrefs> {
    let db = &app.state::<AppState>().storage;
    db.delete_setting(KEY_IGNORED)?;
    prefs(db)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FP_A: &str = "0123456789abcdef0123456789abcdef";
    const FP_B: &str = "fedcba9876543210fedcba9876543210";

    #[test]
    fn on_by_default_and_can_be_turned_off() {
        let db = Storage::open_in_memory().unwrap();
        assert_eq!(
            prefs(&db).unwrap(),
            SecretGuardPrefs {
                enabled: true,
                ignored_count: 0
            }
        );
        db.set_setting(KEY_ENABLED, &false).unwrap();
        assert!(!enabled(&db).unwrap());
    }

    #[test]
    fn ignored_fingerprints_are_per_repository() {
        let db = Storage::open_in_memory().unwrap();
        let a = Path::new("/work/a");
        let b = Path::new("/work/b");
        add_ignored(&db, a, &[FP_A.into(), FP_A.into()]).unwrap();
        add_ignored(&db, b, &[FP_B.into()]).unwrap();
        assert_eq!(
            ignored_for(&db, a).unwrap(),
            HashSet::from([FP_A.to_owned()])
        );
        assert_eq!(
            ignored_for(&db, b).unwrap(),
            HashSet::from([FP_B.to_owned()])
        );
        assert!(ignored_for(&db, Path::new("/work/c")).unwrap().is_empty());
        assert_eq!(prefs(&db).unwrap().ignored_count, 2);

        db.delete_setting(KEY_IGNORED).unwrap();
        assert_eq!(prefs(&db).unwrap().ignored_count, 0);
    }

    #[test]
    fn rejects_anything_but_a_fingerprint() {
        let db = Storage::open_in_memory().unwrap();
        let err = add_ignored(&db, Path::new("/r"), &["ghp_notafingerprint".into()]);
        assert!(err.is_err());
        assert_eq!(prefs(&db).unwrap().ignored_count, 0);
    }

    #[test]
    fn keeps_a_bounded_list() {
        let db = Storage::open_in_memory().unwrap();
        let many: Vec<String> = (0..MAX_IGNORED_PER_REPO + 5)
            .map(|i| format!("{i:032x}"))
            .collect();
        add_ignored(&db, Path::new("/r"), &many).unwrap();
        let kept = ignored_for(&db, Path::new("/r")).unwrap();
        assert_eq!(kept.len(), MAX_IGNORED_PER_REPO);
        assert!(!kept.contains(&format!("{:032x}", 0)));
    }

    #[test]
    fn repo_keys_ignore_separators_and_trailing_slash() {
        assert_eq!(
            repo_key(Path::new("/work/repo/")),
            repo_key(Path::new("/work/repo"))
        );
        if cfg!(windows) {
            assert_eq!(
                repo_key(Path::new(r"D:\Work\Repo")),
                repo_key(Path::new("d:/work/repo"))
            );
        }
    }
}
