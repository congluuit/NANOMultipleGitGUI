//! Phase 6.1: "safe branch switch" — double-click a branch in the sidebar (or Checkout in its
//! menu) to switch to it without ever losing uncommitted work.
//!
//! - [`branch_switch_check`] is read-only: what the switch would run into (operation in
//!   progress, uncommitted changes, branch checked out in another worktree, detached HEAD whose
//!   commits no branch keeps). The UI explains it and asks when needed.
//! - [`branch_switch_run`] checks the blocking conditions again (the repository may have
//!   changed while a dialog was open), stashes the changes when asked
//!   (`git stash push --include-untracked`, message [`AUTOSTASH_PREFIX`]`<old branch>`), checks
//!   out the branch (creating the tracking branch for a remote one) and pops the stash back
//!   when the checkout fails. After a successful switch to `B`, the newest autostash made when
//!   leaving `B` is popped back (or only reported, see [`RestoreStatus::Pending`]).
//! - [`branch_switch_restore`] pops that autostash on request ("Restore now").
//!
//! Preferences (`branchSwitch.dirtyAction`, `branchSwitch.autoRestore`) live in the settings
//! table.

use std::collections::BTreeSet;
use std::path::Path;

use nanomgg_git_engine::ops::{self, FailureKind, OpError, RepoOperation};
use nanomgg_git_engine::stash::{self, StashOptions};
use nanomgg_git_engine::undo::{self, UndoKind};
use nanomgg_git_engine::{
    AccountContext, GitError, GitRunner, branch, status, submodule, worktrees,
};
use nanomgg_storage::Storage;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};

use crate::accounts::context_for;
use crate::error::{AppError, AppResult};
use crate::repos::repo_root;
use crate::state::AppState;

/// Message of the stash made when leaving a branch, followed by the branch name. Fixed so the
/// stash can be recognised later (also by users reading the stash list).
pub const AUTOSTASH_PREFIX: &str = "nanomgg-autostash: ";

const KEY_DIRTY_ACTION: &str = "branchSwitch.dirtyAction";
const KEY_AUTO_RESTORE: &str = "branchSwitch.autoRestore";

/// Branch to switch to.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SwitchTarget {
    /// A local branch (`refs/heads/<name>`).
    Local { name: String },
    /// A remote-tracking branch `<remote>/<branch>`: switches to the local branch tracking it,
    /// creating it when there is none.
    Remote { remote: String, branch: String },
}

/// What to do with uncommitted changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SwitchDirtyMode {
    /// Stash them (autostash) before switching.
    Stash,
    /// Plain checkout: git keeps them when they do not clash with the other branch.
    Carry,
}

/// Saved answer to "you have uncommitted changes".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum DirtyAction {
    #[default]
    Ask,
    Stash,
    Carry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BranchSwitchPrefs {
    pub dirty_action: DirtyAction,
    /// Pop the autostash of a branch when coming back to it.
    pub auto_restore: bool,
}

impl Default for BranchSwitchPrefs {
    fn default() -> Self {
        Self {
            dirty_action: DirtyAction::Ask,
            auto_restore: true,
        }
    }
}

/// Partial update of [`BranchSwitchPrefs`].
#[derive(Debug, Clone, Default, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BranchSwitchPrefsPatch {
    #[serde(default)]
    #[specta(optional)]
    pub dirty_action: Option<DirtyAction>,
    #[serde(default)]
    #[specta(optional)]
    pub auto_restore: Option<bool>,
}

/// Read-only pre-flight of a switch.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BranchSwitchCheck {
    /// Local branch that will be checked out.
    pub branch: String,
    /// It does not exist yet: it will be created from the remote branch, tracking it.
    pub create: bool,
    /// Checked-out branch; `None` when HEAD is detached.
    pub current: Option<String>,
    /// Already on `branch`: nothing to do.
    pub already_current: bool,
    /// Merge / rebase / cherry-pick / revert… stopped half-way.
    pub operation: Option<RepoOperation>,
    /// Files still in conflict.
    pub conflicts: u32,
    /// Files with uncommitted changes (staged, unstaged and untracked; changes inside
    /// submodules are not counted: a stash cannot hold them and checkout leaves them alone).
    pub changes: u32,
    /// Folder of another worktree where `branch` is checked out.
    pub checked_out_at: Option<String>,
    /// HEAD is detached on a commit that no branch contains: leaving it makes those commits
    /// hard to find.
    pub detached_orphan: bool,
    /// Short id of the detached commit.
    pub head_short: Option<String>,
    /// No commit yet (stashing is impossible).
    pub unborn: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SwitchStatus {
    /// Already on that branch; nothing was done.
    Unchanged,
    Switched,
    /// Refused (`reason`, `message`); the repository is as before, except when `stashKept`.
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum RestoreStatus {
    /// Popped cleanly.
    Restored,
    /// Popped with conflicts: the files hold conflict markers, the stash entry is kept.
    Conflicts,
    /// Not popped: automatic restore is off, or the working tree had changes (`dirty`).
    Pending,
    /// git refused (e.g. it would overwrite local changes); the stash entry is kept.
    Failed,
}

/// The autostash of the branch that was just checked out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AutostashRestore {
    pub status: RestoreStatus,
    /// Files in the stash entry.
    pub files: u32,
    /// Conflicted paths (`Conflicts`).
    pub conflicts: Vec<String>,
    /// The working tree was not clean (`Pending`).
    pub dirty: bool,
    /// git's explanation (`Failed`).
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BranchSwitchResult {
    pub status: SwitchStatus,
    /// Local branch switched to (or the one that was asked for).
    pub branch: String,
    /// Branch (or `None` = detached HEAD) we came from.
    pub from: Option<String>,
    /// The local branch was created from the remote branch.
    pub created: bool,
    /// The uncommitted changes were stashed (autostash of `from`).
    pub stashed: bool,
    /// Files with uncommitted changes before the switch (see [`BranchSwitchCheck::changes`]).
    pub changes: u32,
    pub restore: Option<AutostashRestore>,
    /// Submodules now checked out at another commit than the one the branch records.
    pub submodules: Vec<String>,
    pub reason: Option<FailureKind>,
    pub message: Option<String>,
    /// `checkedOutElsewhere`: folder of that worktree.
    pub worktree: Option<String>,
    /// Blocked after stashing and the stash could not be popped back: the changes are in the
    /// newest stash entry.
    pub stash_kept: bool,
}

impl BranchSwitchResult {
    fn new(status: SwitchStatus, branch: &str, from: Option<String>) -> Self {
        Self {
            status,
            branch: branch.to_owned(),
            from,
            created: false,
            stashed: false,
            changes: 0,
            restore: None,
            submodules: Vec::new(),
            reason: None,
            message: None,
            worktree: None,
            stash_kept: false,
        }
    }

    fn blocked(branch: &str, from: Option<String>, reason: FailureKind, message: String) -> Self {
        Self {
            reason: Some(reason),
            message: Some(nanomgg_logging::redact(&message).into_owned()),
            ..Self::new(SwitchStatus::Blocked, branch, from)
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Preferences.

pub fn load_prefs(db: &Storage) -> AppResult<BranchSwitchPrefs> {
    let d = BranchSwitchPrefs::default();
    Ok(BranchSwitchPrefs {
        dirty_action: db
            .get_setting(KEY_DIRTY_ACTION)
            .ok()
            .flatten()
            .unwrap_or(d.dirty_action),
        auto_restore: db
            .get_setting(KEY_AUTO_RESTORE)
            .ok()
            .flatten()
            .unwrap_or(d.auto_restore),
    })
}

pub fn save_prefs(db: &Storage, patch: BranchSwitchPrefsPatch) -> AppResult<BranchSwitchPrefs> {
    if let Some(a) = patch.dirty_action {
        db.set_setting(KEY_DIRTY_ACTION, &a)?;
    }
    if let Some(on) = patch.auto_restore {
        db.set_setting(KEY_AUTO_RESTORE, &on)?;
    }
    load_prefs(db)
}

// ---------------------------------------------------------------------------------------------
// Git helpers.

fn none() -> AccountContext {
    AccountContext::default()
}

fn op_error(e: OpError) -> AppError {
    match e {
        OpError::Git(e) => e.into(),
        other => AppError::Git(nanomgg_logging::redact(&other.to_string()).into_owned()),
    }
}

/// Branch a stash entry subject belongs to: `On main: nanomgg-autostash: main` → `main`.
/// Branch names cannot contain `:` nor spaces, so the prefix git adds (`On <branch>: ` or
/// `WIP on …`) is cut at the first `": "` and the rest must be exactly our message.
pub fn autostash_branch(subject: &str) -> Option<&str> {
    let (_, msg) = subject.split_once(": ")?;
    let name = msg.strip_prefix(AUTOSTASH_PREFIX)?;
    (!name.is_empty() && !name.contains(char::is_whitespace)).then_some(name)
}

/// Index of the newest autostash made when leaving `branch`.
pub fn find_autostash(git: &GitRunner, repo: &Path, branch: &str) -> Result<Option<u32>, GitError> {
    let list = git.run_text(repo, &none(), ["stash", "list", "--format=%gd%x1f%s"])?;
    Ok(list.lines().find_map(|line| {
        let (selector, subject) = line.split_once('\u{1f}')?;
        let index = stash::parse_stash_index(selector)?;
        (autostash_branch(subject) == Some(branch)).then_some(index)
    }))
}

/// Files with uncommitted changes, counted once each (see [`BranchSwitchCheck::changes`]).
fn change_count(st: &status::WorkingTreeStatus) -> u32 {
    let paths: BTreeSet<&str> = st
        .staged
        .iter()
        .chain(st.conflicted.iter())
        .chain(st.unstaged.iter().filter(|e| !e.submodule))
        .map(|e| e.path.as_str())
        .collect();
    paths.len() as u32
}

/// The local branch tracking `remote/branch`; else a local branch with the same name (switching
/// to it rather than failing on "already exists"); `None` = it has to be created.
fn tracking_branch(
    git: &GitRunner,
    repo: &Path,
    remote: &str,
    branch: &str,
) -> Result<Option<String>, GitError> {
    let wanted = format!("refs/remotes/{remote}/{branch}");
    let out = git.run_text(
        repo,
        &none(),
        [
            "for-each-ref",
            "--format=%(refname:short)%1f%(upstream)",
            "refs/heads/",
        ],
    )?;
    let mut tracking = Vec::new();
    let mut same_name = None;
    for line in out.lines() {
        let Some((name, upstream)) = line.split_once('\u{1f}') else {
            continue;
        };
        if upstream == wanted {
            tracking.push(name.to_owned());
        }
        if name == branch {
            same_name = Some(name.to_owned());
        }
    }
    // Several branches may track it: prefer the one with the same name.
    if let Some(n) = tracking.iter().find(|n| *n == branch) {
        return Ok(Some(n.clone()));
    }
    Ok(tracking.into_iter().next().or(same_name))
}

/// `true` when the detached HEAD commit is on no local or remote branch.
fn head_is_orphan(git: &GitRunner, repo: &Path) -> bool {
    git.run_text(
        repo,
        &none(),
        [
            "for-each-ref",
            "--count=1",
            "--format=%(refname)",
            "--contains",
            "HEAD",
            "refs/heads/",
            "refs/remotes/",
        ],
    )
    .map(|s| s.is_empty())
    .unwrap_or(false)
}

/// Resolves the target: (local branch, must be created).
fn resolve_target(
    git: &GitRunner,
    repo: &Path,
    target: &SwitchTarget,
) -> Result<(String, bool), AppError> {
    match target {
        SwitchTarget::Local { name } => Ok((name.clone(), false)),
        SwitchTarget::Remote { remote, branch } => {
            if remote.is_empty() || remote.starts_with('-') || branch.is_empty() {
                return Err(AppError::InvalidInput("invalid remote branch".into()));
            }
            Ok(match tracking_branch(git, repo, remote, branch)? {
                Some(local) => (local, false),
                None => (branch.clone(), true),
            })
        }
    }
}

/// Pre-flight; reads only.
pub fn check(git: &GitRunner, repo: &Path, target: &SwitchTarget) -> AppResult<BranchSwitchCheck> {
    let (branch, create) = resolve_target(git, repo, target)?;
    let st = status::read_status(git, repo)?;
    let state = ops::operation_state(git, repo).map_err(op_error)?;
    let detached = st.branch.is_none() && !st.unborn;
    let checked_out_at = if create {
        None
    } else {
        worktrees::list(git, repo)
            .unwrap_or_default()
            .into_iter()
            .find(|w| !w.is_current && !w.prunable && w.branch.as_deref() == Some(branch.as_str()))
            .map(|w| w.path)
    };
    Ok(BranchSwitchCheck {
        already_current: st.branch.as_deref() == Some(branch.as_str()),
        current: st.branch.clone(),
        operation: state.operation.filter(|op| *op != RepoOperation::Bisect),
        conflicts: state.conflicts.len().max(st.conflicted.len()) as u32,
        changes: change_count(&st),
        checked_out_at,
        detached_orphan: detached && head_is_orphan(git, repo),
        head_short: detached
            .then(|| st.head_oid.as_deref().map(|o| o.chars().take(7).collect()))
            .flatten(),
        unborn: st.unborn,
        branch,
        create,
    })
}

/// Pops `stash@{index}` restoring the index too; falls back to a plain pop when git cannot
/// restore the index (`Conflicts in index. Try without --index.`), which applies nothing.
fn pop_back(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    index: u32,
) -> Result<stash::StashApplyOutcome, GitError> {
    match stash::stash_pop(git, repo, ctx, index, true) {
        Ok(o) => Ok(o),
        Err(GitError::Failed { .. }) => stash::stash_pop(git, repo, ctx, index, false),
        Err(e) => Err(e),
    }
}

fn stash_file_count(git: &GitRunner, repo: &Path, index: u32) -> u32 {
    stash::stash_show(git, repo, index)
        .map(|d| (d.files.len() + d.untracked.len()) as u32)
        .unwrap_or(0)
}

/// Pops the newest autostash of `branch` (the caller checked it is the current branch).
fn restore_impl(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    index: u32,
) -> AutostashRestore {
    let files = stash_file_count(git, repo, index);
    let mut r = AutostashRestore {
        status: RestoreStatus::Restored,
        files,
        conflicts: Vec::new(),
        dirty: false,
        message: None,
    };
    match pop_back(git, repo, ctx, index) {
        Ok(o) if o.conflicts.is_empty() => {}
        Ok(o) => {
            r.status = RestoreStatus::Conflicts;
            r.conflicts = o.conflicts;
        }
        Err(e) => {
            r.status = RestoreStatus::Failed;
            r.message = Some(nanomgg_logging::redact(&e.to_string()).into_owned());
        }
    }
    r
}

/// Submodules checked out at another commit than the one recorded by the current branch.
fn stale_submodules(git: &GitRunner, repo: &Path) -> Vec<String> {
    if !repo.join(".gitmodules").is_file() {
        return Vec::new();
    }
    submodule::list(git, repo)
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s.depth == 0 && s.state == submodule::SubmoduleState::OutOfDate)
        .map(|s| s.path)
        .collect()
}

/// The whole switch: re-check → (stash) → checkout → (roll back) → restore autostash.
pub fn run(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    target: &SwitchTarget,
    mode: SwitchDirtyMode,
    auto_restore: bool,
) -> AppResult<BranchSwitchResult> {
    let c = check(git, repo, target)?;
    let from = c.current.clone();
    let branch = c.branch.clone();
    if c.already_current {
        return Ok(BranchSwitchResult::new(
            SwitchStatus::Unchanged,
            &branch,
            from,
        ));
    }
    if let Some(op) = c.operation {
        return Ok(BranchSwitchResult::blocked(
            &branch,
            from,
            FailureKind::OperationInProgress,
            format!("a {op:?} is in progress; continue or abort it first"),
        ));
    }
    if c.conflicts > 0 {
        return Ok(BranchSwitchResult::blocked(
            &branch,
            from,
            FailureKind::UnmergedFiles,
            format!("{} file(s) are still in conflict", c.conflicts),
        ));
    }
    if let Some(path) = c.checked_out_at {
        let mut r = BranchSwitchResult::blocked(
            &branch,
            from,
            FailureKind::CheckedOutElsewhere,
            format!("'{branch}' is already checked out at '{path}'"),
        );
        r.worktree = Some(path);
        return Ok(r);
    }

    // Stash first (never possible before the first commit: such changes are carried).
    let mut stash_oid = None;
    if c.changes > 0 && mode == SwitchDirtyMode::Stash && !c.unborn {
        let label = match (&from, &c.head_short) {
            (Some(b), _) => b.clone(),
            // Never matches a branch name (spaces are not allowed in branch names).
            (None, short) => format!("(detached {})", short.as_deref().unwrap_or("HEAD")),
        };
        let opts = StashOptions {
            message: Some(format!("{AUTOSTASH_PREFIX}{label}")),
            include_untracked: true,
            ..StashOptions::default()
        };
        let created = stash::stash_create(git, repo, ctx, &opts)?;
        stash_oid = created.oid.filter(|_| created.created);
    }

    let switched = match target {
        SwitchTarget::Remote { remote, branch: rb } if c.create => {
            // Recorded for Undo like `branch_checkout_remote`.
            let kind = UndoKind::CreateBranch {
                name: branch.clone(),
            };
            undo::record(git, repo, kind, &[&branch], || {
                branch::checkout_remote_branch(git, repo, ctx, remote, rb, None).map(|_| ())
            })
        }
        _ => branch::checkout_branch(git, repo, ctx, &branch),
    };

    if let Err(e) = switched {
        let (reason, message) = match e {
            OpError::Rejected { kind, message } => (kind, message),
            OpError::Cancelled => (FailureKind::InvalidInput, "cancelled".to_owned()),
            OpError::Git(GitError::Failed { stderr, .. }) => (
                ops::failure_kind(&stderr).unwrap_or(FailureKind::InvalidInput),
                stderr,
            ),
            OpError::Git(e) => return Err(e.into()),
        };
        let mut r = BranchSwitchResult::blocked(&branch, from, reason, message);
        if let Some(oid) = stash_oid {
            // Put the changes back exactly where they were; only our own entry is popped.
            let top = stash::resolve_stash(git, repo, 0).ok();
            let popped = top.as_deref() == Some(oid.as_str())
                && pop_back(git, repo, ctx, 0).is_ok_and(|o| o.conflicts.is_empty());
            r.stash_kept = !popped;
        }
        return Ok(r);
    }

    let mut r = BranchSwitchResult::new(SwitchStatus::Switched, &branch, from);
    r.created = c.create;
    r.stashed = stash_oid.is_some();
    r.changes = c.changes;
    if let Some(index) = find_autostash(git, repo, &branch).unwrap_or(None) {
        let dirty = status::read_status(git, repo)
            .map(|s| change_count(&s) > 0)
            .unwrap_or(true);
        r.restore = Some(if auto_restore && !dirty {
            restore_impl(git, repo, ctx, index)
        } else {
            AutostashRestore {
                status: RestoreStatus::Pending,
                files: stash_file_count(git, repo, index),
                conflicts: Vec::new(),
                dirty,
                message: None,
            }
        });
    }
    r.submodules = stale_submodules(git, repo);
    Ok(r)
}

/// "Restore now": pops the newest autostash of `branch`, which must be checked out.
pub fn restore(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    branch: &str,
) -> AppResult<Option<AutostashRestore>> {
    if ops::current_branch(git, repo).as_deref() != Some(branch) {
        return Err(AppError::InvalidInput(format!(
            "'{branch}' is not the checked-out branch"
        )));
    }
    Ok(find_autostash(git, repo, branch)?.map(|index| restore_impl(git, repo, ctx, index)))
}

// ---------------------------------------------------------------------------------------------
// Commands.

async fn blocking<T: Send + 'static>(
    app: AppHandle,
    repo_id: i64,
    f: impl FnOnce(&AppState, &Path) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    let repo = repo_root(&app.state::<AppState>(), repo_id)?;
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppState>(), &repo))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

/// What switching to `target` would run into (reads only).
#[tauri::command]
#[specta::specta]
pub async fn branch_switch_check(
    app: AppHandle,
    repo_id: i64,
    target: SwitchTarget,
) -> AppResult<BranchSwitchCheck> {
    blocking(app, repo_id, move |st, repo| check(&st.git, repo, &target)).await
}

/// Switches to `target` (see the module docs). `autoRestore` comes from the preferences.
#[tauri::command]
#[specta::specta]
pub async fn branch_switch_run(
    app: AppHandle,
    repo_id: i64,
    target: SwitchTarget,
    mode: SwitchDirtyMode,
) -> AppResult<BranchSwitchResult> {
    blocking(app, repo_id, move |st, repo| {
        let auto_restore = load_prefs(&st.storage)?.auto_restore;
        // The stash and merge commits record the repo account's identity.
        let ctx = context_for(st, repo_id, None)?;
        let r = run(&st.git, repo, &ctx, &target, mode, auto_restore)?;
        tracing::info!(
            repo = repo_id,
            status = ?r.status,
            stashed = r.stashed,
            restore = ?r.restore.as_ref().map(|x| x.status),
            reason = ?r.reason,
            "branch switch"
        );
        Ok(r)
    })
    .await
}

/// Pops the newest autostash of the checked-out `branch`; `None` when there is none.
#[tauri::command]
#[specta::specta]
pub async fn branch_switch_restore(
    app: AppHandle,
    repo_id: i64,
    branch: String,
) -> AppResult<Option<AutostashRestore>> {
    blocking(app, repo_id, move |st, repo| {
        let ctx = context_for(st, repo_id, None)?;
        restore(&st.git, repo, &ctx, &branch)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub fn branch_switch_prefs_get(app: AppHandle) -> AppResult<BranchSwitchPrefs> {
    load_prefs(&app.state::<AppState>().storage)
}

#[tauri::command]
#[specta::specta]
pub fn branch_switch_prefs_set(
    app: AppHandle,
    patch: BranchSwitchPrefsPatch,
) -> AppResult<BranchSwitchPrefs> {
    save_prefs(&app.state::<AppState>().storage, patch)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g() -> GitRunner {
        GitRunner::default()
    }

    fn git(root: &Path, args: &[&str]) -> String {
        g().run_text(root, &none(), args).unwrap()
    }

    fn write(root: &Path, file: &str, text: &str) {
        std::fs::write(root.join(file), text).unwrap();
    }

    fn read(root: &Path, file: &str) -> String {
        std::fs::read_to_string(root.join(file)).unwrap()
    }

    /// Repo with `main` (a.txt = "main") and `feature` (a.txt = "feature"), on `main`.
    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        git(r, &["init", "-q", "-b", "main"]);
        for (k, v) in [
            ("user.name", "T"),
            ("user.email", "t@example.com"),
            ("core.autocrlf", "false"),
            ("commit.gpgsign", "false"),
        ] {
            git(r, &["config", k, v]);
        }
        write(r, "a.txt", "main\n");
        write(r, "b.txt", "same\n");
        git(r, &["add", "."]);
        git(r, &["commit", "-q", "-m", "first"]);
        git(r, &["switch", "-q", "-c", "feature"]);
        write(r, "a.txt", "feature\n");
        git(r, &["commit", "-q", "-am", "feature"]);
        git(r, &["switch", "-q", "main"]);
        dir
    }

    fn local(name: &str) -> SwitchTarget {
        SwitchTarget::Local { name: name.into() }
    }

    fn current(r: &Path) -> String {
        git(r, &["branch", "--show-current"])
    }

    fn stash_count(r: &Path) -> usize {
        git(r, &["stash", "list"]).lines().count()
    }

    #[test]
    fn recognises_autostash_subjects() {
        assert_eq!(
            autostash_branch("On main: nanomgg-autostash: main"),
            Some("main")
        );
        assert_eq!(
            autostash_branch("On feature/x: nanomgg-autostash: feature/x"),
            Some("feature/x")
        );
        assert_eq!(
            autostash_branch("On (no branch): nanomgg-autostash: (detached abc1234)"),
            None
        );
        assert_eq!(autostash_branch("WIP on main: abc first"), None);
        assert_eq!(autostash_branch("On main: my own stash"), None);
        assert_eq!(autostash_branch("On main: nanomgg-autostash: "), None);
    }

    #[test]
    fn prefs_default_to_ask_and_auto_restore() {
        let db = Storage::open_in_memory().unwrap();
        assert_eq!(load_prefs(&db).unwrap(), BranchSwitchPrefs::default());
        let p = save_prefs(
            &db,
            BranchSwitchPrefsPatch {
                dirty_action: Some(DirtyAction::Stash),
                auto_restore: None,
            },
        )
        .unwrap();
        assert_eq!(p.dirty_action, DirtyAction::Stash);
        assert!(p.auto_restore);
        let p = save_prefs(
            &db,
            BranchSwitchPrefsPatch {
                auto_restore: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(p.dirty_action, DirtyAction::Stash);
        assert!(!p.auto_restore);
        assert_eq!(
            db.settings().unwrap()[KEY_DIRTY_ACTION],
            serde_json::json!("stash")
        );
    }

    #[test]
    fn clean_switch_and_already_current() {
        let dir = repo();
        let r = dir.path();
        let c = check(&g(), r, &local("feature")).unwrap();
        assert_eq!(c.changes, 0);
        assert!(!c.already_current && !c.create && c.operation.is_none());
        let out = run(
            &g(),
            r,
            &none(),
            &local("feature"),
            SwitchDirtyMode::Stash,
            true,
        )
        .unwrap();
        assert_eq!(out.status, SwitchStatus::Switched);
        assert_eq!(out.from.as_deref(), Some("main"));
        assert!(!out.stashed && out.restore.is_none());
        assert_eq!(current(r), "feature");
        let again = run(
            &g(),
            r,
            &none(),
            &local("feature"),
            SwitchDirtyMode::Stash,
            true,
        )
        .unwrap();
        assert_eq!(again.status, SwitchStatus::Unchanged);
    }

    #[test]
    fn stashes_changes_and_restores_them_when_coming_back() {
        let dir = repo();
        let r = dir.path();
        write(r, "a.txt", "work in progress\n");
        write(r, "new.txt", "untracked\n");
        git(r, &["add", "a.txt"]);
        let c = check(&g(), r, &local("feature")).unwrap();
        assert_eq!(c.changes, 2);

        let out = run(
            &g(),
            r,
            &none(),
            &local("feature"),
            SwitchDirtyMode::Stash,
            true,
        )
        .unwrap();
        assert_eq!(out.status, SwitchStatus::Switched);
        assert!(out.stashed);
        assert_eq!(out.changes, 2);
        assert_eq!(current(r), "feature");
        assert_eq!(read(r, "a.txt"), "feature\n");
        assert!(!r.join("new.txt").exists());
        assert!(git(r, &["stash", "list"]).contains("nanomgg-autostash: main"));
        assert_eq!(find_autostash(&g(), r, "main").unwrap(), Some(0));
        assert_eq!(find_autostash(&g(), r, "feature").unwrap(), None);

        let back = run(
            &g(),
            r,
            &none(),
            &local("main"),
            SwitchDirtyMode::Stash,
            true,
        )
        .unwrap();
        assert_eq!(back.status, SwitchStatus::Switched);
        let restore = back.restore.unwrap();
        assert_eq!(restore.status, RestoreStatus::Restored);
        assert_eq!(restore.files, 2);
        assert_eq!(read(r, "a.txt"), "work in progress\n");
        assert_eq!(read(r, "new.txt"), "untracked\n");
        // The staged change is staged again.
        assert_eq!(git(r, &["diff", "--cached", "--name-only"]), "a.txt");
        assert_eq!(stash_count(r), 0);
    }

    #[test]
    fn restore_is_only_offered_when_off_or_dirty() {
        let dir = repo();
        let r = dir.path();
        write(r, "a.txt", "wip\n");
        run(
            &g(),
            r,
            &none(),
            &local("feature"),
            SwitchDirtyMode::Stash,
            true,
        )
        .unwrap();
        // Auto restore off.
        let back = run(
            &g(),
            r,
            &none(),
            &local("main"),
            SwitchDirtyMode::Stash,
            false,
        )
        .unwrap();
        let p = back.restore.unwrap();
        assert_eq!(
            (p.status, p.files, p.dirty),
            (RestoreStatus::Pending, 1, false)
        );
        assert_eq!(stash_count(r), 1);
        // "Restore now".
        let done = restore(&g(), r, &none(), "main").unwrap().unwrap();
        assert_eq!(done.status, RestoreStatus::Restored);
        assert_eq!(read(r, "a.txt"), "wip\n");
        assert!(restore(&g(), r, &none(), "main").unwrap().is_none());
        assert!(restore(&g(), r, &none(), "feature").is_err());

        // Carrying other changes back: the autostash is not popped on top of them.
        run(
            &g(),
            r,
            &none(),
            &local("feature"),
            SwitchDirtyMode::Stash,
            true,
        )
        .unwrap();
        write(r, "b.txt", "carried\n");
        let back = run(
            &g(),
            r,
            &none(),
            &local("main"),
            SwitchDirtyMode::Carry,
            true,
        )
        .unwrap();
        assert_eq!(back.status, SwitchStatus::Switched);
        assert!(!back.stashed);
        let p = back.restore.unwrap();
        assert_eq!((p.status, p.dirty), (RestoreStatus::Pending, true));
        assert_eq!(read(r, "b.txt"), "carried\n");
    }

    #[test]
    fn restore_conflicts_keep_the_stash() {
        let dir = repo();
        let r = dir.path();
        write(r, "a.txt", "wip on main\n");
        run(
            &g(),
            r,
            &none(),
            &local("feature"),
            SwitchDirtyMode::Stash,
            true,
        )
        .unwrap();
        // main moves on while away, touching the same line.
        git(r, &["switch", "-q", "main"]);
        write(r, "a.txt", "main changed\n");
        git(r, &["commit", "-q", "-am", "main moved"]);
        git(r, &["switch", "-q", "feature"]);
        let back = run(
            &g(),
            r,
            &none(),
            &local("main"),
            SwitchDirtyMode::Stash,
            true,
        )
        .unwrap();
        let p = back.restore.unwrap();
        assert_eq!(p.status, RestoreStatus::Conflicts);
        assert_eq!(p.conflicts, vec!["a.txt".to_owned()]);
        assert_eq!(stash_count(r), 1);
    }

    #[test]
    fn carry_is_refused_when_it_would_overwrite() {
        let dir = repo();
        let r = dir.path();
        write(r, "a.txt", "mine\n");
        let out = run(
            &g(),
            r,
            &none(),
            &local("feature"),
            SwitchDirtyMode::Carry,
            true,
        )
        .unwrap();
        assert_eq!(out.status, SwitchStatus::Blocked);
        assert_eq!(out.reason, Some(FailureKind::LocalChanges));
        assert_eq!(current(r), "main");
        assert_eq!(read(r, "a.txt"), "mine\n");
        // Carrying a change that does not clash works.
        write(r, "a.txt", "main\n");
        write(r, "b.txt", "carried\n");
        let out = run(
            &g(),
            r,
            &none(),
            &local("feature"),
            SwitchDirtyMode::Carry,
            true,
        )
        .unwrap();
        assert_eq!(out.status, SwitchStatus::Switched);
        assert_eq!(read(r, "b.txt"), "carried\n");
    }

    #[test]
    fn failed_checkout_pops_the_stash_back() {
        let dir = repo();
        let r = dir.path();
        write(r, "a.txt", "precious\n");
        write(r, "u.txt", "untracked\n");
        let out = run(
            &g(),
            r,
            &none(),
            &local("missing"),
            SwitchDirtyMode::Stash,
            true,
        )
        .unwrap();
        assert_eq!(out.status, SwitchStatus::Blocked);
        assert_eq!(out.reason, Some(FailureKind::NotFound));
        assert!(!out.stash_kept);
        assert_eq!(read(r, "a.txt"), "precious\n");
        assert_eq!(read(r, "u.txt"), "untracked\n");
        assert_eq!(stash_count(r), 0);
        assert_eq!(current(r), "main");
    }

    #[test]
    fn operation_in_progress_and_conflicts_block() {
        let dir = repo();
        let r = dir.path();
        write(r, "a.txt", "main 2\n");
        git(r, &["commit", "-q", "-am", "main 2"]);
        // Conflicting merge leaves MERGE_HEAD + a conflicted file.
        let _ = g().run(r, &none(), ["merge", "feature"]);
        let c = check(&g(), r, &local("feature")).unwrap();
        assert_eq!(c.operation, Some(RepoOperation::Merge));
        assert_eq!(c.conflicts, 1);
        let out = run(
            &g(),
            r,
            &none(),
            &local("feature"),
            SwitchDirtyMode::Stash,
            true,
        )
        .unwrap();
        assert_eq!(out.reason, Some(FailureKind::OperationInProgress));
        assert_eq!(stash_count(r), 0);
    }

    #[test]
    fn branch_in_another_worktree_blocks() {
        let dir = repo();
        let r = dir.path();
        let wt = tempfile::tempdir().unwrap();
        let wt_path = wt.path().join("wt");
        git(
            r,
            &[
                "worktree",
                "add",
                "-q",
                wt_path.to_str().unwrap(),
                "feature",
            ],
        );
        let c = check(&g(), r, &local("feature")).unwrap();
        assert!(c.checked_out_at.is_some());
        let out = run(
            &g(),
            r,
            &none(),
            &local("feature"),
            SwitchDirtyMode::Stash,
            true,
        )
        .unwrap();
        assert_eq!(out.reason, Some(FailureKind::CheckedOutElsewhere));
        assert!(out.worktree.is_some());
        git(
            r,
            &["worktree", "remove", "--force", wt_path.to_str().unwrap()],
        );
    }

    #[test]
    fn detached_head_off_every_branch_is_flagged() {
        let dir = repo();
        let r = dir.path();
        git(r, &["switch", "-q", "--detach", "main"]);
        let c = check(&g(), r, &local("feature")).unwrap();
        assert!(c.current.is_none() && !c.detached_orphan);
        assert_eq!(c.head_short.as_deref().map(str::len), Some(7));
        write(r, "c.txt", "lost?\n");
        git(r, &["add", "c.txt"]);
        git(r, &["commit", "-q", "-m", "detached work"]);
        assert!(check(&g(), r, &local("feature")).unwrap().detached_orphan);
    }

    #[test]
    fn remote_branch_uses_the_tracking_branch_or_creates_it() {
        let dir = repo();
        let r = dir.path();
        let remote = tempfile::tempdir().unwrap();
        git(remote.path(), &["init", "-q", "--bare"]);
        git(
            r,
            &["remote", "add", "origin", remote.path().to_str().unwrap()],
        );
        git(r, &["push", "-q", "origin", "main", "feature"]);
        git(r, &["fetch", "-q", "origin"]);
        git(r, &["branch", "-q", "-D", "feature"]);

        let target = SwitchTarget::Remote {
            remote: "origin".into(),
            branch: "feature".into(),
        };
        let c = check(&g(), r, &target).unwrap();
        assert_eq!((c.branch.as_str(), c.create), ("feature", true));
        let out = run(&g(), r, &none(), &target, SwitchDirtyMode::Stash, true).unwrap();
        assert_eq!(out.status, SwitchStatus::Switched);
        assert!(out.created);
        assert_eq!(current(r), "feature");
        assert_eq!(
            git(r, &["rev-parse", "--abbrev-ref", "feature@{upstream}"]),
            "origin/feature"
        );

        // A tracking branch with another name is reused.
        git(r, &["switch", "-q", "main"]);
        git(r, &["branch", "-q", "-m", "feature", "my-feature"]);
        let c = check(&g(), r, &target).unwrap();
        assert_eq!((c.branch.as_str(), c.create), ("my-feature", false));
        let main = SwitchTarget::Remote {
            remote: "origin".into(),
            branch: "main".into(),
        };
        // `main` has no upstream but the same name: switch to it.
        let c = check(&g(), r, &main).unwrap();
        assert_eq!(
            (c.branch.as_str(), c.create, c.already_current),
            ("main", false, true)
        );
    }
}
