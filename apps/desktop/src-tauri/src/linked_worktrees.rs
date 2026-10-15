//! Linked worktree commands (`git worktree`, PLAN §5.9). Named "linked worktrees" because
//! `worktree.rs` is the *working tree* (staging) of the open repository.
//!
//! Opening a worktree as a tab uses `repo_open` with its path. A branch checked out in one
//! worktree cannot be checked out in another: git's refusal comes back as the
//! `checkedOutElsewhere` failure kind (also from `branch_checkout`).

use std::path::{Path, PathBuf};

use nanomgg_git_engine::ops::OpError;
use nanomgg_git_engine::worktrees::{self, Worktree, WorktreeTarget};
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

/// Runs a worktree operation on a blocking thread as the repo's default account (hooks such
/// as `post-checkout` of `worktree add` see its identity).
async fn run_op<T: Send + 'static>(
    app: AppHandle,
    repo_id: i64,
    f: impl FnOnce(&AppState, &Path, &nanomgg_git_engine::AccountContext) -> Result<T, OpError>
    + Send
    + 'static,
) -> AppResult<Outcome<T>> {
    let root = repo_root(&app.state::<AppState>(), repo_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        let ctx = accounts::context_for(&st, repo_id, None)?;
        outcome(f(&st, &root, &ctx))
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

/// Worktrees of the repository (main first; `isCurrent` marks the open one).
#[tauri::command]
#[specta::specta]
pub async fn linked_worktree_list(app: AppHandle, repo_id: i64) -> AppResult<Vec<Worktree>> {
    let root = repo_root(&app.state::<AppState>(), repo_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        Ok(worktrees::list(&app.state::<AppState>().git, &root)?)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

/// `git worktree add` at the absolute folder `path` (missing or empty) for `target`.
#[tauri::command]
#[specta::specta]
pub async fn linked_worktree_add(
    app: AppHandle,
    repo_id: i64,
    path: String,
    target: WorktreeTarget,
) -> AppResult<Outcome<Worktree>> {
    run_op(app, repo_id, move |st, root, ctx| {
        worktrees::add(&st.git, root, ctx, &PathBuf::from(&path), &target)
    })
    .await
}

/// `git worktree remove [--force]`; changes inside come back as `dirtyWorktree`.
#[tauri::command]
#[specta::specta]
pub async fn linked_worktree_remove(
    app: AppHandle,
    repo_id: i64,
    path: String,
    force: bool,
) -> AppResult<Outcome<()>> {
    run_op(app, repo_id, move |st, root, ctx| {
        worktrees::remove(&st.git, root, ctx, &path, force)
    })
    .await
}

/// `git worktree prune -v` (`dryRun`: only list what would be pruned).
#[tauri::command]
#[specta::specta]
pub async fn linked_worktree_prune(
    app: AppHandle,
    repo_id: i64,
    dry_run: bool,
) -> AppResult<Outcome<Vec<String>>> {
    run_op(app, repo_id, move |st, root, ctx| {
        worktrees::prune(&st.git, root, ctx, dry_run)
    })
    .await
}

/// `git worktree lock [--reason]` / `unlock`.
#[tauri::command]
#[specta::specta]
pub async fn linked_worktree_lock(
    app: AppHandle,
    repo_id: i64,
    path: String,
    locked: bool,
    reason: Option<String>,
) -> AppResult<Outcome<()>> {
    run_op(app, repo_id, move |st, root, ctx| {
        worktrees::set_locked(&st.git, root, ctx, &path, locked, reason.as_deref())
    })
    .await
}
