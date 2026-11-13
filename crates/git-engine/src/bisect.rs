//! `git bisect` with a state the UI can show: which commits are known good / bad, how many
//! candidates are left and, at the end, the first bad commit.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ops::{self, FailureKind, OpError};
use crate::{AccountContext, GitRunner};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct BisectState {
    pub active: bool,
    /// Commit being tested (HEAD).
    pub current: Option<String>,
    pub current_subject: Option<String>,
    pub bad: Option<String>,
    pub good: Vec<String>,
    /// Commits that can still be the first bad one (the bad commit included).
    pub remaining: u32,
    /// The answer, once git has narrowed it down.
    pub first_bad: Option<String>,
    pub first_bad_subject: Option<String>,
    /// Branch (or commit) to go back to when the bisect ends.
    pub start: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum BisectMark {
    Good,
    Bad,
    /// This commit cannot be tested (does not build…).
    Skip,
}

/// `# first bad commit: [<oid>] <subject>` of `git bisect log` (git 2.56 and later quote the
/// term: `# first 'bad' commit: [...]`).
pub fn parse_first_bad(log: &str) -> Option<(String, String)> {
    log.lines().rev().find_map(|line| {
        let rest = line.strip_prefix("# first ")?;
        let (_term, rest) = rest.split_once(" commit: [")?;
        let (oid, subject) = rest.split_once(']')?;
        Some((oid.to_owned(), subject.trim().to_owned()))
    })
}

fn subject(git: &GitRunner, repo: &Path, oid: &str) -> Option<String> {
    git.run_text(
        repo,
        &AccountContext::default(),
        ["log", "-1", "--format=%s", oid, "--"],
    )
    .ok()
}

/// Current bisect, or an inactive state when none is running.
pub fn state(git: &GitRunner, repo: &Path) -> Result<BisectState, OpError> {
    let dir = ops::git_dir(git, repo)?;
    if !dir.join("BISECT_LOG").exists() {
        return Ok(BisectState::default());
    }
    let ctx = AccountContext::default();
    let refs = git
        .run_text(
            repo,
            &ctx,
            [
                "for-each-ref",
                "--format=%(objectname) %(refname)",
                "refs/bisect",
            ],
        )
        .unwrap_or_default();
    let mut bad = None;
    let mut good = Vec::new();
    for line in refs.lines() {
        let Some((oid, name)) = line.split_once(' ') else {
            continue;
        };
        if name == "refs/bisect/bad" {
            bad = Some(oid.to_owned());
        } else if name.starts_with("refs/bisect/good-") {
            good.push(oid.to_owned());
        }
    }
    let remaining = match &bad {
        Some(bad) if !good.is_empty() => {
            let mut args = vec!["rev-list".to_owned(), "--count".into(), bad.clone()];
            args.extend(good.iter().map(|g| format!("^{g}")));
            git.run_text(repo, &ctx, &args)
                .ok()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0)
        }
        _ => 0,
    };
    let log = std::fs::read_to_string(dir.join("BISECT_LOG")).unwrap_or_default();
    let first = parse_first_bad(&log);
    let current = ops::head_oid(git, repo);
    Ok(BisectState {
        active: true,
        current_subject: current.as_deref().and_then(|c| subject(git, repo, c)),
        current,
        bad,
        good,
        remaining,
        first_bad: first.as_ref().map(|f| f.0.clone()),
        first_bad_subject: first.map(|f| f.1),
        start: std::fs::read_to_string(dir.join("BISECT_START"))
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty()),
    })
}

/// Starts a bisect between a commit known to be broken and one known to work; git checks
/// out a commit in the middle for testing.
pub fn start(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    bad: &str,
    good: &str,
) -> Result<BisectState, OpError> {
    ops::ensure_idle(git, repo)?;
    let bad = ops::resolve_commit(git, repo, bad)?;
    let good = ops::resolve_commit(git, repo, good)?;
    if bad == good {
        return Err(OpError::rejected(
            FailureKind::InvalidInput,
            "the good and the bad commit are the same",
        ));
    }
    let is_ancestor = git
        .run(
            repo,
            &AccountContext::default(),
            ["merge-base", "--is-ancestor", &good, &bad],
        )
        .is_ok();
    if !is_ancestor {
        return Err(OpError::rejected(
            FailureKind::InvalidInput,
            "the good commit must be older than the bad one (an ancestor of it)",
        ));
    }
    if let Err(err) = ops::run(git, repo, ctx, &["bisect", "start", &bad, &good, "--"]) {
        // A refused checkout leaves a half-started bisect behind.
        let _ = ops::writer(git).run(repo, ctx, ["bisect", "reset"]);
        return Err(err);
    }
    state(git, repo)
}

/// Marks the tested commit; git moves to the next candidate (or names the first bad one).
pub fn mark(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    mark: BisectMark,
) -> Result<BisectState, OpError> {
    if !state(git, repo)?.active {
        return Err(OpError::rejected(
            FailureKind::NoOperation,
            "no bisect in progress",
        ));
    }
    let word = match mark {
        BisectMark::Good => "good",
        BisectMark::Bad => "bad",
        BisectMark::Skip => "skip",
    };
    ops::run(git, repo, ctx, &["bisect", word])?;
    state(git, repo)
}

/// Ends the bisect and goes back to where it started.
pub fn reset(git: &GitRunner, repo: &Path, ctx: &AccountContext) -> Result<(), OpError> {
    ops::run(git, repo, ctx, &["bisect", "reset"]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::test_support::TestRepo;

    #[test]
    fn finds_the_first_bad_commit() {
        let repo = TestRepo::new();
        let good = repo.head();
        repo.commit("a.txt", "1\n", "one");
        let culprit = repo.commit("a.txt", "broken\n", "break it");
        repo.commit("b.txt", "2\n", "two");
        let bad = repo.commit("c.txt", "3\n", "three");

        let mut st = start(&repo.git, &repo.path, &repo.ctx, &bad, &good).unwrap();
        assert!(st.active);
        assert_eq!(st.bad.as_deref(), Some(bad.as_str()));
        assert_eq!(st.start.as_deref(), Some("main"));
        assert!(st.remaining >= 2, "{st:?}");
        let mut rounds = 0;
        while st.first_bad.is_none() {
            let broken = repo.read("a.txt") == "broken\n";
            let verdict = if broken {
                BisectMark::Bad
            } else {
                BisectMark::Good
            };
            st = mark(&repo.git, &repo.path, &repo.ctx, verdict).unwrap();
            rounds += 1;
            assert!(rounds < 10);
        }
        assert_eq!(st.first_bad.as_deref(), Some(culprit.as_str()));
        assert_eq!(st.first_bad_subject.as_deref(), Some("break it"));

        reset(&repo.git, &repo.path, &repo.ctx).unwrap();
        assert!(!state(&repo.git, &repo.path).unwrap().active);
        assert_eq!(repo.branch(), "main");
        assert_eq!(repo.head(), bad);
    }

    #[test]
    fn reads_the_result_line_of_old_and_new_git() {
        let old = "# bad: [aaa] three\n# first bad commit: [61aa2e1] break it\n";
        assert_eq!(
            parse_first_bad(old),
            Some(("61aa2e1".into(), "break it".into()))
        );
        let new = "git bisect good 67c3\n# first 'bad' commit: [61aa2e1] break [it]\n# good: [67c3] one\n";
        assert_eq!(
            parse_first_bad(new),
            Some(("61aa2e1".into(), "break [it]".into()))
        );
        assert_eq!(
            parse_first_bad("# bad: [aaa] three\n# good: [bbb] one\n"),
            None
        );
    }

    #[test]
    fn refuses_a_good_commit_that_is_not_older() {
        let repo = TestRepo::new();
        let first = repo.head();
        let second = repo.commit("a.txt", "1\n", "one");
        let err = start(&repo.git, &repo.path, &repo.ctx, &first, &second).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::InvalidInput));
        assert!(!state(&repo.git, &repo.path).unwrap().active);
        let err = mark(&repo.git, &repo.path, &repo.ctx, BisectMark::Good).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::NoOperation));
    }
}
