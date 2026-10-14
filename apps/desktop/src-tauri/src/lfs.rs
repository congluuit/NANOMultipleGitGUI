//! Git LFS commands: repository LFS status, `git lfs install --local`, track / untrack,
//! re-staging files as pointers and the Large File Guard (PLAN §6.1, §6.2).

use nanomgg_lfs::guard::{self, GuardProvider, GuardReport, GuardThresholds};
use nanomgg_lfs::{LfsError, LfsStatus, detect, setup};
use tauri::{AppHandle, Manager};

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

#[cfg(test)]
mod tests {
    use super::*;

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
