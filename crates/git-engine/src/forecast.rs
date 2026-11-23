//! Merge conflict forecast: whether merging one revision into another would conflict, and in
//! which files, without touching the working tree, the index or any ref.
//!
//! Uses `git merge-tree --write-tree` (git 2.38+), which runs the real merge machinery in
//! memory. It may write blob and tree objects to the object database (unreferenced objects
//! that `git gc` removes later), nothing else. Idea from ioz-labs/gitfleet (`merge_forecast`).
//!
//! Every failure (older git, unrelated histories, unborn branch, unknown revision) gives
//! [`ForecastKind::Unknown`] so the UI simply shows nothing.

use std::path::Path;

use serde::Serialize;

use crate::ops::resolve_commit;
use crate::{AccountContext, GitRunner};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ForecastKind {
    /// Could not tell (git older than 2.38, unrelated histories, bad revision…).
    Unknown,
    /// The target already contains the revision: nothing to merge.
    UpToDate,
    /// The target is an ancestor of the revision: the merge only moves the branch.
    FastForward,
    /// A real merge is needed and it has no conflicts.
    Clean,
    /// A real merge is needed and these files conflict.
    Conflicts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct MergeForecast {
    pub kind: ForecastKind,
    /// Conflicting paths (repository relative, in git's order) when `kind` is `conflicts`.
    pub conflicts: Vec<String>,
}

impl MergeForecast {
    fn of(kind: ForecastKind) -> Self {
        Self {
            kind,
            conflicts: Vec::new(),
        }
    }

    pub fn unknown() -> Self {
        Self::of(ForecastKind::Unknown)
    }
}

/// Forecasts merging `rev` into `base` (`None` = HEAD). Never fails: see the module docs.
pub fn forecast(git: &GitRunner, repo: &Path, base: Option<&str>, rev: &str) -> MergeForecast {
    let (Ok(ours), Ok(theirs)) = (
        resolve_commit(git, repo, base.unwrap_or("HEAD")),
        resolve_commit(git, repo, rev),
    ) else {
        return MergeForecast::unknown();
    };
    if ours == theirs || is_ancestor(git, repo, &theirs, &ours) {
        return MergeForecast::of(ForecastKind::UpToDate);
    }
    if is_ancestor(git, repo, &ours, &theirs) {
        return MergeForecast::of(ForecastKind::FastForward);
    }
    // Resolved ids only, so nothing here can be read as an option.
    let output = git
        .command(
            repo,
            &AccountContext::default(),
            [
                "merge-tree",
                "--write-tree",
                "--name-only",
                "--no-messages",
                "-z",
                &ours,
                &theirs,
            ],
        )
        .output();
    match output {
        Ok(out) => parse_merge_tree(out.status.code(), &out.stdout),
        Err(_) => MergeForecast::unknown(),
    }
}

fn is_ancestor(git: &GitRunner, repo: &Path, ancestor: &str, of: &str) -> bool {
    git.run(
        repo,
        &AccountContext::default(),
        ["merge-base", "--is-ancestor", ancestor, of],
    )
    .is_ok()
}

fn is_oid(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Reads `git merge-tree --write-tree --name-only --no-messages -z` output: the tree id, then
/// (on conflicts) each conflicted path, every item NUL-terminated. Exit code 0 means clean,
/// 1 means conflicts; anything else (usage error 129 on git < 2.38, 128 on unrelated
/// histories) or output that does not start with a tree id is unknown.
fn parse_merge_tree(code: Option<i32>, stdout: &[u8]) -> MergeForecast {
    let text = String::from_utf8_lossy(stdout);
    let mut items = text.split('\0');
    let tree_ok = items.next().is_some_and(|first| is_oid(first.trim()));
    if !tree_ok {
        return MergeForecast::unknown();
    }
    match code {
        Some(0) => MergeForecast::of(ForecastKind::Clean),
        Some(1) => {
            let mut conflicts: Vec<String> = Vec::new();
            for path in items.filter(|p| !p.is_empty()) {
                if !conflicts.iter().any(|c| c == path) {
                    conflicts.push(path.to_owned());
                }
            }
            if conflicts.is_empty() {
                MergeForecast::unknown()
            } else {
                MergeForecast {
                    kind: ForecastKind::Conflicts,
                    conflicts,
                }
            }
        }
        _ => MergeForecast::unknown(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::test_support::TestRepo;

    const TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

    fn conflicts(f: &MergeForecast) -> Vec<&str> {
        f.conflicts.iter().map(String::as_str).collect()
    }

    #[test]
    fn parses_merge_tree_output() {
        let clean = parse_merge_tree(Some(0), format!("{TREE}\0").as_bytes());
        assert_eq!(clean.kind, ForecastKind::Clean);

        let out = format!("{TREE}\0a.txt\0dir/b c.txt\0a.txt\0");
        let f = parse_merge_tree(Some(1), out.as_bytes());
        assert_eq!(f.kind, ForecastKind::Conflicts);
        assert_eq!(conflicts(&f), ["a.txt", "dir/b c.txt"]);

        // Usage error from an old git, garbage, empty conflict list, killed process.
        let unknown = [
            parse_merge_tree(Some(129), b"usage: git merge-tree"),
            parse_merge_tree(Some(1), b"not a tree\0a.txt\0"),
            parse_merge_tree(Some(1), format!("{TREE}\0").as_bytes()),
            parse_merge_tree(Some(128), b""),
            parse_merge_tree(None, format!("{TREE}\0").as_bytes()),
        ];
        for f in unknown {
            assert_eq!(f, MergeForecast::unknown());
        }
    }

    #[test]
    fn up_to_date_and_fast_forward() {
        let r = TestRepo::new();
        r.git(&["switch", "-q", "-c", "topic"]);
        r.commit("t.txt", "t\n", "t1");
        r.git(&["switch", "-q", "main"]);

        let f = forecast(&r.git, &r.path, None, "topic");
        assert_eq!(f.kind, ForecastKind::FastForward);
        // The other way round: main is already in topic.
        let f = forecast(&r.git, &r.path, Some("topic"), "main");
        assert_eq!(f.kind, ForecastKind::UpToDate);
        let f = forecast(&r.git, &r.path, None, "main");
        assert_eq!(f.kind, ForecastKind::UpToDate);
    }

    #[test]
    fn clean_and_conflicting_merges_leave_the_repo_alone() {
        let r = TestRepo::new();
        r.git(&["switch", "-q", "-c", "topic"]);
        r.commit("README.md", "topic\n", "t1");
        r.commit("t.txt", "t\n", "t2");
        r.commit("both.txt", "topic side\n", "t3");
        r.git(&["switch", "-q", "-c", "side", "main"]);
        r.commit("s.txt", "s\n", "s1");
        r.git(&["switch", "-q", "main"]);
        r.commit("README.md", "main\n", "m1");
        r.commit("both.txt", "main side\n", "m2");
        // Local changes must survive and must not matter.
        r.write("dirty.txt", "dirty\n");
        r.git(&["add", "dirty.txt"]);
        let head = r.head();
        let status = r.git(&["status", "--porcelain"]);

        let f = forecast(&r.git, &r.path, None, "topic");
        assert_eq!(f.kind, ForecastKind::Conflicts);
        let mut got = conflicts(&f);
        got.sort_unstable();
        assert_eq!(got, ["README.md", "both.txt"]);

        let f = forecast(&r.git, &r.path, None, "side");
        assert_eq!(f, MergeForecast::of(ForecastKind::Clean));
        // Another base than HEAD: topic and side do not overlap.
        let f = forecast(&r.git, &r.path, Some("side"), "topic");
        assert_eq!(f.kind, ForecastKind::Clean);

        assert_eq!(r.head(), head);
        assert_eq!(r.git(&["status", "--porcelain"]), status);
        assert!(!r.path.join(".git/MERGE_HEAD").exists());
    }

    #[test]
    fn failures_are_unknown() {
        let r = TestRepo::new();
        assert_eq!(
            forecast(&r.git, &r.path, None, "nope"),
            MergeForecast::unknown()
        );
        assert_eq!(
            forecast(&r.git, &r.path, None, "--output=x"),
            MergeForecast::unknown()
        );
        assert_eq!(
            forecast(&r.git, &r.path, Some("nope"), "main"),
            MergeForecast::unknown()
        );
        assert!(!r.path.join("x").exists());

        // Unrelated histories: git refuses to merge them.
        r.git(&["switch", "-q", "--orphan", "other"]);
        r.commit("o.txt", "o\n", "o1");
        r.git(&["switch", "-q", "main"]);
        assert_eq!(
            forecast(&r.git, &r.path, None, "other"),
            MergeForecast::unknown()
        );

        // Unborn branch.
        r.git(&["switch", "-q", "--orphan", "empty"]);
        assert_eq!(
            forecast(&r.git, &r.path, None, "main"),
            MergeForecast::unknown()
        );
    }
}
