//! `git blame --porcelain` of one file at a revision (or the working tree), parsed into the
//! file's lines, the commit each line comes from and the commits' metadata.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use crate::blob::{self, check_path};
use crate::file_history::check_rev;
use crate::{AccountContext, GitError, GitRunner};

/// Files larger than this are not blamed.
pub const MAX_BLAME_BYTES: u64 = 5 * 1024 * 1024;
const ZERO_OID: &str = "0000000000000000000000000000000000000000";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct BlameCommit {
    pub oid: String,
    pub author: String,
    pub email: String,
    /// Author time, seconds since the Unix epoch.
    pub time: i64,
    pub summary: String,
    /// Path of the file in this commit (differs after a rename).
    pub filename: String,
    /// Parent commit and the file's path there, to blame "before this change".
    pub previous: Option<String>,
    pub previous_path: Option<String>,
    /// Root / boundary commit: there is nothing older to blame.
    pub boundary: bool,
    /// Lines changed in the working tree that are not committed yet.
    pub uncommitted: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct BlameResult {
    pub path: String,
    /// Revision blamed; `None` = working tree.
    pub rev: Option<String>,
    /// Text of each line (without line ending).
    pub lines: Vec<String>,
    /// For each line: index into `commits`.
    pub line_commits: Vec<u32>,
    /// For each line: its line number in that commit's version of the file.
    pub orig_lines: Vec<u32>,
    pub commits: Vec<BlameCommit>,
    pub too_large: bool,
    pub binary: bool,
}

/// Parses `git blame --porcelain` output.
pub fn parse_porcelain(raw: &[u8]) -> BlameResult {
    let mut out = BlameResult::default();
    let mut index: HashMap<String, u32> = HashMap::new();
    let mut current: Option<u32> = None;
    let mut orig = 0u32;
    for line in raw.split(|&b| b == b'\n') {
        if let Some(content) = line.strip_prefix(b"\t") {
            let content = content.strip_suffix(b"\r").unwrap_or(content);
            if content.contains(&0) {
                out.binary = true;
            }
            out.lines
                .push(String::from_utf8_lossy(content).into_owned());
            out.line_commits.push(current.unwrap_or(0));
            out.orig_lines.push(orig);
            continue;
        }
        let text = String::from_utf8_lossy(line);
        let (key, value) = text.split_once(' ').unwrap_or((&text, ""));
        if key.len() == 40 && key.bytes().all(|b| b.is_ascii_hexdigit()) {
            orig = value
                .split(' ')
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            let next = out.commits.len() as u32;
            let i = *index.entry(key.to_owned()).or_insert_with(|| {
                out.commits.push(BlameCommit {
                    oid: key.to_owned(),
                    uncommitted: key == ZERO_OID,
                    ..Default::default()
                });
                next
            });
            current = Some(i);
            continue;
        }
        let Some(c) = current.and_then(|i| out.commits.get_mut(i as usize)) else {
            continue;
        };
        match key {
            "author" => c.author = value.to_owned(),
            "author-mail" => {
                c.email = value
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .to_owned();
            }
            "author-time" => c.time = value.parse().unwrap_or(0),
            "summary" => c.summary = value.to_owned(),
            "filename" => c.filename = value.to_owned(),
            "boundary" => c.boundary = true,
            "previous" => {
                if let Some((oid, path)) = value.split_once(' ') {
                    c.previous = Some(oid.to_owned());
                    c.previous_path = Some(path.to_owned());
                }
            }
            _ => {}
        }
    }
    out
}

/// Blames `path` at `rev` (hex id or `HEAD`), or the working tree when `rev` is `None`.
pub fn blame(
    git: &GitRunner,
    root: &Path,
    path: &str,
    rev: Option<&str>,
) -> Result<BlameResult, GitError> {
    check_path(path)?;
    if let Some(rev) = rev {
        check_rev(rev)?;
    }
    let size = match rev {
        Some(rev) => blob::object_size(git, root, &format!("{rev}:{path}")),
        None => std::fs::metadata(root.join(path)).ok().map(|m| m.len()),
    };
    let too_large = BlameResult {
        path: path.to_owned(),
        rev: rev.map(str::to_owned),
        too_large: true,
        ..Default::default()
    };
    if size.is_some_and(|s| s > MAX_BLAME_BYTES) {
        return Ok(too_large);
    }
    let mut args = vec![
        "-c",
        "core.quotePath=false",
        "--literal-pathspecs",
        "blame",
        "--porcelain",
    ];
    if let Some(rev) = rev {
        args.push(rev);
    }
    args.extend(["--", path]);
    let out = git.run(root, &AccountContext::default(), &args)?;
    let mut result = parse_porcelain(&out.stdout);
    result.path = path.to_owned();
    result.rev = rev.map(str::to_owned);
    if result.binary {
        result.lines.clear();
        result.line_commits.clear();
        result.orig_lines.clear();
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blob::tests::Repo;

    #[test]
    fn parses_porcelain_with_repeated_commits() {
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        let raw = format!(
            "{a} 1 1 2\nauthor Ann\nauthor-mail <ann@x>\nauthor-time 100\nsummary first\nboundary\nfilename f.txt\n\tone\n{a} 2 2\n\ttwo\n{b} 1 3 1\nauthor Bob\nauthor-mail <bob@x>\nauthor-time 200\nsummary second\nprevious {a} old.txt\nfilename f.txt\n\tthree\r\n"
        );
        let r = parse_porcelain(raw.as_bytes());
        assert_eq!(r.lines, ["one", "two", "three"]);
        assert_eq!(r.line_commits, [0, 0, 1]);
        assert_eq!(r.orig_lines, [1, 2, 1]);
        assert_eq!(r.commits.len(), 2);
        assert!(r.commits[0].boundary);
        assert_eq!(r.commits[0].email, "ann@x");
        assert_eq!(r.commits[1].previous.as_deref(), Some(a.as_str()));
        assert_eq!(r.commits[1].previous_path.as_deref(), Some("old.txt"));
        assert_eq!(r.commits[1].time, 200);
    }

    #[test]
    fn blames_through_a_rename_and_the_working_tree() {
        let r = Repo::new();
        r.write("a.txt", "one\ntwo\nthree\n");
        let c1 = r.commit_all("first");
        r.run(&["mv", "a.txt", "b.txt"]);
        r.commit_all("rename");
        r.write("b.txt", "one\nTWO\nthree\n");
        let c3 = r.commit_all("second");
        r.write("b.txt", "one\nTWO\nthree\nfour\n");

        let head = blame(&r.git, r.p(), "b.txt", Some("HEAD")).unwrap();
        assert_eq!(head.lines, ["one", "TWO", "three"]);
        let oid = |i: usize| head.commits[head.line_commits[i] as usize].oid.clone();
        assert_eq!(oid(0), c1);
        assert_eq!(oid(1), c3);
        assert_eq!(oid(2), c1);
        // Lines from before the rename carry the old file name.
        assert_eq!(
            head.commits[head.line_commits[0] as usize].filename,
            "a.txt"
        );
        let second = &head.commits[head.line_commits[1] as usize];
        assert_eq!(second.summary, "second");
        assert_eq!(second.previous_path.as_deref(), Some("b.txt"));

        // Blame "before this change" at the parent.
        let parent = second.previous.clone().unwrap();
        let before = blame(&r.git, r.p(), "b.txt", Some(&parent)).unwrap();
        assert_eq!(before.lines, ["one", "two", "three"]);

        let wt = blame(&r.git, r.p(), "b.txt", None).unwrap();
        assert_eq!(wt.lines.len(), 4);
        assert!(wt.commits[wt.line_commits[3] as usize].uncommitted);
        assert!(wt.rev.is_none());
    }

    #[test]
    fn rejects_bad_revisions_and_paths() {
        let r = Repo::new();
        assert!(blame(&r.git, r.p(), "a", Some("--reverse")).is_err());
        assert!(blame(&r.git, r.p(), "--contents=x", None).is_err());
    }
}
