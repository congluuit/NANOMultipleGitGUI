//! Rebase: plain (`git rebase <onto> [<branch>]`) and interactive with a prepared todo list,
//! plus the helpers branch drag-and-drop needs (relation between two branches, fast-forward of
//! a branch that is not checked out).
//!
//! # Never an interactive editor
//! - The todo list is stored as a blob (`git hash-object -w --stdin`) and installed by
//!   `GIT_SEQUENCE_EDITOR='git cat-file blob <oid> >"$1" #'`: git runs the editor through `sh`
//!   (Git for Windows too) with the todo path as `$1`, and the trailing `#` comments out the
//!   `"$@"` git appends. Nothing user-controlled ends up in the command line, so paths with
//!   spaces or non-ASCII characters are no problem.
//! - New messages (reword, squash, edit) are blobs too, applied right after their commit by
//!   `exec git cat-file blob <oid> | git commit --amend … -F -`. The todo is self-contained: a
//!   rebase stopped on a conflict or an `edit` can be continued later (from the app or a
//!   terminal) without any environment of ours.
//! - `GIT_EDITOR=:` (see [`writer`]) keeps git's prepared message whenever it would ask.
//!
//! # Safety
//! A backup ref (`refs/nanomgg/backup/<ts>`, see [`guard`](crate::guard)) is created before
//! anything is rewritten. Rewriting commits that are reachable from a remote-tracking ref is
//! refused with [`FailureKind::PushedCommits`] unless the caller confirms (`allow_pushed`).
//! Uncommitted changes to tracked files are refused with [`FailureKind::LocalChanges`] unless
//! `autostash` is set.

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::branch::validate_branch_name;
use crate::commit::is_valid_oid;
use crate::guard::create_backup;
use crate::ops::{
    FailureKind, OpError, RepoOperation, classify, current_branch, current_operation, ensure_idle,
    head_oid, operation_state, ref_exists, resolve_commit, writer,
};
use crate::{AccountContext, GitError, GitRunner};

/// Commits listed in detail by [`preview`] / [`todo`] (counts stay exact).
pub const MAX_LISTED: usize = 2000;

fn none() -> AccountContext {
    AccountContext::default()
}

fn invalid(message: impl Into<String>) -> OpError {
    OpError::rejected(FailureKind::InvalidInput, message)
}

/// One commit that a rebase would rewrite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RebaseCommit {
    pub oid: String,
    pub short_oid: String,
    pub subject: String,
    /// Full message (subject, blank line, body), trailing whitespace trimmed.
    pub message: String,
    pub author_name: String,
    /// Author time, Unix seconds.
    pub time: i64,
    /// Reachable from a remote-tracking ref (already pushed somewhere).
    pub pushed: bool,
    pub is_merge: bool,
}

/// What a rebase of `branch` onto `onto` would do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RebasePreview {
    /// Branch being rebased; `None` when HEAD is detached.
    pub branch: Option<String>,
    /// The branch is the checked-out one (otherwise git checks it out first).
    pub is_current: bool,
    /// Tip of the branch.
    pub head: String,
    /// Full id of the new base (plain rebase) or of the exclusive base (interactive).
    pub onto: String,
    /// Commits to rewrite, oldest first (at most [`MAX_LISTED`]).
    pub commits: Vec<RebaseCommit>,
    pub total: u32,
    /// How many of them are already on a remote-tracking ref.
    pub pushed: u32,
    /// Merge commits in the range (a plain rebase flattens them; interactive refuses).
    pub merges: u32,
    /// Staged or unstaged changes to tracked files.
    pub dirty: bool,
    /// Nothing to do: `onto` is already in the branch.
    pub up_to_date: bool,
    /// The branch is behind `onto` only: the rebase is a fast-forward (nothing rewritten).
    pub fast_forward: bool,
}

fn rev_list(git: &GitRunner, repo: &Path, args: &[&str]) -> Result<Vec<String>, OpError> {
    let mut full = vec!["rev-list"];
    full.extend_from_slice(args);
    full.push("--");
    Ok(git
        .run_text(repo, &none(), &full)?
        .lines()
        .map(str::to_owned)
        .collect())
}

fn is_ancestor(git: &GitRunner, repo: &Path, a: &str, b: &str) -> bool {
    git.run(repo, &none(), ["merge-base", "--is-ancestor", a, b])
        .is_ok()
}

/// Staged or unstaged changes to tracked files (untracked files never block a rebase).
pub fn has_tracked_changes(git: &GitRunner, repo: &Path) -> Result<bool, OpError> {
    let out = git.run_text(
        repo,
        &none(),
        [
            "status",
            "--porcelain",
            "--untracked-files=no",
            "--ignore-submodules",
        ],
    )?;
    Ok(!out.is_empty())
}

/// Parses `git log --format=%H%x1f%h%x1f%P%x1f%an%x1f%at%x1f%B%x1e`.
pub fn parse_commits(raw: &str, local_only: &HashSet<String>) -> Vec<RebaseCommit> {
    raw.split('\x1e')
        .filter_map(|rec| {
            let rec = rec.trim_start_matches(['\n', '\r']);
            if rec.is_empty() {
                return None;
            }
            let f: Vec<&str> = rec.splitn(6, '\x1f').collect();
            if f.len() < 6 {
                return None;
            }
            let message = f[5].trim_end().to_owned();
            Some(RebaseCommit {
                oid: f[0].to_owned(),
                short_oid: f[1].to_owned(),
                subject: message.lines().next().unwrap_or("").to_owned(),
                message,
                author_name: f[3].to_owned(),
                time: f[4].trim().parse().unwrap_or(0),
                pushed: !local_only.contains(f[0]),
                is_merge: f[2].split_whitespace().count() > 1,
            })
        })
        .collect()
}

/// Lists `base..tip` (oldest first) with pushed / merge flags.
fn range(
    git: &GitRunner,
    repo: &Path,
    base: &str,
    tip: &str,
) -> Result<(Vec<RebaseCommit>, u32, u32, u32), OpError> {
    let exclude = format!("^{base}");
    let all = rev_list(git, repo, &[tip, &exclude])?;
    let total = all.len() as u32;
    let local: HashSet<String> = rev_list(git, repo, &[tip, &exclude, "--not", "--remotes"])?
        .into_iter()
        .collect();
    let pushed = all.iter().filter(|c| !local.contains(*c)).count() as u32;
    let merges = rev_list(git, repo, &[tip, &exclude, "--merges"])?.len() as u32;
    let commits = if total == 0 {
        Vec::new()
    } else {
        let max = format!("--max-count={MAX_LISTED}");
        let raw = git.run_text(
            repo,
            &none(),
            [
                "log",
                "--no-show-signature",
                "--no-color",
                &max,
                "--format=%H%x1f%h%x1f%P%x1f%an%x1f%at%x1f%B%x1e",
                tip,
                &exclude,
                "--",
            ],
        )?;
        let mut list = parse_commits(&raw, &local);
        list.reverse();
        list
    };
    Ok((commits, total, pushed, merges))
}

/// Resolves the branch to rebase: `branch` (validated, must exist) or the checked-out one.
fn target_branch(
    git: &GitRunner,
    repo: &Path,
    branch: Option<&str>,
) -> Result<(Option<String>, bool, String), OpError> {
    let current = current_branch(git, repo);
    match branch {
        Some(b) if current.as_deref() != Some(b) => {
            validate_branch_name(git, repo, b)?;
            let full = format!("refs/heads/{b}");
            if !ref_exists(git, repo, &full) {
                return Err(OpError::rejected(
                    FailureKind::NotFound,
                    format!("branch '{b}' not found"),
                ));
            }
            let tip = git.run_text(repo, &none(), ["rev-parse", "--verify", &full])?;
            Ok((Some(b.to_owned()), false, tip))
        }
        _ => {
            let tip = head_oid(git, repo).ok_or_else(|| {
                OpError::rejected(FailureKind::NotFound, "the branch has no commits yet")
            })?;
            Ok((current, true, tip))
        }
    }
}

/// What `git rebase <onto> [<branch>]` would rewrite (`branch` defaults to the current one).
pub fn preview(
    git: &GitRunner,
    repo: &Path,
    onto: &str,
    branch: Option<&str>,
) -> Result<RebasePreview, OpError> {
    let onto = resolve_commit(git, repo, onto)?;
    let (branch, is_current, tip) = target_branch(git, repo, branch)?;
    let up_to_date = is_ancestor(git, repo, &onto, &tip);
    let fast_forward = !up_to_date && is_ancestor(git, repo, &tip, &onto);
    let (commits, total, pushed, merges) = if up_to_date || fast_forward {
        (Vec::new(), 0, 0, 0)
    } else {
        range(git, repo, &onto, &tip)?
    };
    Ok(RebasePreview {
        branch,
        is_current,
        head: tip,
        onto,
        commits,
        total,
        pushed,
        merges,
        dirty: has_tracked_changes(git, repo)?,
        up_to_date,
        fast_forward,
    })
}

/// Commits of the current branch after `base` (exclusive), oldest first — the rows of the
/// interactive rebase dialog. When `base` is not an ancestor of HEAD (another branch), the
/// point where the two histories diverge (`git merge-base`) is used instead, so the rows are
/// the commits of the current branch that `base` does not have — and they stay where they are.
pub fn todo(git: &GitRunner, repo: &Path, base: &str) -> Result<RebasePreview, OpError> {
    let mut base = resolve_commit(git, repo, base)?;
    let tip = head_oid(git, repo)
        .ok_or_else(|| OpError::rejected(FailureKind::NotFound, "the branch has no commits"))?;
    if !is_ancestor(git, repo, &base, &tip) {
        base = git
            .run_text(repo, &none(), ["merge-base", &base, &tip])
            .ok()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                invalid(format!(
                    "{} has no history in common with the current branch",
                    &base[..10.min(base.len())]
                ))
            })?;
    }
    if base == tip {
        return Err(invalid(
            "pick a commit below HEAD: there is nothing after it on the current branch",
        ));
    }
    let (commits, total, pushed, merges) = range(git, repo, &base, &tip)?;
    Ok(RebasePreview {
        branch: current_branch(git, repo),
        is_current: true,
        head: tip,
        onto: base,
        commits,
        total,
        pushed,
        merges,
        dirty: has_tracked_changes(git, repo)?,
        up_to_date: false,
        fast_forward: false,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum RebaseStatus {
    /// Finished; the branch points at the rewritten commits.
    Completed,
    /// Nothing to do.
    UpToDate,
    /// Stopped on conflicts: resolve, stage, then continue / skip / abort.
    Conflicts,
    /// Stopped without conflicts (an `edit` step, or a step git could not apply on its own):
    /// amend if you like, then continue.
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RebaseOutcome {
    pub status: RebaseStatus,
    pub conflicts: Vec<String>,
    /// Progress when stopped (1-based step / total).
    pub step: Option<u32>,
    pub total_steps: Option<u32>,
    /// Backup of the branch tip taken before the rebase (`None` for continue / skip).
    pub backup_ref: Option<String>,
    pub head: Option<String>,
    /// Rebased branch, when known.
    pub branch: Option<String>,
    /// `--autostash` could not re-apply the stashed changes cleanly; they stay in the stash.
    pub autostash_conflict: bool,
    /// git's own explanation when it stopped without conflicts.
    pub message: Option<String>,
}

impl RebaseOutcome {
    fn up_to_date(head: Option<String>, branch: Option<String>) -> Self {
        Self {
            status: RebaseStatus::UpToDate,
            conflicts: Vec::new(),
            step: None,
            total_steps: None,
            backup_ref: None,
            head,
            branch,
            autostash_conflict: false,
            message: None,
        }
    }
}

fn autostash_conflict(output: &str) -> bool {
    let s = output.to_ascii_lowercase();
    s.contains("autostash resulted in conflicts")
        || s.contains("your changes are safe in the stash")
}

/// Turns the result of a `git rebase …` invocation into an outcome.
fn settle(
    git: &GitRunner,
    repo: &Path,
    result: Result<std::process::Output, GitError>,
    backup_ref: Option<String>,
    branch: Option<String>,
) -> Result<RebaseOutcome, OpError> {
    let state = operation_state(git, repo)?;
    let in_rebase = state.operation == Some(RepoOperation::Rebase);
    let mut out = RebaseOutcome {
        status: RebaseStatus::Completed,
        conflicts: Vec::new(),
        step: None,
        total_steps: None,
        backup_ref,
        head: head_oid(git, repo),
        branch: branch.or_else(|| current_branch(git, repo)),
        autostash_conflict: false,
        message: None,
    };
    match result {
        Ok(output) => {
            let text = format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            out.autostash_conflict = autostash_conflict(&text);
            if in_rebase {
                out.status = RebaseStatus::Stopped;
                out.step = state.step;
                out.total_steps = state.total_steps;
                out.message = Some(text.trim().to_owned()).filter(|m| !m.is_empty());
            }
            Ok(out)
        }
        Err(err) => {
            if !state.conflicts.is_empty() {
                out.status = RebaseStatus::Conflicts;
                out.conflicts = state.conflicts;
                out.step = state.step;
                out.total_steps = state.total_steps;
                return Ok(out);
            }
            if in_rebase {
                // Stopped for another reason (failed exec, empty commit…): the user decides.
                out.status = RebaseStatus::Stopped;
                out.step = state.step;
                out.total_steps = state.total_steps;
                if let GitError::Failed { stderr, .. } = &err {
                    out.autostash_conflict = autostash_conflict(stderr);
                    out.message = Some(stderr.clone());
                }
                return Ok(out);
            }
            if let GitError::Failed { stderr, .. } = &err
                && autostash_conflict(stderr)
            {
                out.autostash_conflict = true;
                return Ok(out);
            }
            Err(classify(err))
        }
    }
}

/// Plain rebase request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RebaseRequest {
    /// New base: branch, remote branch, tag or commit id.
    pub onto: String,
    /// Branch to rebase; `None` = the checked-out branch. Another branch is checked out first.
    #[serde(default)]
    #[cfg_attr(feature = "specta", specta(optional))]
    pub branch: Option<String>,
    /// Stash uncommitted changes first and re-apply them afterwards.
    #[serde(default)]
    pub autostash: bool,
    /// The user confirmed rewriting commits that are already on a remote.
    #[serde(default)]
    pub allow_pushed: bool,
}

/// Commits an interactive plan really rewrites: everything from the first step that differs
/// from "pick, original order" on (git keeps the unchanged prefix as is).
pub fn rewritten_by(commits: &[RebaseCommit], steps: &[TodoStep]) -> (u32, u32) {
    let first = steps
        .iter()
        .zip(commits)
        .position(|(s, c)| s.action != TodoAction::Pick || !c.oid.starts_with(&s.oid))
        .unwrap_or(commits.len());
    let rewritten = &commits[first.min(commits.len())..];
    (
        rewritten.iter().filter(|c| c.pushed).count() as u32,
        rewritten.len() as u32,
    )
}

fn check_preconditions(
    p: &RebasePreview,
    autostash: bool,
    allow_pushed: bool,
) -> Result<(), OpError> {
    if p.branch.is_none() {
        return Err(OpError::rejected(
            FailureKind::DetachedHead,
            "check out a branch first: a detached HEAD cannot be rebased from here",
        ));
    }
    if p.dirty && !autostash {
        return Err(OpError::rejected(
            FailureKind::LocalChanges,
            "you have uncommitted changes: commit or stash them, or enable autostash",
        ));
    }
    if p.pushed > 0 && !allow_pushed {
        return Err(OpError::rejected(
            FailureKind::PushedCommits,
            format!(
                "{} of the {} commits to rewrite are already on a remote",
                p.pushed, p.total
            ),
        ));
    }
    Ok(())
}

/// `git rebase <onto> [<branch>]` after the checks above and a backup of the branch tip.
pub fn rebase(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    req: &RebaseRequest,
) -> Result<RebaseOutcome, OpError> {
    ensure_idle(git, repo)?;
    let p = preview(git, repo, &req.onto, req.branch.as_deref())?;
    if p.up_to_date {
        return Ok(RebaseOutcome::up_to_date(Some(p.head), p.branch));
    }
    check_preconditions(&p, req.autostash, req.allow_pushed)?;
    let branch = p.branch.clone().unwrap_or_default();
    if !p.is_current {
        // An explicit checkout (its own reflog entry, so Undo can go back step by step) rather
        // than `git rebase <onto> <branch>`. Local changes travel with it or block it.
        writer(git)
            .run(
                repo,
                ctx,
                ["switch", "--no-guess", "--end-of-options", &branch],
            )
            .map_err(classify)?;
    }
    let backup = create_backup(git, repo, &p.head, &branch, "rebase")?;
    let args = [
        "rebase",
        if req.autostash {
            "--autostash"
        } else {
            "--no-autostash"
        },
        "--end-of-options",
        &p.onto,
    ];
    let result = writer(git).run(repo, ctx, args);
    settle(git, repo, result, Some(backup), p.branch)
}

/// Action of one todo row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum TodoAction {
    Pick,
    /// Keep the change, replace the message.
    Reword,
    /// Meld into the previous commit; `message` is the combined message (default: git's).
    Squash,
    /// Meld into the previous commit, keeping the previous message.
    Fixup,
    Drop,
    /// Stop after applying it so it can be amended.
    Edit,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TodoStep {
    pub oid: String,
    pub action: TodoAction,
    /// New message for `reword` (required), `squash` (combined message) or `edit`.
    #[serde(default)]
    #[cfg_attr(feature = "specta", specta(optional))]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct InteractiveRebaseRequest {
    /// Exclusive base: the rows are the commits after it.
    pub base: String,
    /// Every commit of `base..HEAD` exactly once, in the new order (oldest first).
    pub steps: Vec<TodoStep>,
    #[serde(default)]
    pub autostash: bool,
    #[serde(default)]
    pub allow_pushed: bool,
}

/// Stores `content` as a blob and returns its id.
fn write_blob(git: &GitRunner, repo: &Path, content: &str) -> Result<String, OpError> {
    let out = git.run_with_input(
        repo,
        &none(),
        ["hash-object", "-w", "--no-filters", "--stdin"],
        content.as_bytes(),
    )?;
    let oid = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if !is_valid_oid(&oid) {
        return Err(invalid("could not store the todo list"));
    }
    Ok(oid)
}

/// The `exec` line that replaces HEAD's message with blob `oid`.
fn amend_exec(oid: &str) -> String {
    format!(
        "exec git cat-file blob {oid} | git commit --amend --quiet --no-verify --allow-empty --cleanup=whitespace -F -"
    )
}

/// Normalises a message from the UI (CRLF → LF, trailing whitespace trimmed).
fn clean_message(message: &str) -> String {
    let m = message.replace("\r\n", "\n");
    format!("{}\n", m.trim_end())
}

/// Validates the steps against `base..HEAD` and builds the todo text (message blobs are
/// written as a side effect). Returns `None` when the plan changes nothing.
pub fn build_todo(
    git: &GitRunner,
    repo: &Path,
    commits: &[RebaseCommit],
    steps: &[TodoStep],
) -> Result<Option<String>, OpError> {
    if commits.iter().any(|c| c.is_merge) {
        return Err(invalid(
            "the range contains merge commits; interactive rebase from here is not supported",
        ));
    }
    if steps.len() != commits.len() {
        return Err(invalid(format!(
            "expected {} steps, got {}",
            commits.len(),
            steps.len()
        )));
    }
    // Map each step to the full id of exactly one commit of the range.
    let mut used = HashSet::new();
    let mut full_ids = Vec::with_capacity(steps.len());
    for step in steps {
        if !is_valid_oid(&step.oid) {
            return Err(invalid(format!("invalid commit id: {:?}", step.oid)));
        }
        let matches: Vec<&RebaseCommit> = commits
            .iter()
            .filter(|c| c.oid.starts_with(&step.oid.to_ascii_lowercase()))
            .collect();
        let [commit] = matches.as_slice() else {
            return Err(invalid(format!(
                "{} is not (exactly) one commit of the range",
                step.oid
            )));
        };
        if !used.insert(commit.oid.clone()) {
            return Err(invalid(format!("{} is listed twice", commit.short_oid)));
        }
        full_ids.push(commit.oid.clone());
    }
    match steps.iter().find(|s| s.action != TodoAction::Drop) {
        Some(first) if matches!(first.action, TodoAction::Squash | TodoAction::Fixup) => {
            return Err(invalid(
                "the first kept commit cannot be squashed: there is no commit before it",
            ));
        }
        _ => {}
    }
    let unchanged = steps
        .iter()
        .zip(commits)
        .all(|(s, c)| s.action == TodoAction::Pick && c.oid.starts_with(&s.oid));
    if unchanged {
        return Ok(None);
    }
    let mut todo = String::new();
    for (step, oid) in steps.iter().zip(&full_ids) {
        let message = step
            .message
            .as_deref()
            .filter(|m| !m.trim().is_empty())
            .map(clean_message);
        match step.action {
            TodoAction::Pick => todo.push_str(&format!("pick {oid}\n")),
            TodoAction::Drop => todo.push_str(&format!("drop {oid}\n")),
            TodoAction::Fixup => todo.push_str(&format!("fixup {oid}\n")),
            TodoAction::Reword => {
                let Some(m) = message else {
                    return Err(invalid(format!(
                        "reword {}: the new message is empty",
                        &oid[..7]
                    )));
                };
                todo.push_str(&format!("pick {oid}\n"));
                todo.push_str(&amend_exec(&write_blob(git, repo, &m)?));
                todo.push('\n');
            }
            TodoAction::Squash => {
                todo.push_str(&format!("squash {oid}\n"));
                if let Some(m) = message {
                    todo.push_str(&amend_exec(&write_blob(git, repo, &m)?));
                    todo.push('\n');
                }
            }
            TodoAction::Edit => match message {
                Some(m) => {
                    todo.push_str(&format!("pick {oid}\n"));
                    todo.push_str(&amend_exec(&write_blob(git, repo, &m)?));
                    todo.push_str("\nbreak\n");
                }
                None => todo.push_str(&format!("edit {oid}\n")),
            },
        }
    }
    Ok(Some(todo))
}

/// Interactive rebase of the current branch from `base` with the given plan, without ever
/// opening an editor (see the module docs).
pub fn rebase_interactive(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    req: &InteractiveRebaseRequest,
) -> Result<RebaseOutcome, OpError> {
    ensure_idle(git, repo)?;
    let p = todo(git, repo, &req.base)?;
    if p.total as usize > p.commits.len() {
        return Err(invalid(format!(
            "too many commits ({}) for an interactive rebase from here",
            p.total
        )));
    }
    let (pushed, total) = rewritten_by(&p.commits, &req.steps);
    let checked = RebasePreview {
        pushed,
        total,
        ..p.clone()
    };
    check_preconditions(&checked, req.autostash, req.allow_pushed)?;
    let Some(todo_text) = build_todo(git, repo, &p.commits, &req.steps)? else {
        return Ok(RebaseOutcome::up_to_date(Some(p.head), p.branch));
    };
    let branch = p.branch.clone().unwrap_or_default();
    let todo_blob = write_blob(git, repo, &todo_text)?;
    let backup = create_backup(git, repo, &p.head, &branch, "rebase-interactive")?;
    let editor = format!("git cat-file blob {todo_blob} >\"$1\" #");
    let result = writer(git).with_env("GIT_SEQUENCE_EDITOR", editor).run(
        repo,
        ctx,
        [
            "rebase",
            "-i",
            "--no-autosquash",
            if req.autostash {
                "--autostash"
            } else {
                "--no-autostash"
            },
            "--end-of-options",
            &p.onto,
        ],
    );
    settle(git, repo, result, Some(backup), p.branch)
}

/// `git rebase --continue` once conflicts are resolved and staged (or after an `edit` stop).
pub fn continue_rebase(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
) -> Result<RebaseOutcome, OpError> {
    let state = operation_state(git, repo)?;
    if state.operation != Some(RepoOperation::Rebase) {
        return Err(OpError::rejected(
            FailureKind::NoOperation,
            "no rebase in progress",
        ));
    }
    if !state.conflicts.is_empty() {
        return Err(OpError::rejected(
            FailureKind::UnmergedFiles,
            format!(
                "resolve and stage {} conflicted file(s) first",
                state.conflicts.len()
            ),
        ));
    }
    let result = writer(git).run(repo, ctx, ["rebase", "--continue"]);
    settle(git, repo, result, None, None)
}

/// `git rebase --skip`: drops the commit being applied and goes on.
pub fn skip_rebase(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
) -> Result<RebaseOutcome, OpError> {
    if current_operation(git, repo)? != Some(RepoOperation::Rebase) {
        return Err(OpError::rejected(
            FailureKind::NoOperation,
            "no rebase in progress",
        ));
    }
    let result = writer(git).run(repo, ctx, ["rebase", "--skip"]);
    settle(git, repo, result, None, None)
}

// ----- drag-and-drop helpers -----------------------------------------------------------------

/// How two local branches relate (for the drop menu).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct BranchRelation {
    pub source: String,
    pub target: String,
    /// Commits on `source` that `target` lacks.
    pub ahead: u32,
    /// Commits on `target` that `source` lacks.
    pub behind: u32,
    /// `target` can be fast-forwarded to `source` (it is an ancestor and they differ).
    pub can_fast_forward: bool,
    pub current: Option<String>,
}

fn local_tip(git: &GitRunner, repo: &Path, name: &str) -> Result<String, OpError> {
    validate_branch_name(git, repo, name)?;
    let full = format!("refs/heads/{name}");
    if !ref_exists(git, repo, &full) {
        return Err(OpError::rejected(
            FailureKind::NotFound,
            format!("branch '{name}' not found"),
        ));
    }
    Ok(git.run_text(repo, &none(), ["rev-parse", "--verify", &full])?)
}

pub fn branch_relation(
    git: &GitRunner,
    repo: &Path,
    source: &str,
    target: &str,
) -> Result<BranchRelation, OpError> {
    let s = local_tip(git, repo, source)?;
    let t = local_tip(git, repo, target)?;
    let counts = git.run_text(
        repo,
        &none(),
        [
            "rev-list",
            "--left-right",
            "--count",
            &format!("{s}...{t}"),
            "--",
        ],
    )?;
    let mut it = counts
        .split_whitespace()
        .map(|n| n.parse::<u32>().unwrap_or(0));
    let ahead = it.next().unwrap_or(0);
    let behind = it.next().unwrap_or(0);
    Ok(BranchRelation {
        source: source.to_owned(),
        target: target.to_owned(),
        ahead,
        behind,
        can_fast_forward: ahead > 0 && behind == 0,
        current: current_branch(git, repo),
    })
}

/// Moves local branch `target` forward to `source`'s tip. The checked-out branch goes through
/// `git merge --ff-only`; another branch is moved with `update-ref` (checked: fast-forward only,
/// and the old value must not have changed meanwhile).
pub fn fast_forward_branch(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    target: &str,
    source: &str,
) -> Result<String, OpError> {
    let rel = branch_relation(git, repo, source, target)?;
    if !rel.can_fast_forward {
        return Err(OpError::rejected(
            FailureKind::NonFastForward,
            format!("'{target}' cannot be fast-forwarded to '{source}'"),
        ));
    }
    let s = local_tip(git, repo, source)?;
    if rel.current.as_deref() == Some(target) {
        ensure_idle(git, repo)?;
        writer(git)
            .run(repo, ctx, ["merge", "--ff-only", "--end-of-options", &s])
            .map_err(classify)?;
    } else {
        let t = local_tip(git, repo, target)?;
        git.run(
            repo,
            &none(),
            [
                "update-ref",
                "-m",
                &format!("nanomgg: fast-forward to {source}"),
                &format!("refs/heads/{target}"),
                &s,
                &t,
            ],
        )
        .map_err(classify)?;
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::{abort_operation, operation_state};
    use crate::progress::test_support::TestRepo;

    fn repo() -> TestRepo {
        let r = TestRepo::new();
        r.git(&["config", "core.autocrlf", "false"]);
        r
    }

    fn subjects(r: &TestRepo, range: &str) -> Vec<String> {
        r.git(&["log", "--reverse", "--format=%s", range])
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn step(oid: &str, action: TodoAction) -> TodoStep {
        TodoStep {
            oid: oid.to_owned(),
            action,
            message: None,
        }
    }

    fn with_msg(oid: &str, action: TodoAction, m: &str) -> TodoStep {
        TodoStep {
            oid: oid.to_owned(),
            action,
            message: Some(m.to_owned()),
        }
    }

    fn irb(base: &str, steps: Vec<TodoStep>) -> InteractiveRebaseRequest {
        InteractiveRebaseRequest {
            base: base.to_owned(),
            steps,
            autostash: false,
            allow_pushed: false,
        }
    }

    /// main: init → base; topic: base → a → b → c (one file each).
    fn linear() -> (TestRepo, String, [String; 3]) {
        let r = repo();
        let base = r.commit("base.txt", "base\n", "base");
        r.git(&["switch", "-q", "-c", "topic"]);
        let a = r.commit("a.txt", "a\n", "add a");
        let b = r.commit("b.txt", "b\n", "add b");
        let c = r.commit("c.txt", "c\n", "add c");
        (r, base, [a, b, c])
    }

    #[test]
    fn plain_rebase_with_backup_and_up_to_date() {
        let r = repo();
        r.git(&["switch", "-q", "-c", "topic"]);
        r.commit("t.txt", "t\n", "topic 1");
        let old_tip = r.commit("t2.txt", "t2\n", "topic 2");
        r.git(&["switch", "-q", "main"]);
        let main_tip = r.commit("m.txt", "m\n", "main 1");
        r.git(&["switch", "-q", "topic"]);

        let p = preview(&r.git, &r.path, "main", None).unwrap();
        assert_eq!(p.branch.as_deref(), Some("topic"));
        assert_eq!(p.total, 2);
        assert_eq!(p.commits[0].subject, "topic 1");
        assert!(!p.up_to_date && !p.fast_forward && !p.dirty);

        let req = RebaseRequest {
            onto: "main".into(),
            ..Default::default()
        };
        let out = rebase(&r.git, &r.path, &r.ctx, &req).unwrap();
        assert_eq!(out.status, RebaseStatus::Completed);
        let backup = out.backup_ref.unwrap();
        assert_eq!(r.git(&["rev-parse", &backup]), old_tip);
        assert_eq!(r.git(&["rev-parse", "HEAD~2"]), main_tip);
        assert_eq!(subjects(&r, "main..topic"), ["topic 1", "topic 2"]);

        let out = rebase(&r.git, &r.path, &r.ctx, &req).unwrap();
        assert_eq!(out.status, RebaseStatus::UpToDate);
        assert!(out.backup_ref.is_none());
    }

    #[test]
    fn rebase_other_branch_checks_it_out() {
        let r = repo();
        r.git(&["switch", "-q", "-c", "topic"]);
        r.commit("t.txt", "t\n", "topic 1");
        r.git(&["switch", "-q", "main"]);
        let main_tip = r.commit("m.txt", "m\n", "main 1");
        let req = RebaseRequest {
            onto: "main".into(),
            branch: Some("topic".into()),
            ..Default::default()
        };
        let p = preview(&r.git, &r.path, "main", Some("topic")).unwrap();
        assert!(!p.is_current);
        let out = rebase(&r.git, &r.path, &r.ctx, &req).unwrap();
        assert_eq!(out.status, RebaseStatus::Completed);
        assert_eq!(r.git(&["symbolic-ref", "--short", "HEAD"]), "topic");
        assert_eq!(r.git(&["rev-parse", "HEAD~1"]), main_tip);
    }

    #[test]
    fn conflicts_skip_continue_and_abort() {
        let r = repo();
        r.git(&["switch", "-q", "-c", "topic"]);
        r.commit("README.md", "topic\n", "topic readme");
        r.commit("t.txt", "t\n", "topic file");
        let old_tip = r.head();
        r.git(&["switch", "-q", "main"]);
        r.commit("README.md", "main\n", "main readme");
        r.git(&["switch", "-q", "topic"]);
        let req = RebaseRequest {
            onto: "main".into(),
            ..Default::default()
        };

        let out = rebase(&r.git, &r.path, &r.ctx, &req).unwrap();
        assert_eq!(out.status, RebaseStatus::Conflicts);
        assert_eq!(out.conflicts, vec!["README.md".to_owned()]);
        assert_eq!((out.step, out.total_steps), (Some(1), Some(2)));
        // Abort restores the original tip.
        abort_operation(&r.git, &r.path, &r.ctx).unwrap();
        assert_eq!(r.head(), old_tip);
        assert_eq!(r.git(&["symbolic-ref", "--short", "HEAD"]), "topic");

        // Skip the conflicting commit.
        rebase(&r.git, &r.path, &r.ctx, &req).unwrap();
        let out = skip_rebase(&r.git, &r.path, &r.ctx).unwrap();
        assert_eq!(out.status, RebaseStatus::Completed);
        assert_eq!(subjects(&r, "main..topic"), ["topic file"]);

        // Resolve and continue.
        r.git(&["reset", "-q", "--hard", &old_tip]);
        rebase(&r.git, &r.path, &r.ctx, &req).unwrap();
        let err = continue_rebase(&r.git, &r.path, &r.ctx).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::UnmergedFiles));
        r.write("README.md", "both\n");
        r.git(&["add", "README.md"]);
        let out = continue_rebase(&r.git, &r.path, &r.ctx).unwrap();
        assert_eq!(out.status, RebaseStatus::Completed, "{out:?}");
        assert_eq!(subjects(&r, "main..topic"), ["topic readme", "topic file"]);
        assert_eq!(r.read("README.md"), "both\n");
        let err = skip_rebase(&r.git, &r.path, &r.ctx).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::NoOperation));
    }

    #[test]
    fn dirty_tree_is_refused_or_autostashed() {
        let r = repo();
        r.git(&["switch", "-q", "-c", "topic"]);
        r.commit("t.txt", "t\n", "topic 1");
        r.git(&["switch", "-q", "main"]);
        r.commit("m.txt", "m\n", "main 1");
        r.git(&["switch", "-q", "topic"]);
        r.write("README.md", "dirty\n");
        let mut req = RebaseRequest {
            onto: "main".into(),
            ..Default::default()
        };
        assert!(preview(&r.git, &r.path, "main", None).unwrap().dirty);
        let err = rebase(&r.git, &r.path, &r.ctx, &req).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::LocalChanges));
        assert!(
            crate::guard::list_backups(&r.git, &r.path)
                .unwrap()
                .is_empty()
        );
        req.autostash = true;
        let out = rebase(&r.git, &r.path, &r.ctx, &req).unwrap();
        assert_eq!(out.status, RebaseStatus::Completed, "{out:?}");
        assert!(!out.autostash_conflict);
        assert_eq!(r.read("README.md"), "dirty\n");
        assert_eq!(r.read("m.txt"), "m\n");
    }

    #[test]
    fn pushed_commits_need_confirmation() {
        let dir = tempfile::tempdir().unwrap();
        let r = TestRepo::init_at(&dir.path().join("work"));
        r.git(&["config", "core.autocrlf", "false"]);
        r.git
            .run(
                dir.path(),
                &r.ctx,
                ["init", "-q", "--bare", "-b", "main", "remote.git"],
            )
            .unwrap();
        let url = dir
            .path()
            .join("remote.git")
            .to_string_lossy()
            .replace('\\', "/");
        r.git(&["remote", "add", "origin", &url]);
        r.git(&["switch", "-q", "-c", "topic"]);
        r.commit("t.txt", "t\n", "topic 1");
        r.git(&["push", "-q", "origin", "topic"]);
        r.commit("t2.txt", "t2\n", "topic 2 (local)");
        r.git(&["switch", "-q", "main"]);
        r.commit("m.txt", "m\n", "main 1");
        r.git(&["switch", "-q", "topic"]);

        let p = preview(&r.git, &r.path, "main", None).unwrap();
        assert_eq!((p.total, p.pushed), (2, 1));
        assert!(p.commits[0].pushed && !p.commits[1].pushed);
        let mut req = RebaseRequest {
            onto: "main".into(),
            ..Default::default()
        };
        let err = rebase(&r.git, &r.path, &r.ctx, &req).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::PushedCommits));
        req.allow_pushed = true;
        assert_eq!(
            rebase(&r.git, &r.path, &r.ctx, &req).unwrap().status,
            RebaseStatus::Completed
        );
    }

    #[test]
    fn interactive_only_counts_commits_it_rewrites_as_pushed() {
        let dir = tempfile::tempdir().unwrap();
        let r = TestRepo::init_at(&dir.path().join("work"));
        r.git(&["config", "core.autocrlf", "false"]);
        r.git
            .run(
                dir.path(),
                &r.ctx,
                ["init", "-q", "--bare", "-b", "main", "remote.git"],
            )
            .unwrap();
        let url = dir
            .path()
            .join("remote.git")
            .to_string_lossy()
            .replace('\\', "/");
        r.git(&["remote", "add", "origin", &url]);
        let base = r.head();
        let a = r.commit("a.txt", "a\n", "pushed a");
        r.git(&["push", "-q", "origin", "main"]);
        let b = r.commit("b.txt", "b\n", "local b");
        let c = r.commit("c.txt", "c\n", "local c");
        // Only b and c change: allowed without confirmation.
        let req = irb(
            &base,
            vec![
                step(&a, TodoAction::Pick),
                step(&c, TodoAction::Pick),
                step(&b, TodoAction::Pick),
            ],
        );
        let out = rebase_interactive(&r.git, &r.path, &r.ctx, &req).unwrap();
        assert_eq!(out.status, RebaseStatus::Completed);
        assert_eq!(
            r.git(&["rev-parse", "HEAD~2"]),
            a,
            "pushed commit kept as is"
        );
        // Touching the pushed commit needs the confirmation.
        let p = todo(&r.git, &r.path, &base).unwrap();
        let steps: Vec<TodoStep> = vec![
            with_msg(&p.commits[0].oid, TodoAction::Reword, "A"),
            step(&p.commits[1].oid, TodoAction::Pick),
            step(&p.commits[2].oid, TodoAction::Pick),
        ];
        let mut req = irb(&base, steps);
        let err = rebase_interactive(&r.git, &r.path, &r.ctx, &req).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::PushedCommits));
        req.allow_pushed = true;
        let out = rebase_interactive(&r.git, &r.path, &r.ctx, &req).unwrap();
        assert_eq!(out.status, RebaseStatus::Completed);
    }

    #[test]
    fn interactive_reorder_drop_fixup_and_reword() {
        let (r, base, [a, b, c]) = linear();
        let p = todo(&r.git, &r.path, &base).unwrap();
        assert_eq!(p.total, 3);
        assert_eq!(p.commits[0].oid, a);
        // Unchanged plan: nothing happens.
        let same = irb(
            &base,
            vec![
                step(&a, TodoAction::Pick),
                step(&b, TodoAction::Pick),
                step(&c, TodoAction::Pick),
            ],
        );
        let out = rebase_interactive(&r.git, &r.path, &r.ctx, &same).unwrap();
        assert_eq!(out.status, RebaseStatus::UpToDate);
        assert_eq!(r.head(), c);

        let req = irb(
            &base,
            vec![
                with_msg(
                    &c[..8],
                    TodoAction::Reword,
                    "C reworded\n\nwith body\r\n# kept line",
                ),
                step(&a, TodoAction::Pick),
                step(&b, TodoAction::Fixup),
            ],
        );
        let out = rebase_interactive(&r.git, &r.path, &r.ctx, &req).unwrap();
        assert_eq!(out.status, RebaseStatus::Completed, "{out:?}");
        assert_eq!(r.git(&["rev-parse", out.backup_ref.as_ref().unwrap()]), c);
        assert_eq!(
            subjects(&r, &format!("{base}..HEAD")),
            ["C reworded", "add a"]
        );
        assert_eq!(
            r.git(&["log", "-1", "--format=%B", "HEAD~1"]),
            "C reworded\n\nwith body\n# kept line"
        );
        // fixup folded b into a.
        assert_eq!(
            r.git(&["show", "--name-only", "--format=", "HEAD"]),
            "a.txt\nb.txt"
        );
        assert_eq!(r.read("b.txt"), "b\n");

        // Drop.
        let p = todo(&r.git, &r.path, &base).unwrap();
        let req = irb(
            &base,
            vec![
                step(&p.commits[0].oid, TodoAction::Drop),
                step(&p.commits[1].oid, TodoAction::Pick),
            ],
        );
        rebase_interactive(&r.git, &r.path, &r.ctx, &req).unwrap();
        assert_eq!(subjects(&r, &format!("{base}..HEAD")), ["add a"]);
        assert!(!r.path.join("c.txt").exists());
    }

    #[test]
    fn interactive_rebase_is_one_undo_step() {
        let (r, base, [a, b, c]) = linear();
        // Journal starts before the rebase (as in the app, which reads it on every refresh).
        crate::undo::state(&r.git, &r.path).unwrap();
        let req = irb(
            &base,
            vec![
                with_msg(&a, TodoAction::Reword, "A!"),
                step(&c, TodoAction::Pick),
                step(&b, TodoAction::Squash),
            ],
        );
        let out = rebase_interactive(&r.git, &r.path, &r.ctx, &req).unwrap();
        assert_eq!(out.status, RebaseStatus::Completed, "{out:?}");
        let s = crate::undo::state(&r.git, &r.path).unwrap();
        assert_eq!(
            s.undo.map(|s| s.op),
            Some(crate::undo::UndoKind::Rebase {
                branch: "topic".into()
            })
        );
        crate::undo::undo(&r.git, &r.path).unwrap();
        assert_eq!(r.head(), c);
        assert_eq!(
            subjects(&r, &format!("{base}..HEAD")),
            ["add a", "add b", "add c"]
        );
    }

    #[test]
    fn interactive_squash_with_message() {
        let (r, base, [a, b, c]) = linear();
        let req = irb(
            &base,
            vec![
                step(&a, TodoAction::Pick),
                with_msg(&b, TodoAction::Squash, "a + b together"),
                step(&c, TodoAction::Squash),
            ],
        );
        let out = rebase_interactive(&r.git, &r.path, &r.ctx, &req).unwrap();
        assert_eq!(out.status, RebaseStatus::Completed, "{out:?}");
        assert_eq!(
            r.git(&["rev-list", "--count", &format!("{base}..HEAD")]),
            "1"
        );
        // c squashed without a message: git's combination of both messages, no comments.
        let msg = r.git(&["log", "-1", "--format=%B"]);
        assert!(msg.starts_with("a + b together"), "{msg}");
        assert!(msg.contains("add c") && !msg.contains('#'), "{msg}");
    }

    #[test]
    fn interactive_edit_stops_then_continues() {
        let (r, base, [a, b, c]) = linear();
        let req = irb(
            &base,
            vec![
                step(&a, TodoAction::Pick),
                step(&b, TodoAction::Edit),
                step(&c, TodoAction::Pick),
            ],
        );
        let out = rebase_interactive(&r.git, &r.path, &r.ctx, &req).unwrap();
        assert_eq!(out.status, RebaseStatus::Stopped, "{out:?}");
        assert_eq!(out.step, Some(2));
        let s = operation_state(&r.git, &r.path).unwrap();
        assert_eq!(s.operation, Some(RepoOperation::Rebase));
        assert_eq!(r.git(&["log", "-1", "--format=%s"]), "add b");
        // Amend while stopped, then continue.
        r.write("b.txt", "b edited\n");
        r.git(&["add", "b.txt"]);
        r.git(&["commit", "-q", "--amend", "--no-edit"]);
        let out = continue_rebase(&r.git, &r.path, &r.ctx).unwrap();
        assert_eq!(out.status, RebaseStatus::Completed, "{out:?}");
        assert_eq!(
            subjects(&r, &format!("{base}..HEAD")),
            ["add a", "add b", "add c"]
        );
        assert_eq!(r.read("b.txt"), "b edited\n");

        // Edit with a new message: applies the message, then stops (break).
        let p = todo(&r.git, &r.path, &base).unwrap();
        let req = irb(
            &base,
            vec![
                with_msg(&p.commits[0].oid, TodoAction::Edit, "A, edited message"),
                step(&p.commits[1].oid, TodoAction::Pick),
                step(&p.commits[2].oid, TodoAction::Pick),
            ],
        );
        let out = rebase_interactive(&r.git, &r.path, &r.ctx, &req).unwrap();
        assert_eq!(out.status, RebaseStatus::Stopped, "{out:?}");
        assert_eq!(r.git(&["log", "-1", "--format=%s"]), "A, edited message");
        let out = continue_rebase(&r.git, &r.path, &r.ctx).unwrap();
        assert_eq!(out.status, RebaseStatus::Completed);
        assert_eq!(
            subjects(&r, &format!("{base}..HEAD")),
            ["A, edited message", "add b", "add c"]
        );
    }

    #[test]
    fn interactive_conflict_then_abort_restores_everything() {
        let r = repo();
        let base = r.commit("f.txt", "1\n", "base");
        r.git(&["switch", "-q", "-c", "topic"]);
        let a = r.commit("f.txt", "2\n", "two");
        let b = r.commit("f.txt", "3\n", "three");
        r.write("untracked.txt", "keep me\n");
        // Swapping two edits of the same line conflicts.
        let req = irb(
            &base,
            vec![step(&b, TodoAction::Pick), step(&a, TodoAction::Pick)],
        );
        let out = rebase_interactive(&r.git, &r.path, &r.ctx, &req).unwrap();
        assert_eq!(out.status, RebaseStatus::Conflicts, "{out:?}");
        assert_eq!(out.conflicts, vec!["f.txt".to_owned()]);
        abort_operation(&r.git, &r.path, &r.ctx).unwrap();
        assert_eq!(r.head(), b);
        assert_eq!(r.git(&["symbolic-ref", "--short", "HEAD"]), "topic");
        assert_eq!(r.read("f.txt"), "3\n");
        assert_eq!(r.read("untracked.txt"), "keep me\n");
        assert_eq!(
            r.git(&["status", "--porcelain", "--untracked-files=no"]),
            ""
        );

        // Resolve variant: conflict on the reword step keeps the new message.
        let req = irb(
            &base,
            vec![
                step(&b, TodoAction::Pick),
                with_msg(&a, TodoAction::Reword, "two (reworded)"),
            ],
        );
        let out = rebase_interactive(&r.git, &r.path, &r.ctx, &req).unwrap();
        assert_eq!(out.status, RebaseStatus::Conflicts);
        r.write("f.txt", "3\n");
        r.git(&["add", "f.txt"]);
        let out = continue_rebase(&r.git, &r.path, &r.ctx).unwrap();
        // Resolving to the original content makes the first pick produce the same tree, the
        // second one may become empty or conflict again: either way git stops or finishes.
        let mut guard = 0;
        let mut out = out;
        while out.status != RebaseStatus::Completed && guard < 5 {
            guard += 1;
            if !out.conflicts.is_empty() {
                r.write("f.txt", "2\n");
                r.git(&["add", "f.txt"]);
                out = continue_rebase(&r.git, &r.path, &r.ctx).unwrap();
            } else {
                out = skip_rebase(&r.git, &r.path, &r.ctx).unwrap();
            }
        }
        assert_eq!(out.status, RebaseStatus::Completed, "{out:?}");
        assert!(
            r.git(&["log", "--format=%s", &format!("{base}..HEAD")])
                .contains("two (reworded)")
        );
    }

    #[test]
    fn interactive_validation() {
        let (r, base, [a, b, c]) = linear();
        let check = |steps: Vec<TodoStep>| {
            rebase_interactive(&r.git, &r.path, &r.ctx, &irb(&base, steps))
                .unwrap_err()
                .kind()
        };
        let inv = Some(FailureKind::InvalidInput);
        // Missing / duplicate / foreign commits.
        assert_eq!(check(vec![step(&a, TodoAction::Pick)]), inv);
        assert_eq!(
            check(vec![
                step(&a, TodoAction::Pick),
                step(&a, TodoAction::Pick),
                step(&c, TodoAction::Pick)
            ]),
            inv
        );
        assert_eq!(
            check(vec![
                step(&base, TodoAction::Pick),
                step(&b, TodoAction::Pick),
                step(&c, TodoAction::Pick)
            ]),
            inv
        );
        assert_eq!(
            check(vec![
                step("--exec=x", TodoAction::Pick),
                step(&b, TodoAction::Pick),
                step(&c, TodoAction::Pick)
            ]),
            inv
        );
        // Squash first, reword without message.
        assert_eq!(
            check(vec![
                step(&a, TodoAction::Drop),
                step(&b, TodoAction::Squash),
                step(&c, TodoAction::Pick)
            ]),
            inv
        );
        assert_eq!(
            check(vec![
                with_msg(&a, TodoAction::Reword, "  "),
                step(&b, TodoAction::Pick),
                step(&c, TodoAction::Pick)
            ]),
            inv
        );
        assert_eq!(r.head(), c);
        assert!(
            crate::guard::list_backups(&r.git, &r.path)
                .unwrap()
                .is_empty()
        );
        // Base must be below HEAD; another branch means "since we diverged".
        assert!(todo(&r.git, &r.path, &c).is_err());
        r.git(&["switch", "-q", "-c", "side", &base]);
        let side = r.commit("s.txt", "s\n", "side");
        r.git(&["switch", "-q", "topic"]);
        let p = todo(&r.git, &r.path, &side).unwrap();
        assert_eq!(p.onto, base);
        assert_eq!(p.total, 3);
        // A branch that already contains HEAD: nothing to edit.
        r.git(&["branch", "ahead", "topic"]);
        assert!(todo(&r.git, &r.path, "ahead").is_err());
    }

    #[test]
    fn interactive_refuses_merges_in_range() {
        let r = repo();
        let base = r.head();
        r.git(&["switch", "-q", "-c", "side"]);
        r.commit("s.txt", "s\n", "side");
        r.git(&["switch", "-q", "main"]);
        r.commit("m.txt", "m\n", "main");
        r.git(&["merge", "-q", "--no-ff", "-m", "merge side", "side"]);
        let p = todo(&r.git, &r.path, &base).unwrap();
        assert_eq!(p.merges, 1);
        assert!(p.commits.iter().any(|c| c.is_merge));
        let steps = p
            .commits
            .iter()
            .map(|c| step(&c.oid, TodoAction::Pick))
            .collect();
        let err = rebase_interactive(&r.git, &r.path, &r.ctx, &irb(&base, steps)).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::InvalidInput));
        assert!(err.to_string().contains("merge"));
    }

    #[test]
    fn works_in_a_path_with_spaces_and_unicode() {
        let dir = tempfile::tempdir().unwrap();
        let r = TestRepo::init_at(&dir.path().join("thư mục có dấu cách"));
        r.git(&["config", "core.autocrlf", "false"]);
        let base = r.head();
        let a = r.commit("a.txt", "a\n", "add a");
        let b = r.commit("b.txt", "b\n", "add b");
        let req = irb(
            &base,
            vec![
                with_msg(&b, TodoAction::Reword, "B — tiếng Việt có dấu"),
                step(&a, TodoAction::Pick),
            ],
        );
        let out = rebase_interactive(&r.git, &r.path, &r.ctx, &req).unwrap();
        assert_eq!(out.status, RebaseStatus::Completed, "{out:?}");
        assert_eq!(
            subjects(&r, &format!("{base}..HEAD")),
            ["B — tiếng Việt có dấu", "add a"]
        );
    }

    #[test]
    fn relation_and_fast_forward() {
        let r = repo();
        r.git(&["branch", "behind"]);
        r.git(&["switch", "-q", "-c", "ahead"]);
        let tip = r.commit("x.txt", "x\n", "x");
        r.git(&["switch", "-q", "main"]);
        let rel = branch_relation(&r.git, &r.path, "ahead", "behind").unwrap();
        assert_eq!((rel.ahead, rel.behind, rel.can_fast_forward), (1, 0, true));
        assert_eq!(rel.current.as_deref(), Some("main"));
        // Not checked out: update-ref.
        assert_eq!(
            fast_forward_branch(&r.git, &r.path, &r.ctx, "behind", "ahead").unwrap(),
            tip
        );
        assert_eq!(r.git(&["rev-parse", "behind"]), tip);
        // Checked out: merge --ff-only.
        fast_forward_branch(&r.git, &r.path, &r.ctx, "main", "ahead").unwrap();
        assert_eq!(r.head(), tip);
        assert_eq!(r.read("x.txt"), "x\n");
        // Diverged: refused.
        r.commit("y.txt", "y\n", "y");
        let err = fast_forward_branch(&r.git, &r.path, &r.ctx, "ahead", "behind").unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::NonFastForward));
        assert!(branch_relation(&r.git, &r.path, "-x", "main").is_err());
    }
}
