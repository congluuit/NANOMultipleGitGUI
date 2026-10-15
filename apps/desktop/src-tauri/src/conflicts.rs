//! Conflict editor commands: read a conflicted file (stages + working file), explain which
//! commits "ours" / "theirs" are, preview a stage (images, LFS objects), and resolve a file by
//! saving edited content, keeping one side or deleting it.

use std::path::Path;

use nanomgg_git_engine::blob::{self, BlobPreview, BlobSource};
use nanomgg_git_engine::conflict::{self, ConflictContext, ConflictFile, ConflictSide};
use nanomgg_git_engine::ops::OpError;
use nanomgg_git_engine::{AccountContext, GitRunner};
use tauri::{AppHandle, Manager};

use crate::accounts;
use crate::branches::Outcome;
use crate::error::{AppError, AppResult};
use crate::repos::repo_root;
use crate::state::AppState;

fn outcome<T>(result: Result<T, OpError>) -> AppResult<Outcome<T>> {
    match result {
        Ok(value) => Ok(Outcome::Done { value }),
        Err(OpError::Rejected { kind, message }) => Ok(Outcome::Rejected {
            reason: kind,
            message: nanomgg_logging::redact(&message).into_owned(),
        }),
        Err(OpError::Cancelled) => Ok(Outcome::Cancelled),
        Err(OpError::Git(e)) => Err(e.into()),
    }
}

/// Runs a read off the IPC thread.
async fn read<T: Send + 'static>(
    app: AppHandle,
    repo_id: i64,
    f: impl FnOnce(&GitRunner, &Path) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    let root = repo_root(&app.state::<AppState>(), repo_id)?;
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppState>().git, &root))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

/// Runs a resolution write as the repo's account.
async fn write<T: Send + 'static>(
    app: AppHandle,
    repo_id: i64,
    f: impl FnOnce(&GitRunner, &Path, &AccountContext) -> Result<T, OpError> + Send + 'static,
) -> AppResult<Outcome<T>> {
    let root = repo_root(&app.state::<AppState>(), repo_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        let ctx = accounts::context_for(&st, repo_id, None)?;
        outcome(f(&st.git, &root, &ctx))
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

/// Operation in progress and the commits behind "ours" / "theirs".
#[tauri::command]
#[specta::specta]
pub async fn conflict_context(app: AppHandle, repo_id: i64) -> AppResult<ConflictContext> {
    read(app, repo_id, |git, root| {
        conflict::conflict_context(git, root).map_err(|e| match e {
            OpError::Git(g) => g.into(),
            other => AppError::Internal(other.to_string()),
        })
    })
    .await
}

/// Stages and working-tree text of one conflicted file.
#[tauri::command]
#[specta::specta]
pub async fn conflict_file(app: AppHandle, repo_id: i64, path: String) -> AppResult<ConflictFile> {
    read(app, repo_id, move |git, root| {
        Ok(conflict::conflict_file(git, root, &path)?)
    })
    .await
}

/// Preview of a file from a commit, an index stage or the working tree (images, LFS objects).
#[tauri::command]
#[specta::specta]
pub async fn blob_preview(
    app: AppHandle,
    repo_id: i64,
    source: BlobSource,
    path: String,
) -> AppResult<Option<BlobPreview>> {
    read(app, repo_id, move |git, root| {
        Ok(blob::blob_preview(git, root, &source, &path)?)
    })
    .await
}

/// Writes the edited result (keeping line endings and BOM) and marks the file resolved.
#[tauri::command]
#[specta::specta]
pub async fn conflict_resolve_content(
    app: AppHandle,
    repo_id: i64,
    path: String,
    content: String,
) -> AppResult<Outcome<()>> {
    tracing::info!(repo = repo_id, "resolve conflict with edited content");
    write(app, repo_id, move |git, root, _| {
        conflict::resolve_with_content(git, root, &path, &content)
    })
    .await
}

/// Keeps one side of a conflicted file as a whole (`checkout --ours/--theirs` + add, or `rm`
/// when that side deleted it).
#[tauri::command]
#[specta::specta]
pub async fn conflict_take_side(
    app: AppHandle,
    repo_id: i64,
    path: String,
    side: ConflictSide,
) -> AppResult<Outcome<()>> {
    tracing::info!(repo = repo_id, ?side, "resolve conflict by taking a side");
    write(app, repo_id, move |git, root, ctx| {
        conflict::resolve_take(git, root, ctx, &path, side)
    })
    .await
}

/// Resolves a conflicted file by deleting it (`git rm`).
#[tauri::command]
#[specta::specta]
pub async fn conflict_delete_file(
    app: AppHandle,
    repo_id: i64,
    path: String,
) -> AppResult<Outcome<()>> {
    tracing::info!(repo = repo_id, "resolve conflict by deleting the file");
    write(app, repo_id, move |git, root, ctx| {
        conflict::resolve_delete(git, root, ctx, &path)
    })
    .await
}
