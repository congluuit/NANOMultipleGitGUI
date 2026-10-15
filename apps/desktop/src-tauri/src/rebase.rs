//! Rebase commands (step 3.1): preview, plain rebase, interactive rebase with a prepared plan,
//! continue / skip, and the branch drag-and-drop helpers (relation, fast-forward).
//!
//! Rejections (dirty tree, commits already pushed, invalid plan…) come back as
//! [`Outcome::Rejected`]; conflicts and `edit` stops are a structured [`RebaseOutcome`].

use nanomgg_git_engine::rebase::{
    self, BranchRelation, InteractiveRebaseRequest, RebaseOutcome, RebasePreview, RebaseRequest,
};
use nanomgg_git_engine::undo::{self, UndoKind};
use tauri::AppHandle;

use crate::branches::{Outcome, run_op};
use crate::error::AppResult;

/// What `git rebase <onto>` would rewrite for `branch` (default: the current branch).
#[tauri::command]
#[specta::specta]
pub async fn rebase_preview(
    app: AppHandle,
    repo_id: i64,
    onto: String,
    branch: Option<String>,
) -> AppResult<Outcome<RebasePreview>> {
    run_op(app, repo_id, move |git, repo, _| {
        rebase::preview(git, repo, &onto, branch.as_deref())
    })
    .await
}

/// Rows of the interactive rebase dialog: commits of the current branch after `base`.
#[tauri::command]
#[specta::specta]
pub async fn rebase_todo(
    app: AppHandle,
    repo_id: i64,
    base: String,
) -> AppResult<Outcome<RebasePreview>> {
    run_op(app, repo_id, move |git, repo, _| {
        rebase::todo(git, repo, &base)
    })
    .await
}

/// Plain rebase (backup ref first; pushed commits need `allowPushed`).
#[tauri::command]
#[specta::specta]
pub async fn rebase_start(
    app: AppHandle,
    repo_id: i64,
    request: RebaseRequest,
) -> AppResult<Outcome<RebaseOutcome>> {
    run_op(app, repo_id, move |git, repo, ctx| {
        rebase::rebase(git, repo, ctx, &request)
    })
    .await
}

/// Interactive rebase with the plan from the dialog; never opens an editor.
#[tauri::command]
#[specta::specta]
pub async fn rebase_interactive(
    app: AppHandle,
    repo_id: i64,
    request: InteractiveRebaseRequest,
) -> AppResult<Outcome<RebaseOutcome>> {
    run_op(app, repo_id, move |git, repo, ctx| {
        rebase::rebase_interactive(git, repo, ctx, &request)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn rebase_continue(app: AppHandle, repo_id: i64) -> AppResult<Outcome<RebaseOutcome>> {
    run_op(app, repo_id, rebase::continue_rebase).await
}

/// Drops the commit the rebase stopped at and goes on.
#[tauri::command]
#[specta::specta]
pub async fn rebase_skip(app: AppHandle, repo_id: i64) -> AppResult<Outcome<RebaseOutcome>> {
    run_op(app, repo_id, rebase::skip_rebase).await
}

/// How `source` relates to `target` (for the branch drop menu).
#[tauri::command]
#[specta::specta]
pub async fn branch_relation(
    app: AppHandle,
    repo_id: i64,
    source: String,
    target: String,
) -> AppResult<Outcome<BranchRelation>> {
    run_op(app, repo_id, move |git, repo, _| {
        rebase::branch_relation(git, repo, &source, &target)
    })
    .await
}

/// Fast-forwards local branch `target` to `source` (recorded for Undo). Returns the new tip.
#[tauri::command]
#[specta::specta]
pub async fn branch_fast_forward(
    app: AppHandle,
    repo_id: i64,
    target: String,
    source: String,
) -> AppResult<Outcome<String>> {
    run_op(app, repo_id, move |git, repo, ctx| {
        let kind = UndoKind::FastForward {
            branch: target.clone(),
            source: source.clone(),
        };
        undo::record(git, repo, kind, &[&target], || {
            rebase::fast_forward_branch(git, repo, ctx, &target, &source)
        })
    })
    .await
}
