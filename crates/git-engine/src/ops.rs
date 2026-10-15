//! Commit-level write operations (the commit context menu): tags, cherry-pick, revert, reset,
//! plus detection, abort and continue of an in-progress merge / rebase / cherry-pick / revert.
//!
//! Also home of the error model shared by the write modules ([`branch`](crate::branch),
//! [`merge`](crate::merge), [`remote`](crate::remote)): failures git reports in a recognisable
//! way become [`OpError::Rejected`] with a [`FailureKind`] the UI can act on (offer a force
//! delete, a stash, a pull…). Conflicts are not errors: they come back as structured outcomes.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::commit::is_valid_oid;
use crate::{AccountContext, GitError, GitRunner};

/// Recognisable reasons a write operation was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum FailureKind {
    /// Branch / tag / remote name rejected by `git check-ref-format` or our own checks.
    InvalidName,
    /// Other invalid argument (malformed commit id, empty tag message…).
    InvalidInput,
    /// Branch, tag, remote, commit or remote repository does not exist.
    NotFound,
    AlreadyExists,
    /// The branch is checked out (here or in another worktree).
    CurrentBranch,
    /// `git branch -d` refused: the branch has commits not merged anywhere (use force).
    NotFullyMerged,
    /// Local modifications would be overwritten (checkout, merge, cherry-pick, pull…).
    LocalChanges,
    /// Untracked files would be overwritten.
    UntrackedFiles,
    /// The index has unresolved conflicts.
    UnmergedFiles,
    /// A merge / rebase / cherry-pick / revert is already in progress.
    OperationInProgress,
    /// Abort / continue requested but nothing is in progress.
    NoOperation,
    /// The change is already present (empty cherry-pick) or there is nothing to commit.
    NothingToCommit,
    /// Push rejected because the remote has new commits, or a fast-forward is impossible.
    NonFastForward,
    /// `--force-with-lease` refused: the remote branch moved since the last fetch.
    StaleLease,
    /// Refused by the server (hook declined, protected branch…).
    RemoteRejected,
    AuthFailed,
    ConnectionFailed,
    /// The branch has no upstream configured.
    NoUpstream,
    /// The operation needs a branch but HEAD is detached.
    DetachedHead,
    /// Signed in, but the account may not read/write this repository (HTTP 403).
    PermissionDenied,
    /// GitHub refused a change under `.github/workflows/` because the token lacks the
    /// `workflow` scope.
    WorkflowScope,
    /// The branch is protected (no direct / force push, required reviews, rulesets).
    ProtectedBranch,
    /// GitHub "Block command line pushes that expose my email" (GH007).
    EmailPrivacy,
    /// Push protection found a secret (token, key) in the pushed commits.
    SecretDetected,
    /// A pushed file exceeds the server's size limit (GitHub GH001: 100 MB).
    FileTooLarge,
    /// The branch is checked out in another (linked) worktree, so it cannot be checked out
    /// here too.
    CheckedOutElsewhere,
    /// The worktree / submodule folder still has modified or untracked files (needs force).
    DirtyWorktree,
    /// The linked worktree is locked (`git worktree lock`); unlock it first.
    WorktreeLocked,
    /// GPG / SSH signing failed (missing key, gpg not installed, passphrase refused…).
    SigningFailed,
    /// A rebase / history rewrite would change commits that are already on a remote; the
    /// user has to confirm explicitly (only local history changes).
    PushedCommits,
}

/// Error of a write operation.
#[derive(Debug, thiserror::Error)]
pub enum OpError {
    /// Refused for a recognisable reason; `message` is git's stderr (or our own explanation).
    #[error("{message}")]
    Rejected { kind: FailureKind, message: String },
    /// Stopped through a [`CancelToken`](crate::progress::CancelToken).
    #[error("operation cancelled")]
    Cancelled,
    /// Any other failure.
    #[error(transparent)]
    Git(#[from] GitError),
}

impl OpError {
    pub fn rejected(kind: FailureKind, message: impl Into<String>) -> Self {
        Self::Rejected {
            kind,
            message: message.into(),
        }
    }

    /// The failure kind, if recognised.
    pub fn kind(&self) -> Option<FailureKind> {
        match self {
            Self::Rejected { kind, .. } => Some(*kind),
            _ => None,
        }
    }
}

/// Recognises a failure from git's (English) stderr.
pub fn failure_kind(stderr: &str) -> Option<FailureKind> {
    use FailureKind::*;
    let s = stderr.to_ascii_lowercase();
    let has = |p: &str| s.contains(p);
    let any = |ps: &[&str]| ps.iter().any(|p| s.contains(p));
    Some(if has("(stale info)") {
        StaleLease
    } else if any(&[
        "failed to sign the data",
        "signing failed",
        "couldn't load public key",
        "ssh-keygen -y sign is needed",
    ]) {
        SigningFailed
    } else if any(&["is already used by worktree", "is already checked out at"]) {
        CheckedOutElsewhere
    } else if any(&[
        "contains modified or untracked files",
        "contains local modifications",
    ]) {
        DirtyWorktree
    } else if has("locked working tree") {
        WorktreeLocked
    } else if has("workflow") && any(&["without `workflow` scope", "without 'workflow' scope"]) {
        WorkflowScope
    } else if any(&[
        "gh001: large files detected",
        "exceeds github's file size limit",
    ]) {
        FileTooLarge
    } else if any(&[
        "push cannot contain secrets",
        "gh013: secret",
        "secret scanning",
        "push protection",
    ]) {
        SecretDetected
    } else if any(&["gh007", "push declined due to email privacy"]) {
        EmailPrivacy
    } else if any(&[
        "gh006: protected branch",
        "protected branch hook declined",
        "not allowed to push code to protected branches",
        "not allowed to force push code to a protected branch",
        "gh013: repository rule violations",
        "cannot force-push to this branch",
    ]) {
        ProtectedBranch
    } else if has("[remote rejected]") {
        RemoteRejected
    } else if any(&[
        "[rejected]",
        "non-fast-forward",
        "(fetch first)",
        "not possible to fast-forward",
        "diverging branches can't be fast-forwarded",
    ]) {
        NonFastForward
    } else if has("not fully merged") {
        NotFullyMerged
    } else if has("untracked working tree files would be") {
        UntrackedFiles
    } else if any(&[
        "would be overwritten by",
        "please commit your changes or stash them",
        "you have unstaged changes",
        "your index contains uncommitted changes",
    ]) {
        LocalChanges
    } else if any(&[
        "not concluded your merge",
        "already in progress",
        "cannot switch branch while",
        "in the middle of",
    ]) {
        OperationInProgress
    } else if any(&[
        "unmerged files",
        "unmerged paths",
        "needs merge",
        "resolve your current index first",
        "fix conflicts",
    ]) {
        UnmergedFiles
    } else if any(&[
        "no merge to abort",
        "no cherry-pick or revert in progress",
        "no cherry-pick in progress",
        "no revert in progress",
        "no rebase in progress",
        "not bisecting",
    ]) {
        NoOperation
    } else if any(&[
        "nothing to commit",
        "is now empty",
        "nothing added to commit",
    ]) {
        NothingToCommit
    } else if any(&[
        "returned error: 403",
        "you do not have access",
        "permission to",
        "access denied",
        "403 forbidden",
    ]) {
        PermissionDenied
    } else if any(&[
        "returned error: 401",
        "authentication failed",
        "could not read username",
        "could not read password",
        "permission denied (publickey",
        "terminal prompts disabled",
        "invalid username or password",
        "invalid credentials",
    ]) {
        AuthFailed
    } else if any(&[
        "does not appear to be a git repository",
        "repository not found",
        "couldn't find remote ref",
        "remote ref does not exist",
    ]) {
        NotFound
    } else if any(&[
        "could not resolve host",
        "failed to connect",
        "connection timed out",
        "connection refused",
        "unable to access",
        "could not read from remote repository",
        "the remote end hung up",
    ]) {
        ConnectionFailed
    } else if any(&[
        "no upstream branch",
        "no tracking information",
        "no upstream configured",
        "no upstream information",
    ]) {
        NoUpstream
    } else if (has("cannot delete branch") && has("checked out")) || has("used by worktree") {
        CurrentBranch
    } else if has("already exists") {
        AlreadyExists
    } else if any(&[
        "not a valid branch name",
        "not a valid tag name",
        "not a valid ref",
    ]) {
        InvalidName
    } else if any(&[
        "not a commit",
        "unknown revision",
        "invalid reference",
        "bad revision",
        "not a valid object name",
        "did not match any",
        "not found",
    ]) {
        NotFound
    } else if has("not currently on a branch") {
        DetachedHead
    } else {
        return None;
    })
}

/// Turns a failed git command into [`OpError::Rejected`] when its stderr is recognisable.
pub fn classify(err: GitError) -> OpError {
    if let GitError::Failed { stderr, .. } = &err
        && let Some(kind) = failure_kind(stderr)
    {
        return OpError::Rejected {
            kind,
            message: stderr.clone(),
        };
    }
    OpError::Git(err)
}

/// Runner for write commands: English messages (so [`failure_kind`] can recognise them) and
/// no interactive editor (`GIT_EDITOR=:` keeps the prepared message).
pub(crate) fn writer(git: &GitRunner) -> GitRunner {
    git.clone()
        .with_env("LC_ALL", "en_US.UTF-8")
        .with_env("LANG", "en_US.UTF-8")
        .with_env("LANGUAGE", "en")
        .with_env("GIT_EDITOR", ":")
        .with_env("GIT_MERGE_AUTOEDIT", "no")
}

/// Runs a write command and returns trimmed stdout; failures are classified.
pub(crate) fn run<S: AsRef<std::ffi::OsStr>>(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    args: &[S],
) -> Result<String, OpError> {
    writer(git).run_text(repo, ctx, args).map_err(classify)
}

/// Rejects empty values, values that git could read as an option and control characters.
pub(crate) fn check_arg(value: &str, what: &str) -> Result<(), OpError> {
    if value.is_empty() || value.starts_with('-') || value.chars().any(char::is_control) {
        return Err(OpError::rejected(
            FailureKind::InvalidName,
            format!("invalid {what}: {value:?}"),
        ));
    }
    Ok(())
}

/// Whether a fully qualified ref (`refs/heads/x`) exists.
pub(crate) fn ref_exists(git: &GitRunner, repo: &Path, full_ref: &str) -> bool {
    full_ref.starts_with("refs/")
        && git
            .run(
                repo,
                &AccountContext::default(),
                ["show-ref", "--verify", "--quiet", full_ref],
            )
            .is_ok()
}

/// Current HEAD commit, `None` on an unborn branch.
pub fn head_oid(git: &GitRunner, repo: &Path) -> Option<String> {
    git.run_text(
        repo,
        &AccountContext::default(),
        ["rev-parse", "-q", "--verify", "HEAD"],
    )
    .ok()
    .filter(|s| !s.is_empty())
}

/// Checked-out branch, `None` when HEAD is detached.
pub fn current_branch(git: &GitRunner, repo: &Path) -> Option<String> {
    git.run_text(
        repo,
        &AccountContext::default(),
        ["symbolic-ref", "-q", "--short", "HEAD"],
    )
    .ok()
    .filter(|s| !s.is_empty())
}

/// Resolves any revision (branch, remote branch, tag, oid, `HEAD~2`) to a full commit id.
pub fn resolve_commit(git: &GitRunner, repo: &Path, rev: &str) -> Result<String, OpError> {
    check_arg(rev, "revision")?;
    git.run_text(
        repo,
        &AccountContext::default(),
        [
            "rev-parse",
            "-q",
            "--verify",
            "--end-of-options",
            &format!("{rev}^{{commit}}"),
        ],
    )
    .ok()
    .filter(|s| !s.is_empty())
    .ok_or_else(|| OpError::rejected(FailureKind::NotFound, format!("unknown revision: {rev}")))
}

/// Validates a commit id from the UI and returns `(full id, parents)`.
fn commit_id(git: &GitRunner, repo: &Path, oid: &str) -> Result<(String, Vec<String>), OpError> {
    if !is_valid_oid(oid) {
        return Err(OpError::rejected(
            FailureKind::InvalidInput,
            format!("invalid commit id: {oid:?}"),
        ));
    }
    let full = resolve_commit(git, repo, oid)?;
    let line = git.run_text(
        repo,
        &AccountContext::default(),
        ["rev-list", "--parents", "-n", "1", &full, "--"],
    )?;
    let parents = line.split_whitespace().skip(1).map(str::to_owned).collect();
    Ok((full, parents))
}

/// Absolute git directory (per worktree: that is where MERGE_HEAD & co. live).
pub fn git_dir(git: &GitRunner, repo: &Path) -> Result<PathBuf, OpError> {
    Ok(PathBuf::from(git.run_text(
        repo,
        &AccountContext::default(),
        ["rev-parse", "--absolute-git-dir"],
    )?))
}

/// Paths with unresolved conflicts (`git diff --name-only --diff-filter=U -z`).
pub fn conflicted_paths(git: &GitRunner, repo: &Path) -> Result<Vec<String>, OpError> {
    let out = git.run(
        repo,
        &AccountContext::default(),
        ["diff", "--name-only", "--diff-filter=U", "-z"],
    )?;
    let mut paths: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    paths.dedup();
    Ok(paths)
}

/// Multi-step operation the repository is in the middle of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum RepoOperation {
    Merge,
    Rebase,
    CherryPick,
    Revert,
    Bisect,
    /// `git am` (patch series).
    ApplyMailbox,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct OperationState {
    pub operation: Option<RepoOperation>,
    /// Unmerged paths (also filled after a conflicting `merge --squash`, which has no state).
    pub conflicts: Vec<String>,
    /// Rebase / am progress: current step (1-based) and total.
    pub step: Option<u32>,
    pub total_steps: Option<u32>,
}

fn read_number(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn detect_operation(dir: &Path) -> (Option<RepoOperation>, Option<u32>, Option<u32>) {
    let merge_dir = dir.join("rebase-merge");
    if merge_dir.is_dir() {
        return (
            Some(RepoOperation::Rebase),
            read_number(&merge_dir.join("msgnum")),
            read_number(&merge_dir.join("end")),
        );
    }
    let apply_dir = dir.join("rebase-apply");
    if apply_dir.is_dir() {
        let op = if apply_dir.join("applying").exists() {
            RepoOperation::ApplyMailbox
        } else {
            RepoOperation::Rebase
        };
        return (
            Some(op),
            read_number(&apply_dir.join("next")),
            read_number(&apply_dir.join("last")),
        );
    }
    let op = if dir.join("MERGE_HEAD").exists() {
        Some(RepoOperation::Merge)
    } else if dir.join("CHERRY_PICK_HEAD").exists() {
        Some(RepoOperation::CherryPick)
    } else if dir.join("REVERT_HEAD").exists() {
        Some(RepoOperation::Revert)
    } else if dir.join("BISECT_LOG").exists() {
        Some(RepoOperation::Bisect)
    } else {
        None
    };
    (op, None, None)
}

/// The in-progress operation alone (cheap: only checks files in the git dir).
pub fn current_operation(git: &GitRunner, repo: &Path) -> Result<Option<RepoOperation>, OpError> {
    Ok(detect_operation(&git_dir(git, repo)?).0)
}

/// In-progress operation, its progress and the conflicted paths.
pub fn operation_state(git: &GitRunner, repo: &Path) -> Result<OperationState, OpError> {
    let (operation, step, total_steps) = detect_operation(&git_dir(git, repo)?);
    Ok(OperationState {
        operation,
        conflicts: conflicted_paths(git, repo)?,
        step,
        total_steps,
    })
}

/// Refuses to start a new operation while another one is in progress.
pub(crate) fn ensure_idle(git: &GitRunner, repo: &Path) -> Result<(), OpError> {
    match current_operation(git, repo)? {
        None => Ok(()),
        Some(op) => Err(OpError::rejected(
            FailureKind::OperationInProgress,
            format!("a {op:?} is in progress; continue or abort it first"),
        )),
    }
}

/// Result of an operation that can stop on conflicts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct OpOutcome {
    /// Unmerged paths; empty when the operation went through.
    pub conflicts: Vec<String>,
    /// Operation left in progress (resolve, then continue or abort); `None` when finished.
    pub pending: Option<RepoOperation>,
    /// HEAD afterwards.
    pub head: Option<String>,
}

fn finished(git: &GitRunner, repo: &Path) -> Result<OpOutcome, OpError> {
    Ok(OpOutcome {
        conflicts: Vec::new(),
        pending: current_operation(git, repo)?,
        head: head_oid(git, repo),
    })
}

/// After a failed command: conflicts become an outcome, anything else is classified.
pub(crate) fn conflicts_or(
    git: &GitRunner,
    repo: &Path,
    err: GitError,
) -> Result<OpOutcome, OpError> {
    let state = operation_state(git, repo)?;
    if !state.conflicts.is_empty() {
        return Ok(OpOutcome {
            conflicts: state.conflicts,
            pending: state.operation,
            head: head_oid(git, repo),
        });
    }
    Err(classify(err))
}

/// `git check-ref-format refs/tags/<name>`.
pub fn validate_tag_name(git: &GitRunner, repo: &Path, name: &str) -> Result<(), OpError> {
    check_arg(name, "tag name")?;
    git.run(
        repo,
        &AccountContext::default(),
        ["check-ref-format", &format!("refs/tags/{name}")],
    )
    .map(|_| ())
    .map_err(|_| {
        OpError::rejected(
            FailureKind::InvalidName,
            format!("invalid tag name: {name}"),
        )
    })
}

/// Creates tag `name` at commit `oid`: annotated when `message` is given, lightweight otherwise.
pub fn create_tag(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    name: &str,
    oid: &str,
    message: Option<&str>,
) -> Result<(), OpError> {
    validate_tag_name(git, repo, name)?;
    let (full, _) = commit_id(git, repo, oid)?;
    if ref_exists(git, repo, &format!("refs/tags/{name}")) {
        return Err(OpError::rejected(
            FailureKind::AlreadyExists,
            format!("tag '{name}' already exists"),
        ));
    }
    let mut args: Vec<&str> = vec!["tag"];
    if let Some(message) = message {
        if message.trim().is_empty() {
            return Err(OpError::rejected(
                FailureKind::InvalidInput,
                "an annotated tag needs a message",
            ));
        }
        // `-m` always consumes the next argument, even one starting with `-`. Identities with
        // signing sign their annotated tags too (`-s` uses the injected `user.signingkey`).
        let annotate = if ctx.signing.is_some() { "-s" } else { "-a" };
        args.extend([annotate, "-m", message]);
    }
    args.extend(["--end-of-options", name, &full]);
    run(git, repo, ctx, &args).map(|_| ())
}

pub fn delete_tag(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    name: &str,
) -> Result<(), OpError> {
    validate_tag_name(git, repo, name)?;
    if !ref_exists(git, repo, &format!("refs/tags/{name}")) {
        return Err(OpError::rejected(
            FailureKind::NotFound,
            format!("tag '{name}' not found"),
        ));
    }
    run(git, repo, ctx, &["tag", "-d", "--end-of-options", name]).map(|_| ())
}

fn apply_commit(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    oid: &str,
    revert: bool,
) -> Result<OpOutcome, OpError> {
    ensure_idle(git, repo)?;
    let (full, parents) = commit_id(git, repo, oid)?;
    let command = if revert { "revert" } else { "cherry-pick" };
    let mut args = vec![command];
    if revert {
        args.push("--no-edit");
    }
    if parents.len() > 1 {
        // Merge commits are replayed relative to their first parent (the mainline).
        args.extend(["-m", "1"]);
    }
    args.push(&full);
    match writer(git).run(repo, ctx, &args) {
        Ok(_) => finished(git, repo),
        Err(err) => {
            let state = operation_state(git, repo)?;
            if state.conflicts.is_empty()
                && matches!(
                    state.operation,
                    Some(RepoOperation::CherryPick | RepoOperation::Revert)
                )
            {
                // Stopped without conflicts: the change is already present (empty pick).
                let _ = writer(git).run(repo, ctx, [command, "--abort"]);
                return Err(OpError::rejected(
                    FailureKind::NothingToCommit,
                    format!(
                        "{command}: the changes of {} are already present",
                        &full[..10]
                    ),
                ));
            }
            conflicts_or(git, repo, err)
        }
    }
}

/// Cherry-picks commit `oid` onto HEAD (merge commits against their first parent).
pub fn cherry_pick(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    oid: &str,
) -> Result<OpOutcome, OpError> {
    apply_commit(git, repo, ctx, oid, false)
}

/// Reverts commit `oid` with git's default message (merge commits against their first parent).
pub fn revert(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    oid: &str,
) -> Result<OpOutcome, OpError> {
    apply_commit(git, repo, ctx, oid, true)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ResetMode {
    /// Move the branch only; changes stay staged.
    Soft,
    /// Move the branch and reset the index; changes stay in the working tree.
    Mixed,
    /// Discard everything (index and working tree) — destructive.
    Hard,
}

/// Moves the current branch (or detached HEAD) to commit `oid`.
pub fn reset(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    oid: &str,
    mode: ResetMode,
) -> Result<(), OpError> {
    let (full, _) = commit_id(git, repo, oid)?;
    let flag = match mode {
        ResetMode::Soft => "--soft",
        ResetMode::Mixed => "--mixed",
        ResetMode::Hard => "--hard",
    };
    run(git, repo, ctx, &["reset", "-q", flag, &full, "--"]).map(|_| ())
}

/// Aborts the in-progress operation and returns which one it was.
pub fn abort_operation(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
) -> Result<RepoOperation, OpError> {
    let Some(op) = current_operation(git, repo)? else {
        return Err(OpError::rejected(
            FailureKind::NoOperation,
            "no operation in progress",
        ));
    };
    let args: &[&str] = match op {
        RepoOperation::Merge => &["merge", "--abort"],
        RepoOperation::Rebase => &["rebase", "--abort"],
        RepoOperation::CherryPick => &["cherry-pick", "--abort"],
        RepoOperation::Revert => &["revert", "--abort"],
        RepoOperation::Bisect => &["bisect", "reset"],
        RepoOperation::ApplyMailbox => &["am", "--abort"],
    };
    run(git, repo, ctx, args)?;
    Ok(op)
}

/// Continues the in-progress operation once its conflicts are resolved and staged. May stop
/// again on conflicts (next rebase step), reported in the outcome.
pub fn continue_operation(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
) -> Result<OpOutcome, OpError> {
    let state = operation_state(git, repo)?;
    let Some(op) = state.operation else {
        return Err(OpError::rejected(
            FailureKind::NoOperation,
            "no operation in progress",
        ));
    };
    if !state.conflicts.is_empty() {
        return Err(OpError::rejected(
            FailureKind::UnmergedFiles,
            format!(
                "resolve and stage {} conflicted file(s) first",
                state.conflicts.len()
            ),
        ));
    }
    let args: &[&str] = match op {
        RepoOperation::Merge => &["commit", "--no-edit"],
        RepoOperation::Rebase => &["rebase", "--continue"],
        RepoOperation::CherryPick => &["cherry-pick", "--continue"],
        RepoOperation::Revert => &["revert", "--continue"],
        RepoOperation::ApplyMailbox => &["am", "--continue"],
        RepoOperation::Bisect => {
            return Err(OpError::rejected(
                FailureKind::InvalidInput,
                "a bisect cannot be continued from here",
            ));
        }
    };
    match writer(git).run(repo, ctx, args) {
        Ok(_) => finished(git, repo),
        Err(err) => conflicts_or(git, repo, err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_status_failures_are_told_apart() {
        let forbidden = "remote: you do not have access to this repository\nfatal: unable to access 'http://127.0.0.1:1/a/b.git/': The requested URL returned error: 403";
        assert_eq!(failure_kind(forbidden), Some(FailureKind::PermissionDenied));
        let github = "remote: Permission to acme/app.git denied to bob.\nfatal: unable to access 'https://github.com/acme/app.git/': The requested URL returned error: 403";
        assert_eq!(failure_kind(github), Some(FailureKind::PermissionDenied));
        let unauthorized =
            "fatal: unable to access 'https://x/y.git/': The requested URL returned error: 401";
        assert_eq!(failure_kind(unauthorized), Some(FailureKind::AuthFailed));
        let offline = "fatal: unable to access 'https://x/y.git/': Could not resolve host: x";
        assert_eq!(failure_kind(offline), Some(FailureKind::ConnectionFailed));
    }
    use crate::progress::test_support::TestRepo;

    #[test]
    fn recognises_common_failures() {
        let cases = [
            (
                "error: the branch 'topic' is not fully merged\nhint: If you are sure you want to delete it, run 'git branch -D topic'",
                FailureKind::NotFullyMerged,
            ),
            (
                "error: Your local changes to the following files would be overwritten by checkout:\n\ta.txt\nPlease commit your changes or stash them before you switch branches.\nAborting",
                FailureKind::LocalChanges,
            ),
            (
                "error: The following untracked working tree files would be overwritten by checkout:\n\tb.txt",
                FailureKind::UntrackedFiles,
            ),
            (
                "To ../r.git\n ! [rejected]        main -> main (non-fast-forward)\nerror: failed to push some refs",
                FailureKind::NonFastForward,
            ),
            (
                " ! [rejected]        main -> main (fetch first)",
                FailureKind::NonFastForward,
            ),
            (
                " ! [rejected]        main -> main (stale info)",
                FailureKind::StaleLease,
            ),
            (
                " ! [remote rejected] main -> main (pre-receive hook declined)",
                FailureKind::RemoteRejected,
            ),
            (
                "To https://github.com/o/r.git\n ! [remote rejected] main -> main (refusing to allow an OAuth App to create or update workflow `.github/workflows/ci.yml` without `workflow` scope)\nerror: failed to push some refs",
                FailureKind::WorkflowScope,
            ),
            (
                "remote: error: GH001: Large files detected. You may want to try Git Large File Storage\nremote: error: File big.bin is 120.00 MB; this exceeds GitHub's file size limit of 100.00 MB\n ! [remote rejected] main -> main (pre-receive hook declined)",
                FailureKind::FileTooLarge,
            ),
            (
                "remote: error: GH013: Repository rule violations found for refs/heads/main.\nremote: - GITHUB PUSH PROTECTION\nremote:   Push cannot contain secrets\n ! [remote rejected] main -> main (push declined due to repository rule violations)",
                FailureKind::SecretDetected,
            ),
            (
                "remote: error: GH007: Your push would publish a private email address.\n ! [remote rejected] main -> main (push declined due to email privacy restrictions)",
                FailureKind::EmailPrivacy,
            ),
            (
                "remote: error: GH006: Protected branch update failed for refs/heads/main.\n ! [remote rejected] main -> main (protected branch hook declined)",
                FailureKind::ProtectedBranch,
            ),
            (
                "remote: GitLab: You are not allowed to push code to protected branches on this project.\n ! [remote rejected] main -> main (pre-receive hook declined)",
                FailureKind::ProtectedBranch,
            ),
            (
                "fatal: could not read Username for 'https://github.com': terminal prompts disabled",
                FailureKind::AuthFailed,
            ),
            (
                "fatal: unable to access 'https://nope.invalid/x.git/': Could not resolve host: nope.invalid",
                FailureKind::ConnectionFailed,
            ),
            (
                "fatal: 'nowhere' does not appear to be a git repository\nfatal: Could not read from remote repository.",
                FailureKind::NotFound,
            ),
            (
                "There is no tracking information for the current branch.",
                FailureKind::NoUpstream,
            ),
            (
                "fatal: a branch named 'x' already exists",
                FailureKind::AlreadyExists,
            ),
            (
                "fatal: Not possible to fast-forward, aborting.",
                FailureKind::NonFastForward,
            ),
            (
                "error: Cannot delete branch 'main' checked out at 'C:/r'",
                FailureKind::CurrentBranch,
            ),
            (
                "fatal: There is no merge to abort (MERGE_HEAD missing).",
                FailureKind::NoOperation,
            ),
            (
                "fatal: 'topic' is already used by worktree at 'C:/Temp/wt1b'",
                FailureKind::CheckedOutElsewhere,
            ),
            (
                "fatal: '../wt' contains modified or untracked files, use --force to delete it",
                FailureKind::DirtyWorktree,
            ),
            (
                "fatal: cannot remove a locked working tree, lock reason: usb\nuse 'remove -f -f' to override or unlock first",
                FailureKind::WorktreeLocked,
            ),
            (
                "error: gpg failed to sign the data:\ngpg: signing failed: No secret key\n\nfatal: failed to write commit object",
                FailureKind::SigningFailed,
            ),
            (
                "error: Couldn't load public key C:/k/nokey: No such file or directory?\n\nfatal: failed to write commit object",
                FailureKind::SigningFailed,
            ),
            (
                "error: Committing is not possible because you have unmerged files.",
                FailureKind::UnmergedFiles,
            ),
            (
                "fatal: You have not concluded your merge (MERGE_HEAD exists).",
                FailureKind::OperationInProgress,
            ),
        ];
        for (stderr, kind) in cases {
            assert_eq!(failure_kind(stderr), Some(kind), "{stderr}");
        }
        assert_eq!(failure_kind("fatal: something unexpected"), None);
    }

    #[test]
    fn creates_and_deletes_tags() {
        let r = TestRepo::new();
        let head = r.head();
        create_tag(&r.git, &r.path, &r.ctx, "v1", &head[..8], None).unwrap();
        create_tag(
            &r.git,
            &r.path,
            &r.ctx,
            "v2",
            &head,
            Some("--force release"),
        )
        .unwrap();
        assert_eq!(r.git(&["rev-parse", "v1^{commit}"]), head);
        assert_eq!(r.git(&["cat-file", "-t", "v2"]), "tag", "annotated");
        assert_eq!(
            r.git(&["tag", "-l", "--format=%(contents:subject)", "v2"]),
            "--force release"
        );
        let err = create_tag(&r.git, &r.path, &r.ctx, "v1", &head, None).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::AlreadyExists));
        let err = create_tag(&r.git, &r.path, &r.ctx, "v3", &head, Some("  ")).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::InvalidInput));
        for bad in ["--output=x", "-d", "bad..name", "a b", ""] {
            let err = create_tag(&r.git, &r.path, &r.ctx, bad, &head, None).unwrap_err();
            assert_eq!(err.kind(), Some(FailureKind::InvalidName), "{bad:?}");
        }
        let err = create_tag(&r.git, &r.path, &r.ctx, "v4", "HEAD", None).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::InvalidInput));
        assert!(!r.path.join("x").exists());

        delete_tag(&r.git, &r.path, &r.ctx, "v1").unwrap();
        assert!(!ref_exists(&r.git, &r.path, "refs/tags/v1"));
        let err = delete_tag(&r.git, &r.path, &r.ctx, "v1").unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::NotFound));
        let err = delete_tag(&r.git, &r.path, &r.ctx, "-D").unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::InvalidName));
    }

    #[test]
    fn cherry_pick_clean_conflict_abort_and_continue() {
        let r = TestRepo::new();
        r.git(&["switch", "-q", "-c", "topic"]);
        let clean = r.commit("t.txt", "topic\n", "topic file");
        let conflicting = r.commit("README.md", "topic readme\n", "topic readme");
        r.git(&["switch", "-q", "main"]);
        r.commit("README.md", "main readme\n", "main readme");

        let out = cherry_pick(&r.git, &r.path, &r.ctx, &clean).unwrap();
        assert!(out.conflicts.is_empty() && out.pending.is_none());
        assert_eq!(out.head, Some(r.head()));
        assert_eq!(r.read("t.txt"), "topic\n");

        // Same change again: nothing to apply, state cleaned up.
        let err = cherry_pick(&r.git, &r.path, &r.ctx, &clean).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::NothingToCommit));
        assert_eq!(current_operation(&r.git, &r.path).unwrap(), None);

        let before = r.head();
        let out = cherry_pick(&r.git, &r.path, &r.ctx, &conflicting).unwrap();
        assert_eq!(out.conflicts, vec!["README.md".to_owned()]);
        assert_eq!(out.pending, Some(RepoOperation::CherryPick));
        let state = operation_state(&r.git, &r.path).unwrap();
        assert_eq!(state.operation, Some(RepoOperation::CherryPick));

        // A second operation is refused while one is in progress.
        let err = revert(&r.git, &r.path, &r.ctx, &clean).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::OperationInProgress));
        // Continue with unresolved conflicts is refused.
        let err = continue_operation(&r.git, &r.path, &r.ctx).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::UnmergedFiles));

        assert_eq!(
            abort_operation(&r.git, &r.path, &r.ctx).unwrap(),
            RepoOperation::CherryPick
        );
        assert_eq!(r.head(), before);
        assert_eq!(r.read("README.md"), "main readme\n");
        let err = abort_operation(&r.git, &r.path, &r.ctx).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::NoOperation));

        // Resolve and continue.
        cherry_pick(&r.git, &r.path, &r.ctx, &conflicting).unwrap();
        r.write("README.md", "resolved\n");
        r.git(&["add", "README.md"]);
        let out = continue_operation(&r.git, &r.path, &r.ctx).unwrap();
        assert!(out.conflicts.is_empty() && out.pending.is_none());
        assert_eq!(r.git(&["log", "-1", "--format=%s"]), "topic readme");
        assert_eq!(r.read("README.md"), "resolved\n");
    }

    #[test]
    fn cherry_pick_and_revert_merge_commits_use_mainline() {
        let r = TestRepo::new();
        r.git(&["switch", "-q", "-c", "topic"]);
        r.commit("t.txt", "t\n", "topic");
        r.git(&["switch", "-q", "main"]);
        r.commit("m.txt", "m\n", "main");
        r.git(&["merge", "-q", "--no-ff", "-m", "merge topic", "topic"]);
        let merge = r.head();

        let out = revert(&r.git, &r.path, &r.ctx, &merge).unwrap();
        assert!(out.conflicts.is_empty());
        assert!(!r.path.join("t.txt").exists(), "topic change reverted");
        assert!(r.git(&["log", "-1", "--format=%s"]).starts_with("Revert"));

        r.git(&["switch", "-q", "-c", "other", "HEAD~2"]);
        let out = cherry_pick(&r.git, &r.path, &r.ctx, &merge).unwrap();
        assert!(out.conflicts.is_empty());
        assert_eq!(r.read("t.txt"), "t\n");
    }

    #[test]
    fn revert_conflict_reports_paths() {
        let r = TestRepo::new();
        let first = r.commit("a.txt", "one\n", "a1");
        r.commit("a.txt", "two\n", "a2");
        let out = revert(&r.git, &r.path, &r.ctx, &first).unwrap();
        assert_eq!(out.conflicts, vec!["a.txt".to_owned()]);
        assert_eq!(out.pending, Some(RepoOperation::Revert));
        abort_operation(&r.git, &r.path, &r.ctx).unwrap();
        assert_eq!(current_operation(&r.git, &r.path).unwrap(), None);
    }

    #[test]
    fn reset_modes() {
        let r = TestRepo::new();
        let base = r.head();
        r.commit("a.txt", "a\n", "a");
        reset(&r.git, &r.path, &r.ctx, &base, ResetMode::Soft).unwrap();
        assert_eq!(r.head(), base);
        assert_eq!(r.git(&["diff", "--cached", "--name-only"]), "a.txt");

        r.git(&["commit", "-q", "-m", "a again"]);
        reset(&r.git, &r.path, &r.ctx, &base, ResetMode::Mixed).unwrap();
        assert_eq!(r.git(&["diff", "--cached", "--name-only"]), "");
        assert_eq!(r.git(&["status", "--porcelain"]), "?? a.txt");

        r.git(&["add", "a.txt"]);
        r.git(&["commit", "-q", "-m", "a third"]);
        r.write("README.md", "dirty\n");
        reset(&r.git, &r.path, &r.ctx, &base, ResetMode::Hard).unwrap();
        assert_eq!(r.head(), base);
        assert!(!r.path.join("a.txt").exists());
        assert_eq!(r.read("README.md"), "hello\n");

        for bad in ["--hard", "HEAD~1", "-q"] {
            let err = reset(&r.git, &r.path, &r.ctx, bad, ResetMode::Hard).unwrap_err();
            assert_eq!(err.kind(), Some(FailureKind::InvalidInput), "{bad}");
        }
        let err = reset(&r.git, &r.path, &r.ctx, "deadbeef", ResetMode::Soft).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::NotFound));
    }

    #[test]
    fn detects_merge_and_rebase_states() {
        let r = TestRepo::new();
        r.git(&["switch", "-q", "-c", "topic"]);
        r.commit("README.md", "topic\n", "topic");
        r.commit("x.txt", "x\n", "topic 2");
        r.git(&["switch", "-q", "main"]);
        r.commit("README.md", "main\n", "main");
        assert!(!r.try_git(&["merge", "topic"]));
        let s = operation_state(&r.git, &r.path).unwrap();
        assert_eq!(s.operation, Some(RepoOperation::Merge));
        assert_eq!(s.conflicts, vec!["README.md".to_owned()]);
        r.write("README.md", "both\n");
        r.git(&["add", "README.md"]);
        let out = continue_operation(&r.git, &r.path, &r.ctx).unwrap();
        assert!(out.pending.is_none());
        assert_eq!(r.git(&["rev-list", "--count", "--merges", "HEAD"]), "1");

        r.git(&["switch", "-q", "-c", "re", "topic"]);
        r.commit("README.md", "re\n", "re");
        assert!(!r.try_git(&["rebase", "main"]));
        let s = operation_state(&r.git, &r.path).unwrap();
        assert_eq!(s.operation, Some(RepoOperation::Rebase));
        assert!(s.step.is_some() && s.total_steps.is_some(), "{s:?}");
        assert_eq!(
            abort_operation(&r.git, &r.path, &r.ctx).unwrap(),
            RepoOperation::Rebase
        );
        assert_eq!(current_operation(&r.git, &r.path).unwrap(), None);
    }
}
