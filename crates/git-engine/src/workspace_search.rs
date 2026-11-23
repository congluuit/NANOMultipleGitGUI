//! Searching the files of several repositories at once (the search panel's "All open
//! repositories" scope).
//!
//! Each repository is searched with the same `git grep` as the single-repository search
//! ([`edit::search`](crate::edit::search)), a few at a time. Every repository's result is handed
//! to a callback as soon as it is known, so the UI fills in while slower repositories are still
//! being searched. A shared budget caps the total number of matching lines, and the
//! [`CancelToken`] stops every running `git grep` within a few milliseconds (the query changed,
//! or the user cancelled).

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use serde::Serialize;

use crate::GitRunner;
use crate::edit::{SearchMatch, SearchOptions, search_stoppable};
use crate::progress::CancelToken;

/// Longest error reason sent to the UI.
const MAX_REASON_CHARS: usize = 200;

/// One repository to search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchTarget {
    pub repo_id: i64,
    pub name: String,
    /// `None`: the folder is gone (moved, deleted, unplugged drive).
    pub root: Option<PathBuf>,
}

/// Limits of a multi-repository search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchLimits {
    /// Matching lines over every repository together.
    pub total: usize,
    /// Matching lines of one repository, so one big repository cannot fill the whole list.
    pub per_repo: usize,
    /// Repositories searched at the same time.
    pub concurrency: usize,
}

/// The result of one repository.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RepoSearchResult {
    pub repo_id: i64,
    pub name: String,
    pub matches: Vec<SearchMatch>,
    /// More lines matched than were kept (per-repository or total limit).
    pub truncated: bool,
    /// The folder is gone.
    pub missing: bool,
    /// Why the search failed (first line of git's message), `None` when it worked.
    pub error: Option<String>,
}

/// How a multi-repository search ended.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSearchSummary {
    /// Repositories whose result was reported.
    pub searched: u32,
    /// Matching lines reported over every repository.
    pub total: u32,
    /// The total limit was reached: some lines (or whole repositories) were left out.
    pub truncated: bool,
    /// Repositories not searched because the total limit was reached first.
    pub skipped: u32,
    /// Stopped by the [`CancelToken`]; results reported before that still stand.
    pub cancelled: bool,
}

/// First meaningful line of git's error, without the `fatal: ` / `error: ` prefix.
fn short_reason(e: &crate::GitError) -> String {
    let text = match e {
        crate::GitError::Failed { stderr, .. } if !stderr.trim().is_empty() => stderr.clone(),
        other => other.to_string(),
    };
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("search failed");
    let line = line
        .strip_prefix("fatal: ")
        .or_else(|| line.strip_prefix("error: "))
        .unwrap_or(line);
    let mut out: String = line.chars().take(MAX_REASON_CHARS).collect();
    if line.chars().count() > MAX_REASON_CHARS {
        out.push_str("...");
    }
    out
}

/// Searches every target, `limits.concurrency` at a time, and calls `on_result` once per
/// repository as soon as its result is known (in completion order, from worker threads).
/// Repositories without any match are reported too (with no matches), so the UI can count
/// them as done. Nothing is reported after `cancel` is set.
pub fn search_repos(
    git: &GitRunner,
    targets: &[SearchTarget],
    query: &str,
    opts: SearchOptions,
    limits: SearchLimits,
    cancel: &CancelToken,
    on_result: &(dyn Fn(RepoSearchResult) + Sync),
) -> WorkspaceSearchSummary {
    let total_limit = limits.total.max(1);
    let per_repo = limits.per_repo.clamp(1, total_limit);
    let workers = limits.concurrency.clamp(1, targets.len().max(1));

    let next = AtomicUsize::new(0);
    // Lines that may still be reported; reaching 0 stops every running search.
    let budget = Mutex::new(total_limit);
    let exhausted = AtomicBool::new(false);
    let summary = Mutex::new(WorkspaceSearchSummary::default());
    let should_stop = || cancel.is_cancelled() || exhausted.load(Ordering::SeqCst);

    let worker = || {
        loop {
            if should_stop() {
                return;
            }
            let i = next.fetch_add(1, Ordering::SeqCst);
            let Some(target) = targets.get(i) else {
                return;
            };
            let mut result = RepoSearchResult {
                repo_id: target.repo_id,
                name: target.name.clone(),
                ..Default::default()
            };
            match &target.root {
                None => result.missing = true,
                Some(root) => {
                    let limit = (*budget.lock().unwrap_or_else(|e| e.into_inner())).min(per_repo);
                    match search_stoppable(git, root, query, opts, limit.max(1), &should_stop) {
                        // Stopped half-way: by a cancel (nothing more is reported) or by the
                        // total limit (this repository counts as not searched).
                        Ok(run) if run.stopped => return,
                        Ok(run) => {
                            result.matches = run.results.matches;
                            result.truncated = run.results.truncated;
                        }
                        Err(e) => result.error = Some(short_reason(&e)),
                    }
                }
            }
            // Report under the lock so the budget, the summary and the order of the
            // callbacks agree, and a cancel that came meanwhile is honoured.
            let mut left = budget.lock().unwrap_or_else(|e| e.into_inner());
            if cancel.is_cancelled() {
                return;
            }
            if exhausted.load(Ordering::SeqCst) {
                // Another repository used the last of the budget while this one ran.
                return;
            }
            if result.matches.len() > *left {
                result.matches.truncate(*left);
                result.truncated = true;
            }
            *left -= result.matches.len();
            let mut s = summary.lock().unwrap_or_else(|e| e.into_inner());
            s.searched += 1;
            s.total += result.matches.len() as u32;
            if *left == 0 {
                s.truncated = true;
                exhausted.store(true, Ordering::SeqCst);
            }
            drop(s);
            on_result(result);
            drop(left);
        }
    };

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(worker);
        }
    });

    let mut s = summary.into_inner().unwrap_or_else(|e| e.into_inner());
    s.cancelled = cancel.is_cancelled();
    if !s.cancelled {
        s.skipped = (targets.len() as u32).saturating_sub(s.searched);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blob::tests::Repo;

    fn limits(total: usize, per_repo: usize) -> SearchLimits {
        SearchLimits {
            total,
            per_repo,
            concurrency: 4,
        }
    }

    fn target(id: i64, repo: &Repo) -> SearchTarget {
        SearchTarget {
            repo_id: id,
            name: format!("repo{id}"),
            root: Some(repo.p().to_path_buf()),
        }
    }

    fn collect(
        git: &GitRunner,
        targets: &[SearchTarget],
        query: &str,
        opts: SearchOptions,
        limits: SearchLimits,
        cancel: &CancelToken,
    ) -> (Vec<RepoSearchResult>, WorkspaceSearchSummary) {
        let got = Mutex::new(Vec::new());
        let summary = search_repos(git, targets, query, opts, limits, cancel, &|r| {
            got.lock().unwrap().push(r);
        });
        let mut got = got.into_inner().unwrap();
        got.sort_by_key(|r| r.repo_id);
        (got, summary)
    }

    #[test]
    fn searches_every_repository_and_reports_failures() {
        let a = Repo::new();
        a.write("a.txt", "needle one\nhay\nNeedle two\n");
        a.commit_all("init");
        let b = Repo::new();
        b.write("src/b.rs", "// no match here\n");
        b.commit_all("init");
        let c = Repo::new();
        c.write("c.md", "a needle\n");
        c.commit_all("init");
        let not_git = tempfile::tempdir().unwrap();
        let targets = vec![
            target(1, &a),
            target(2, &b),
            target(3, &c),
            SearchTarget {
                repo_id: 4,
                name: "gone".into(),
                root: None,
            },
            SearchTarget {
                repo_id: 5,
                name: "plain folder".into(),
                root: Some(not_git.path().to_path_buf()),
            },
        ];
        let (got, summary) = collect(
            &a.git,
            &targets,
            "needle",
            SearchOptions::default(),
            limits(100, 100),
            &CancelToken::new(),
        );
        assert_eq!(got.len(), 5);
        let counts: Vec<_> = got.iter().map(|r| (r.repo_id, r.matches.len())).collect();
        assert_eq!(counts, [(1, 2), (2, 0), (3, 1), (4, 0), (5, 0)]);
        assert_eq!(got[0].name, "repo1");
        assert_eq!(got[0].matches[1].line, 3);
        assert!(got[3].missing && got[3].error.is_none());
        let reason = got[4].error.as_deref().unwrap();
        assert!(
            !reason.starts_with("fatal:") && !reason.contains('\n'),
            "{reason}"
        );
        assert_eq!(
            summary,
            WorkspaceSearchSummary {
                searched: 5,
                total: 3,
                truncated: false,
                skipped: 0,
                cancelled: false,
            }
        );

        // The options are the single-repository ones.
        let case = SearchOptions {
            case_sensitive: true,
            ..Default::default()
        };
        let (got, summary) = collect(
            &a.git,
            &targets[..3],
            "Needle",
            case,
            limits(100, 100),
            &CancelToken::new(),
        );
        assert_eq!(summary.total, 1);
        assert_eq!(got[0].matches[0].line, 3);

        // An invalid regular expression fails in every repository, with a short reason.
        let regex = SearchOptions {
            regex: true,
            ..Default::default()
        };
        let (got, _) = collect(
            &a.git,
            &targets[..1],
            "(",
            regex,
            limits(100, 100),
            &CancelToken::new(),
        );
        assert!(got[0].error.is_some());
    }

    #[test]
    fn caps_per_repository_and_in_total() {
        let repos: Vec<Repo> = (0..3)
            .map(|_| {
                let r = Repo::new();
                r.write("f.txt", "hit\n".repeat(10));
                r.commit_all("init");
                r
            })
            .collect();
        let targets: Vec<_> = repos
            .iter()
            .enumerate()
            .map(|(i, r)| target(i as i64 + 1, r))
            .collect();
        let git = &repos[0].git;
        let opts = SearchOptions::default();

        // Per repository: 4 lines each, 12 in all.
        let (got, summary) = collect(
            git,
            &targets,
            "hit",
            opts,
            limits(100, 4),
            &CancelToken::new(),
        );
        assert!(got.iter().all(|r| r.matches.len() == 4 && r.truncated));
        assert_eq!(summary.total, 12);
        assert!(!summary.truncated);

        // In total: never more than 15 lines; the limit marks the summary truncated.
        for concurrency in [1, 3] {
            let l = SearchLimits {
                total: 15,
                per_repo: 100,
                concurrency,
            };
            let (got, summary) = collect(git, &targets, "hit", opts, l, &CancelToken::new());
            let total: usize = got.iter().map(|r| r.matches.len()).sum();
            assert_eq!(total, 15);
            assert_eq!(summary.total, 15);
            assert!(summary.truncated);
            assert_eq!(summary.searched + summary.skipped, 3);
            assert!(got.iter().any(|r| r.truncated));
        }
    }

    #[test]
    fn a_cancel_stops_at_once_and_reports_nothing_more() {
        let r = Repo::new();
        r.write("a.txt", "x\n".repeat(50));
        r.commit_all("init");
        let targets: Vec<_> = (1..=4).map(|i| target(i, &r)).collect();
        let opts = SearchOptions::default();

        // Cancelled before the start: nothing runs, nothing is reported.
        let cancel = CancelToken::new();
        cancel.cancel();
        let (got, summary) = collect(&r.git, &targets, "x", opts, limits(100, 100), &cancel);
        assert!(got.is_empty());
        assert!(summary.cancelled && summary.searched == 0 && summary.skipped == 0);

        // Cancelled once the first repository is reported: one at a time, the others never
        // report anything.
        let cancel = CancelToken::new();
        let got = Mutex::new(Vec::new());
        let one_at_a_time = SearchLimits {
            total: 100,
            per_repo: 100,
            concurrency: 1,
        };
        let summary = search_repos(
            &r.git,
            &targets,
            "x",
            opts,
            one_at_a_time,
            &cancel,
            &|res| {
                got.lock().unwrap().push(res.repo_id);
                cancel.cancel();
            },
        );
        assert_eq!(got.into_inner().unwrap(), [1]);
        assert!(summary.cancelled);
        assert_eq!(summary.searched, 1);
    }

    #[test]
    fn a_stop_kills_git_while_it_runs() {
        let r = Repo::new();
        r.write("a.txt", "x\n".repeat(5000));
        r.commit_all("init");
        let opts = SearchOptions::default();
        // Not stopped: every line.
        let run = search_stoppable(&r.git, r.p(), "x", opts, 10_000, &|| false).unwrap();
        assert!(!run.stopped);
        assert_eq!(run.results.matches.len(), 5000);

        // Asked to stop right after git started (the first check is before the spawn): git is
        // killed before a single line is read, and that is not an error.
        let calls = AtomicUsize::new(0);
        let stop = || calls.fetch_add(1, Ordering::SeqCst) >= 1;
        let run = search_stoppable(&r.git, r.p(), "x", opts, 10_000, &stop).unwrap();
        assert!(run.stopped);
        assert!(run.results.matches.is_empty());

        // Stopping before the start does not even spawn git.
        let run = search_stoppable(&r.git, r.p(), "x", opts, 10_000, &|| true).unwrap();
        assert!(run.stopped && run.results.matches.is_empty());
    }
}
