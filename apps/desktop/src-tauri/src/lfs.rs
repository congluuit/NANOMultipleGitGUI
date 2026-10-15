//! Git LFS commands: repository LFS status, `git lfs install --local`, track / untrack,
//! re-staging files as pointers and the Large File Guard (PLAN §6.1, §6.2); locks, the LFS
//! file list and maintenance (prune / fsck / local size) (PLAN §6.6, §6.7).

use nanomgg_lfs::files::{self, LfsFile};
use nanomgg_lfs::guard::{self, GuardProvider, GuardReport, GuardThresholds};
use nanomgg_lfs::locks::{self, LfsLock, LockError, LockFailure, LockList};
use nanomgg_lfs::maintenance::{self, FsckReport, LfsStorage, PruneReport};
use nanomgg_lfs::{LfsError, LfsStatus, detect, setup};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};

use crate::accounts;
use crate::error::{AppError, AppResult};
use crate::repos::repo_root;
use crate::state::AppState;

impl From<LfsError> for AppError {
    fn from(e: LfsError) -> Self {
        match e {
            LfsError::Git(git) => git.into(),
            LfsError::Io(io) => Self::Git(io.to_string()),
            LfsError::NotInstalled => Self::Unsupported(e.to_string()),
            LfsError::InvalidPattern(_) | LfsError::InvalidPath(_) => {
                Self::InvalidInput(e.to_string())
            }
        }
    }
}

/// Runs blocking git work for an open repository off the IPC thread.
async fn with_repo<T: Send + 'static>(
    app: AppHandle,
    repo_id: i64,
    f: impl FnOnce(&AppState, &std::path::Path) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    let repo = repo_root(&app.state::<AppState>(), repo_id)?;
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppState>(), &repo))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

/// git-lfs version, LFS patterns, hooks and filter config of the repository.
#[tauri::command]
#[specta::specta]
pub async fn lfs_status(app: AppHandle, repo_id: i64) -> AppResult<LfsStatus> {
    with_repo(app, repo_id, |st, repo| Ok(detect::detect(&st.git, repo)?)).await
}

/// `git lfs install --local` (this repository only), then the refreshed status.
#[tauri::command]
#[specta::specta]
pub async fn lfs_install_local(app: AppHandle, repo_id: i64) -> AppResult<LfsStatus> {
    with_repo(app, repo_id, move |st, repo| {
        setup::install_local(&st.git, repo)?;
        tracing::info!(repo = repo_id, "git lfs install --local");
        Ok(detect::detect(&st.git, repo)?)
    })
    .await
}

/// `git lfs track [--lockable] -- <pattern>`; `true` when `.gitattributes` changed.
#[tauri::command]
#[specta::specta]
pub async fn lfs_track(
    app: AppHandle,
    repo_id: i64,
    pattern: String,
    lockable: bool,
) -> AppResult<bool> {
    with_repo(app, repo_id, move |st, repo| {
        Ok(setup::track(&st.git, repo, &pattern, lockable)?)
    })
    .await
}

/// `git lfs untrack -- <pattern>`; `true` when `.gitattributes` changed.
#[tauri::command]
#[specta::specta]
pub async fn lfs_untrack(app: AppHandle, repo_id: i64, pattern: String) -> AppResult<bool> {
    with_repo(app, repo_id, move |st, repo| {
        Ok(setup::untrack(&st.git, repo, &pattern)?)
    })
    .await
}

/// After `lfs_track`: stage `.gitattributes`, renormalize already-tracked paths and add new
/// ones, so every path is staged as an LFS pointer.
#[tauri::command]
#[specta::specta]
pub async fn lfs_restage(app: AppHandle, repo_id: i64, paths: Vec<String>) -> AppResult<()> {
    with_repo(app, repo_id, move |st, repo| {
        Ok(setup::stage_as_lfs(&st.git, repo, &paths)?)
    })
    .await
}

/// Large File Guard over `paths` (staged / new files). `thresholds` overrides the provider
/// defaults (GitHub warn 50 MiB / block 100 MiB, GitLab and others warn 50 MiB, no block).
#[tauri::command]
#[specta::specta]
pub async fn large_file_check(
    app: AppHandle,
    repo_id: i64,
    paths: Vec<String>,
    provider: GuardProvider,
    thresholds: Option<GuardThresholds>,
) -> AppResult<GuardReport> {
    with_repo(app, repo_id, move |st, repo| {
        let thresholds = thresholds.unwrap_or_else(|| GuardThresholds::for_provider(provider));
        Ok(guard::check(&st.git, repo, &paths, thresholds)?)
    })
    .await
}

// ----- locks, files, maintenance (PLAN §6.6, §6.7) ---------------------------------------------

/// Result of a lock command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum LockOutcome<T> {
    Done {
        value: T,
    },
    /// git-lfs refused; `failure.kind` says why (already locked, not owner, no permission…).
    Failed {
        failure: LockFailure,
    },
}

fn lock_outcome<T>(result: Result<T, LockError>) -> AppResult<LockOutcome<T>> {
    match result {
        Ok(value) => Ok(LockOutcome::Done { value }),
        Err(LockError::Failed(mut failure)) => {
            failure.message = nanomgg_logging::redact(&failure.message).into_owned();
            Ok(LockOutcome::Failed { failure })
        }
        Err(LockError::Lfs(e)) => Err(e.into()),
    }
}

/// Context + user name of the account the lock API runs as (the repo default, which follows
/// `origin` — the remote git-lfs uses unless configured otherwise).
fn lock_context(
    st: &AppState,
    repo_id: i64,
) -> AppResult<(nanomgg_git_engine::AccountContext, Option<String>)> {
    let ctx = accounts::context_for(st, repo_id, None)?;
    let root = repo_root(st, repo_id)?;
    let username = accounts::resolve_repo(st, repo_id, &root)?
        .default
        .and_then(|r| st.storage.account(&r.account_id).ok().flatten())
        .map(|a| a.username);
    Ok((ctx, username))
}

/// `git lfs locks --verify --json` (falls back to the plain list): who holds which file.
#[tauri::command]
#[specta::specta]
pub async fn lfs_locks(app: AppHandle, repo_id: i64) -> AppResult<LockOutcome<LockList>> {
    with_repo(app, repo_id, move |st, repo| {
        let (ctx, username) = lock_context(st, repo_id)?;
        lock_outcome(locks::list(&st.git, repo, &ctx, None, username.as_deref()))
    })
    .await
}

/// `git lfs lock <path>` as the repo's account.
#[tauri::command]
#[specta::specta]
pub async fn lfs_lock(
    app: AppHandle,
    repo_id: i64,
    path: String,
) -> AppResult<LockOutcome<LfsLock>> {
    with_repo(app, repo_id, move |st, repo| {
        let (ctx, _) = lock_context(st, repo_id)?;
        let r = locks::lock(&st.git, repo, &ctx, None, &path);
        tracing::info!(repo = repo_id, ok = r.is_ok(), "git lfs lock");
        lock_outcome(r)
    })
    .await
}

/// `git lfs unlock [--force]` by path (or lock id when the file is gone).
#[tauri::command]
#[specta::specta]
pub async fn lfs_unlock(
    app: AppHandle,
    repo_id: i64,
    path: Option<String>,
    id: Option<String>,
    force: bool,
) -> AppResult<LockOutcome<()>> {
    with_repo(app, repo_id, move |st, repo| {
        let (ctx, _) = lock_context(st, repo_id)?;
        let r = locks::unlock(
            &st.git,
            repo,
            &ctx,
            None,
            path.as_deref(),
            id.as_deref(),
            force,
        );
        tracing::info!(repo = repo_id, force, ok = r.is_ok(), "git lfs unlock");
        lock_outcome(r)
    })
    .await
}

/// Marks / unmarks an LFS pattern `lockable`; `true` when `.gitattributes` changed.
#[tauri::command]
#[specta::specta]
pub async fn lfs_set_lockable(
    app: AppHandle,
    repo_id: i64,
    pattern: String,
    lockable: bool,
) -> AppResult<bool> {
    with_repo(app, repo_id, move |st, repo| {
        Ok(locks::set_lockable(&st.git, repo, &pattern, lockable)?)
    })
    .await
}

/// LFS files of the checkout: size, downloaded / pointer, lockable.
#[tauri::command]
#[specta::specta]
pub async fn lfs_files(app: AppHandle, repo_id: i64) -> AppResult<Vec<LfsFile>> {
    with_repo(app, repo_id, move |st, repo| {
        Ok(files::list(
            &st.git,
            repo,
            &nanomgg_git_engine::AccountContext::default(),
        )?)
    })
    .await
}

/// Size of the local LFS object store.
#[tauri::command]
#[specta::specta]
pub async fn lfs_storage(app: AppHandle, repo_id: i64) -> AppResult<LfsStorage> {
    with_repo(app, repo_id, move |st, repo| {
        Ok(maintenance::storage(&st.git, repo))
    })
    .await
}

/// `git lfs prune --dry-run`: what would be deleted and how much it frees.
#[tauri::command]
#[specta::specta]
pub async fn lfs_prune_preview(
    app: AppHandle,
    repo_id: i64,
    verify_remote: bool,
) -> AppResult<PruneReport> {
    with_repo(app, repo_id, move |st, repo| {
        let ctx = accounts::context_for(st, repo_id, None)?;
        Ok(maintenance::prune_preview(
            &st.git,
            repo,
            &ctx,
            verify_remote,
        )?)
    })
    .await
}

/// `git lfs prune` (after the user confirmed the preview).
#[tauri::command]
#[specta::specta]
pub async fn lfs_prune(
    app: AppHandle,
    repo_id: i64,
    verify_remote: bool,
) -> AppResult<PruneReport> {
    with_repo(app, repo_id, move |st, repo| {
        let ctx = accounts::context_for(st, repo_id, None)?;
        let report = maintenance::prune(&st.git, repo, &ctx, verify_remote)?;
        tracing::info!(repo = repo_id, bytes = report.bytes, "git lfs prune");
        Ok(report)
    })
    .await
}

/// `git lfs fsck --dry-run` (objects and/or pointers), read-only.
#[tauri::command]
#[specta::specta]
pub async fn lfs_fsck(
    app: AppHandle,
    repo_id: i64,
    objects: bool,
    pointers: bool,
) -> AppResult<FsckReport> {
    with_repo(app, repo_id, move |st, repo| {
        Ok(maintenance::fsck(&st.git, repo, objects, pointers)?)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_outcomes_are_tagged_and_redacted() {
        let failed: LockOutcome<()> = lock_outcome(Err(LockError::Failed(LockFailure {
            kind: locks::LockFailureKind::NoPermission,
            message: "403 https://user:ghp_abcdefghijklmnopqrstuvwxyz0123456789@github.com/a"
                .into(),
        })))
        .unwrap();
        let json = serde_json::to_value(&failed).unwrap();
        assert_eq!(json["status"], "failed");
        assert_eq!(json["failure"]["kind"], "noPermission");
        assert!(
            !json["failure"]["message"]
                .as_str()
                .unwrap()
                .contains("ghp_abcdefghijklmnopqrstuvwxyz0123456789")
        );
        let done: LockOutcome<u8> = lock_outcome(Ok(1)).unwrap();
        assert_eq!(
            serde_json::to_value(&done).unwrap(),
            serde_json::json!({ "status": "done", "value": 1 })
        );
    }

    #[test]
    fn lfs_errors_map_to_app_error_kinds() {
        let json = |e: LfsError| serde_json::to_value(AppError::from(e)).unwrap()["kind"].clone();
        assert_eq!(json(LfsError::InvalidPattern("-x".into())), "invalidInput");
        assert_eq!(json(LfsError::InvalidPath("../x".into())), "invalidInput");
        assert_eq!(json(LfsError::NotInstalled), "unsupported");
        assert_eq!(
            json(LfsError::Git(nanomgg_git_engine::GitError::Failed {
                args: "lfs track".into(),
                status: "1".into(),
                stderr: "boom".into(),
            })),
            "git"
        );
    }
}
