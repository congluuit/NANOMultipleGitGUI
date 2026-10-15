//! Read-only file views: history of a file (following renames), blame, the file as of a commit
//! and both sides of an image diff.

use std::path::Path;

use nanomgg_git_engine::GitRunner;
use nanomgg_git_engine::blame::{self, BlameResult};
use nanomgg_git_engine::diff::{self, DiffRequest, ImagePair};
use nanomgg_git_engine::file_history::{self, FileAtCommit, FileHistoryPage};
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};
use crate::repos::repo_root;
use crate::state::AppState;

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

/// One page of the commits that changed `path` (`git log --follow`), newest first.
#[tauri::command]
#[specta::specta]
pub async fn file_history(
    app: AppHandle,
    repo_id: i64,
    path: String,
    rev: Option<String>,
    skip: u32,
    limit: u32,
) -> AppResult<FileHistoryPage> {
    read(app, repo_id, move |git, root| {
        Ok(file_history::file_history(
            git,
            root,
            &path,
            rev.as_deref(),
            skip,
            limit,
        )?)
    })
    .await
}

/// `git blame` of `path` at `rev` (hex id or `HEAD`), or of the working tree when `None`.
#[tauri::command]
#[specta::specta]
pub async fn blame_file(
    app: AppHandle,
    repo_id: i64,
    path: String,
    rev: Option<String>,
) -> AppResult<BlameResult> {
    read(app, repo_id, move |git, root| {
        Ok(blame::blame(git, root, &path, rev.as_deref())?)
    })
    .await
}

/// The file as of one commit (read-only viewer).
#[tauri::command]
#[specta::specta]
pub async fn file_at_commit(
    app: AppHandle,
    repo_id: i64,
    commit: String,
    path: String,
) -> AppResult<FileAtCommit> {
    read(app, repo_id, move |git, root| {
        Ok(file_history::file_at_commit(git, root, &commit, &path)?)
    })
    .await
}

/// Old and new version of an image for the image diff (LFS objects from the local store).
#[tauri::command]
#[specta::specta]
pub async fn diff_images(
    app: AppHandle,
    repo_id: i64,
    request: DiffRequest,
) -> AppResult<ImagePair> {
    read(app, repo_id, move |git, root| {
        Ok(diff::image_pair(git, root, &request)?)
    })
    .await
}
