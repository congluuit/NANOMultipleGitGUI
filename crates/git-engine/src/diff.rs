//! Diffs of a single file (working tree, index or a commit), both as hunks for the hunk view
//! and as full old/new contents for the side-by-side editor, plus partial patches that stage,
//! unstage or discard selected hunks and lines.

use std::hash::{Hash, Hasher};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::commit::is_valid_oid;
use crate::{AccountContext, GitError, GitRunner};

/// Diffs bigger than this are not parsed into hunks.
const MAX_DIFF_BYTES: usize = 8 * 1024 * 1024;
const MAX_DIFF_LINES: usize = 50_000;
/// Contents bigger than this are not sent to the editor.
const MAX_CONTENT_BYTES: usize = 5 * 1024 * 1024;
/// Same heuristic as git: a NUL in the first 8000 bytes means binary.
const BINARY_SNIFF: usize = 8000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum DiffSide {
    /// Working tree against the index.
    Unstaged,
    /// Index against HEAD.
    Staged,
    /// A file git does not track yet.
    Untracked,
    /// A commit against its first parent.
    Commit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DiffRequest {
    pub side: DiffSide,
    pub path: String,
    /// Previous path of a rename (staged or in a commit).
    #[serde(default)]
    #[cfg_attr(feature = "specta", specta(optional))]
    pub old_path: Option<String>,
    /// Commit id, for [`DiffSide::Commit`].
    #[serde(default)]
    #[cfg_attr(feature = "specta", specta(optional))]
    pub commit: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum LineKind {
    Context,
    Add,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub kind: LineKind,
    /// Line text without the `+`/`-`/` ` prefix and without the line ending.
    pub text: String,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    /// Followed by `\ No newline at end of file`.
    pub no_newline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    /// The `@@ -a,b +c,d @@ context` line.
    pub header: String,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    pub old_path: Option<String>,
    pub binary: bool,
    /// Too big to show as hunks.
    pub too_large: bool,
    pub new_file: bool,
    pub deleted_file: bool,
    pub old_mode: Option<String>,
    pub new_mode: Option<String>,
    /// Individual hunks and lines can be staged/unstaged/discarded.
    pub partial: bool,
    pub hunks: Vec<Hunk>,
    pub additions: u32,
    pub deletions: u32,
    /// Identifies the exact diff shown, so line operations can refuse a stale view.
    pub fingerprint: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FileContents {
    /// `None` when the file does not exist on that side.
    pub original: Option<String>,
    pub modified: Option<String>,
    pub binary: bool,
    pub too_large: bool,
}

/// Lines picked in one hunk (indices into [`Hunk::lines`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct LineSelection {
    pub hunk: u32,
    pub lines: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum PatchAction {
    /// Unstaged lines → index.
    Stage,
    /// Staged lines → back out of the index.
    Unstage,
    /// Unstaged lines → removed from the working tree.
    Discard,
}

// ---------------------------------------------------------------------------------------------
// Raw unified-diff model (bytes are kept as-is so patches round-trip any encoding/line ending).

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawLine {
    kind: u8,
    content: Vec<u8>,
    no_newline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawHunk {
    old_start: u32,
    old_lines: u32,
    new_start: u32,
    new_lines: u32,
    header: Vec<u8>,
    lines: Vec<RawLine>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RawDiff {
    /// `diff --git`, `index`, `---`, `+++` lines etc., each ending with `\n`.
    header: Vec<u8>,
    hunks: Vec<RawHunk>,
}

fn fingerprint(bytes: &[u8]) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn lossy(bytes: &[u8]) -> String {
    let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
    String::from_utf8_lossy(bytes).into_owned()
}

fn looks_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(BINARY_SNIFF)].contains(&0)
}

/// `-12,3` / `+4` → (start, count).
fn parse_range(s: &str) -> Option<(u32, u32)> {
    let s = &s[1..];
    match s.split_once(',') {
        Some((a, b)) => Some((a.parse().ok()?, b.parse().ok()?)),
        None => Some((s.parse().ok()?, 1)),
    }
}

fn parse_hunk_header(line: &[u8]) -> Option<RawHunk> {
    let text = std::str::from_utf8(line).ok()?;
    let rest = text.strip_prefix("@@ ")?;
    let mut parts = rest.split(' ');
    let (old_start, old_lines) = parse_range(parts.next()?)?;
    let (new_start, new_lines) = parse_range(parts.next()?)?;
    Some(RawHunk {
        old_start,
        old_lines,
        new_start,
        new_lines,
        header: line.to_vec(),
        lines: Vec::new(),
    })
}

/// Splits multi-file diff output into per-file sections.
fn split_files(raw: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut pos = 0;
    for line in raw.split_inclusive(|&b| b == b'\n') {
        if line.starts_with(b"diff --git ") || line.starts_with(b"diff --cc ") {
            starts.push(pos);
        }
        pos += line.len();
    }
    starts
        .iter()
        .enumerate()
        .map(|(i, &s)| &raw[s..starts.get(i + 1).copied().unwrap_or(raw.len())])
        .collect()
}

fn parse_raw(section: &[u8]) -> RawDiff {
    let mut diff = RawDiff::default();
    for line in section.split_inclusive(|&b| b == b'\n') {
        let body = line.strip_suffix(b"\n").unwrap_or(line);
        if body.starts_with(b"@@ ")
            && let Some(h) = parse_hunk_header(body)
        {
            diff.hunks.push(h);
            continue;
        }
        let Some(hunk) = diff.hunks.last_mut() else {
            diff.header.extend_from_slice(line);
            continue;
        };
        match body.first() {
            Some(b'\\') => {
                if let Some(last) = hunk.lines.last_mut() {
                    last.no_newline = true;
                }
            }
            Some(&k @ (b' ' | b'+' | b'-')) => hunk.lines.push(RawLine {
                kind: k,
                content: body[1..].to_vec(),
                no_newline: false,
            }),
            // `diff.suppressBlankEmpty` prints empty context lines without the space.
            None => hunk.lines.push(RawLine {
                kind: b' ',
                content: Vec::new(),
                no_newline: false,
            }),
            _ => {}
        }
    }
    diff
}

fn header_value(header: &str, prefix: &str) -> Option<String> {
    header
        .lines()
        .find_map(|l| l.strip_prefix(prefix))
        .map(|v| v.trim().to_owned())
}

fn to_file_diff(path: &str, old_path: Option<&str>, raw: &[u8]) -> FileDiff {
    let mut out = FileDiff {
        path: path.to_owned(),
        old_path: old_path.map(str::to_owned),
        fingerprint: fingerprint(raw),
        ..Default::default()
    };
    if raw.len() > MAX_DIFF_BYTES || raw.iter().filter(|&&b| b == b'\n').count() > MAX_DIFF_LINES {
        out.too_large = true;
        return out;
    }
    let sections = split_files(raw);
    let wanted = format!(" b/{path}");
    let section = sections
        .iter()
        .find(|s| {
            s.split(|&b| b == b'\n')
                .next()
                .is_some_and(|first| String::from_utf8_lossy(first).ends_with(&wanted))
        })
        .or(sections.first())
        .copied()
        .unwrap_or_default();
    let parsed = parse_raw(section);
    let header = String::from_utf8_lossy(&parsed.header).into_owned();
    out.binary = header
        .lines()
        .any(|l| l.starts_with("Binary files ") || l == "GIT binary patch");
    out.new_file = header.lines().any(|l| l.starts_with("new file mode"));
    out.deleted_file = header.lines().any(|l| l.starts_with("deleted file mode"));
    out.old_mode = header_value(&header, "old mode ");
    out.new_mode = header_value(&header, "new mode ");
    if let Some(from) = header_value(&header, "rename from ") {
        out.old_path = Some(from);
    }
    out.partial = partial_ok(&parsed);
    out.hunks = parsed
        .hunks
        .iter()
        .map(|h| {
            let (mut o, mut n) = (h.old_start, h.new_start);
            let lines = h
                .lines
                .iter()
                .map(|l| {
                    let (kind, old_line, new_line) = match l.kind {
                        b'+' => {
                            out.additions += 1;
                            n += 1;
                            (LineKind::Add, None, Some(n - 1))
                        }
                        b'-' => {
                            out.deletions += 1;
                            o += 1;
                            (LineKind::Delete, Some(o - 1), None)
                        }
                        _ => {
                            o += 1;
                            n += 1;
                            (LineKind::Context, Some(o - 1), Some(n - 1))
                        }
                    };
                    DiffLine {
                        kind,
                        text: lossy(&l.content),
                        old_line,
                        new_line,
                        no_newline: l.no_newline,
                    }
                })
                .collect();
            Hunk {
                header: lossy(&h.header),
                old_start: h.old_start,
                old_lines: h.old_lines,
                new_start: h.new_start,
                new_lines: h.new_lines,
                lines,
            }
        })
        .collect();
    out
}

/// Line-level patches are only built for plain content changes of an existing file.
fn partial_ok(d: &RawDiff) -> bool {
    let header = String::from_utf8_lossy(&d.header);
    !d.hunks.is_empty()
        && !header.lines().any(|l| {
            l.starts_with("new file mode")
                || l.starts_with("deleted file mode")
                || l.starts_with("rename ")
                || l.starts_with("copy ")
                || l.starts_with("Binary files ")
                || l == "GIT binary patch"
        })
}

fn invalid(what: &str) -> GitError {
    GitError::Failed {
        args: "diff".into(),
        status: "rejected".into(),
        stderr: what.to_owned(),
    }
}

/// Rejects paths that could be read as options or escape the repository.
fn check_path(path: &str) -> Result<(), GitError> {
    let bad = path.is_empty()
        || path.starts_with('-')
        || path.contains('\0')
        || Path::new(path).is_absolute()
        || path.split(['/', '\\']).any(|c| c == "..");
    if bad {
        Err(invalid(&format!("invalid path: {path:?}")))
    } else {
        Ok(())
    }
}

fn check_request(req: &DiffRequest) -> Result<(), GitError> {
    check_path(&req.path)?;
    if let Some(old) = &req.old_path {
        check_path(old)?;
    }
    if req.side == DiffSide::Commit && !req.commit.as_deref().is_some_and(is_valid_oid) {
        return Err(invalid("a valid commit id is required"));
    }
    Ok(())
}

const BASE_ARGS: [&str; 10] = [
    "-c",
    "core.quotePath=false",
    "-c",
    "diff.noprefix=false",
    "--literal-pathspecs",
    "--no-pager",
    "-c",
    "diff.mnemonicPrefix=false",
    "-c",
    "diff.relative=false",
];

const FORMAT_ARGS: [&str; 6] = [
    "--no-color",
    "--no-ext-diff",
    "--no-textconv",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "-U3",
];

fn raw_diff(git: &GitRunner, root: &Path, req: &DiffRequest) -> Result<Vec<u8>, GitError> {
    let mut args: Vec<String> = BASE_ARGS.iter().map(|s| (*s).to_owned()).collect();
    match req.side {
        DiffSide::Unstaged => args.push("diff".into()),
        DiffSide::Staged => args.extend(["diff".into(), "--cached".into(), "-M".into()]),
        DiffSide::Commit => args.extend([
            "show".into(),
            "--format=".into(),
            "-M".into(),
            "--diff-merges=first-parent".into(),
        ]),
        DiffSide::Untracked => unreachable!("untracked files have no git diff"),
    }
    args.extend(FORMAT_ARGS.iter().map(|s| (*s).to_owned()));
    if let (DiffSide::Commit, Some(c)) = (req.side, &req.commit) {
        args.push(c.clone());
    }
    args.push("--".into());
    if let Some(old) = &req.old_path {
        args.push(old.clone());
    }
    args.push(req.path.clone());
    Ok(git.run(root, &AccountContext::default(), &args)?.stdout)
}

fn untracked_diff(root: &Path, path: &str) -> Result<FileDiff, GitError> {
    let bytes = std::fs::read(root.join(path))?;
    let mut out = FileDiff {
        path: path.to_owned(),
        new_file: true,
        fingerprint: fingerprint(&bytes),
        ..Default::default()
    };
    if looks_binary(&bytes) {
        out.binary = true;
        return Ok(out);
    }
    if bytes.len() > MAX_DIFF_BYTES {
        out.too_large = true;
        return Ok(out);
    }
    let mut lines: Vec<&[u8]> = bytes.split(|&b| b == b'\n').collect();
    let no_newline = !bytes.is_empty() && !bytes.ends_with(b"\n");
    if !no_newline {
        lines.pop();
    }
    if lines.len() > MAX_DIFF_LINES {
        out.too_large = true;
        return Ok(out);
    }
    let count = lines.len() as u32;
    out.additions = count;
    if count > 0 {
        out.hunks.push(Hunk {
            header: format!("@@ -0,0 +1,{count} @@"),
            old_start: 0,
            old_lines: 0,
            new_start: 1,
            new_lines: count,
            lines: lines
                .iter()
                .enumerate()
                .map(|(i, l)| DiffLine {
                    kind: LineKind::Add,
                    text: lossy(l),
                    old_line: None,
                    new_line: Some(i as u32 + 1),
                    no_newline: no_newline && i + 1 == lines.len(),
                })
                .collect(),
        });
    }
    Ok(out)
}

/// Diff of one file, parsed into hunks.
pub fn file_diff(git: &GitRunner, root: &Path, req: &DiffRequest) -> Result<FileDiff, GitError> {
    check_request(req)?;
    if req.side == DiffSide::Untracked {
        return untracked_diff(root, &req.path);
    }
    let raw = raw_diff(git, root, req)?;
    Ok(to_file_diff(&req.path, req.old_path.as_deref(), &raw))
}

/// Raw bytes of `spec` (`<rev>:<path>` or `:<path>` for the index), `None` if it does not exist.
fn blob(git: &GitRunner, root: &Path, spec: &str) -> Option<Vec<u8>> {
    git.run(root, &AccountContext::default(), ["cat-file", "blob", spec])
        .ok()
        .map(|o| o.stdout)
}

/// Full old/new contents of a file for the side-by-side editor.
pub fn file_contents(
    git: &GitRunner,
    root: &Path,
    req: &DiffRequest,
) -> Result<FileContents, GitError> {
    check_request(req)?;
    let old_path = req.old_path.as_deref().unwrap_or(&req.path);
    let worktree = || std::fs::read(root.join(&req.path)).ok();
    let (original, modified) = match req.side {
        DiffSide::Unstaged => (blob(git, root, &format!(":{}", req.path)), worktree()),
        DiffSide::Untracked => (None, worktree()),
        DiffSide::Staged => (
            blob(git, root, &format!("HEAD:{old_path}")),
            blob(git, root, &format!(":{}", req.path)),
        ),
        DiffSide::Commit => {
            let c = req.commit.as_deref().unwrap_or_default();
            (
                blob(git, root, &format!("{c}^1:{old_path}")),
                blob(git, root, &format!("{c}:{}", req.path)),
            )
        }
    };
    let mut out = FileContents::default();
    for side in [&original, &modified].into_iter().flatten() {
        out.binary |= looks_binary(side);
        out.too_large |= side.len() > MAX_CONTENT_BYTES;
    }
    if !out.binary && !out.too_large {
        out.original = original.map(|b| String::from_utf8_lossy(&b).into_owned());
        out.modified = modified.map(|b| String::from_utf8_lossy(&b).into_owned());
    }
    Ok(out)
}

/// Builds a patch containing only the selected `+`/`-` lines.
///
/// With `reverse == false` the patch is meant to be applied forwards onto the old side (index
/// for staging): unselected deletions stay as context, unselected additions are dropped.
/// With `reverse == true` it is applied with `--reverse` onto the new side (index for unstaging,
/// working tree for discarding): unselected additions stay as context, unselected deletions
/// are dropped. Returns `None` when nothing was selected.
fn build_patch(diff: &RawDiff, selection: &[LineSelection], reverse: bool) -> Option<Vec<u8>> {
    let mut out = diff.header.clone();
    let mut delta: i64 = 0;
    let mut any = false;
    for (index, hunk) in diff.hunks.iter().enumerate() {
        let Some(sel) = selection.iter().find(|s| s.hunk as usize == index) else {
            continue;
        };
        let mut body: Vec<u8> = Vec::new();
        let (mut old_count, mut new_count, mut changes) = (0u32, 0u32, 0u32);
        for (i, line) in hunk.lines.iter().enumerate() {
            let selected = sel.lines.contains(&(i as u32));
            let kind = match (line.kind, selected, reverse) {
                (b' ', ..) => b' ',
                (k, true, _) => k,
                (b'-', false, false) | (b'+', false, true) => b' ',
                _ => continue,
            };
            match kind {
                b' ' => {
                    old_count += 1;
                    new_count += 1;
                }
                b'-' => {
                    old_count += 1;
                    changes += 1;
                }
                _ => {
                    new_count += 1;
                    changes += 1;
                }
            }
            body.push(kind);
            body.extend_from_slice(&line.content);
            body.push(b'\n');
            if line.no_newline {
                body.extend_from_slice(b"\\ No newline at end of file\n");
            }
        }
        if changes == 0 {
            continue;
        }
        any = true;
        let (old_start, new_start) = if reverse {
            (i64::from(hunk.new_start) - delta, i64::from(hunk.new_start))
        } else {
            (i64::from(hunk.old_start), i64::from(hunk.old_start) + delta)
        };
        delta += i64::from(new_count) - i64::from(old_count);
        out.extend_from_slice(
            format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@\n").as_bytes(),
        );
        out.extend_from_slice(&body);
    }
    any.then_some(out)
}

/// Stages, unstages or discards the selected lines of `path`. `fingerprint` must match the
/// diff the selection was made on, otherwise nothing is changed.
pub fn apply_lines(
    git: &GitRunner,
    root: &Path,
    action: PatchAction,
    path: &str,
    fingerprint_seen: &str,
    selection: &[LineSelection],
) -> Result<(), GitError> {
    check_path(path)?;
    let req = DiffRequest {
        side: if action == PatchAction::Unstage {
            DiffSide::Staged
        } else {
            DiffSide::Unstaged
        },
        path: path.to_owned(),
        old_path: None,
        commit: None,
    };
    let raw = raw_diff(git, root, &req)?;
    if fingerprint(&raw) != fingerprint_seen {
        return Err(GitError::Failed {
            args: "apply".into(),
            status: "stale".into(),
            stderr: "the file changed since its diff was shown; refresh and try again".into(),
        });
    }
    let sections = split_files(&raw);
    let Some(section) = sections.first() else {
        return Ok(());
    };
    let parsed = parse_raw(section);
    if !partial_ok(&parsed) {
        return Err(invalid("this change can only be staged as a whole file"));
    }
    let reverse = action != PatchAction::Stage;
    let Some(patch) = build_patch(&parsed, selection, reverse) else {
        return Ok(());
    };
    let mut args = vec!["apply", "--recount", "--whitespace=nowarn"];
    if action != PatchAction::Discard {
        args.push("--cached");
    }
    if reverse {
        args.push("--reverse");
    }
    args.push("-");
    git.run_with_input(root, &AccountContext::default(), &args, &patch)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Identity;

    const SAMPLE: &str = concat!(
        "diff --git a/f.txt b/f.txt\n",
        "index 111..222 100644\n",
        "--- a/f.txt\n",
        "+++ b/f.txt\n",
        "@@ -1,4 +1,4 @@ fn main\n",
        " one\n",
        "-two\n",
        "+TWO\n",
        " three\n",
        "-four\n",
        "\\ No newline at end of file\n",
        "+four\n",
        "@@ -10 +10,2 @@\n",
        " ten\n",
        "+eleven\n",
    );

    #[test]
    fn parses_hunks_with_line_numbers() {
        let d = to_file_diff("f.txt", None, SAMPLE.as_bytes());
        assert!(d.partial && !d.binary && !d.new_file);
        assert_eq!(d.hunks.len(), 2);
        assert_eq!((d.additions, d.deletions), (3, 2));
        let h = &d.hunks[0];
        assert_eq!(
            (h.old_start, h.old_lines, h.new_start, h.new_lines),
            (1, 4, 1, 4)
        );
        assert_eq!(h.lines[1].kind, LineKind::Delete);
        assert_eq!(h.lines[1].old_line, Some(2));
        assert_eq!(h.lines[2].new_line, Some(2));
        assert!(h.lines[4].no_newline);
        assert_eq!(h.lines[5].new_line, Some(4));
        assert_eq!((d.hunks[1].old_start, d.hunks[1].old_lines), (10, 1));
    }

    #[test]
    fn builds_forward_and_reverse_partial_patches() {
        let raw = parse_raw(SAMPLE.as_bytes());
        // Stage only "+TWO": "-two" stays as context, the other changes vanish.
        let sel = [LineSelection {
            hunk: 0,
            lines: vec![2],
        }];
        let p = String::from_utf8(build_patch(&raw, &sel, false).unwrap()).unwrap();
        assert!(p.contains("@@ -1,4 +1,5 @@\n one\n two\n+TWO\n three\n four\n\\ No newline"));
        assert!(!p.contains("eleven"));
        // Reverse: unselected additions become context, unselected deletions vanish.
        let p = String::from_utf8(build_patch(&raw, &sel, true).unwrap()).unwrap();
        assert!(p.contains("@@ -1,3 +1,4 @@\n one\n+TWO\n three\n four\n"));
        // Second hunk only: its new start shifts by nothing before it.
        let sel = [LineSelection {
            hunk: 1,
            lines: vec![1],
        }];
        let p = String::from_utf8(build_patch(&raw, &sel, false).unwrap()).unwrap();
        assert!(p.ends_with("@@ -10,1 +10,2 @@\n ten\n+eleven\n"));
        assert!(build_patch(&raw, &[], false).is_none());
        let only_context = [LineSelection {
            hunk: 0,
            lines: vec![0, 3],
        }];
        assert!(build_patch(&raw, &only_context, false).is_none());
    }

    #[test]
    fn rejects_bad_paths() {
        for p in ["", "-x", "../out", "a/../../b", "--output=x"] {
            assert!(check_path(p).is_err(), "{p}");
        }
        assert!(check_path("dir/tên file.txt").is_ok());
        let bad_commit = DiffRequest {
            side: DiffSide::Commit,
            path: "a".into(),
            old_path: None,
            commit: Some("--output=x".into()),
        };
        assert!(check_request(&bad_commit).is_err());
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
                        email: "t@x.dev".into(),
                    }),
                    ..Default::default()
                },
            };
            r.run(&["init", "-q", "-b", "main"]);
            r.run(&["config", "core.autocrlf", "false"]);
            r
        }
        fn p(&self) -> &Path {
            self.dir.path()
        }
        fn run(&self, args: &[&str]) -> String {
            self.git.run_text(self.p(), &self.ctx, args).unwrap()
        }
        fn write(&self, name: &str, text: &str) {
            std::fs::write(self.p().join(name), text).unwrap();
        }
        fn read(&self, name: &str) -> String {
            std::fs::read_to_string(self.p().join(name)).unwrap()
        }
        fn diff(&self, side: DiffSide, path: &str) -> FileDiff {
            file_diff(
                &self.git,
                self.p(),
                &DiffRequest {
                    side,
                    path: path.into(),
                    old_path: None,
                    commit: None,
                },
            )
            .unwrap()
        }
    }

    /// Indices of the changed lines of hunk `h`.
    fn changes(d: &FileDiff, h: usize) -> Vec<u32> {
        d.hunks[h]
            .lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.kind != LineKind::Context)
            .map(|(i, _)| i as u32)
            .collect()
    }

    #[test]
    fn stage_unstage_and_discard_lines_on_a_real_repo() {
        let r = Repo::new();
        let base: String = (1..=30).map(|i| format!("line {i}\n")).collect();
        r.write("f.txt", &base);
        r.run(&["add", "."]);
        r.run(&["commit", "-q", "-m", "base"]);
        // Two separate hunks: change line 2 and line 25.
        let edited = base
            .replace("line 2\n", "line 2 changed\n")
            .replace("line 25\n", "line 25 changed\nextra\n");
        r.write("f.txt", &edited);

        let d = r.diff(DiffSide::Unstaged, "f.txt");
        assert_eq!(d.hunks.len(), 2);
        assert!(d.partial);

        // Stage the second hunk only.
        let sel = [LineSelection {
            hunk: 1,
            lines: changes(&d, 1),
        }];
        apply_lines(
            &r.git,
            r.p(),
            PatchAction::Stage,
            "f.txt",
            &d.fingerprint,
            &sel,
        )
        .unwrap();
        let staged = r.diff(DiffSide::Staged, "f.txt");
        assert_eq!(staged.hunks.len(), 1);
        assert!(staged.hunks[0].lines.iter().any(|l| l.text == "extra"));
        let unstaged = r.diff(DiffSide::Unstaged, "f.txt");
        assert_eq!(unstaged.hunks.len(), 1);
        assert!(
            unstaged.hunks[0]
                .lines
                .iter()
                .any(|l| l.text == "line 2 changed")
        );

        // A stale fingerprint is refused.
        assert!(
            apply_lines(
                &r.git,
                r.p(),
                PatchAction::Stage,
                "f.txt",
                &d.fingerprint,
                &sel
            )
            .is_err()
        );

        // Unstage just the "extra" line, keeping "line 25 changed" staged.
        let extra = staged.hunks[0]
            .lines
            .iter()
            .position(|l| l.text == "extra")
            .unwrap() as u32;
        let sel = [LineSelection {
            hunk: 0,
            lines: vec![extra],
        }];
        apply_lines(
            &r.git,
            r.p(),
            PatchAction::Unstage,
            "f.txt",
            &staged.fingerprint,
            &sel,
        )
        .unwrap();
        let index = r.run(&["show", ":f.txt"]);
        assert!(index.contains("line 25 changed") && !index.contains("extra"));

        // Discard the unstaged "line 2" change from the working tree; "extra" stays.
        let unstaged = r.diff(DiffSide::Unstaged, "f.txt");
        let h = unstaged
            .hunks
            .iter()
            .position(|h| h.lines.iter().any(|l| l.text == "line 2 changed"))
            .unwrap();
        let sel = [LineSelection {
            hunk: h as u32,
            lines: changes(&unstaged, h),
        }];
        apply_lines(
            &r.git,
            r.p(),
            PatchAction::Discard,
            "f.txt",
            &unstaged.fingerprint,
            &sel,
        )
        .unwrap();
        let wt = r.read("f.txt");
        assert!(wt.contains("line 2\n") && !wt.contains("line 2 changed"));
        assert!(wt.contains("line 25 changed\nextra\n"));
    }

    #[test]
    fn line_operations_with_crlf_working_tree() {
        let r = Repo::new();
        r.run(&["config", "core.autocrlf", "true"]);
        let base: String = (1..=20).map(|i| format!("line {i}\r\n")).collect();
        r.write("w.txt", &base);
        r.run(&["add", "."]);
        r.run(&["commit", "-q", "-m", "base"]);
        let edited = base
            .replace("line 2\r\n", "line 2 x\r\n")
            .replace("line 18\r\n", "line 18 y\r\n");
        r.write("w.txt", &edited);
        let d = r.diff(DiffSide::Unstaged, "w.txt");
        assert_eq!(d.hunks.len(), 2);
        let sel = [LineSelection {
            hunk: 0,
            lines: changes(&d, 0),
        }];
        apply_lines(
            &r.git,
            r.p(),
            PatchAction::Stage,
            "w.txt",
            &d.fingerprint,
            &sel,
        )
        .unwrap();
        assert!(r.run(&["show", ":w.txt"]).contains("line 2 x"));
        let d = r.diff(DiffSide::Unstaged, "w.txt");
        let sel = [LineSelection {
            hunk: 0,
            lines: changes(&d, 0),
        }];
        apply_lines(
            &r.git,
            r.p(),
            PatchAction::Discard,
            "w.txt",
            &d.fingerprint,
            &sel,
        )
        .unwrap();
        let wt = r.read("w.txt");
        assert!(wt.contains("line 18\r\n"), "{wt:?}");
        assert!(wt.contains("line 2 x\r\n"));
    }

    #[test]
    fn untracked_new_and_commit_diffs() {
        let r = Repo::new();
        r.write("new.txt", "a\nb");
        let d = r.diff(DiffSide::Untracked, "new.txt");
        assert!(d.new_file && !d.partial);
        assert_eq!(d.additions, 2);
        assert!(d.hunks[0].lines[1].no_newline);

        std::fs::write(r.p().join("bin.dat"), [0u8, 1, 2, 3]).unwrap();
        assert!(r.diff(DiffSide::Untracked, "bin.dat").binary);

        r.run(&["add", "new.txt"]);
        let staged = r.diff(DiffSide::Staged, "new.txt");
        assert!(staged.new_file && !staged.partial);
        r.run(&["commit", "-q", "-m", "one"]);
        let first = r.run(&["rev-parse", "HEAD"]);

        r.write("new.txt", "a\nB\n");
        r.run(&["commit", "-qam", "two"]);
        let second = r.run(&["rev-parse", "HEAD"]);
        let commit_req = |c: &str| DiffRequest {
            side: DiffSide::Commit,
            path: "new.txt".into(),
            old_path: None,
            commit: Some(c.into()),
        };
        let d = file_diff(&r.git, r.p(), &commit_req(&second)).unwrap();
        assert_eq!((d.additions, d.deletions), (1, 1));
        let root = file_diff(&r.git, r.p(), &commit_req(&first)).unwrap();
        assert!(root.new_file);

        let c = file_contents(&r.git, r.p(), &commit_req(&second)).unwrap();
        assert_eq!(c.original.as_deref(), Some("a\nb"));
        assert_eq!(c.modified.as_deref(), Some("a\nB\n"));
        let c = file_contents(&r.git, r.p(), &commit_req(&first)).unwrap();
        assert_eq!(c.original, None);

        r.write("new.txt", "worktree\n");
        let c = file_contents(
            &r.git,
            r.p(),
            &DiffRequest {
                side: DiffSide::Unstaged,
                path: "new.txt".into(),
                old_path: None,
                commit: None,
            },
        )
        .unwrap();
        assert_eq!(c.original.as_deref(), Some("a\nB\n"));
        assert_eq!(c.modified.as_deref(), Some("worktree\n"));
    }

    #[test]
    fn staged_rename_is_reported_with_old_path() {
        let r = Repo::new();
        r.write("old.txt", "same content here\nmore\nlines\n");
        r.run(&["add", "."]);
        r.run(&["commit", "-q", "-m", "c"]);
        r.run(&["mv", "old.txt", "new.txt"]);
        let d = file_diff(
            &r.git,
            r.p(),
            &DiffRequest {
                side: DiffSide::Staged,
                path: "new.txt".into(),
                old_path: Some("old.txt".into()),
                commit: None,
            },
        )
        .unwrap();
        assert_eq!(d.old_path.as_deref(), Some("old.txt"));
        assert!(!d.partial && !d.new_file);
    }
}
