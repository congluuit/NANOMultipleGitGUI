//! Numbers about a repository: who commits, how often, and which files change the most.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use crate::{AccountContext, GitError, GitRunner};

const WEEK: i64 = 7 * 24 * 3600;
/// Weeks of activity shown (one year).
pub const WEEKS: usize = 52;
const TOP_AUTHORS: usize = 20;
const TOP_FILES: usize = 15;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct AuthorStat {
    pub name: String,
    pub email: String,
    pub commits: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FileStat {
    pub path: String,
    /// Commits of the last year that touched the file.
    pub changes: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RepoStats {
    /// Commits reachable from HEAD.
    pub total_commits: u32,
    pub author_count: u32,
    /// Most active authors first (at most 20).
    pub authors: Vec<AuthorStat>,
    /// Commits per week over the last year, oldest week first (always 52 entries).
    pub weeks: Vec<u32>,
    /// Start of the newest week, seconds since the Unix epoch.
    pub last_week_start: i64,
    /// Files changed most often in the last year.
    pub files: Vec<FileStat>,
    pub first_commit: Option<i64>,
    pub last_commit: Option<i64>,
}

/// Parses `git shortlog -sne`: `   12\tName <email>`.
pub fn parse_shortlog(raw: &str) -> Vec<AuthorStat> {
    raw.lines()
        .filter_map(|line| {
            let (count, who) = line.trim().split_once('\t')?;
            let (name, email) = match who.rsplit_once(" <") {
                Some((n, e)) => (n.trim(), e.trim_end_matches('>')),
                None => (who.trim(), ""),
            };
            Some(AuthorStat {
                name: name.to_owned(),
                email: email.to_owned(),
                commits: count.trim().parse().ok()?,
            })
        })
        .collect()
}

/// Parses `git log --format=%x01%at --name-only`: commits per week (relative to `now`) and
/// how often each file changed.
pub fn parse_activity(raw: &str, now: i64) -> (Vec<u32>, Vec<FileStat>) {
    let mut weeks = vec![0u32; WEEKS];
    let mut files: HashMap<&str, u32> = HashMap::new();
    for record in raw.split('\u{1}').filter(|r| !r.trim().is_empty()) {
        let mut lines = record.lines();
        let Some(time) = lines.next().and_then(|t| t.trim().parse::<i64>().ok()) else {
            continue;
        };
        let age = ((now - time).max(0) / WEEK) as usize;
        if age < WEEKS {
            weeks[WEEKS - 1 - age] += 1;
        }
        for path in lines.map(str::trim).filter(|p| !p.is_empty()) {
            *files.entry(path).or_default() += 1;
        }
    }
    let mut files: Vec<FileStat> = files
        .into_iter()
        .map(|(path, changes)| FileStat {
            path: path.to_owned(),
            changes,
        })
        .collect();
    files.sort_by(|a, b| b.changes.cmp(&a.changes).then_with(|| a.path.cmp(&b.path)));
    files.truncate(TOP_FILES);
    (weeks, files)
}

/// Statistics of the history reachable from HEAD (`now`: seconds since the Unix epoch).
pub fn read(git: &GitRunner, repo: &Path, now: i64) -> Result<RepoStats, GitError> {
    let ctx = AccountContext::default();
    if git
        .run(repo, &ctx, ["rev-parse", "-q", "--verify", "HEAD"])
        .is_err()
    {
        return Ok(RepoStats {
            weeks: vec![0; WEEKS],
            last_week_start: now - WEEK,
            ..Default::default()
        });
    }
    let total = git
        .run_text(repo, &ctx, ["rev-list", "--count", "HEAD"])?
        .parse()
        .unwrap_or(0);
    let mut authors = parse_shortlog(&git.run_text(repo, &ctx, ["shortlog", "-sne", "HEAD"])?);
    let author_count = authors.len() as u32;
    authors.truncate(TOP_AUTHORS);
    let activity = git.run(
        repo,
        &ctx,
        [
            "-c",
            "core.quotePath=false",
            "log",
            "--no-merges",
            "--since=52.weeks",
            "--format=%x01%at",
            "--name-only",
            "HEAD",
        ],
    )?;
    let (weeks, files) = parse_activity(&String::from_utf8_lossy(&activity.stdout), now);
    let time = |args: &[&str]| -> Option<i64> {
        git.run_text(repo, &ctx, args)
            .ok()?
            .lines()
            .next()?
            .trim()
            .parse()
            .ok()
    };
    Ok(RepoStats {
        total_commits: total,
        author_count,
        authors,
        weeks,
        last_week_start: now - WEEK,
        files,
        first_commit: time(&["log", "--max-parents=0", "--format=%at", "HEAD"]),
        last_commit: time(&["log", "-1", "--format=%at", "HEAD"]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::test_support::TestRepo;

    #[test]
    fn counts_commits_authors_and_files() {
        let repo = TestRepo::new();
        repo.commit("a.txt", "1\n", "one");
        repo.commit("a.txt", "2\n", "two");
        repo.commit("b.txt", "1\n", "three");
        let now = repo
            .git(&["log", "-1", "--format=%at"])
            .parse::<i64>()
            .unwrap()
            + 10;

        let stats = read(&repo.git, &repo.path, now).unwrap();
        assert_eq!(stats.total_commits, 4);
        assert_eq!(stats.author_count, 1);
        assert_eq!(stats.authors[0].name, "Tester");
        assert_eq!(stats.authors[0].email, "tester@example.com");
        assert_eq!(stats.authors[0].commits, 4);
        assert_eq!(stats.weeks.len(), WEEKS);
        assert_eq!(stats.weeks[WEEKS - 1], 4);
        assert_eq!(stats.files[0].path, "a.txt");
        assert_eq!(stats.files[0].changes, 2);
        assert!(stats.first_commit.is_some());
        assert!(stats.last_commit >= stats.first_commit);
    }

    #[test]
    fn parses_activity_into_weeks() {
        let now = 100 * WEEK;
        let raw = format!(
            "\u{1}{}\n\nsrc/a.rs\nsrc/b.rs\n\u{1}{}\n\nsrc/a.rs\n\u{1}{}\n\nold.rs\n",
            now - 10,
            now - WEEK - 10,
            now - 60 * WEEK
        );
        let (weeks, files) = parse_activity(&raw, now);
        assert_eq!(weeks[WEEKS - 1], 1);
        assert_eq!(weeks[WEEKS - 2], 1);
        assert_eq!(weeks.iter().sum::<u32>(), 2);
        assert_eq!(files[0].path, "src/a.rs");
        assert_eq!(files[0].changes, 2);
    }

    #[test]
    fn parses_shortlog_lines() {
        let authors = parse_shortlog("    12\tAnn Lee <ann@x.io>\n     3\tbot\n");
        assert_eq!(authors.len(), 2);
        assert_eq!(authors[0].commits, 12);
        assert_eq!(authors[0].name, "Ann Lee");
        assert_eq!(authors[0].email, "ann@x.io");
        assert_eq!(authors[1].name, "bot");
    }
}
