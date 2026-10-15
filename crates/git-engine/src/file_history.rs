//! History of one file (`git log --follow`, following renames), page by page, plus reading the
//! file as of any commit for the read-only viewer.

use std::path::Path;

use serde::Serialize;

use crate::blob::{self, check_path, invalid, looks_binary};
use crate::commit::{FileStatus, Person, is_valid_oid};
use crate::{AccountContext, GitError, GitRunner};

/// Largest page served at once.
pub const MAX_PAGE: u32 = 500;
/// Text larger than this is not sent to the viewer.
pub const MAX_VIEW_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FileHistoryEntry {
    pub oid: String,
    pub parents: Vec<String>,
    pub author: Person,
    pub summary: String,
    pub status: FileStatus,
    /// The file's path in this commit.
    pub path: String,
    /// Previous path when this commit renamed / copied the file.
    pub old_path: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FileHistoryPage {
    pub entries: Vec<FileHistoryEntry>,
    /// More commits after this page.
    pub has_more: bool,
}

fn status_of(code: &str) -> FileStatus {
    match code.chars().next() {
        Some('A') => FileStatus::Added,
        Some('M') => FileStatus::Modified,
        Some('D') => FileStatus::Deleted,
        Some('R') => FileStatus::Renamed,
        Some('C') => FileStatus::Copied,
        Some('T') => FileStatus::TypeChanged,
        _ => FileStatus::Unknown,
    }
}

const RECORD: char = '\u{1e}';
const FORMAT: &str = "--format=%x1e%H%x00%P%x00%an%x00%ae%x00%at%x00%s";

/// Parses `git log -z --name-status` output made with [`FORMAT`]. Commits without a file entry
/// (merges) keep the path of the entry before them, or `fallback`.
pub fn parse_log(raw: &str, fallback: &str) -> Vec<FileHistoryEntry> {
    let mut out = Vec::new();
    let mut current = fallback.to_owned();
    for record in raw.split(RECORD).filter(|r| !r.trim().is_empty()) {
        let mut f = record.split('\0');
        let (Some(oid), Some(parents), Some(name), Some(email), Some(time), Some(summary)) =
            (f.next(), f.next(), f.next(), f.next(), f.next(), f.next())
        else {
            continue;
        };
        let rest: Vec<&str> = f.map(|s| s.trim_start_matches('\n')).collect();
        let mut status = FileStatus::Modified;
        let mut path = current.clone();
        let mut old_path = None;
        let mut it = rest.iter().filter(|s| !s.is_empty());
        if let Some(code) = it.next() {
            status = status_of(code);
            if matches!(status, FileStatus::Renamed | FileStatus::Copied) {
                if let (Some(old), Some(new)) = (it.next(), it.next()) {
                    old_path = Some((*old).to_owned());
                    path = (*new).to_owned();
                }
            } else if let Some(p) = it.next() {
                path = (*p).to_owned();
            }
        }
        // Older commits know the file under its previous name.
        current = old_path.clone().unwrap_or_else(|| path.clone());
        out.push(FileHistoryEntry {
            oid: oid.to_owned(),
            parents: parents.split_whitespace().map(str::to_owned).collect(),
            author: Person {
                name: name.to_owned(),
                email: email.to_owned(),
                time: time.trim().parse().unwrap_or(0),
            },
            summary: summary.to_owned(),
            status,
            path,
            old_path,
        });
    }
    out
}

/// Validates a revision from the UI: a hex object id or `HEAD`.
pub(crate) fn check_rev(rev: &str) -> Result<(), GitError> {
    if rev == "HEAD" || is_valid_oid(rev) {
        Ok(())
    } else {
        Err(invalid(&format!("invalid revision: {rev:?}")))
    }
}

/// Commits that changed `path` (following renames), newest first, starting at `rev`
/// (default HEAD). `skip` / `limit` page through the list.
pub fn file_history(
    git: &GitRunner,
    root: &Path,
    path: &str,
    rev: Option<&str>,
    skip: u32,
    limit: u32,
) -> Result<FileHistoryPage, GitError> {
    check_path(path)?;
    let rev = rev.unwrap_or("HEAD");
    check_rev(rev)?;
    let limit = limit.clamp(1, MAX_PAGE);
    let ctx = AccountContext::default();
    // An unborn branch has no history.
    if git
        .run(
            root,
            &ctx,
            ["rev-parse", "-q", "--verify", &format!("{rev}^{{commit}}")],
        )
        .is_err()
    {
        return Ok(FileHistoryPage::default());
    }
    let out = git.run(
        root,
        &ctx,
        [
            "-c",
            "core.quotePath=false",
            "-c",
            "log.showSignature=false",
            "--literal-pathspecs",
            "log",
            "--follow",
            "-M",
            "-z",
            "--name-status",
            "--no-color",
            "--no-ext-diff",
            "--diff-merges=first-parent",
            FORMAT,
            // `--skip` counts commits before `--follow` filters them: skip here instead.
            &format!("--max-count={}", u64::from(skip) + u64::from(limit) + 1),
            rev,
            "--",
            path,
        ],
    )?;
    let mut entries = parse_log(&String::from_utf8_lossy(&out.stdout), path);
    entries.drain(..entries.len().min(skip as usize));
    let has_more = entries.len() > limit as usize;
    entries.truncate(limit as usize);
    Ok(FileHistoryPage { entries, has_more })
}

/// A file as of one commit, for the read-only viewer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FileAtCommit {
    pub exists: bool,
    pub size: u64,
    /// UTF-8 (lossy) text; `None` for binary or too large files.
    pub text: Option<String>,
    pub binary: bool,
    pub too_large: bool,
    /// Stored as a Git LFS pointer (the text is the pointer).
    pub lfs: Option<blob::LfsPointer>,
    pub image: bool,
}

pub fn file_at_commit(
    git: &GitRunner,
    root: &Path,
    commit: &str,
    path: &str,
) -> Result<FileAtCommit, GitError> {
    check_path(path)?;
    if !is_valid_oid(commit) {
        return Err(invalid(&format!("invalid commit id: {commit:?}")));
    }
    let name = format!("{commit}:{path}");
    let mut out = FileAtCommit {
        image: blob::is_image(path),
        ..Default::default()
    };
    let Some(size) = blob::object_size(git, root, &name) else {
        return Ok(out);
    };
    out.exists = true;
    out.size = size;
    if size > MAX_VIEW_BYTES {
        out.too_large = true;
        return Ok(out);
    }
    let bytes = blob::object_bytes(git, root, &name).unwrap_or_default();
    out.lfs = blob::parse_lfs_pointer(&bytes);
    if looks_binary(&bytes) {
        out.binary = true;
    } else {
        out.text = Some(String::from_utf8_lossy(&bytes).into_owned());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blob::tests::Repo;

    #[test]
    fn follows_renames_and_pages() {
        let r = Repo::new();
        r.write("old name.txt", "a\nb\nc\nd\n");
        let c1 = r.commit_all("add");
        r.write("old name.txt", "a\nb\nc\nd\ne\n");
        r.commit_all("edit old");
        std::fs::create_dir_all(r.p().join("dir")).unwrap();
        r.run(&["mv", "old name.txt", "dir/new.txt"]);
        r.commit_all("rename");
        r.write("other.txt", "x");
        r.commit_all("unrelated");
        r.write("dir/new.txt", "a\nb\nc\nd\ne\nf\n");
        let c5 = r.commit_all("edit new");

        let page = file_history(&r.git, r.p(), "dir/new.txt", None, 0, 100).unwrap();
        let subjects: Vec<_> = page.entries.iter().map(|e| e.summary.as_str()).collect();
        assert_eq!(subjects, ["edit new", "rename", "edit old", "add"]);
        assert!(!page.has_more);
        assert_eq!(page.entries[0].oid, c5);
        assert_eq!(page.entries[1].status, FileStatus::Renamed);
        assert_eq!(page.entries[1].path, "dir/new.txt");
        assert_eq!(page.entries[1].old_path.as_deref(), Some("old name.txt"));
        assert_eq!(page.entries[2].path, "old name.txt");
        assert_eq!(page.entries[3].status, FileStatus::Added);
        assert_eq!(page.entries[3].oid, c1);
        assert_eq!(page.entries[3].author.name, "T");

        let first = file_history(&r.git, r.p(), "dir/new.txt", None, 0, 2).unwrap();
        assert!(first.has_more);
        assert_eq!(first.entries.len(), 2);
        let second = file_history(&r.git, r.p(), "dir/new.txt", None, 2, 2).unwrap();
        assert_eq!(second.entries[0].summary, "edit old");
        assert_eq!(second.entries[0].path, "old name.txt");
        assert!(!second.has_more);

        // Starting at an older commit.
        let at = file_history(&r.git, r.p(), "old name.txt", Some(&c1), 0, 10).unwrap();
        assert_eq!(at.entries.len(), 1);

        let f = file_at_commit(&r.git, r.p(), &c1, "old name.txt").unwrap();
        assert!(f.exists && !f.binary);
        assert_eq!(f.text.as_deref(), Some("a\nb\nc\nd\n"));
        assert!(
            !file_at_commit(&r.git, r.p(), &c1, "dir/new.txt")
                .unwrap()
                .exists
        );
    }

    #[test]
    fn rejects_bad_input_and_handles_unborn() {
        let r = Repo::new();
        assert!(file_history(&r.git, r.p(), "a", Some("--all"), 0, 1).is_err());
        assert!(file_history(&r.git, r.p(), "-p", None, 0, 1).is_err());
        assert!(file_at_commit(&r.git, r.p(), "HEAD~1", "a").is_err());
        let empty = file_history(&r.git, r.p(), "a", None, 0, 10).unwrap();
        assert!(empty.entries.is_empty() && !empty.has_more);
    }
}
