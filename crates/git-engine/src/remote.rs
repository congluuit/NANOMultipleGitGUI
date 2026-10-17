//! Network operations: fetch, pull and push, with parsed progress and cancellation.
//!
//! Commands run with `--progress`; stderr is read while git runs and every progress line is
//! delivered to the caller's callback (see [`crate::progress`]). Credentials come from the
//! account context (Phase 1: the system credential helper); `GIT_TERMINAL_PROMPT=0` is always
//! set by [`AccountContext::env`], so a missing credential fails instead of hanging.
//! Ahead/behind counts are read by [`crate::refs`].

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::branch::validate_branch_name;
use crate::merge::{MergeOutcome, MergeResult, classify_move};
use crate::ops::{
    FailureKind, OpError, check_arg, current_branch, ensure_idle, head_oid, operation_state,
    ref_exists, validate_tag_name, writer,
};
use crate::progress::{CancelToken, Progress, run_streaming};
use crate::{AccountContext, GitRunner};

/// Accepts only the name of a configured remote (never a URL or an option).
pub fn validate_remote(git: &GitRunner, repo: &Path, name: &str) -> Result<(), OpError> {
    check_arg(name, "remote name")?;
    let names = git.run_text(repo, &AccountContext::default(), ["remote"])?;
    if names.lines().any(|l| l.trim() == name) {
        Ok(())
    } else {
        Err(OpError::rejected(
            FailureKind::NotFound,
            format!("remote '{name}' not found"),
        ))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FetchOptions {
    /// Remote to fetch; `None` fetches all remotes.
    pub remote: Option<String>,
    /// Remove remote-tracking branches deleted on the remote.
    pub prune: bool,
    /// Fetch all tags (`--tags`), not only those reachable from fetched branches.
    pub tags: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FetchSummary {
    /// Ref updates git reported (new, updated, pruned branches and tags).
    pub updated_refs: u32,
}

/// Counts ref-update lines (` * [new branch]  x -> origin/x`, `   a..b  main -> origin/main`).
fn count_ref_updates(stderr: &str) -> u32 {
    stderr
        .lines()
        .filter(|l| l.starts_with(' ') && l.contains(" -> ") && !l.contains("[up to date]"))
        .count() as u32
}

pub fn fetch<F: FnMut(Progress)>(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    opts: &FetchOptions,
    cancel: &CancelToken,
    on_progress: F,
) -> Result<FetchSummary, OpError> {
    let mut args = vec!["fetch".to_owned(), "--progress".to_owned()];
    if opts.prune {
        args.push("--prune".into());
    }
    if opts.tags {
        args.push("--tags".into());
    }
    match &opts.remote {
        Some(remote) => {
            validate_remote(git, repo, remote)?;
            args.extend(["--end-of-options".into(), remote.clone()]);
        }
        None => args.push("--all".into()),
    }
    let out = run_streaming(&writer(git), repo, ctx, &args, cancel, on_progress)?;
    Ok(FetchSummary {
        updated_refs: count_ref_updates(&out.stderr),
    })
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum PullMode {
    /// Fetch then merge (`--no-rebase`; fast-forwards when possible).
    #[default]
    Merge,
    /// Fetch then rebase local commits on top (`--rebase`).
    Rebase,
    /// Fetch then fast-forward only (`--ff-only`).
    FastForwardOnly,
    /// Merge like [`Merge`](Self::Merge), also when the two histories share no commit
    /// (`--allow-unrelated-histories`): a repository created on GitHub / GitLab with a README
    /// pulled into a local project that already has commits.
    MergeUnrelated,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PullOptions {
    /// `None` pulls the current branch's upstream.
    pub remote: Option<String>,
    /// Remote branch to pull (requires `remote`); `None` uses the upstream / remote default.
    pub branch: Option<String>,
    pub mode: PullMode,
}

/// Pulls into the current branch. Conflicts (merge or rebase) come back as
/// [`MergeResult::Conflicts`]; a missing upstream is [`FailureKind::NoUpstream`].
pub fn pull<F: FnMut(Progress)>(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    opts: &PullOptions,
    cancel: &CancelToken,
    on_progress: F,
) -> Result<MergeOutcome, OpError> {
    ensure_idle(git, repo)?;
    if current_branch(git, repo).is_none() {
        return Err(OpError::rejected(
            FailureKind::DetachedHead,
            "pull needs a checked-out branch (HEAD is detached)",
        ));
    }
    let mut args: Vec<String> = vec!["pull".into(), "--progress".into()];
    match opts.mode {
        PullMode::Merge => args.extend(["--no-rebase".into(), "--no-edit".into()]),
        PullMode::Rebase => args.push("--rebase".into()),
        PullMode::FastForwardOnly => args.push("--ff-only".into()),
        PullMode::MergeUnrelated => args.extend([
            "--no-rebase".into(),
            "--no-edit".into(),
            "--allow-unrelated-histories".into(),
        ]),
    }
    match (&opts.remote, &opts.branch) {
        (Some(remote), branch) => {
            validate_remote(git, repo, remote)?;
            args.extend(["--end-of-options".into(), remote.clone()]);
            if let Some(branch) = branch {
                validate_branch_name(git, repo, branch)?;
                args.push(branch.clone());
            }
        }
        (None, Some(_)) => {
            return Err(OpError::rejected(
                FailureKind::InvalidInput,
                "a branch to pull needs a remote",
            ));
        }
        (None, None) => {}
    }
    let before = head_oid(git, repo);
    match run_streaming(&writer(git), repo, ctx, &args, cancel, on_progress) {
        Ok(_) => {
            let after = head_oid(git, repo);
            Ok(MergeOutcome {
                result: classify_move(git, repo, before.as_deref(), after.as_deref()),
                conflicts: Vec::new(),
                pending: None,
                head: after,
            })
        }
        Err(OpError::Cancelled) => Err(OpError::Cancelled),
        Err(err) => {
            let state = operation_state(git, repo)?;
            if state.conflicts.is_empty() {
                return Err(err);
            }
            Ok(MergeOutcome {
                result: MergeResult::Conflicts,
                conflicts: state.conflicts,
                pending: state.operation,
                head: head_oid(git, repo),
            })
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PushOptions {
    pub remote: String,
    /// Local branch to push.
    pub branch: String,
    /// Branch name on the remote; defaults to `branch`.
    pub remote_branch: Option<String>,
    /// Record the pushed branch as upstream (`--set-upstream`).
    pub set_upstream: bool,
    /// Overwrite the remote branch, but only if it is still where our remote-tracking branch
    /// says (`--force-with-lease`); otherwise [`FailureKind::StaleLease`].
    pub force_with_lease: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PushSummary {
    /// Nothing had to be sent.
    pub up_to_date: bool,
}

/// `git push --progress <tail...>`; `tail` ends with `--end-of-options <remote> <refspec>`.
fn push_with<F: FnMut(Progress)>(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    tail: &[&str],
    cancel: &CancelToken,
    on_progress: F,
) -> Result<PushSummary, OpError> {
    let mut args: Vec<String> = vec!["push".into(), "--progress".into()];
    args.extend(tail.iter().map(|a| (*a).to_owned()));
    let out = run_streaming(&writer(git), repo, ctx, &args, cancel, on_progress)?;
    Ok(PushSummary {
        up_to_date: out.stderr.contains("Everything up-to-date"),
    })
}

/// Pushes a local branch. A non-fast-forward rejection is [`FailureKind::NonFastForward`].
pub fn push<F: FnMut(Progress)>(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    opts: &PushOptions,
    cancel: &CancelToken,
    on_progress: F,
) -> Result<PushSummary, OpError> {
    validate_remote(git, repo, &opts.remote)?;
    validate_branch_name(git, repo, &opts.branch)?;
    if !ref_exists(git, repo, &format!("refs/heads/{}", opts.branch)) {
        return Err(OpError::rejected(
            FailureKind::NotFound,
            format!("branch '{}' not found", opts.branch),
        ));
    }
    let dst = opts.remote_branch.as_deref().unwrap_or(&opts.branch);
    validate_branch_name(git, repo, dst)?;
    let refspec = format!("refs/heads/{}:refs/heads/{dst}", opts.branch);
    let mut tail = Vec::new();
    if opts.set_upstream {
        tail.push("--set-upstream");
    }
    if opts.force_with_lease {
        tail.push("--force-with-lease");
    }
    tail.extend(["--end-of-options", &opts.remote, &refspec]);
    push_with(git, repo, ctx, &tail, cancel, on_progress)
}

/// Pushes one tag.
pub fn push_tag<F: FnMut(Progress)>(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    remote: &str,
    tag: &str,
    cancel: &CancelToken,
    on_progress: F,
) -> Result<PushSummary, OpError> {
    validate_remote(git, repo, remote)?;
    validate_tag_name(git, repo, tag)?;
    let full = format!("refs/tags/{tag}");
    if !ref_exists(git, repo, &full) {
        return Err(OpError::rejected(
            FailureKind::NotFound,
            format!("tag '{tag}' not found"),
        ));
    }
    let refspec = format!("{full}:{full}");
    push_with(
        git,
        repo,
        ctx,
        &["--end-of-options", remote, &refspec],
        cancel,
        on_progress,
    )
}

/// Deletes `branch` on `remote` (`git push <remote> --delete refs/heads/<branch>`); the
/// remote-tracking branch is removed as well.
pub fn delete_remote_branch<F: FnMut(Progress)>(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    remote: &str,
    branch: &str,
    cancel: &CancelToken,
    on_progress: F,
) -> Result<PushSummary, OpError> {
    validate_remote(git, repo, remote)?;
    validate_branch_name(git, repo, branch)?;
    let refspec = format!("refs/heads/{branch}");
    push_with(
        git,
        repo,
        ctx,
        &["--delete", "--end-of-options", remote, &refspec],
        cancel,
        on_progress,
    )
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::progress::test_support::TestRepo;

    /// A bare `remote.git`, `work` (pushed `main`, upstream set) and a second clone `other`.
    struct Setup {
        _dir: tempfile::TempDir,
        bare: std::path::PathBuf,
        work: TestRepo,
        other: TestRepo,
    }

    fn setup() -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let bare = dir.path().join("remote.git");
        let work = TestRepo::init_at(&dir.path().join("work"));
        work.git
            .run(
                dir.path(),
                &work.ctx,
                ["init", "-q", "--bare", "-b", "main", "remote.git"],
            )
            .unwrap();
        let url = bare.to_string_lossy().replace('\\', "/");
        work.git(&["remote", "add", "origin", &url]);
        work.git(&["push", "-q", "-u", "origin", "main"]);
        work.git
            .run(dir.path(), &work.ctx, ["clone", "-q", &url, "other"])
            .unwrap();
        let other = TestRepo::open(&dir.path().join("other"));
        Setup {
            _dir: dir,
            bare,
            work,
            other,
        }
    }

    fn opts(remote: &str, branch: &str) -> PushOptions {
        PushOptions {
            remote: remote.into(),
            branch: branch.into(),
            ..Default::default()
        }
    }

    fn bare_ref(s: &Setup, r: &str) -> Option<String> {
        s.work
            .git
            .run_text(&s.bare, &s.work.ctx, ["rev-parse", "-q", "--verify", r])
            .ok()
    }

    #[test]
    fn counts_ref_updates() {
        let stderr = "From C:/tmp/remote\n   bf92b8f..cbc67a7  main       -> origin/main\n * [new branch]      x          -> origin/x\n - [deleted]         (none)     -> origin/gone\n = [up to date]      y          -> origin/y";
        assert_eq!(count_ref_updates(stderr), 3);
    }

    #[test]
    fn fetch_reports_progress_updates_and_prunes() {
        let s = setup();
        s.other.git(&["switch", "-q", "-c", "feature"]);
        for i in 0..5 {
            s.other.commit(
                &format!("f{i}.txt"),
                &"x".repeat(5000 + i),
                &format!("f{i}"),
            );
        }
        s.other.git(&["push", "-q", "origin", "feature"]);
        let tip = s.other.head();

        let mut events = Vec::new();
        let sum = fetch(
            &s.work.git,
            &s.work.path,
            &s.work.ctx,
            &FetchOptions {
                remote: Some("origin".into()),
                ..Default::default()
            },
            &CancelToken::new(),
            |p| events.push(p),
        )
        .unwrap();
        assert_eq!(sum.updated_refs, 1);
        assert_eq!(s.work.git(&["rev-parse", "origin/feature"]), tip);
        assert!(!events.is_empty(), "progress events expected");
        assert!(events.iter().any(|p| p.percent == Some(100) && p.done));

        s.other
            .git(&["push", "-q", "origin", "--delete", "feature"]);
        let sum = fetch(
            &s.work.git,
            &s.work.path,
            &s.work.ctx,
            &FetchOptions {
                remote: None,
                prune: true,
                tags: true,
            },
            &CancelToken::new(),
            |_| {},
        )
        .unwrap();
        assert_eq!(sum.updated_refs, 1);
        assert!(!ref_exists(
            &s.work.git,
            &s.work.path,
            "refs/remotes/origin/feature"
        ));

        for bad in ["--upload-pack=evil", "nowhere", "-v"] {
            let err = fetch(
                &s.work.git,
                &s.work.path,
                &s.work.ctx,
                &FetchOptions {
                    remote: Some(bad.into()),
                    ..Default::default()
                },
                &CancelToken::new(),
                |_| {},
            )
            .unwrap_err();
            assert!(
                matches!(
                    err.kind(),
                    Some(FailureKind::InvalidName | FailureKind::NotFound)
                ),
                "{bad}: {err}"
            );
        }
    }

    #[test]
    fn pull_modes() {
        let s = setup();
        let none = CancelToken::new();
        let pull_with = |mode| {
            pull(
                &s.work.git,
                &s.work.path,
                &s.work.ctx,
                &PullOptions {
                    mode,
                    ..Default::default()
                },
                &none,
                |_| {},
            )
        };
        let out = pull_with(PullMode::Merge).unwrap();
        assert_eq!(out.result, MergeResult::UpToDate);

        s.other.commit("o1.txt", "o\n", "o1");
        s.other.git(&["push", "-q"]);
        let out = pull_with(PullMode::FastForwardOnly).unwrap();
        assert_eq!(out.result, MergeResult::FastForward);
        assert_eq!(out.head, Some(s.other.head()));

        // Diverged: ff-only refuses, merge creates a merge commit.
        s.other.commit("o2.txt", "o\n", "o2");
        s.other.git(&["push", "-q"]);
        s.work.commit("w1.txt", "w\n", "w1");
        let err = pull_with(PullMode::FastForwardOnly).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::NonFastForward), "{err}");
        let out = pull_with(PullMode::Merge).unwrap();
        assert_eq!(out.result, MergeResult::Merged);
        s.work.git(&["push", "-q"]);

        // Diverged again: rebase.
        s.other.git(&["pull", "-q", "--no-rebase"]);
        s.other.commit("o3.txt", "o\n", "o3");
        s.other.git(&["push", "-q"]);
        s.work.commit("w2.txt", "w\n", "w2");
        let out = pull_with(PullMode::Rebase).unwrap();
        assert_eq!(out.result, MergeResult::Rebased);
        assert_eq!(s.work.git(&["log", "-1", "--format=%s"]), "w2");

        // Conflicting pull.
        s.work.git(&["push", "-q"]);
        s.other.git(&["pull", "-q", "--no-rebase"]);
        s.other.commit("README.md", "other\n", "o4");
        s.other.git(&["push", "-q"]);
        s.work.commit("README.md", "work\n", "w3");
        let out = pull_with(PullMode::Merge).unwrap();
        assert_eq!(out.result, MergeResult::Conflicts);
        assert_eq!(out.conflicts, vec!["README.md".to_owned()]);
        assert_eq!(out.pending, Some(crate::ops::RepoOperation::Merge));
        let err = pull_with(PullMode::Merge).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::OperationInProgress));
        crate::ops::abort_operation(&s.work.git, &s.work.path, &s.work.ctx).unwrap();

        // No upstream / detached HEAD.
        s.work.git(&["switch", "-q", "-c", "lonely"]);
        let err = pull_with(PullMode::Merge).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::NoUpstream), "{err}");
        s.work.git(&["switch", "-q", "--detach"]);
        let err = pull_with(PullMode::Merge).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::DetachedHead));
    }

    #[test]
    fn pull_merges_unrelated_histories_on_request() {
        let s = setup();
        let none = CancelToken::new();
        // A local project with its own first commit, the remote with another history.
        s.work.git(&["switch", "-q", "--orphan", "local"]);
        s.work.commit(
            "local.txt",
            "l
",
            "local start",
        );
        let pull_with = |mode| {
            pull(
                &s.work.git,
                &s.work.path,
                &s.work.ctx,
                &PullOptions {
                    remote: Some("origin".into()),
                    branch: Some("main".into()),
                    mode,
                },
                &none,
                |_| {},
            )
        };
        let err = pull_with(PullMode::Merge).unwrap_err();
        assert!(err.to_string().contains("unrelated histories"), "{err}");
        let out = pull_with(PullMode::MergeUnrelated).unwrap();
        assert_eq!(out.result, MergeResult::Merged);
        assert!(s.work.path.join("local.txt").exists());
        assert!(s.work.path.join("README.md").exists());
    }

    #[test]
    fn push_new_branch_rejection_lease_tags_and_delete() {
        let s = setup();
        let none = CancelToken::new();
        let w = &s.work;

        // New branch with upstream, progress reported.
        w.git(&["switch", "-q", "-c", "feature"]);
        for i in 0..3 {
            w.commit(&format!("p{i}.txt"), &"y".repeat(4000 + i), "p");
        }
        let mut events = Vec::new();
        let sum = push(
            &w.git,
            &w.path,
            &w.ctx,
            &PushOptions {
                set_upstream: true,
                ..opts("origin", "feature")
            },
            &none,
            |p| events.push(p),
        )
        .unwrap();
        assert!(!sum.up_to_date);
        assert!(
            events
                .iter()
                .any(|p| p.phase == crate::progress::ProgressPhase::Writing)
        );
        assert_eq!(bare_ref(&s, "refs/heads/feature"), Some(w.head()));
        assert_eq!(
            w.git(&["rev-parse", "--abbrev-ref", "feature@{u}"]),
            "origin/feature"
        );
        let sum = push(
            &w.git,
            &w.path,
            &w.ctx,
            &opts("origin", "feature"),
            &none,
            |_| {},
        )
        .unwrap();
        assert!(sum.up_to_date);

        // Someone else pushed to main: our push is rejected (non-fast-forward).
        w.git(&["switch", "-q", "main"]);
        s.other.commit("o.txt", "o\n", "other main");
        s.other.git(&["push", "-q"]);
        w.commit("w.txt", "w\n", "work main");
        let err = push(
            &w.git,
            &w.path,
            &w.ctx,
            &opts("origin", "main"),
            &none,
            |_| {},
        )
        .unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::NonFastForward), "{err}");

        // Force-with-lease without having fetched the new remote tip: stale.
        let lease = PushOptions {
            force_with_lease: true,
            ..opts("origin", "main")
        };
        let err = push(&w.git, &w.path, &w.ctx, &lease, &none, |_| {}).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::StaleLease), "{err}");
        // After fetching, the lease holds and the push overwrites.
        w.git(&["fetch", "-q"]);
        push(&w.git, &w.path, &w.ctx, &lease, &none, |_| {}).unwrap();
        assert_eq!(bare_ref(&s, "refs/heads/main"), Some(w.head()));

        // Push to a differently named remote branch.
        push(
            &w.git,
            &w.path,
            &w.ctx,
            &PushOptions {
                remote_branch: Some("renamed".into()),
                ..opts("origin", "feature")
            },
            &none,
            |_| {},
        )
        .unwrap();
        assert!(bare_ref(&s, "refs/heads/renamed").is_some());

        // Tags.
        w.git(&["tag", "v1"]);
        push_tag(&w.git, &w.path, &w.ctx, "origin", "v1", &none, |_| {}).unwrap();
        assert_eq!(bare_ref(&s, "refs/tags/v1"), Some(w.head()));
        let err = push_tag(&w.git, &w.path, &w.ctx, "origin", "v9", &none, |_| {}).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::NotFound));

        // Delete a remote branch (and its remote-tracking ref).
        delete_remote_branch(&w.git, &w.path, &w.ctx, "origin", "feature", &none, |_| {}).unwrap();
        assert_eq!(bare_ref(&s, "refs/heads/feature"), None);
        assert!(!ref_exists(&w.git, &w.path, "refs/remotes/origin/feature"));
        // Deleting it again: a local remote accepts silently, servers usually answer
        // "remote ref does not exist" (recognised as NotFound).
        if let Err(err) =
            delete_remote_branch(&w.git, &w.path, &w.ctx, "origin", "feature", &none, |_| {})
        {
            assert_eq!(err.kind(), Some(FailureKind::NotFound), "{err}");
        }

        // Option injection.
        for (remote, branch) in [
            ("--receive-pack=evil", "main"),
            ("origin", "--delete"),
            ("origin", "-f"),
            ("origin", "missing"),
        ] {
            let err = push(
                &w.git,
                &w.path,
                &w.ctx,
                &opts(remote, branch),
                &none,
                |_| {},
            )
            .unwrap_err();
            assert!(
                matches!(
                    err.kind(),
                    Some(FailureKind::InvalidName | FailureKind::NotFound)
                ),
                "{remote} {branch}: {err}"
            );
        }
        assert_eq!(bare_ref(&s, "refs/heads/main"), Some(w.head()));
    }

    #[test]
    fn cancel_kills_a_slow_push() {
        let s = setup();
        let w = &s.work;
        // A pre-push hook that never ends (a child process of git), spinning on shell builtins.
        // Cancelling kills the whole tree: on Windows git runs in a Job Object (`crate::job`),
        // which also reaches processes re-parented by the MSYS fork emulation (a `sleep`
        // started from a hook is covered by `progress::tests`).
        let hooks = s._dir.path().join("hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        std::fs::write(hooks.join("pre-push"), "#!/bin/sh\nwhile :; do :; done\n").unwrap();
        // git silently skips hooks that are not executable.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                hooks.join("pre-push"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }
        w.git(&[
            "config",
            "core.hooksPath",
            &hooks.to_string_lossy().replace('\\', "/"),
        ]);
        w.git(&["switch", "-q", "-c", "slow"]);
        w.commit("s.txt", "s\n", "slow");

        let token = CancelToken::new();
        let canceller = token.clone();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(800));
            canceller.cancel();
        });
        let start = Instant::now();
        let err = push(
            &w.git,
            &w.path,
            &w.ctx,
            &opts("origin", "slow"),
            &token,
            |_| {},
        )
        .unwrap_err();
        t.join().unwrap();
        assert!(matches!(err, OpError::Cancelled), "{err}");
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "took {:?}",
            start.elapsed()
        );
        assert_eq!(bare_ref(&s, "refs/heads/slow"), None);

        // The hook process was killed with git (it would spin forever otherwise).
        #[cfg(windows)]
        {
            let marker = hooks.to_string_lossy().replace('\\', "/");
            let query = format!(
                "@(Get-CimInstance Win32_Process | Where-Object {{ $_.Name -ne 'powershell.exe' -and $_.CommandLine -like '*{marker}*' }}).Count"
            );
            let out = std::process::Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", &query])
                .output()
                .unwrap();
            assert_eq!(
                String::from_utf8_lossy(&out.stdout).trim(),
                "0",
                "hook process still running"
            );
        }

        // Nothing is left locked: the same push goes through once the hook is gone.
        std::fs::remove_file(hooks.join("pre-push")).unwrap();
        push(
            &w.git,
            &w.path,
            &w.ctx,
            &opts("origin", "slow"),
            &CancelToken::new(),
            |_| {},
        )
        .unwrap();
        assert_eq!(bare_ref(&s, "refs/heads/slow"), Some(w.head()));
    }
}
