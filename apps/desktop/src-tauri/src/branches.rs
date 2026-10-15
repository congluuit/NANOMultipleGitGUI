//! Branch, merge, commit-menu and remote commands: create / checkout / rename / delete branches,
//! tags, cherry-pick, revert, reset, in-progress operations (abort / continue), merge, and
//! fetch / pull / push with throttled progress events and cancellation.
//!
//! Write commands return an [`Outcome`]: `done` with a value, `rejected` with a recognisable
//! [`FailureKind`] (the UI offers the matching follow-up: force delete, stash, pull…), or
//! `cancelled`. Unexpected git failures are still thrown as [`AppError`].

use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use nanomgg_git_engine::merge::{self, MergeMode, MergeOutcome};
use nanomgg_git_engine::ops::{
    self, FailureKind, OpError, OpOutcome, OperationState, RepoOperation, ResetMode,
};
use nanomgg_git_engine::progress::{CancelToken, Progress, ProgressPhase};
use nanomgg_git_engine::remote::{
    self, FetchOptions, FetchSummary, PullMode, PullOptions, PushOptions, PushSummary,
};
use nanomgg_git_engine::undo::{self, UndoKind};
use nanomgg_git_engine::{AccountContext, GitRunner, branch};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::accounts::{self, AccountOverride};
use crate::error::{AppError, AppResult};
use crate::repos::repo_root;
use crate::state::AppState;

/// Minimum time between two progress events of one transfer (phase changes and `done` lines
/// are always sent).
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// Result of a write command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum Outcome<T> {
    Done {
        value: T,
    },
    /// Refused for a recognisable reason; `message` is git's (redacted) explanation.
    Rejected {
        reason: FailureKind,
        message: String,
    },
    /// Stopped by `transfer_cancel`.
    Cancelled,
}

fn redact(message: &str) -> String {
    nanomgg_logging::redact(message).into_owned()
}

fn outcome<T>(result: Result<T, OpError>) -> AppResult<Outcome<T>> {
    match result {
        Ok(value) => Ok(Outcome::Done { value }),
        Err(OpError::Rejected { kind, message }) => Ok(Outcome::Rejected {
            reason: kind,
            message: redact(&message),
        }),
        Err(OpError::Cancelled) => Ok(Outcome::Cancelled),
        Err(OpError::Git(e)) => Err(e.into()),
    }
}

/// Runs a local write operation (checkout, merge, cherry-pick, revert, reset, tag…) on a
/// blocking thread, as the repo's default account (its identity is what merge / revert /
/// cherry-pick / annotated tags record). Unbound repos use git's own configuration.
pub(crate) async fn run_op<T: Send + 'static>(
    app: AppHandle,
    repo_id: i64,
    f: impl FnOnce(&GitRunner, &Path, &AccountContext) -> Result<T, OpError> + Send + 'static,
) -> AppResult<Outcome<T>> {
    let repo = repo_root(&app.state::<AppState>(), repo_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        let ctx = accounts::context_for(&st, repo_id, None)?;
        outcome(f(&st.git, &repo, &ctx))
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

/// Which account a transfer runs as.
#[derive(Debug, Clone)]
enum Who {
    /// The account resolved for this remote (`None` = repo default).
    Remote(Option<String>),
    /// `pull` without an explicit remote: the current branch's upstream remote.
    Upstream,
    /// One-shot "Push as… / Pull as… / Fetch as…" (`asAccount`).
    Account(AccountOverride),
}

impl Who {
    fn new(remote: Option<String>, as_account: Option<AccountOverride>) -> Self {
        match as_account {
            Some(a) => Self::Account(a),
            None => Self::Remote(remote),
        }
    }
}

/// `branch.<current>.remote`, the remote `git pull` without arguments talks to.
fn upstream_remote(git: &GitRunner, repo: &Path) -> Option<String> {
    let branch = ops::current_branch(git, repo)?;
    git.run_text(
        repo,
        &AccountContext::default(),
        ["config", "--get", &format!("branch.{branch}.remote")],
    )
    .ok()
    .filter(|r| !r.is_empty() && r != ".")
}

/// Resolves the context of a transfer right before running it.
fn transfer_context(
    st: &AppState,
    repo_id: i64,
    repo: &Path,
    who: &Who,
) -> AppResult<AccountContext> {
    match who {
        Who::Account(a) => accounts::context_with(st, repo_id, None, Some(a)),
        Who::Remote(remote) => accounts::context_for(st, repo_id, remote.as_deref()),
        Who::Upstream => {
            let remote = upstream_remote(&st.git, repo);
            accounts::context_for(st, repo_id, remote.as_deref())
        }
    }
}

/// `fetch` without a remote (`git fetch --all`). When every remote resolves to the same
/// account context, one `git fetch --all` runs with it; otherwise each remote is fetched in
/// turn with its own context (stopping at the first rejection or cancel) and the ref
/// updates are summed, so e.g. `origin` (work GitLab) and `upstream` (personal GitHub)
/// each authenticate as their own account.
pub(crate) fn fetch_all(
    st: &AppState,
    repo_id: i64,
    repo: &Path,
    opts: &FetchOptions,
    cancel: &CancelToken,
    sink: ProgressSink,
) -> AppResult<Result<FetchSummary, OpError>> {
    let remotes = accounts::repo_remotes(st, repo)?;
    let mut per_remote = Vec::with_capacity(remotes.len());
    for r in &remotes {
        per_remote.push((
            r.name.clone(),
            accounts::context_for(st, repo_id, Some(&r.name))?,
        ));
    }
    let shared = match per_remote.first() {
        None => Some(AccountContext::default()),
        Some((_, first)) => per_remote
            .iter()
            .all(|(_, c)| c == first)
            .then(|| first.clone()),
    };
    if let Some(ctx) = shared {
        return Ok(remote::fetch(&st.git, repo, &ctx, opts, cancel, sink));
    }
    let mut total = FetchSummary { updated_refs: 0 };
    for (name, ctx) in per_remote {
        let one = FetchOptions {
            remote: Some(name),
            ..opts.clone()
        };
        match remote::fetch(&st.git, repo, &ctx, &one, cancel, &mut *sink) {
            Ok(s) => total.updated_refs += s.updated_refs,
            Err(e) => return Ok(Err(e)),
        }
    }
    Ok(Ok(total))
}

// ---------------------------------------------------------------------------------------------
// Transfers (fetch / pull / push): progress events and cancellation.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum TransferKind {
    Fetch,
    Pull,
    Push,
}

/// Progress of a running fetch / pull / push, at most ~10 per second per operation.
#[derive(Debug, Clone, Serialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    pub repo_id: i64,
    /// Id chosen by the frontend when it started the command.
    pub op_id: String,
    pub kind: TransferKind,
    pub phase: ProgressPhase,
    /// Label printed by git, e.g. `Receiving objects`.
    pub label: String,
    /// Reported by the server (`remote: …`).
    pub remote: bool,
    pub percent: Option<u8>,
    pub current: Option<u64>,
    pub total: Option<u64>,
    /// e.g. `1.20 MiB`.
    pub bytes: Option<String>,
    /// e.g. `2.00 MiB/s`.
    pub throughput: Option<String>,
    pub done: bool,
}

/// Cancel tokens of running transfers, by frontend operation id.
static TRANSFERS: LazyLock<Mutex<HashMap<String, CancelToken>>> = LazyLock::new(Mutex::default);

fn transfers() -> MutexGuard<'static, HashMap<String, CancelToken>> {
    TRANSFERS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Keeps an operation's cancel token registered while it runs (also used by LFS transfers,
/// so `transfer_cancel` stops them too).
pub(crate) struct Registration(String);

impl Registration {
    pub(crate) fn new(op_id: &str) -> AppResult<(Self, CancelToken)> {
        if op_id.is_empty() || op_id.len() > 128 {
            return Err(AppError::InvalidInput("invalid operation id".into()));
        }
        let mut map = transfers();
        if map.contains_key(op_id) {
            return Err(AppError::InvalidInput(format!(
                "operation {op_id} is already running"
            )));
        }
        let token = CancelToken::new();
        map.insert(op_id.to_owned(), token.clone());
        Ok((Self(op_id.to_owned()), token))
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        transfers().remove(&self.0);
    }
}

/// Rate limiter for progress events.
#[derive(Debug, Default)]
struct Throttle {
    last: Option<(Instant, ProgressPhase, String)>,
}

impl Throttle {
    fn ready(&mut self, p: &Progress, now: Instant) -> bool {
        let send = match &self.last {
            None => true,
            Some((at, phase, label)) => {
                p.done
                    || *phase != p.phase
                    || *label != p.label
                    || now.duration_since(*at) >= PROGRESS_INTERVAL
            }
        };
        if send {
            self.last = Some((now, p.phase, p.label.clone()));
        }
        send
    }
}

pub(crate) type ProgressSink<'a> = &'a mut dyn FnMut(Progress);

/// Runs a transfer as `who` (context resolved on the blocking thread, right before git).
async fn run_transfer<T: Send + 'static>(
    app: AppHandle,
    repo_id: i64,
    op_id: String,
    kind: TransferKind,
    who: Who,
    f: impl FnOnce(&GitRunner, &Path, &AccountContext, &CancelToken, ProgressSink) -> Result<T, OpError>
    + Send
    + 'static,
) -> AppResult<Outcome<T>> {
    run_transfer_with(app, repo_id, op_id, kind, move |st, repo, cancel, sink| {
        let ctx = transfer_context(st, repo_id, repo, &who)?;
        Ok(f(&st.git, repo, &ctx, cancel, sink))
    })
    .await
}

async fn run_transfer_with<T: Send + 'static>(
    app: AppHandle,
    repo_id: i64,
    op_id: String,
    kind: TransferKind,
    f: impl FnOnce(&AppState, &Path, &CancelToken, ProgressSink) -> AppResult<Result<T, OpError>>
    + Send
    + 'static,
) -> AppResult<Outcome<T>> {
    let repo = repo_root(&app.state::<AppState>(), repo_id)?;
    // Registered before the thread starts, so a cancel sent right away is not lost.
    let (registration, token) = Registration::new(&op_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _registration = registration;
        let st = app.state::<AppState>();
        let started = Instant::now();
        let mut throttle = Throttle::default();
        let mut emit = |p: Progress| {
            if !throttle.ready(&p, Instant::now()) {
                return;
            }
            let event = TransferProgress {
                repo_id,
                op_id: op_id.clone(),
                kind,
                phase: p.phase,
                label: p.label,
                remote: p.remote,
                percent: p.percent,
                current: p.current,
                total: p.total,
                bytes: p.bytes,
                throughput: p.throughput,
                done: p.done,
            };
            if let Err(e) = event.emit(&app) {
                tracing::warn!(error = %e, "failed to emit transfer progress");
            }
        };
        let result = f(&st, &repo, &token, &mut emit)?;
        let status = match &result {
            Ok(_) => "done".to_owned(),
            Err(OpError::Rejected { kind, .. }) => format!("rejected: {kind:?}"),
            Err(OpError::Cancelled) => "cancelled".to_owned(),
            Err(OpError::Git(_)) => "failed".to_owned(),
        };
        tracing::info!(
            repo = repo_id,
            ?kind,
            status,
            ms = started.elapsed().as_millis() as u64,
            "transfer finished"
        );
        outcome(result)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

/// Asks a running fetch / pull / push to stop. Returns `false` if no such operation runs.
#[tauri::command]
#[specta::specta]
pub fn transfer_cancel(op_id: String) -> AppResult<bool> {
    Ok(match transfers().get(&op_id) {
        Some(token) => {
            token.cancel();
            true
        }
        None => false,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn remote_fetch(
    app: AppHandle,
    repo_id: i64,
    op_id: String,
    remote: Option<String>,
    prune: bool,
    tags: bool,
    as_account: Option<AccountOverride>,
) -> AppResult<Outcome<FetchSummary>> {
    let opts = FetchOptions {
        remote,
        prune,
        tags,
    };
    run_transfer_with(
        app,
        repo_id,
        op_id,
        TransferKind::Fetch,
        move |st, repo, cancel, sink| {
            if opts.remote.is_none() && as_account.is_none() {
                return fetch_all(st, repo_id, repo, &opts, cancel, sink);
            }
            let who = Who::new(opts.remote.clone(), as_account);
            let ctx = transfer_context(st, repo_id, repo, &who)?;
            Ok(remote::fetch(&st.git, repo, &ctx, &opts, cancel, sink))
        },
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn remote_pull(
    app: AppHandle,
    repo_id: i64,
    op_id: String,
    remote: Option<String>,
    branch: Option<String>,
    mode: PullMode,
    as_account: Option<AccountOverride>,
) -> AppResult<Outcome<MergeOutcome>> {
    let who = match (&remote, as_account) {
        (_, Some(a)) => Who::Account(a),
        (Some(r), None) => Who::Remote(Some(r.clone())),
        (None, None) => Who::Upstream,
    };
    let opts = PullOptions {
        remote,
        branch,
        mode,
    };
    run_transfer(
        app,
        repo_id,
        op_id,
        TransferKind::Pull,
        who,
        move |git, repo, ctx, cancel, sink| remote::pull(git, repo, ctx, &opts, cancel, sink),
    )
    .await
}

/// What to push: a local branch to a remote.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PushRequest {
    pub remote: String,
    pub branch: String,
    /// Name on the remote; `null` = same as `branch`.
    pub remote_branch: Option<String>,
    pub set_upstream: bool,
    pub force_with_lease: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn remote_push(
    app: AppHandle,
    repo_id: i64,
    op_id: String,
    request: PushRequest,
    as_account: Option<AccountOverride>,
) -> AppResult<Outcome<PushSummary>> {
    let who = Who::new(Some(request.remote.clone()), as_account);
    let opts = PushOptions {
        remote: request.remote,
        branch: request.branch,
        remote_branch: request.remote_branch,
        set_upstream: request.set_upstream,
        force_with_lease: request.force_with_lease,
    };
    run_transfer(
        app,
        repo_id,
        op_id,
        TransferKind::Push,
        who,
        move |git, repo, ctx, cancel, sink| remote::push(git, repo, ctx, &opts, cancel, sink),
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn remote_push_tag(
    app: AppHandle,
    repo_id: i64,
    op_id: String,
    remote: String,
    tag: String,
    as_account: Option<AccountOverride>,
) -> AppResult<Outcome<PushSummary>> {
    run_transfer(
        app,
        repo_id,
        op_id,
        TransferKind::Push,
        Who::new(Some(remote.clone()), as_account),
        move |git, repo, ctx, cancel, sink| {
            remote::push_tag(git, repo, ctx, &remote, &tag, cancel, sink)
        },
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn remote_delete_branch(
    app: AppHandle,
    repo_id: i64,
    op_id: String,
    remote: String,
    branch: String,
) -> AppResult<Outcome<PushSummary>> {
    run_transfer(
        app,
        repo_id,
        op_id,
        TransferKind::Push,
        Who::Remote(Some(remote.clone())),
        move |git, repo, ctx, cancel, sink| {
            remote::delete_remote_branch(git, repo, ctx, &remote, &branch, cancel, sink)
        },
    )
    .await
}

// ---------------------------------------------------------------------------------------------
// Branches.

/// Creates `name` at `start` (commit id or ref name), optionally switching to it.
#[tauri::command]
#[specta::specta]
pub async fn branch_create(
    app: AppHandle,
    repo_id: i64,
    name: String,
    start: String,
    checkout: bool,
) -> AppResult<Outcome<()>> {
    run_op(app, repo_id, move |git, repo, ctx| {
        // Recorded for Undo (step 3.1).
        let kind = UndoKind::CreateBranch { name: name.clone() };
        undo::record(git, repo, kind, &[&name], || {
            branch::create_branch(git, repo, ctx, &name, &start, checkout)
        })
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn branch_checkout(app: AppHandle, repo_id: i64, name: String) -> AppResult<Outcome<()>> {
    run_op(app, repo_id, move |git, repo, ctx| {
        branch::checkout_branch(git, repo, ctx, &name)
    })
    .await
}

/// Checks out `remote/branch` as a new tracking branch; returns the local branch name.
#[tauri::command]
#[specta::specta]
pub async fn branch_checkout_remote(
    app: AppHandle,
    repo_id: i64,
    remote: String,
    branch: String,
    local_name: Option<String>,
) -> AppResult<Outcome<String>> {
    run_op(app, repo_id, move |git, repo, ctx| {
        let name = local_name.clone().unwrap_or_else(|| branch.clone());
        let kind = UndoKind::CreateBranch { name: name.clone() };
        undo::record(git, repo, kind, &[&name], || {
            branch::checkout_remote_branch(git, repo, ctx, &remote, &branch, local_name.as_deref())
        })
    })
    .await
}

/// Checks out a commit with a detached HEAD.
#[tauri::command]
#[specta::specta]
pub async fn commit_checkout(app: AppHandle, repo_id: i64, oid: String) -> AppResult<Outcome<()>> {
    run_op(app, repo_id, move |git, repo, ctx| {
        branch::checkout_detached(git, repo, ctx, &oid)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn branch_rename(
    app: AppHandle,
    repo_id: i64,
    old_name: String,
    new_name: String,
) -> AppResult<Outcome<()>> {
    run_op(app, repo_id, move |git, repo, ctx| {
        let kind = UndoKind::RenameBranch {
            from: old_name.clone(),
            to: new_name.clone(),
        };
        undo::record(git, repo, kind, &[], || {
            branch::rename_branch(git, repo, ctx, &old_name, &new_name)
        })
    })
    .await
}

/// Deletes a local branch; `notFullyMerged` / `currentBranch` come back as rejections.
#[tauri::command]
#[specta::specta]
pub async fn branch_delete(
    app: AppHandle,
    repo_id: i64,
    name: String,
    force: bool,
) -> AppResult<Outcome<()>> {
    run_op(app, repo_id, move |git, repo, ctx| {
        // The SHA (and upstream) is kept in the undo journal so the branch can come back.
        let kind = UndoKind::DeleteBranch { name: name.clone() };
        undo::record(git, repo, kind, &[&name], || {
            branch::delete_branch(git, repo, ctx, &name, force)
        })
    })
    .await
}

/// Sets (`origin/main`, or a local branch) or removes (`null`) the upstream of `name`.
#[tauri::command]
#[specta::specta]
pub async fn branch_set_upstream(
    app: AppHandle,
    repo_id: i64,
    name: String,
    upstream: Option<String>,
) -> AppResult<Outcome<()>> {
    run_op(app, repo_id, move |git, repo, ctx| {
        branch::set_upstream(git, repo, ctx, &name, upstream.as_deref())
    })
    .await
}

// ---------------------------------------------------------------------------------------------
// Commit menu.

/// Creates a tag at `oid`: annotated when `message` is set, lightweight otherwise.
#[tauri::command]
#[specta::specta]
pub async fn tag_create(
    app: AppHandle,
    repo_id: i64,
    name: String,
    oid: String,
    message: Option<String>,
) -> AppResult<Outcome<()>> {
    run_op(app, repo_id, move |git, repo, ctx| {
        ops::create_tag(git, repo, ctx, &name, &oid, message.as_deref())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn tag_delete(app: AppHandle, repo_id: i64, name: String) -> AppResult<Outcome<()>> {
    run_op(app, repo_id, move |git, repo, ctx| {
        ops::delete_tag(git, repo, ctx, &name)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn commit_cherry_pick(
    app: AppHandle,
    repo_id: i64,
    oid: String,
) -> AppResult<Outcome<OpOutcome>> {
    run_op(app, repo_id, move |git, repo, ctx| {
        ops::cherry_pick(git, repo, ctx, &oid)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn commit_revert(
    app: AppHandle,
    repo_id: i64,
    oid: String,
) -> AppResult<Outcome<OpOutcome>> {
    run_op(app, repo_id, move |git, repo, ctx| {
        ops::revert(git, repo, ctx, &oid)
    })
    .await
}

/// Moves the current branch to `oid` (`hard` discards local changes).
#[tauri::command]
#[specta::specta]
pub async fn reset_to(
    app: AppHandle,
    repo_id: i64,
    oid: String,
    mode: ResetMode,
) -> AppResult<Outcome<()>> {
    run_op(app, repo_id, move |git, repo, ctx| {
        ops::reset(git, repo, ctx, &oid, mode)
    })
    .await
}

/// In-progress merge / rebase / cherry-pick / revert / bisect and the conflicted paths.
#[tauri::command]
#[specta::specta]
pub async fn repo_operation_state(app: AppHandle, repo_id: i64) -> AppResult<OperationState> {
    let repo = repo_root(&app.state::<AppState>(), repo_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        match ops::operation_state(&st.git, &repo) {
            Ok(state) => Ok(state),
            Err(OpError::Git(e)) => Err(e.into()),
            Err(e) => Err(AppError::Git(redact(&e.to_string()))),
        }
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

/// Aborts the in-progress operation; returns which one it was.
#[tauri::command]
#[specta::specta]
pub async fn operation_abort(app: AppHandle, repo_id: i64) -> AppResult<Outcome<RepoOperation>> {
    run_op(app, repo_id, ops::abort_operation).await
}

/// Continues the in-progress operation after conflicts were resolved and staged.
#[tauri::command]
#[specta::specta]
pub async fn operation_continue(app: AppHandle, repo_id: i64) -> AppResult<Outcome<OpOutcome>> {
    run_op(app, repo_id, ops::continue_operation).await
}

/// Merges `rev` (branch / remote branch / tag / commit id) into the current branch.
#[tauri::command]
#[specta::specta]
pub async fn merge_into_current(
    app: AppHandle,
    repo_id: i64,
    rev: String,
    mode: MergeMode,
    message: Option<String>,
) -> AppResult<Outcome<MergeOutcome>> {
    run_op(app, repo_id, move |git, repo, ctx| {
        merge::merge(git, repo, ctx, &rev, mode, message.as_deref())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_serializes_with_status_tag() {
        let done: Outcome<()> = outcome(Ok(())).unwrap();
        assert_eq!(
            serde_json::to_value(&done).unwrap(),
            serde_json::json!({ "status": "done", "value": null })
        );
        let rejected: Outcome<()> = outcome(Err(OpError::rejected(
            FailureKind::NotFullyMerged,
            "error: the branch 'x' is not fully merged",
        )))
        .unwrap();
        assert_eq!(
            serde_json::to_value(&rejected).unwrap(),
            serde_json::json!({
                "status": "rejected",
                "reason": "notFullyMerged",
                "message": "error: the branch 'x' is not fully merged"
            })
        );
        let cancelled: Outcome<u32> = outcome(Err(OpError::Cancelled)).unwrap();
        assert_eq!(
            serde_json::to_value(&cancelled).unwrap(),
            serde_json::json!({ "status": "cancelled" })
        );
        let failed = outcome::<()>(Err(OpError::Git(nanomgg_git_engine::GitError::Failed {
            args: "push".into(),
            status: "1".into(),
            stderr: "boom".into(),
        })));
        assert!(matches!(failed, Err(AppError::Git(_))));
    }

    #[test]
    fn rejection_messages_are_redacted() {
        let r: Outcome<()> = outcome(Err(OpError::rejected(
            FailureKind::AuthFailed,
            "fatal: Authentication failed for 'https://user:ghp_abcdefghijklmnopqrstuvwxyz0123456789@github.com/a/b.git/'",
        )))
        .unwrap();
        let Outcome::Rejected { message, .. } = r else {
            panic!("expected rejection");
        };
        assert!(
            !message.contains("ghp_abcdefghijklmnopqrstuvwxyz0123456789"),
            "{message}"
        );
    }

    #[test]
    fn registry_tracks_and_cancels_operations() {
        let (reg, token) = Registration::new("op-test-1").unwrap();
        assert!(Registration::new("op-test-1").is_err(), "duplicate id");
        assert!(Registration::new("").is_err());
        assert!(transfer_cancel("op-test-1".into()).unwrap());
        assert!(token.is_cancelled());
        drop(reg);
        assert!(!transfer_cancel("op-test-1".into()).unwrap());
    }

    #[test]
    fn throttle_limits_rate_but_keeps_phase_changes_and_done() {
        let p = |phase, done| Progress {
            phase,
            label: format!("{phase:?}"),
            remote: false,
            percent: Some(1),
            current: None,
            total: None,
            bytes: None,
            throughput: None,
            done,
        };
        let mut t = Throttle::default();
        let t0 = Instant::now();
        assert!(t.ready(&p(ProgressPhase::Counting, false), t0));
        assert!(!t.ready(
            &p(ProgressPhase::Counting, false),
            t0 + Duration::from_millis(30)
        ));
        assert!(t.ready(
            &p(ProgressPhase::Counting, true),
            t0 + Duration::from_millis(40)
        ));
        assert!(t.ready(
            &p(ProgressPhase::Receiving, false),
            t0 + Duration::from_millis(50)
        ));
        assert!(!t.ready(
            &p(ProgressPhase::Receiving, false),
            t0 + Duration::from_millis(120)
        ));
        assert!(t.ready(
            &p(ProgressPhase::Receiving, false),
            t0 + Duration::from_millis(160)
        ));
    }
}
