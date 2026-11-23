//! The built-in editor's repository commands: list and search the files (quick open, file
//! tree, search panel), create / rename / delete files and folders, and stage an edited text
//! of a file (stage or unstage one block from the editor).

use std::path::{Path, PathBuf};

use nanomgg_git_engine::GitRunner;
use nanomgg_git_engine::edit::{self, RepoFiles, SearchOptions, SearchResults};
use nanomgg_git_engine::workspace_search::{
    self, RepoSearchResult, SearchLimits, SearchTarget, WorkspaceSearchSummary,
};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use crate::branches::Registration;
use crate::error::{AppError, AppResult};
use crate::repos::repo_root;
use crate::state::AppState;

/// At most this many matching lines are returned by a search.
const MAX_SEARCH_RESULTS: usize = 5000;

async fn with_repo<T: Send + 'static>(
    app: AppHandle,
    repo_id: i64,
    f: impl FnOnce(&GitRunner, &Path) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    let root = repo_root(&app.state::<AppState>(), repo_id)?;
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppState>().git, &root))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

/// Every file of the working tree that is not ignored (quick open, file tree).
#[tauri::command]
#[specta::specta]
pub async fn repo_files(app: AppHandle, repo_id: i64) -> AppResult<RepoFiles> {
    with_repo(app, repo_id, |git, root| Ok(edit::list_files(git, root)?)).await
}

/// Lines of the working tree matching `query` (`git grep`).
#[tauri::command]
#[specta::specta]
pub async fn repo_search(
    app: AppHandle,
    repo_id: i64,
    query: String,
    options: SearchOptions,
    limit: u32,
) -> AppResult<SearchResults> {
    let limit = (limit as usize).clamp(1, MAX_SEARCH_RESULTS);
    with_repo(app, repo_id, move |git, root| {
        Ok(edit::search(git, root, &query, options, limit)?)
    })
    .await
}

/// Matching lines kept per repository when searching every open repository.
const MAX_SEARCH_PER_REPO: usize = 1000;
/// Repositories searched at the same time (`git grep` already uses several threads).
const SEARCH_CONCURRENCY: usize = 3;

/// The open repositories in tab order, for [`workspace_search`].
fn search_targets(st: &AppState) -> AppResult<Vec<SearchTarget>> {
    let tabs: Vec<(i64, Option<PathBuf>)> = {
        let ws = st.workspace();
        ws.tabs
            .iter()
            .map(|id| (*id, ws.repos.get(id).map(|r| r.root.clone())))
            .collect()
    };
    let mut targets = Vec::with_capacity(tabs.len());
    for (repo_id, root) in tabs {
        if let Some(repo) = st.storage.repo(repo_id)? {
            targets.push(SearchTarget {
                repo_id,
                name: repo.name,
                root,
            });
        }
    }
    Ok(targets)
}

/// Searches every open repository (`git grep`, a few at a time). Each repository's result is
/// sent on `on_result` as soon as it is known; at most `limit` lines in all. Cancel with
/// `transfer_cancel(opId)`: running searches stop at once and nothing more is sent.
#[tauri::command]
#[specta::specta]
pub async fn workspace_search(
    app: AppHandle,
    op_id: String,
    query: String,
    options: SearchOptions,
    limit: u32,
    on_result: Channel<RepoSearchResult>,
) -> AppResult<WorkspaceSearchSummary> {
    let total = (limit as usize).clamp(1, MAX_SEARCH_RESULTS);
    // Registered before the thread starts, so a cancel sent right away is not lost.
    let (registration, token) = Registration::new(&op_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _registration = registration;
        let st = app.state::<AppState>();
        let targets = search_targets(&st)?;
        let limits = SearchLimits {
            total,
            per_repo: MAX_SEARCH_PER_REPO.min(total),
            concurrency: SEARCH_CONCURRENCY,
        };
        let send = |mut r: RepoSearchResult| {
            if let Some(e) = r.error.take() {
                r.error = Some(nanomgg_logging::redact(&e).into_owned());
            }
            if let Err(e) = on_result.send(r) {
                tracing::warn!(error = %e, "failed to send a search result");
            }
        };
        let summary = workspace_search::search_repos(
            &st.git, &targets, &query, options, limits, &token, &send,
        );
        tracing::info!(
            repos = targets.len(),
            searched = summary.searched,
            cancelled = summary.cancelled,
            "searched open repositories"
        );
        Ok(summary)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

/// Creates an empty file (with its missing parent folders).
#[tauri::command]
#[specta::specta]
pub async fn file_create(app: AppHandle, repo_id: i64, path: String) -> AppResult<()> {
    tracing::info!(repo = repo_id, "create file");
    with_repo(app, repo_id, move |_, root| {
        Ok(edit::create_file(root, &path)?)
    })
    .await
}

/// Creates a folder (with its missing parents).
#[tauri::command]
#[specta::specta]
pub async fn folder_create(app: AppHandle, repo_id: i64, path: String) -> AppResult<()> {
    tracing::info!(repo = repo_id, "create folder");
    with_repo(app, repo_id, move |_, root| {
        Ok(edit::create_dir(root, &path)?)
    })
    .await
}

/// Renames or moves a file or folder inside the repository.
#[tauri::command]
#[specta::specta]
pub async fn path_rename(app: AppHandle, repo_id: i64, from: String, to: String) -> AppResult<()> {
    tracing::info!(repo = repo_id, "rename path");
    with_repo(app, repo_id, move |_, root| {
        Ok(edit::rename_path(root, &from, &to)?)
    })
    .await
}

/// Deletes a file, or a folder with everything in it.
#[tauri::command]
#[specta::specta]
pub async fn path_delete(app: AppHandle, repo_id: i64, path: String) -> AppResult<()> {
    tracing::info!(repo = repo_id, "delete path");
    with_repo(app, repo_id, move |_, root| {
        Ok(edit::delete_path(root, &path)?)
    })
    .await
}

/// Writes `text` as the staged version of `path`. `expected` is the index text the edit was
/// based on (`None`: not in the index); `false` when the index changed since.
#[tauri::command]
#[specta::specta]
pub async fn index_stage_text(
    app: AppHandle,
    repo_id: i64,
    path: String,
    text: String,
    expected: Option<String>,
) -> AppResult<bool> {
    tracing::info!(repo = repo_id, "stage edited text");
    with_repo(app, repo_id, move |git, root| {
        Ok(edit::stage_text(
            git,
            root,
            &path,
            &text,
            expected.as_deref(),
        )?)
    })
    .await
}
