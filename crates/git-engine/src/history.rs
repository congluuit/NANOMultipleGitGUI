//! A repository's loaded history: commits in topological order, lane layout and ref labels,
//! kept up to date incrementally when refs move.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use serde::Serialize;

use crate::graph::{CommitInfo, GraphLayout, LOG_FORMAT, layout, parse_log, read_log};
use crate::{AccountContext, GitError, GitRunner};

/// Ref labels per commit plus the set of commits that refs point at.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefLabels {
    /// Full labels in display order: `HEAD -> refs/heads/main`, `HEAD` (detached),
    /// `refs/heads/x`, `refs/remotes/origin/x`, `tag: refs/tags/v1`.
    pub by_commit: HashMap<String, Vec<String>>,
    /// Commits pointed at by any branch, remote branch, tag or HEAD (stash excluded).
    pub tips: BTreeSet<String>,
}

const LABEL_FORMAT: &str = "--format=%(objectname)%00%(*objectname)%00%(refname)%00%(symref)";

pub fn parse_ref_labels(
    for_each_ref: &str,
    head_ref: Option<&str>,
    head_oid: Option<&str>,
) -> RefLabels {
    let mut entries: Vec<(String, u8, String)> = Vec::new();
    for line in for_each_ref.lines() {
        let f: Vec<&str> = line.split('\0').collect();
        if f.len() < 4 || !f[3].is_empty() {
            continue; // malformed, or a symbolic ref such as origin/HEAD
        }
        let target = if f[1].is_empty() { f[0] } else { f[1] }.to_owned();
        let name = f[2];
        let (order, label) = if Some(name) == head_ref {
            (0, format!("HEAD -> {name}"))
        } else if name.starts_with("refs/heads/") {
            (1, name.to_owned())
        } else if name.starts_with("refs/remotes/") {
            (2, name.to_owned())
        } else if name.starts_with("refs/tags/") {
            (3, format!("tag: {name}"))
        } else {
            continue;
        };
        entries.push((target, order, label));
    }
    let detached = head_ref.is_none_or(|r| !r.starts_with("refs/heads/"));
    if let (true, Some(oid)) = (detached, head_oid) {
        entries.push((oid.to_owned(), 0, "HEAD".to_owned()));
    }
    entries.sort_by(|a, b| (a.1, &a.2).cmp(&(b.1, &b.2)));

    let mut labels = RefLabels::default();
    for (oid, _, label) in entries {
        labels.tips.insert(oid.clone());
        labels.by_commit.entry(oid).or_default().push(label);
    }
    if let Some(oid) = head_oid {
        labels.tips.insert(oid.to_owned());
    }
    labels
}

pub fn read_ref_labels(git: &GitRunner, repo: &Path) -> Result<RefLabels, GitError> {
    let ctx = AccountContext::default();
    let out = git.run_text(
        repo,
        &ctx,
        [
            "for-each-ref",
            LABEL_FORMAT,
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ],
    )?;
    let head_ref = git
        .run_text(repo, &ctx, ["symbolic-ref", "-q", "HEAD"])
        .ok()
        .filter(|s| !s.is_empty());
    let head_oid = git
        .run_text(repo, &ctx, ["rev-parse", "-q", "--verify", "HEAD"])
        .ok()
        .filter(|s| !s.is_empty());
    Ok(parse_ref_labels(
        &out,
        head_ref.as_deref(),
        head_oid.as_deref(),
    ))
}

/// What a [`RepoGraph::refresh`] had to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum RefreshKind {
    /// First load, or history was rewritten (branch deleted, reset, rebase): read everything.
    Full,
    /// Only new commits were read and prepended.
    Incremental,
    /// No new commits, but labels moved (e.g. a tag was added to an existing commit).
    LabelsOnly,
    Unchanged,
}

pub struct RepoGraph {
    pub commits: Vec<CommitInfo>,
    pub layout: GraphLayout,
    pub labels: RefLabels,
    index: HashMap<String, usize>,
    /// Bumped whenever rows or labels change, so the UI knows to re-fetch rows.
    pub version: u32,
}

impl RepoGraph {
    fn build(commits: Vec<CommitInfo>, labels: RefLabels, version: u32) -> Self {
        let layout = layout(&commits);
        let index = commits
            .iter()
            .enumerate()
            .map(|(i, c)| (c.hash.clone(), i))
            .collect();
        Self {
            commits,
            layout,
            labels,
            index,
            version,
        }
    }

    pub fn load(git: &GitRunner, repo: &Path) -> Result<Self, GitError> {
        let labels = read_ref_labels(git, repo)?;
        let commits = read_log(git, repo, None)?;
        Ok(Self::build(commits, labels, 1))
    }

    /// Brings the graph up to date with the repository. Returns `(kind, new commit count)`.
    pub fn refresh(
        &mut self,
        git: &GitRunner,
        repo: &Path,
    ) -> Result<(RefreshKind, usize), GitError> {
        let labels = read_ref_labels(git, repo)?;
        if labels.tips == self.labels.tips {
            if labels == self.labels {
                return Ok((RefreshKind::Unchanged, 0));
            }
            self.labels = labels;
            self.version += 1;
            return Ok((RefreshKind::LabelsOnly, 0));
        }
        let version = self.version + 1;
        match self.new_commits(git, repo, &labels) {
            Ok(Some(mut added)) => {
                let count = added.len();
                added.append(&mut std::mem::take(&mut self.commits));
                *self = Self::build(added, labels, version);
                Ok((RefreshKind::Incremental, count))
            }
            _ => {
                let commits = read_log(git, repo, None)?;
                let count = commits.len();
                *self = Self::build(commits, labels, version);
                Ok((RefreshKind::Full, count))
            }
        }
    }

    /// Commits reachable now but not from the previously loaded tips, if prepending them is
    /// enough — i.e. no previously loaded commit became unreachable.
    fn new_commits(
        &self,
        git: &GitRunner,
        repo: &Path,
        labels: &RefLabels,
    ) -> Result<Option<Vec<CommitInfo>>, GitError> {
        let ctx = AccountContext::default();
        // Commits reachable from tips that disappeared but not from the current tips are gone
        // from history (deleted branch, reset, rebase); prepending cannot express that.
        // This walks only the changed region instead of the whole history.
        let gone: Vec<&String> = self.labels.tips.difference(&labels.tips).collect();
        if !gone.is_empty() {
            let mut input: String = gone
                .iter()
                .map(|t| {
                    format!(
                        "{t}
"
                    )
                })
                .collect();
            input.extend(labels.tips.iter().map(|t| {
                format!(
                    "^{t}
"
                )
            }));
            let lost: usize = git
                .run_with_input(
                    repo,
                    &ctx,
                    ["rev-list", "--count", "--stdin"],
                    input.as_bytes(),
                )
                .map(|o| {
                    String::from_utf8_lossy(&o.stdout)
                        .trim()
                        .parse()
                        .unwrap_or(1)
                })?;
            if lost > 0 {
                return Ok(None);
            }
        }
        let exclude: String = self.labels.tips.iter().map(|t| format!("^{t}\n")).collect();
        let out = git.run_with_input(
            repo,
            &ctx,
            [
                "-c",
                "log.showSignature=false",
                "log",
                "--exclude=refs/stash",
                "--exclude=refs/nanomgg/*",
                "--all",
                "--topo-order",
                "-z",
                LOG_FORMAT,
                "--stdin",
            ],
            exclude.as_bytes(),
        )?;
        Ok(Some(parse_log(&String::from_utf8_lossy(&out.stdout))))
    }

    pub fn labels_for(&self, hash: &str) -> &[String] {
        self.labels.by_commit.get(hash).map_or(&[], Vec::as_slice)
    }

    /// Row of a full hash, or of the first commit starting with an (at least 4 character) prefix.
    pub fn find(&self, oid: &str) -> Option<usize> {
        let oid = oid.trim().to_ascii_lowercase();
        if let Some(&i) = self.index.get(&oid) {
            return Some(i);
        }
        if oid.len() < 4 || !oid.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        self.commits.iter().position(|c| c.hash.starts_with(&oid))
    }

    /// Rows whose subject, author name/email contain `query` (case-insensitive) or whose hash
    /// starts with it, in graph order.
    pub fn search(&self, query: &str, limit: usize) -> Vec<usize> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let hex = q.len() >= 4 && q.bytes().all(|b| b.is_ascii_hexdigit());
        self.commits
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                (hex && c.hash.starts_with(&q))
                    || c.subject.to_lowercase().contains(&q)
                    || c.author_name.to_lowercase().contains(&q)
                    || c.author_email.to_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .take(limit)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Identity;

    #[test]
    fn labels_are_ordered_and_peeled() {
        let fer = [
            "c1\0\0refs/heads/main\0",
            "c2\0\0refs/heads/topic\0",
            "c1\0\0refs/remotes/origin/main\0",
            "c1\0\0refs/remotes/origin/HEAD\0refs/remotes/origin/main",
            "tagobj\0c2\0refs/tags/v1\0",
            "c1\0\0refs/tags/light\0",
        ]
        .join("\n");
        let l = parse_ref_labels(&fer, Some("refs/heads/main"), Some("c1"));
        assert_eq!(
            l.by_commit["c1"],
            vec![
                "HEAD -> refs/heads/main",
                "refs/remotes/origin/main",
                "tag: refs/tags/light"
            ]
        );
        assert_eq!(
            l.by_commit["c2"],
            vec!["refs/heads/topic", "tag: refs/tags/v1"]
        );
        assert_eq!(l.tips.iter().cloned().collect::<Vec<_>>(), vec!["c1", "c2"]);

        let detached = parse_ref_labels("c1\0\0refs/heads/main\0", None, Some("c9"));
        assert_eq!(detached.by_commit["c9"], vec!["HEAD"]);
        assert!(detached.tips.contains("c9"));
    }

    struct Repo {
        dir: tempfile::TempDir,
        git: GitRunner,
        ctx: AccountContext,
    }

    impl Repo {
        fn new() -> Self {
            let r = Self {
                dir: tempfile::tempdir().unwrap(),
                git: GitRunner::default(),
                ctx: AccountContext {
                    identity: Some(Identity {
                        name: "T".into(),
                        email: "t@x".into(),
                    }),
                    ..Default::default()
                },
            };
            r.run(&["init", "-q", "-b", "main"]);
            r
        }
        fn run(&self, args: &[&str]) -> String {
            self.git.run_text(self.dir.path(), &self.ctx, args).unwrap()
        }
        fn commit(&self, msg: &str) {
            self.run(&["commit", "-q", "--allow-empty", "-m", msg]);
        }
    }

    #[test]
    fn refresh_is_incremental_for_new_commits_and_full_after_rewrites() {
        let r = Repo::new();
        let empty = RepoGraph::load(&r.git, r.dir.path()).unwrap();
        assert!(empty.commits.is_empty());

        r.commit("one");
        r.commit("two");
        let mut g = RepoGraph::load(&r.git, r.dir.path()).unwrap();
        assert_eq!(g.commits.len(), 2);
        assert_eq!(
            g.labels_for(&g.commits[0].hash),
            ["HEAD -> refs/heads/main"]
        );
        let v = g.version;

        assert_eq!(
            g.refresh(&r.git, r.dir.path()).unwrap(),
            (RefreshKind::Unchanged, 0)
        );
        assert_eq!(g.version, v);

        r.commit("three");
        r.run(&["branch", "topic"]);
        r.run(&["checkout", "-q", "topic"]);
        r.commit("four on topic");
        assert_eq!(
            g.refresh(&r.git, r.dir.path()).unwrap(),
            (RefreshKind::Incremental, 2)
        );
        assert_eq!(g.commits.len(), 4);
        assert_eq!(g.commits[0].subject, "four on topic");
        assert_eq!(g.version, v + 1);
        assert_eq!(
            g.labels_for(&g.commits[0].hash),
            ["HEAD -> refs/heads/topic"]
        );
        assert_eq!(g.find(&g.commits[1].hash.clone()), Some(1));

        // A lightweight tag on an existing tip: same commits, new label.
        r.run(&["tag", "v1", "main"]);
        assert_eq!(
            g.refresh(&r.git, r.dir.path()).unwrap(),
            (RefreshKind::LabelsOnly, 0)
        );
        let main_tip = g.find(&r.run(&["rev-parse", "main"])).unwrap();
        assert!(
            g.labels_for(&g.commits[main_tip].hash)
                .contains(&"tag: refs/tags/v1".to_owned())
        );

        // Deleting a branch with unique commits makes them unreachable → full reload.
        r.run(&["checkout", "-q", "main"]);
        r.run(&["branch", "-D", "topic"]);
        assert_eq!(
            g.refresh(&r.git, r.dir.path()).unwrap(),
            (RefreshKind::Full, 3)
        );
        assert_eq!(g.commits.len(), 3);
        assert!(g.commits.iter().all(|c| c.subject != "four on topic"));

        // The same result as loading from scratch.
        let fresh = RepoGraph::load(&r.git, r.dir.path()).unwrap();
        assert_eq!(
            g.commits.iter().map(|c| &c.hash).collect::<Vec<_>>(),
            fresh.commits.iter().map(|c| &c.hash).collect::<Vec<_>>()
        );
    }

    #[test]
    fn moving_refs_without_losing_commits_stays_incremental() {
        let r = Repo::new();
        for m in ["a", "b", "c"] {
            r.commit(m);
        }
        r.run(&["branch", "side", "HEAD~2"]);
        let mut g = RepoGraph::load(&r.git, r.dir.path()).unwrap();

        // Fast-forward an existing branch onto a commit that is already loaded.
        r.run(&["branch", "-f", "side", "main"]);
        assert_eq!(
            g.refresh(&r.git, r.dir.path()).unwrap(),
            (RefreshKind::Incremental, 0)
        );

        // Rewinding main drops commit "c" from history: must reload.
        r.run(&["branch", "-f", "side", "HEAD~1"]);
        r.run(&["reset", "-q", "--hard", "HEAD~1"]);
        assert_eq!(
            g.refresh(&r.git, r.dir.path()).unwrap(),
            (RefreshKind::Full, 2)
        );
        assert!(g.commits.iter().all(|c| c.subject != "c"));
    }

    #[test]
    fn backup_refs_of_rewritten_history_are_not_drawn() {
        let r = Repo::new();
        r.commit("a");
        r.commit("old b");
        r.run(&["update-ref", "refs/nanomgg/backup/1700000000", "HEAD"]);
        r.run(&["reset", "-q", "--hard", "HEAD~1"]);
        r.commit("new b");
        let g = RepoGraph::load(&r.git, r.dir.path()).unwrap();
        let subjects: Vec<_> = g.commits.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(subjects, ["new b", "a"]);
    }

    #[test]
    fn find_and_search() {
        let r = Repo::new();
        r.commit("feat: thêm đăng nhập");
        r.commit("fix: Login crash");
        r.commit("docs: readme");
        let g = RepoGraph::load(&r.git, r.dir.path()).unwrap();
        let h = g.commits[2].hash.clone();
        assert_eq!(g.find(&h), Some(2));
        assert_eq!(g.find(&h[..7]), Some(2));
        assert_eq!(g.find(&h[..7].to_uppercase()), Some(2));
        assert_eq!(g.find("abc"), None, "too short for a prefix");
        assert_eq!(g.find("--all"), None);

        assert_eq!(g.search("login", 10), vec![1]);
        assert_eq!(g.search("ĐĂNG", 10), vec![2]);
        assert_eq!(g.search("t@x", 10).len(), 3, "author email");
        assert_eq!(g.search("t@x", 2).len(), 2, "limit");
        assert_eq!(g.search(&h[..8], 10), vec![2]);
        assert!(g.search("   ", 10).is_empty());
    }
}
