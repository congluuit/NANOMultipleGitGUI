//! Patch files: save a commit or the uncommitted changes as a patch, look inside a patch
//! file and apply it (as changes, or as commits when it carries them).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::commit::is_valid_oid;
use crate::ops::{self, FailureKind, OpError, OpOutcome};
use crate::{AccountContext, GitRunner};

/// Bigger files are refused: a patch is text someone reads.
const MAX_PATCH_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum PatchSource {
    /// Everything not committed yet (staged and unstaged, tracked files).
    Uncommitted,
    /// Only what is staged.
    Staged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ApplyMode {
    /// Put the changes in the working tree; nothing is committed.
    Changes,
    /// Recreate the commits of the patch (author, date, message) with `git am`.
    Commits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PatchFile {
    pub path: String,
    pub added: u32,
    pub removed: u32,
    pub binary: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PatchInfo {
    /// Commits the patch carries (`git format-patch` output); 0 for a plain diff.
    pub commits: u32,
    /// Subjects of those commits, in order.
    pub subjects: Vec<String>,
    pub files: Vec<PatchFile>,
    /// Whether the patch applies cleanly to the working tree as it is now.
    pub applies: bool,
    /// git's explanation when it does not.
    pub problem: Option<String>,
}

fn write_file(dest: &Path, bytes: &[u8]) -> Result<(), OpError> {
    if bytes.is_empty() {
        return Err(OpError::rejected(
            FailureKind::NothingToCommit,
            "there are no changes to save as a patch",
        ));
    }
    std::fs::write(dest, bytes).map_err(|e| OpError::Git(e.into()))
}

/// Saves commit `oid` as a patch file that `git am` can turn back into the same commit.
pub fn save_commit(git: &GitRunner, repo: &Path, oid: &str, dest: &Path) -> Result<(), OpError> {
    if !is_valid_oid(oid) {
        return Err(OpError::rejected(
            FailureKind::InvalidInput,
            format!("invalid commit id: {oid:?}"),
        ));
    }
    let full = ops::resolve_commit(git, repo, oid)?;
    let out = git.run(
        repo,
        &AccountContext::default(),
        ["format-patch", "-1", "--stdout", "--binary", &full, "--"],
    )?;
    write_file(dest, &out.stdout)
}

/// Saves the uncommitted (or only the staged) changes as a plain diff.
pub fn save_changes(
    git: &GitRunner,
    repo: &Path,
    source: PatchSource,
    dest: &Path,
) -> Result<(), OpError> {
    let unborn = ops::head_oid(git, repo).is_none();
    let args: &[&str] = match (source, unborn) {
        (PatchSource::Staged, _) | (PatchSource::Uncommitted, true) => &[
            "diff",
            "--cached",
            "--binary",
            "--no-color",
            "--no-ext-diff",
        ],
        (PatchSource::Uncommitted, false) => {
            &["diff", "HEAD", "--binary", "--no-color", "--no-ext-diff"]
        }
    };
    let out = git.run(repo, &AccountContext::default(), args)?;
    write_file(dest, &out.stdout)
}

/// Commit subjects of a `format-patch` mailbox: one per `From <oid> <date>` separator.
pub fn parse_mailbox_subjects(text: &str) -> Vec<String> {
    let mut subjects = Vec::new();
    let mut in_headers = false;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("From ")
            && rest
                .split(' ')
                .next()
                .is_some_and(|oid| oid.len() >= 40 && oid.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            in_headers = true;
            continue;
        }
        if in_headers {
            if line.is_empty() {
                in_headers = false;
            } else if let Some(subject) = line.strip_prefix("Subject: ") {
                let subject = match subject.strip_prefix('[') {
                    // `[PATCH 2/3] message`
                    Some(rest) => rest.split_once("] ").map_or(subject, |(_, s)| s),
                    None => subject,
                };
                subjects.push(subject.trim().to_owned());
            }
        }
    }
    subjects
}

/// Parses `git apply --numstat -z`: `added\tremoved\tpath\0` (`-` for binary files).
pub fn parse_numstat(raw: &str) -> Vec<PatchFile> {
    raw.split('\0')
        .filter_map(|record| {
            let mut f = record.trim_start_matches('\n').splitn(3, '\t');
            let added = f.next()?;
            let removed = f.next()?;
            let path = f.next()?.to_owned();
            Some(PatchFile {
                binary: added == "-",
                added: added.parse().unwrap_or(0),
                removed: removed.parse().unwrap_or(0),
                path,
            })
        })
        .filter(|f| !f.path.is_empty())
        .collect()
}

fn check_file(file: &Path) -> Result<(), OpError> {
    let meta = std::fs::metadata(file).map_err(|_| {
        OpError::rejected(
            FailureKind::NotFound,
            format!("patch file not found: {}", file.display()),
        )
    })?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_PATCH_BYTES {
        return Err(OpError::rejected(
            FailureKind::InvalidInput,
            "this is not a patch file (empty, a folder, or larger than 64 MB)",
        ));
    }
    Ok(())
}

/// What a patch file contains and whether it applies to the working tree right now.
pub fn inspect(git: &GitRunner, repo: &Path, file: &Path) -> Result<PatchInfo, OpError> {
    check_file(file)?;
    let ctx = AccountContext::default();
    let text = String::from_utf8_lossy(&std::fs::read(file).map_err(|e| OpError::Git(e.into()))?)
        .into_owned();
    let subjects = parse_mailbox_subjects(&text);
    let numstat = ops::writer(git)
        .run(
            repo,
            &ctx,
            [
                "apply".as_ref(),
                "--numstat".as_ref(),
                "-z".as_ref(),
                file.as_os_str(),
            ],
        )
        .map_err(|e| {
            OpError::rejected(
                FailureKind::InvalidInput,
                format!("git cannot read this file as a patch: {e}"),
            )
        })?;
    let files = parse_numstat(&String::from_utf8_lossy(&numstat.stdout));
    if files.is_empty() {
        return Err(OpError::rejected(
            FailureKind::InvalidInput,
            "the file contains no changes git can apply",
        ));
    }
    let check = ops::writer(git).run(
        repo,
        &ctx,
        ["apply".as_ref(), "--check".as_ref(), file.as_os_str()],
    );
    Ok(PatchInfo {
        commits: subjects.len() as u32,
        subjects,
        files,
        applies: check.is_ok(),
        problem: check.err().map(|e| match e {
            crate::GitError::Failed { stderr, .. } => stderr,
            other => other.to_string(),
        }),
    })
}

/// Applies a patch file. Places that do not fit are merged (`--3way`); real conflicts come
/// back in the outcome like those of a merge.
pub fn apply(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    file: &Path,
    mode: ApplyMode,
) -> Result<OpOutcome, OpError> {
    check_file(file)?;
    ops::ensure_idle(git, repo)?;
    let args: Vec<&std::ffi::OsStr> = match mode {
        ApplyMode::Changes => vec![
            "apply".as_ref(),
            "--3way".as_ref(),
            "--whitespace=nowarn".as_ref(),
            file.as_os_str(),
        ],
        ApplyMode::Commits => vec![
            "am".as_ref(),
            // Without it `git am` drops the CR of CRLF files and the patch no longer fits.
            "--keep-cr".as_ref(),
            "--3way".as_ref(),
            "--whitespace=nowarn".as_ref(),
            file.as_os_str(),
        ],
    };
    match ops::writer(git).run(repo, ctx, args) {
        Ok(_) => Ok(OpOutcome {
            conflicts: Vec::new(),
            pending: ops::current_operation(git, repo)?,
            head: ops::head_oid(git, repo),
        }),
        Err(err) => ops::conflicts_or(git, repo, err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::test_support::TestRepo;

    #[test]
    fn a_commit_travels_as_a_patch() {
        let repo = TestRepo::new();
        let base = repo.head();
        let oid = repo.commit("a.txt", "one\ntwo\n", "add a file");
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.patch");
        save_commit(&repo.git, &repo.path, &oid, &file).unwrap();

        repo.git(&["reset", "-q", "--hard", &base]);
        let info = inspect(&repo.git, &repo.path, &file).unwrap();
        assert_eq!(info.commits, 1);
        assert_eq!(info.subjects, ["add a file"]);
        assert_eq!(info.files.len(), 1);
        assert_eq!(info.files[0].path, "a.txt");
        assert_eq!(info.files[0].added, 2);
        assert!(info.applies);

        let out = apply(&repo.git, &repo.path, &repo.ctx, &file, ApplyMode::Commits).unwrap();
        assert!(out.conflicts.is_empty());
        assert_eq!(repo.git(&["log", "-1", "--format=%s"]), "add a file");
        assert_eq!(repo.read("a.txt"), "one\ntwo\n");
    }

    #[test]
    fn a_commit_with_crlf_lines_applies_back() {
        let repo = TestRepo::new();
        repo.git(&["config", "core.autocrlf", "false"]);
        // Every line ends with CR LF (byte 13, byte 10), as Windows editors write them.
        let crlf = |lines: &[&str]| -> String {
            lines
                .iter()
                .map(|l| format!("{l}{}{}", char::from(13u8), char::from(10u8)))
                .collect()
        };
        let before = crlf(&["one", "two"]);
        let after = crlf(&["one", "two", "three"]);
        assert!(after.as_bytes().contains(&13));
        let base = repo.commit("notes.txt", &before, "crlf file");
        let oid = repo.commit("notes.txt", &after, "add a line");
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("crlf.patch");
        save_commit(&repo.git, &repo.path, &oid, &file).unwrap();
        repo.git(&["reset", "-q", "--hard", &base]);

        let out = apply(&repo.git, &repo.path, &repo.ctx, &file, ApplyMode::Commits).unwrap();
        assert!(out.conflicts.is_empty(), "{out:?}");
        assert!(out.pending.is_none(), "{out:?}");
        assert_eq!(repo.git(&["log", "-1", "--format=%s"]), "add a line");
        let bytes = std::fs::read(repo.path.join("notes.txt")).unwrap();
        assert_eq!(bytes, after.as_bytes());
    }

    #[test]
    fn uncommitted_changes_travel_as_a_diff() {
        let repo = TestRepo::new();
        repo.write("README.md", "hello\nworld\n");
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("wip.diff");
        save_changes(&repo.git, &repo.path, PatchSource::Uncommitted, &file).unwrap();
        repo.git(&["checkout", "-q", "--", "README.md"]);
        assert_eq!(repo.read("README.md"), "hello\n");

        let info = inspect(&repo.git, &repo.path, &file).unwrap();
        assert_eq!(info.commits, 0);
        let out = apply(&repo.git, &repo.path, &repo.ctx, &file, ApplyMode::Changes).unwrap();
        assert!(out.conflicts.is_empty());
        assert_eq!(repo.read("README.md"), "hello\nworld\n");

        // Nothing left to save once it is committed.
        repo.git(&["commit", "-q", "-am", "world"]);
        let err = save_changes(&repo.git, &repo.path, PatchSource::Uncommitted, &file).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::NothingToCommit));
    }

    #[test]
    fn rejects_files_that_are_not_patches() {
        let repo = TestRepo::new();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("notes.txt");
        std::fs::write(&file, "just some text\n").unwrap();
        let err = inspect(&repo.git, &repo.path, &file).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::InvalidInput));
        let err = inspect(&repo.git, &repo.path, &dir.path().join("missing")).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::NotFound));
    }

    #[test]
    fn parses_mailbox_subjects() {
        let text = "From 1111111111111111111111111111111111111111 Mon Sep 17 00:00:00 2001\nFrom: A <a@x>\nSubject: [PATCH 1/2] first\n\nbody\nSubject: not a header\n---\nFrom 2222222222222222222222222222222222222222 Mon Sep 17 00:00:00 2001\nSubject: second\n\n";
        assert_eq!(parse_mailbox_subjects(text), ["first", "second"]);
    }
}
