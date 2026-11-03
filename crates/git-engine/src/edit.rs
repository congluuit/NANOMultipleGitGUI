//! The built-in editor's access to the working tree: reading and writing files, listing and
//! searching the repository's files, creating / renaming / deleting files and folders, and
//! staging an edited version of a file straight into the index (stage one block).
//!
//! Files are only offered for editing when saving them back cannot lose anything: valid UTF-8
//! text, not binary, not too large, and not a symlink (writing would follow it, possibly out of
//! the repository). Every read returns a `version` of the bytes on disk; a save carries the
//! version it was based on and is refused when the file changed in the meantime.

use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};

use crate::diff::check_path;
use crate::{AccountContext, GitError, GitRunner, blob};

/// Same limit as the diff editor: bigger files are not sent to the frontend.
const MAX_EDIT_BYTES: usize = 5 * 1024 * 1024;
const BINARY_SNIFF: usize = 8000;
/// Version of a file that does not exist.
const MISSING: &str = "missing";

/// A working-tree file as the editor needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct WorktreeFile {
    /// Contents, `None` unless the file exists and is editable text.
    pub text: Option<String>,
    pub exists: bool,
    pub binary: bool,
    pub too_large: bool,
    /// Not valid UTF-8 (another encoding): shown read-only so saving cannot corrupt it.
    pub not_utf8: bool,
    pub symlink: bool,
    /// Identifies the bytes on disk; passed back to [`write_file`].
    pub version: String,
}

/// Result of [`write_file`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SaveOutcome {
    /// `false` when the file changed on disk since `expected` was read; nothing was written.
    pub saved: bool,
    /// Version of the file on disk now.
    pub version: String,
}

fn version(bytes: &[u8]) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn invalid(what: &str) -> GitError {
    GitError::Failed {
        args: "edit".into(),
        status: "rejected".into(),
        stderr: what.to_owned(),
    }
}

fn is_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

/// `root/path`, refusing paths that leave the repository through `..` or a symlinked folder,
/// and anything inside `.git`.
fn resolve(root: &Path, path: &str) -> Result<PathBuf, GitError> {
    check_path(path)?;
    if path
        .split(['/', '\\'])
        .any(|c| c.eq_ignore_ascii_case(".git") || c.is_empty() || c == ".")
    {
        return Err(invalid(&format!("invalid path: {path:?}")));
    }
    let full = root.join(path);
    // The closest folder that exists must still be inside the repository.
    let mut existing = full.parent();
    while let Some(dir) = existing
        && !dir.exists()
    {
        existing = dir.parent();
    }
    if let Some(dir) = existing
        && let (Ok(dir), Ok(root)) = (dir.canonicalize(), root.canonicalize())
        && !dir.starts_with(&root)
    {
        return Err(invalid(&format!("{path} is outside the repository")));
    }
    Ok(full)
}

/// Reads `path` (relative to the repository root) for the editor.
pub fn read_file(root: &Path, path: &str) -> Result<WorktreeFile, GitError> {
    let full = resolve(root, path)?;
    let mut out = WorktreeFile {
        version: MISSING.into(),
        ..WorktreeFile::default()
    };
    let meta = match std::fs::symlink_metadata(&full) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e.into()),
    };
    out.exists = true;
    if meta.file_type().is_symlink() {
        out.symlink = true;
        return Ok(out);
    }
    if meta.is_dir() {
        return Err(invalid(&format!("{path} is a folder")));
    }
    if meta.len() > MAX_EDIT_BYTES as u64 {
        out.too_large = true;
        return Ok(out);
    }
    let bytes = std::fs::read(&full)?;
    out.version = version(&bytes);
    out.binary = bytes[..bytes.len().min(BINARY_SNIFF)].contains(&0);
    if out.binary {
        return Ok(out);
    }
    match String::from_utf8(bytes) {
        Ok(text) => out.text = Some(text),
        Err(_) => out.not_utf8 = true,
    }
    Ok(out)
}

/// Writes `text` to `path`. With `expected`, nothing is written when the file on disk is no
/// longer that version (changed by another program); the outcome then reports `saved: false`.
pub fn write_file(
    root: &Path,
    path: &str,
    text: &str,
    expected: Option<&str>,
) -> Result<SaveOutcome, GitError> {
    let full = resolve(root, path)?;
    if is_symlink(&full) {
        return Err(invalid(&format!("{path} is a symlink")));
    }
    if let Some(expected) = expected {
        let current = match std::fs::read(&full) {
            Ok(bytes) => version(&bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => MISSING.into(),
            Err(e) => return Err(e.into()),
        };
        if current != expected {
            return Ok(SaveOutcome {
                saved: false,
                version: current,
            });
        }
    }
    std::fs::write(&full, text.as_bytes())?;
    Ok(SaveOutcome {
        saved: true,
        version: version(text.as_bytes()),
    })
}

// ---------------------------------------------------------------------------------------------
// Listing and searching the repository's files.

/// Files listed for quick open and the file tree are capped (huge monorepos).
const MAX_LIST: usize = 200_000;

/// Every file of the working tree that is not ignored.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RepoFiles {
    /// Tracked files present on disk, sorted.
    pub tracked: Vec<String>,
    /// Untracked files that are not ignored, sorted.
    pub untracked: Vec<String>,
    /// Folders with no listed file inside (empty folders, nested repositories), sorted.
    pub folders: Vec<String>,
    pub truncated: bool,
}

fn ls_files(git: &GitRunner, root: &Path, args: &[&str]) -> Result<Vec<String>, GitError> {
    let mut all = vec!["ls-files", "-z"];
    all.extend_from_slice(args);
    let out = git.run(root, &AccountContext::default(), &all)?;
    let mut paths: Vec<String> = out
        .stdout
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

pub fn list_files(git: &GitRunner, root: &Path) -> Result<RepoFiles, GitError> {
    let deleted: HashSet<String> = ls_files(git, root, &["--deleted"])?.into_iter().collect();
    let mut tracked: Vec<String> = ls_files(git, root, &["--cached"])?
        .into_iter()
        .filter(|p| !deleted.contains(p))
        .collect();
    // `--others` lists a nested repository as `dir/`: a folder, not a file.
    let (mut folders, mut untracked): (Vec<String>, Vec<String>) =
        ls_files(git, root, &["--others", "--exclude-standard"])?
            .into_iter()
            .partition(|p| p.ends_with('/'));
    // `--directory` collapses untracked folders to `dir/`, which also reveals empty ones.
    for dir in ls_files(
        git,
        root,
        &["--others", "--exclude-standard", "--directory"],
    )? {
        let Some(name) = dir.strip_suffix('/') else {
            continue;
        };
        let at = untracked.partition_point(|u| u.as_str() < dir.as_str());
        let has_files = untracked.get(at).is_some_and(|u| u.starts_with(&dir));
        if !has_files && !folders.contains(&dir) {
            folders.push(name.to_owned());
        }
    }
    for f in &mut folders {
        if let Some(name) = f.strip_suffix('/') {
            *f = name.to_owned();
        }
    }
    folders.sort();
    folders.dedup();
    let truncated = tracked.len() + untracked.len() > MAX_LIST;
    tracked.truncate(MAX_LIST);
    untracked.truncate(MAX_LIST - tracked.len());
    Ok(RepoFiles {
        tracked,
        untracked,
        folders,
        truncated,
    })
}

/// How [`search`] matches the query.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
    /// Extended regular expression instead of plain text.
    pub regex: bool,
    /// Also search untracked files (never ignored ones).
    pub untracked: bool,
}

/// One matching line.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SearchMatch {
    pub path: String,
    /// 1-based line number.
    pub line: u32,
    /// 1-based column of the first match, in UTF-16 units (what the editor counts).
    pub column: u32,
    /// The line, or the part of a long line around the match.
    pub text: String,
    /// Column (0-based, UTF-16) of `text`'s first character in the line.
    pub text_start: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub matches: Vec<SearchMatch>,
    /// More lines matched than `limit`.
    pub truncated: bool,
}

/// Characters of a long line kept before / after the match.
const PREVIEW_BEFORE: usize = 60;
const PREVIEW_LEN: usize = 240;

/// Parses one `git grep -z -n --column` line: `path\0line\0column\0text`.
fn parse_grep_line(raw: &[u8]) -> Option<SearchMatch> {
    let raw = raw.strip_suffix(b"\n").unwrap_or(raw);
    let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
    let mut parts = raw.splitn(4, |&b| b == 0);
    let path = String::from_utf8_lossy(parts.next()?).into_owned();
    let line: u32 = std::str::from_utf8(parts.next()?).ok()?.parse().ok()?;
    let byte_col: usize = std::str::from_utf8(parts.next()?).ok()?.parse().ok()?;
    let content = parts.next()?;
    let before = &content[..byte_col.saturating_sub(1).min(content.len())];
    let before_chars: Vec<char> = String::from_utf8_lossy(before).chars().collect();
    let text: Vec<char> = String::from_utf8_lossy(content).chars().collect();
    let start = before_chars.len().saturating_sub(PREVIEW_BEFORE);
    let end = (start + PREVIEW_LEN).min(text.len());
    let utf16 = |chars: &[char]| chars.iter().map(|c| c.len_utf16()).sum::<usize>() as u32;
    Some(SearchMatch {
        path,
        line,
        column: utf16(&before_chars) + 1,
        text: text[start.min(end)..end].iter().collect(),
        text_start: utf16(&text[..start.min(text.len())]),
    })
}

/// `git grep` over the working tree (tracked files, and untracked ones on request), stopping
/// after `limit` matching lines.
pub fn search(
    git: &GitRunner,
    root: &Path,
    query: &str,
    opts: SearchOptions,
    limit: usize,
) -> Result<SearchResults, GitError> {
    let mut out = SearchResults::default();
    if query.is_empty() {
        return Ok(out);
    }
    let mut args = vec!["grep", "-z", "-n", "--column", "-I", "--no-color"];
    if !opts.case_sensitive {
        args.push("-i");
    }
    if opts.whole_word {
        args.push("-w");
    }
    args.push(if opts.regex { "-E" } else { "-F" });
    if opts.untracked {
        args.push("--untracked");
    }
    args.extend(["-e", query, "--"]);
    let mut child = git
        .command(root, &AccountContext::default(), &args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stderr = child.stderr.take().expect("piped stderr");
    let errors = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    let mut reader = BufReader::new(child.stdout.take().expect("piped stdout"));
    let mut buf = Vec::new();
    loop {
        buf.clear();
        if reader.read_until(b'\n', &mut buf)? == 0 {
            break;
        }
        if out.matches.len() >= limit {
            out.truncated = true;
            let _ = child.kill();
            break;
        }
        if let Some(m) = parse_grep_line(&buf) {
            out.matches.push(m);
        }
    }
    drop(reader);
    let status = child.wait()?;
    let stderr = errors.join().unwrap_or_default();
    // 1: nothing matched.
    if out.truncated || status.success() || (status.code() == Some(1) && stderr.trim().is_empty()) {
        return Ok(out);
    }
    Err(GitError::Failed {
        args: format!("grep {query}"),
        status: status.to_string(),
        stderr: stderr.trim().to_owned(),
    })
}

// ---------------------------------------------------------------------------------------------
// Creating, renaming and deleting files and folders.

fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

/// Creates an empty file (and its missing parent folders). Refused when it already exists.
pub fn create_file(root: &Path, path: &str) -> Result<(), GitError> {
    let full = resolve(root, path)?;
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&full)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => invalid(&format!("{path} already exists")),
            _ => e.into(),
        })?;
    Ok(())
}

/// Creates a folder (and its missing parents). Refused when something is already there.
pub fn create_dir(root: &Path, path: &str) -> Result<(), GitError> {
    let full = resolve(root, path)?;
    if exists(&full) {
        return Err(invalid(&format!("{path} already exists")));
    }
    std::fs::create_dir_all(&full)?;
    Ok(())
}

/// Renames / moves a file or folder inside the repository (a plain move: git sees the rename
/// once both sides are staged). Changing only the letter case is allowed on any file system.
pub fn rename_path(root: &Path, from: &str, to: &str) -> Result<(), GitError> {
    let src = resolve(root, from)?;
    let dst = resolve(root, to)?;
    if !exists(&src) {
        return Err(invalid(&format!("{from} does not exist")));
    }
    if exists(&dst) && !from.eq_ignore_ascii_case(to) {
        return Err(invalid(&format!("{to} already exists")));
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(&src, &dst)?;
    Ok(())
}

/// Deletes a file, or a folder with everything in it. A symlink is removed, not followed.
pub fn delete_path(root: &Path, path: &str) -> Result<(), GitError> {
    let full = resolve(root, path)?;
    let meta = std::fs::symlink_metadata(&full)?;
    if meta.file_type().is_symlink() {
        // A link to a folder is a directory entry on Windows.
        std::fs::remove_file(&full).or_else(|_| std::fs::remove_dir(&full))?;
    } else if meta.is_dir() {
        std::fs::remove_dir_all(&full)?;
    } else {
        std::fs::remove_file(&full)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Staging edited text.

/// Puts `text` in the index as the staged version of `path` (the editor's "stage / unstage
/// this block": the frontend builds the whole new index text). `expected` is the index text the
/// edit was based on, `None` when the file was not in the index; when the index changed since,
/// nothing is written and `false` is returned. The working tree is not touched.
pub fn stage_text(
    git: &GitRunner,
    root: &Path,
    path: &str,
    text: &str,
    expected: Option<&str>,
) -> Result<bool, GitError> {
    check_path(path)?;
    let ctx = AccountContext::default();
    let entries = git.run_text(root, &ctx, ["ls-files", "-s", "--", path])?;
    let entries: Vec<&str> = entries.lines().filter(|l| !l.is_empty()).collect();
    if entries.len() > 1 {
        return Err(invalid(&format!("{path} has unresolved conflicts")));
    }
    let mode = entries
        .first()
        .and_then(|l| l.split_whitespace().next())
        .unwrap_or("100644")
        .to_owned();
    if mode != "100644" && mode != "100755" {
        return Err(invalid(&format!(
            "{path} is not a regular file in the index"
        )));
    }
    let current = if entries.is_empty() {
        None
    } else {
        let bytes = blob::object_bytes(git, root, &format!(":{path}")).unwrap_or_default();
        if blob::parse_lfs_pointer(&bytes).is_some() {
            return Err(invalid(&format!("{path} is stored in Git LFS")));
        }
        let Ok(text) = String::from_utf8(bytes) else {
            return Err(invalid(&format!("{path} is not UTF-8 text in the index")));
        };
        Some(text)
    };
    if current.as_deref() != expected {
        return Ok(false);
    }
    let out = git.run_with_input(
        root,
        &ctx,
        ["hash-object", "-w", "--stdin"],
        text.as_bytes(),
    )?;
    let oid = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    git.run(
        root,
        &ctx,
        [
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("{mode},{oid},{path}"),
        ],
    )?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_text_and_flags_other_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("a.txt"), "hello\r\nworld\n").unwrap();
        std::fs::write(root.join("bin.dat"), [1u8, 0, 2]).unwrap();
        std::fs::write(root.join("latin1.txt"), [b'c', 0xe9, b'\n']).unwrap();

        let a = read_file(root, "a.txt").unwrap();
        assert_eq!(a.text.as_deref(), Some("hello\r\nworld\n"));
        assert!(a.exists && !a.binary && !a.not_utf8);
        assert_eq!(a.version, version(b"hello\r\nworld\n"));

        let bin = read_file(root, "bin.dat").unwrap();
        assert!(bin.binary && bin.text.is_none());

        let latin = read_file(root, "latin1.txt").unwrap();
        assert!(latin.not_utf8 && latin.text.is_none());

        let gone = read_file(root, "gone.txt").unwrap();
        assert!(!gone.exists);
        assert_eq!(gone.version, MISSING);
    }

    #[test]
    fn refuses_paths_outside_the_repository() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_file(dir.path(), "../x.txt").is_err());
        assert!(write_file(dir.path(), "../x.txt", "", None).is_err());
        assert!(read_file(dir.path(), "/etc/passwd").is_err());
    }

    #[test]
    fn save_checks_the_version_it_was_based_on() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("f.txt"), "one\n").unwrap();
        let v1 = read_file(root, "f.txt").unwrap().version;

        let out = write_file(root, "f.txt", "two\n", Some(&v1)).unwrap();
        assert!(out.saved);
        assert_eq!(
            std::fs::read_to_string(root.join("f.txt")).unwrap(),
            "two\n"
        );
        assert_eq!(out.version, read_file(root, "f.txt").unwrap().version);

        // Based on the old version: refused, the file keeps the newer contents.
        let stale = write_file(root, "f.txt", "three\n", Some(&v1)).unwrap();
        assert!(!stale.saved);
        assert_eq!(stale.version, out.version);
        assert_eq!(
            std::fs::read_to_string(root.join("f.txt")).unwrap(),
            "two\n"
        );

        // Without a version (overwrite) it is written.
        assert!(write_file(root, "f.txt", "three\n", None).unwrap().saved);
        assert_eq!(
            std::fs::read_to_string(root.join("f.txt")).unwrap(),
            "three\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_read_only() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("target.txt"), "x").unwrap();
        std::os::unix::fs::symlink(root.join("target.txt"), root.join("link.txt")).unwrap();
        let f = read_file(root, "link.txt").unwrap();
        assert!(f.symlink && f.text.is_none());
        assert!(write_file(root, "link.txt", "y", None).is_err());
        assert_eq!(
            std::fs::read_to_string(root.join("target.txt")).unwrap(),
            "x"
        );
    }

    #[test]
    fn lists_tracked_untracked_and_empty_folders() {
        let r = crate::blob::tests::Repo::new();
        r.write("a.txt", "a");
        r.write("src/b.rs", "b");
        r.write("gone.txt", "x");
        r.commit_all("init");
        std::fs::remove_file(r.p().join("gone.txt")).unwrap();
        r.write("new/c.md", "c");
        r.write(".gitignore", "*.log");
        r.write("debug.log", "ignored");
        std::fs::create_dir_all(r.p().join("empty/inner")).unwrap();

        let files = list_files(&r.git, r.p()).unwrap();
        assert_eq!(files.tracked, ["a.txt", "src/b.rs"]);
        assert_eq!(files.untracked, [".gitignore", "new/c.md"]);
        assert_eq!(files.folders, ["empty"]);
        assert!(!files.truncated);
    }

    #[test]
    fn searches_with_columns_and_options() {
        let r = crate::blob::tests::Repo::new();
        r.write("a.txt", "hello world\nsay Hello\n");
        r.write("d/b.txt", "café hello\n");
        r.commit_all("init");
        r.write("u.txt", "hello untracked\n");

        let all = search(&r.git, r.p(), "hello", SearchOptions::default(), 100).unwrap();
        let found: Vec<_> = all
            .matches
            .iter()
            .map(|m| (m.path.as_str(), m.line, m.column))
            .collect();
        // Columns count UTF-16 units: "café " is 5 characters (6 bytes).
        assert_eq!(found, [("a.txt", 1, 1), ("a.txt", 2, 5), ("d/b.txt", 1, 6)]);

        let case = SearchOptions {
            case_sensitive: true,
            ..Default::default()
        };
        let exact = search(&r.git, r.p(), "Hello", case, 100).unwrap();
        assert_eq!(exact.matches.len(), 1);

        let untracked = SearchOptions {
            untracked: true,
            ..Default::default()
        };
        let with = search(&r.git, r.p(), "untracked", untracked, 100).unwrap();
        assert_eq!(with.matches.len(), 1);

        let limited = search(&r.git, r.p(), "hello", SearchOptions::default(), 1).unwrap();
        assert!(limited.truncated && limited.matches.len() == 1);

        let none = search(&r.git, r.p(), "nothing", SearchOptions::default(), 10).unwrap();
        assert!(none.matches.is_empty());
        let regex = SearchOptions {
            regex: true,
            ..Default::default()
        };
        assert!(search(&r.git, r.p(), "(", regex, 10).is_err());
    }

    #[test]
    fn creates_renames_and_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        create_file(root, "a/b/new.txt").unwrap();
        assert!(root.join("a/b/new.txt").is_file());
        assert!(create_file(root, "a/b/new.txt").is_err());
        create_dir(root, "folder/sub").unwrap();
        assert!(create_dir(root, "folder").is_err());

        rename_path(root, "a/b/new.txt", "moved/renamed.txt").unwrap();
        assert!(root.join("moved/renamed.txt").is_file());
        assert!(rename_path(root, "missing.txt", "x.txt").is_err());
        create_file(root, "taken.txt").unwrap();
        assert!(rename_path(root, "moved/renamed.txt", "taken.txt").is_err());
        // Only the letter case changes.
        rename_path(root, "taken.txt", "Taken.txt").unwrap();

        delete_path(root, "folder").unwrap();
        assert!(!root.join("folder").exists());
        delete_path(root, "moved/renamed.txt").unwrap();
        assert!(!root.join("moved/renamed.txt").exists());

        for bad in [".git/config", "a/.GIT/x", "../out.txt", "a//b"] {
            assert!(create_file(root, bad).is_err(), "{bad}");
            assert!(delete_path(root, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn stages_text_based_on_the_expected_index() {
        let r = crate::blob::tests::Repo::new();
        r.write("f.txt", "one\ntwo\n");
        r.commit_all("init");
        r.write("f.txt", "ONE\ntwo\nthree\n");

        // Stage only the first line's change; the working tree keeps everything.
        let staged = stage_text(&r.git, r.p(), "f.txt", "ONE\ntwo\n", Some("one\ntwo\n"));
        assert!(staged.unwrap());
        assert_eq!(r.run(&["show", ":f.txt"]), "ONE\ntwo");
        assert_eq!(r.read("f.txt"), b"ONE\ntwo\nthree\n");

        // Based on an outdated index: refused.
        let stale = stage_text(&r.git, r.p(), "f.txt", "x\n", Some("one\ntwo\n"));
        assert!(!stale.unwrap());
        assert_eq!(r.run(&["show", ":f.txt"]), "ONE\ntwo");

        // A file not in the index yet.
        r.write("n.txt", "new\n");
        assert!(stage_text(&r.git, r.p(), "n.txt", "new\n", None).unwrap());
        assert_eq!(r.run(&["show", ":n.txt"]), "new");
    }
}
