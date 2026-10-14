//! Stash commands: create, apply / pop / drop by index and show a stash's files.
//! The stash list itself comes with `repo_refs` (`RepoRefs::stashes`).

use nanomgg_git_engine::stash::{
    self, StashApplyOutcome, StashCreated, StashDetails, StashOptions,
};
use tauri::{AppHandle, Manager};

use crate::accounts::context_for;
use crate::error::{AppError, AppResult};
use crate::repos::repo_root;
use crate::state::AppState;

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

/// `git stash push`. `created: false` means there was nothing to stash.
#[tauri::command]
#[specta::specta]
pub async fn stash_create(
    app: AppHandle,
    repo_id: i64,
    options: StashOptions,
) -> AppResult<StashCreated> {
    with_repo(app, repo_id, move |st, repo| {
        // The stash commit records the repo account's identity.
        let ctx = context_for(st, repo_id, None)?;
        let created = stash::stash_create(&st.git, repo, &ctx, &options)?;
        tracing::info!(repo = repo_id, ?created, "stash created");
        Ok(created)
    })
    .await
}

/// `git stash apply [--index] stash@{index}`. Conflicts are returned in the outcome, not as
/// an error; the entry stays in the list.
#[tauri::command]
#[specta::specta]
pub async fn stash_apply(
    app: AppHandle,
    repo_id: i64,
    index: u32,
    restore_index: bool,
) -> AppResult<StashApplyOutcome> {
    with_repo(app, repo_id, move |st, repo| {
        Ok(stash::stash_apply(
            &st.git,
            repo,
            &context_for(st, repo_id, None)?,
            index,
            restore_index,
        )?)
    })
    .await
}

/// `git stash pop [--index] stash@{index}`. On conflicts the entry is kept (`dropped: false`).
#[tauri::command]
#[specta::specta]
pub async fn stash_pop(
    app: AppHandle,
    repo_id: i64,
    index: u32,
    restore_index: bool,
) -> AppResult<StashApplyOutcome> {
    with_repo(app, repo_id, move |st, repo| {
        Ok(stash::stash_pop(
            &st.git,
            repo,
            &context_for(st, repo_id, None)?,
            index,
            restore_index,
        )?)
    })
    .await
}

/// `git stash drop stash@{index}`; returns the dropped commit id (for an undo hint).
#[tauri::command]
#[specta::specta]
pub async fn stash_drop(app: AppHandle, repo_id: i64, index: u32) -> AppResult<String> {
    with_repo(app, repo_id, move |st, repo| {
        let oid = stash::stash_drop(&st.git, repo, index)?;
        tracing::info!(repo = repo_id, index, %oid, "stash dropped");
        Ok(oid)
    })
    .await
}

/// Files of `stash@{index}`: tracked changes plus untracked files (third parent).
#[tauri::command]
#[specta::specta]
pub async fn stash_show(app: AppHandle, repo_id: i64, index: u32) -> AppResult<StashDetails> {
    with_repo(app, repo_id, move |st, repo| {
        Ok(stash::stash_show(&st.git, repo, index)?)
    })
    .await
}
