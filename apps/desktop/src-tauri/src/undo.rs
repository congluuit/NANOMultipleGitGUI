//! Undo / redo commands (step 3.1). The journal lives in `.git/nanomgg/undo.json` and is fed
//! by the HEAD reflog (see `nanomgg_git_engine::undo`).

use nanomgg_git_engine::ops::OpError;
use nanomgg_git_engine::undo::{self, UndoOutcome, UndoState};
use tauri::{AppHandle, Manager};

use crate::branches::{Outcome, run_op};
use crate::error::{AppError, AppResult};
use crate::repos::repo_root;
use crate::state::AppState;

/// What the Undo / Redo buttons would do now.
#[tauri::command]
#[specta::specta]
pub async fn undo_state(app: AppHandle, repo_id: i64) -> AppResult<UndoState> {
    let repo = repo_root(&app.state::<AppState>(), repo_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        match undo::state(&st.git, &repo) {
            Ok(state) => Ok(state),
            Err(OpError::Git(e)) => Err(e.into()),
            Err(e) => Err(AppError::Git(e.to_string())),
        }
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

/// Undoes the most recent operation (never discards uncommitted changes).
#[tauri::command]
#[specta::specta]
pub async fn undo_run(app: AppHandle, repo_id: i64) -> AppResult<Outcome<UndoOutcome>> {
    run_op(app, repo_id, |git, repo, _| undo::undo(git, repo)).await
}

/// Redoes the operation undone last, if the repository did not change since.
#[tauri::command]
#[specta::specta]
pub async fn redo_run(app: AppHandle, repo_id: i64) -> AppResult<Outcome<UndoOutcome>> {
    run_op(app, repo_id, |git, repo, _| undo::redo(git, repo)).await
}
