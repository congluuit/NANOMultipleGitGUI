//! Details of a single commit: metadata, full message and changed files with line counts.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use crate::{AccountContext, GitError, GitRunner};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Person {
    pub name: String,
    pub email: String,
    /// Seconds since the Unix epoch.
    pub time: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Unknown,
}

impl FileStatus {
    fn from_code(code: &str) -> Self {
        match code.chars().next() {
            Some('A') => Self::Added,
            Some('M') => Self::Modified,
            Some('D') => Self::Deleted,
            Some('R') => Self::Renamed,
            Some('C') => Self::Copied,
            Some('T') => Self::TypeChanged,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub status: FileStatus,
    pub path: String,
    /// Previous path for renames and copies.
    pub old_path: Option<String>,
    /// `None` for binary files.
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CommitDetails {
    pub hash: String,
    pub parents: Vec<String>,
    pub author: Person,
    pub committer: Person,
    pub summary: String,
    /// Message after the summary line, trimmed.
    pub body: String,
    pub files: Vec<ChangedFile>,
    pub additions: u32,
    pub deletions: u32,
    /// For merge commits, files are listed against this (first) parent.
    pub compared_to: Option<String>,
}

/// Full or abbreviated object id. Rejects anything else so user input can never become a git
/// option (e.g. `--output=...`).
pub fn is_valid_oid(s: &str) -> bool {
    (4..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Parses `git diff-tree --name-status -z` output.
pub fn parse_name_status(raw: &str) -> Vec<(FileStatus, String, Option<String>)> {
    let mut out = Vec::new();
    let mut it = raw.split('\0').filter(|s| !s.is_empty());
    while let Some(code) = it.next() {
        let status = FileStatus::from_code(code);
        if matches!(status, FileStatus::Renamed | FileStatus::Copied) {
            let (Some(old), Some(new)) = (it.next(), it.next()) else {
                break;
            };
            out.push((status, new.to_owned(), Some(old.to_owned())));
        } else if let Some(path) = it.next() {
            out.push((status, path.to_owned(), None));
        }
    }
    out
}

/// Parses `git diff-tree --numstat -z` output into `path -> (additions, deletions)`;
/// binary files map to `(None, None)`. Renames are keyed by the new path.
pub fn parse_numstat(raw: &str) -> HashMap<String, (Option<u32>, Option<u32>)> {
    let mut out = HashMap::new();
    let mut it = raw.split('\0');
    while let Some(record) = it.next() {
        let mut f = record.splitn(3, '\t');
        let (Some(a), Some(d), Some(path)) = (f.next(), f.next(), f.next()) else {
            continue;
        };
        let path = if path.is_empty() {
            let _old = it.next();
            match it.next() {
                Some(new) => new,
                None => break,
            }
        } else {
            path
        };
        out.insert(path.to_owned(), (a.parse().ok(), d.parse().ok()));
    }
    out
}

const META_FORMAT: &str = "--format=%H%x00%P%x00%an%x00%ae%x00%at%x00%cn%x00%ce%x00%ct%x00%B";

pub fn commit_details(git: &GitRunner, repo: &Path, oid: &str) -> Result<CommitDetails, GitError> {
    if !is_valid_oid(oid) {
        return Err(GitError::Failed {
            args: "show".into(),
            status: "rejected".into(),
            stderr: format!("invalid commit id: {oid:?}"),
        });
    }
    let ctx = AccountContext::default();
    let meta = git.run(
        repo,
        &ctx,
        [
            "-c",
            "log.showSignature=false",
            "show",
            "-s",
            META_FORMAT,
            oid,
            "--",
        ],
    )?;
    let meta = String::from_utf8_lossy(&meta.stdout);
    let f: Vec<&str> = meta.splitn(9, '\0').collect();
    if f.len() < 9 {
        return Err(GitError::Failed {
            args: "show".into(),
            status: "parse".into(),
            stderr: "unexpected output from git show".into(),
        });
    }
    let hash = f[0].to_owned();
    let parents: Vec<String> = f[1].split_whitespace().map(str::to_owned).collect();
    let person = |n: &str, e: &str, t: &str| Person {
        name: n.to_owned(),
        email: e.to_owned(),
        time: t.trim().parse().unwrap_or(0),
    };
    let message = f[8].trim_end();
    let (summary, body) = match message.split_once('\n') {
        Some((s, b)) => (s.trim().to_owned(), b.trim().to_owned()),
        None => (message.trim().to_owned(), String::new()),
    };

    // Root commits are compared with the empty tree, merges with their first parent.
    let compared_to = parents.first().cloned();
    let range: Vec<String> = match &compared_to {
        Some(p) => vec![p.clone(), hash.clone()],
        None => vec!["--root".into(), hash.clone()],
    };
    let diff = |kind: &str| -> Result<String, GitError> {
        let mut args = vec![
            "diff-tree".to_owned(),
            "-r".to_owned(),
            "-M".to_owned(),
            "-z".to_owned(),
            "--no-commit-id".to_owned(),
            kind.to_owned(),
        ];
        args.extend(range.iter().cloned());
        let out = git.run(repo, &ctx, &args)?;
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let stats = parse_numstat(&diff("--numstat")?);
    let mut additions = 0;
    let mut deletions = 0;
    let files = parse_name_status(&diff("--name-status")?)
        .into_iter()
        .map(|(status, path, old_path)| {
            let (a, d) = stats.get(&path).copied().unwrap_or((None, None));
            additions += a.unwrap_or(0);
            deletions += d.unwrap_or(0);
            ChangedFile {
                status,
                path,
                old_path,
                additions: a,
                deletions: d,
            }
        })
        .collect();

    Ok(CommitDetails {
        compared_to: (parents.len() > 1).then(|| parents[0].clone()),
        hash,
        parents,
        author: person(f[2], f[3], f[4]),
        committer: person(f[5], f[6], f[7]),
        summary,
        body,
        files,
        additions,
        deletions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Identity;

    #[test]
    fn oid_validation() {
        assert!(is_valid_oid("abcd"));
        assert!(is_valid_oid(&"a".repeat(40)));
        assert!(!is_valid_oid("abc"));
        assert!(!is_valid_oid("--output=x"));
        assert!(!is_valid_oid("HEAD"));
        assert!(!is_valid_oid(&"a".repeat(65)));
    }

    #[test]
    fn parses_name_status_with_renames() {
        let raw = "M\0src/a.rs\0R087\0old/b.rs\0new/b.rs\0A\0tên file.txt\0D\0gone\0";
        assert_eq!(
            parse_name_status(raw),
            vec![
                (FileStatus::Modified, "src/a.rs".into(), None),
                (
                    FileStatus::Renamed,
                    "new/b.rs".into(),
                    Some("old/b.rs".into())
                ),
                (FileStatus::Added, "tên file.txt".into(), None),
                (FileStatus::Deleted, "gone".into(), None),
            ]
        );
    }

    #[test]
    fn parses_numstat_with_renames_and_binaries() {
        let raw = "3\t1\tsrc/a.rs\0-\t-\timg.png\x000\t0\t\0old/b.rs\0new/b.rs\0";
        let s = parse_numstat(raw);
        assert_eq!(s["src/a.rs"], (Some(3), Some(1)));
        assert_eq!(s["img.png"], (None, None));
        assert_eq!(s["new/b.rs"], (Some(0), Some(0)));
    }

    #[test]
    fn details_of_root_normal_and_merge_commits() {
        let dir = tempfile::tempdir().unwrap();
        let git = GitRunner::default();
        let ctx = AccountContext {
            identity: Some(Identity {
                name: "Nguyễn A".into(),
                email: "a@x.dev".into(),
            }),
            ..Default::default()
        };
        let p = dir.path();
        let run = |args: &[&str]| git.run_text(p, &ctx, args).unwrap();
        run(&["init", "-q", "-b", "main"]);
        std::fs::write(p.join("a.txt"), "1\n2\n3\n").unwrap();
        std::fs::write(p.join("bin.dat"), [0u8, 159, 146, 150]).unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "root commit"]);
        let root = run(&["rev-parse", "HEAD"]);

        std::fs::write(p.join("a.txt"), "1\nhai\n3\n4\n").unwrap();
        run(&["mv", "bin.dat", "moved.dat"]);
        run(&[
            "commit",
            "-qam",
            "update\n\nLonger explanation\nover two lines.",
        ]);
        let second = run(&["rev-parse", "HEAD"]);

        run(&["checkout", "-q", "-b", "topic"]);
        std::fs::write(p.join("t.txt"), "t\n").unwrap();
        run(&["add", "t.txt"]);
        run(&["commit", "-q", "-m", "topic work"]);
        run(&["checkout", "-q", "main"]);
        run(&["merge", "-q", "--no-ff", "-m", "merge topic", "topic"]);
        let merge = run(&["rev-parse", "HEAD"]);

        let d = commit_details(&git, p, &root).unwrap();
        assert_eq!(d.summary, "root commit");
        assert!(d.parents.is_empty() && d.compared_to.is_none());
        assert_eq!(d.author.name, "Nguyễn A");
        assert_eq!(d.files.len(), 2);
        let bin = d.files.iter().find(|f| f.path == "bin.dat").unwrap();
        assert_eq!((bin.additions, bin.deletions), (None, None), "binary");
        assert_eq!((d.additions, d.deletions), (3, 0));

        let d = commit_details(&git, p, &second[..10]).unwrap();
        assert_eq!(d.hash, second);
        assert_eq!(d.summary, "update");
        assert_eq!(d.body, "Longer explanation\nover two lines.");
        let a = d.files.iter().find(|f| f.path == "a.txt").unwrap();
        assert_eq!(a.status, FileStatus::Modified);
        assert_eq!((a.additions, a.deletions), (Some(2), Some(1)));
        let mv = d.files.iter().find(|f| f.path == "moved.dat").unwrap();
        assert_eq!(mv.status, FileStatus::Renamed);
        assert_eq!(mv.old_path.as_deref(), Some("bin.dat"));

        let d = commit_details(&git, p, &merge).unwrap();
        assert_eq!(d.parents.len(), 2);
        assert_eq!(d.compared_to.as_deref(), Some(second.as_str()));
        assert_eq!(d.files.len(), 1);
        assert_eq!(d.files[0].path, "t.txt");

        assert!(commit_details(&git, p, "--output=pwned").is_err());
        assert!(!p.join("pwned").exists());
    }
}
