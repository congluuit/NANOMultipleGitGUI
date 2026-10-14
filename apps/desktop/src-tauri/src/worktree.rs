//! Working tree and commit commands: status, file diffs, staging files/hunks/lines, discarding
//! changes and creating or amending commits.

use std::path::PathBuf;

use nanomgg_git_engine::commit_write::{self, CommitOptions, CommitResult, GitIdentity};
use nanomgg_git_engine::diff::{
    self, DiffRequest, FileContents, FileDiff, LineSelection, PatchAction,
};
use nanomgg_git_engine::status::{WorkingTreeStatus, read_status};
use nanomgg_git_engine::{GitRunner, index};
use tauri::{AppHandle, Manager};

use crate::accounts::{self, AccountOverride};
use crate::error::{AppError, AppResult};
use crate::repos::repo_root;
use crate::state::AppState;

/// Resolves the repository and runs `f` with its root off the IPC thread.
async fn with_repo<T: Send + 'static>(
    app: AppHandle,
    repo_id: i64,
    f: impl FnOnce(&GitRunner, &PathBuf) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    let root = repo_root(&app.state::<AppState>(), repo_id)?;
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppState>().git, &root))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn worktree_status(app: AppHandle, repo_id: i64) -> AppResult<WorkingTreeStatus> {
    with_repo(app, repo_id, |git, root| Ok(read_status(git, root)?)).await
}

#[tauri::command]
#[specta::specta]
pub async fn file_diff(app: AppHandle, repo_id: i64, request: DiffRequest) -> AppResult<FileDiff> {
    with_repo(app, repo_id, move |git, root| {
        Ok(diff::file_diff(git, root, &request)?)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn file_contents(
    app: AppHandle,
    repo_id: i64,
    request: DiffRequest,
) -> AppResult<FileContents> {
    with_repo(app, repo_id, move |git, root| {
        Ok(diff::file_contents(git, root, &request)?)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn stage_files(app: AppHandle, repo_id: i64, paths: Vec<String>) -> AppResult<()> {
    with_repo(app, repo_id, move |git, root| {
        Ok(index::stage(git, root, &paths)?)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn unstage_files(app: AppHandle, repo_id: i64, paths: Vec<String>) -> AppResult<()> {
    with_repo(app, repo_id, move |git, root| {
        Ok(index::unstage(git, root, &paths)?)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn stage_all(app: AppHandle, repo_id: i64) -> AppResult<()> {
    with_repo(app, repo_id, |git, root| Ok(index::stage_all(git, root)?)).await
}

#[tauri::command]
#[specta::specta]
pub async fn unstage_all(app: AppHandle, repo_id: i64) -> AppResult<()> {
    with_repo(app, repo_id, |git, root| Ok(index::unstage_all(git, root)?)).await
}

/// Discards unstaged changes of tracked files and deletes the given untracked files.
#[tauri::command]
#[specta::specta]
pub async fn discard_files(
    app: AppHandle,
    repo_id: i64,
    tracked: Vec<String>,
    untracked: Vec<String>,
) -> AppResult<()> {
    tracing::info!(
        repo = repo_id,
        tracked = tracked.len(),
        untracked = untracked.len(),
        "discard files"
    );
    with_repo(app, repo_id, move |git, root| {
        Ok(index::discard(git, root, &tracked, &untracked)?)
    })
    .await
}

/// Stages, unstages or discards selected lines/hunks of one file. `fingerprint` is the value of
/// the diff the user made the selection on; a changed file is refused.
#[tauri::command]
#[specta::specta]
pub async fn apply_lines(
    app: AppHandle,
    repo_id: i64,
    action: PatchAction,
    path: String,
    fingerprint: String,
    selection: Vec<LineSelection>,
) -> AppResult<()> {
    with_repo(app, repo_id, move |git, root| {
        Ok(diff::apply_lines(
            git,
            root,
            action,
            &path,
            &fingerprint,
            &selection,
        )?)
    })
    .await
}

/// Commits as the repo's account (identity injected through `GIT_CONFIG_*`), or as
/// `as_account` for a one-shot "Commit as…". Without an override, the identity copy in
/// `.git/config` is refreshed first when "write identity to repo config" is on and it differs;
/// an override never touches the repo config.
pub(crate) fn commit_create_impl(
    st: &AppState,
    repo_id: i64,
    message: &str,
    options: &CommitOptions,
    as_account: Option<&AccountOverride>,
) -> AppResult<CommitResult> {
    let root = repo_root(st, repo_id)?;
    if as_account.is_none()
        && let Err(e) = accounts::sync_repo_config(st, repo_id)
    {
        tracing::warn!(repo = repo_id, error = %e, "could not update repo identity config");
    }
    let ctx = accounts::context_with(st, repo_id, None, as_account)?;
    Ok(commit_write::commit(
        &st.git, &root, &ctx, message, options,
    )?)
}

#[tauri::command]
#[specta::specta]
pub async fn commit_create(
    app: AppHandle,
    repo_id: i64,
    message: String,
    options: CommitOptions,
    as_account: Option<AccountOverride>,
) -> AppResult<CommitResult> {
    let overridden = as_account.is_some();
    let result = tauri::async_runtime::spawn_blocking(move || {
        commit_create_impl(
            &app.state::<AppState>(),
            repo_id,
            &message,
            &options,
            as_account.as_ref(),
        )
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;
    tracing::info!(repo = repo_id, hash = %result.hash, overridden, "commit created");
    Ok(result)
}

/// Name and email git's own configuration (repo + global) gives for this repository, i.e. what
/// git outside the app records. Deliberately without the account context: the "Commit as"
/// block shows the bound account's identity itself and warns when this one differs; unbound
/// repos commit with exactly this identity.
#[tauri::command]
#[specta::specta]
pub async fn commit_identity(app: AppHandle, repo_id: i64) -> AppResult<GitIdentity> {
    with_repo(app, repo_id, |git, root| {
        Ok(commit_write::identity(
            git,
            root,
            &nanomgg_git_engine::AccountContext::default(),
        ))
    })
    .await
}

/// Message of the last commit, to prefill "amend".
#[tauri::command]
#[specta::specta]
pub async fn head_message(app: AppHandle, repo_id: i64) -> AppResult<Option<String>> {
    with_repo(app, repo_id, |git, root| {
        Ok(commit_write::head_message(git, root))
    })
    .await
}
