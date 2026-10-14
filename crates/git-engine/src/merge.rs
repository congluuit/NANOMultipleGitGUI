//! Merging a branch (or any commit) into the current branch, with a structured outcome:
//! fast-forward, merge commit, already up to date, squash staged, or conflicts to resolve.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ops::{
    OpError, OpOutcome, RepoOperation, conflicts_or, ensure_idle, head_oid, resolve_commit, writer,
};
use crate::{AccountContext, GitRunner};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum MergeMode {
    /// Only fast-forward; refuse when a merge commit would be needed.
    FastForwardOnly,
    /// Fast-forward when possible, otherwise create a merge commit (`--ff`, overrides config).
    #[default]
    FastForward,
    /// Always create a merge commit.
    NoFastForward,
    /// Stage the combined changes without committing (`--squash`).
    Squash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum MergeResult {
    UpToDate,
    FastForward,
    /// A merge commit was created.
    Merged,
    /// Local commits were replayed on top (pull with rebase).
    Rebased,
    /// `--squash`: changes are staged, nothing committed yet.
    SquashStaged,
    /// Stopped on conflicts: see `conflicts` and `pending`.
    Conflicts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct MergeOutcome {
    pub result: MergeResult,
    /// Unmerged paths when `result` is `conflicts`.
    pub conflicts: Vec<String>,
    /// Operation left in progress (`merge` or `rebase`; `None` after a conflicting squash).
    pub pending: Option<RepoOperation>,
    /// HEAD afterwards.
    pub head: Option<String>,
}

impl MergeOutcome {
    fn done(result: MergeResult, head: Option<String>) -> Self {
        Self {
            result,
            conflicts: Vec::new(),
            pending: None,
            head,
        }
    }

    pub(crate) fn from_conflicts(o: OpOutcome) -> Self {
        Self {
            result: if o.conflicts.is_empty() {
                MergeResult::UpToDate
            } else {
                MergeResult::Conflicts
            },
            conflicts: o.conflicts,
            pending: o.pending,
            head: o.head,
        }
    }
}

/// How HEAD moved from `before` to `after` (shared with `pull`).
pub(crate) fn classify_move(
    git: &GitRunner,
    repo: &Path,
    before: Option<&str>,
    after: Option<&str>,
) -> MergeResult {
    let (Some(after), Some(before)) = (after, before) else {
        return if after.is_some() {
            MergeResult::FastForward // unborn branch got its first commit
        } else {
            MergeResult::UpToDate
        };
    };
    if before == after {
        return MergeResult::UpToDate;
    }
    let ctx = AccountContext::default();
    let parents = git
        .run_text(
            repo,
            &ctx,
            ["rev-list", "--parents", "-n", "1", after, "--"],
        )
        .unwrap_or_default();
    let parents: Vec<&str> = parents.split_whitespace().skip(1).collect();
    if parents.len() > 1 && parents[0] == before {
        return MergeResult::Merged;
    }
    let ancestor = git
        .run(repo, &ctx, ["merge-base", "--is-ancestor", before, after])
        .is_ok();
    if ancestor {
        MergeResult::FastForward
    } else {
        MergeResult::Rebased
    }
}

/// Merges `rev` (branch, remote branch, tag or commit id) into the current branch.
///
/// `message` overrides the merge commit message (ignored for squash). Conflicts are returned
/// as [`MergeResult::Conflicts`]; refusals such as local changes in the way or an impossible
/// fast-forward come back as [`OpError::Rejected`].
pub fn merge(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    rev: &str,
    mode: MergeMode,
    message: Option<&str>,
) -> Result<MergeOutcome, OpError> {
    ensure_idle(git, repo)?;
    // Validates `rev` (no leading `-`, must name a commit).
    resolve_commit(git, repo, rev)?;
    let before = head_oid(git, repo);
    let mut args = vec![
        "merge",
        match mode {
            MergeMode::FastForwardOnly => "--ff-only",
            MergeMode::FastForward => "--ff",
            MergeMode::NoFastForward => "--no-ff",
            MergeMode::Squash => "--squash",
        },
    ];
    if mode != MergeMode::Squash {
        args.push("--no-edit");
        if let Some(m) = message.filter(|m| !m.trim().is_empty()) {
            args.extend(["-m", m]);
        }
    }
    // The name (not the resolved id) is passed so the default message reads "Merge branch 'x'".
    args.extend(["--end-of-options", rev]);
    match writer(git).run(repo, ctx, &args) {
        Ok(out) => {
            let after = head_oid(git, repo);
            let result = if mode == MergeMode::Squash {
                if String::from_utf8_lossy(&out.stdout).contains("Squash commit") {
                    MergeResult::SquashStaged
                } else {
                    MergeResult::UpToDate
                }
            } else {
                classify_move(git, repo, before.as_deref(), after.as_deref())
            };
            Ok(MergeOutcome::done(result, after))
        }
        Err(err) => conflicts_or(git, repo, err).map(MergeOutcome::from_conflicts),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::{FailureKind, abort_operation, current_operation};
    use crate::progress::test_support::TestRepo;

    /// main: init → m1; topic: init → t1 (no conflict between them).
    fn diverged() -> TestRepo {
        let r = TestRepo::new();
        r.git(&["switch", "-q", "-c", "topic"]);
        r.commit("t.txt", "t\n", "t1");
        r.git(&["switch", "-q", "main"]);
        r.commit("m.txt", "m\n", "m1");
        r
    }

    #[test]
    fn fast_forward_and_up_to_date() {
        let r = TestRepo::new();
        r.git(&["switch", "-q", "-c", "topic"]);
        let tip = r.commit("t.txt", "t\n", "t1");
        r.git(&["switch", "-q", "main"]);

        let out = merge(
            &r.git,
            &r.path,
            &r.ctx,
            "topic",
            MergeMode::FastForward,
            None,
        )
        .unwrap();
        assert_eq!(out.result, MergeResult::FastForward);
        assert_eq!(out.head.as_deref(), Some(tip.as_str()));
        let out = merge(
            &r.git,
            &r.path,
            &r.ctx,
            "topic",
            MergeMode::FastForward,
            None,
        )
        .unwrap();
        assert_eq!(out.result, MergeResult::UpToDate);
        let out = merge(
            &r.git,
            &r.path,
            &r.ctx,
            &tip,
            MergeMode::FastForwardOnly,
            None,
        )
        .unwrap();
        assert_eq!(out.result, MergeResult::UpToDate);
    }

    #[test]
    fn no_ff_with_message_and_ff_only_refusal() {
        let r = diverged();
        let err = merge(
            &r.git,
            &r.path,
            &r.ctx,
            "topic",
            MergeMode::FastForwardOnly,
            None,
        )
        .unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::NonFastForward), "{err}");

        let out = merge(
            &r.git,
            &r.path,
            &r.ctx,
            "topic",
            MergeMode::NoFastForward,
            Some("--merge topic in"),
        )
        .unwrap();
        assert_eq!(out.result, MergeResult::Merged);
        assert_eq!(r.git(&["log", "-1", "--format=%s"]), "--merge topic in");
        assert_eq!(r.read("t.txt"), "t\n");

        // Default message names the branch.
        let r = diverged();
        let out = merge(
            &r.git,
            &r.path,
            &r.ctx,
            "topic",
            MergeMode::FastForward,
            None,
        )
        .unwrap();
        assert_eq!(out.result, MergeResult::Merged);
        assert_eq!(r.git(&["log", "-1", "--format=%s"]), "Merge branch 'topic'");
    }

    #[test]
    fn squash_stages_without_committing() {
        let r = diverged();
        let before = r.head();
        let out = merge(&r.git, &r.path, &r.ctx, "topic", MergeMode::Squash, None).unwrap();
        assert_eq!(out.result, MergeResult::SquashStaged);
        assert_eq!(r.head(), before);
        assert_eq!(r.git(&["diff", "--cached", "--name-only"]), "t.txt");
        assert_eq!(current_operation(&r.git, &r.path).unwrap(), None);
    }

    #[test]
    fn conflicts_are_an_outcome() {
        let r = TestRepo::new();
        r.git(&["switch", "-q", "-c", "topic"]);
        r.commit("README.md", "topic\n", "t1");
        r.git(&["switch", "-q", "main"]);
        let before = r.commit("README.md", "main\n", "m1");

        let out = merge(
            &r.git,
            &r.path,
            &r.ctx,
            "topic",
            MergeMode::NoFastForward,
            None,
        )
        .unwrap();
        assert_eq!(out.result, MergeResult::Conflicts);
        assert_eq!(out.conflicts, vec!["README.md".to_owned()]);
        assert_eq!(out.pending, Some(RepoOperation::Merge));
        let err = merge(
            &r.git,
            &r.path,
            &r.ctx,
            "topic",
            MergeMode::FastForward,
            None,
        )
        .unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::OperationInProgress));
        abort_operation(&r.git, &r.path, &r.ctx).unwrap();
        assert_eq!(r.head(), before);

        let out = merge(&r.git, &r.path, &r.ctx, "topic", MergeMode::Squash, None).unwrap();
        assert_eq!(out.result, MergeResult::Conflicts);
        assert_eq!(out.pending, None);
        r.git(&["reset", "-q", "--hard"]);
    }

    #[test]
    fn local_changes_and_bad_revisions_are_refused() {
        let r = TestRepo::new();
        r.git(&["switch", "-q", "-c", "topic"]);
        r.commit("README.md", "topic\n", "t1");
        r.git(&["switch", "-q", "main"]);
        r.commit("m.txt", "m\n", "m1");
        r.write("README.md", "dirty\n");
        let err = merge(
            &r.git,
            &r.path,
            &r.ctx,
            "topic",
            MergeMode::FastForward,
            None,
        )
        .unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::LocalChanges), "{err}");
        assert_eq!(r.read("README.md"), "dirty\n");

        for bad in ["--output=x", "-s", "--abort"] {
            let err =
                merge(&r.git, &r.path, &r.ctx, bad, MergeMode::FastForward, None).unwrap_err();
            assert_eq!(err.kind(), Some(FailureKind::InvalidName), "{bad}");
        }
        let err = merge(
            &r.git,
            &r.path,
            &r.ctx,
            "nope",
            MergeMode::FastForward,
            None,
        )
        .unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::NotFound));
        assert!(!r.path.join("x").exists());
    }
}
