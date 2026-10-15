//! Conflict resolution: what each side of a conflicted file contains (index stages 1/2/3 plus
//! the working-tree file with conflict markers), which commits "ours" and "theirs" are for the
//! operation in progress, and the writes that resolve a file (save edited content, take one
//! side as a whole, or delete it) followed by `git add` / `git rm`.
//!
//! Git's "ours" is always the commit HEAD points to: during a merge that is the current branch,
//! during a rebase it is the branch being rebased **onto** (plus the commits already replayed)
//! and "theirs" is the user's own commit being replayed. The context returned here lets the UI
//! explain that instead of showing the bare words.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::blob::{self, LfsPointer, check_path, looks_binary, parse_lfs_pointer};
use crate::ops::{self, FailureKind, OpError, RepoOperation};
use crate::{AccountContext, GitError, GitRunner, index};

/// Text larger than this is not sent to the editor (the file can still be resolved as a whole).
pub const MAX_TEXT_BYTES: u64 = 5 * 1024 * 1024;
const BOM: &[u8] = b"\xEF\xBB\xBF";

/// What happened to the file on each side (porcelain `UU`, `AA`, `DU`…).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ConflictKind {
    BothModified,
    BothAdded,
    /// Deleted on our side, modified on theirs.
    DeletedByUs,
    /// Modified on our side, deleted on theirs.
    DeletedByThem,
    AddedByUs,
    AddedByThem,
    BothDeleted,
}

impl ConflictKind {
    fn from_stages(base: bool, ours: bool, theirs: bool) -> Self {
        match (base, ours, theirs) {
            (true, true, true) => Self::BothModified,
            (false, true, true) => Self::BothAdded,
            (true, false, true) => Self::DeletedByUs,
            (true, true, false) => Self::DeletedByThem,
            (false, true, false) => Self::AddedByUs,
            (false, false, true) => Self::AddedByThem,
            _ => Self::BothDeleted,
        }
    }
}

/// One index stage of a conflicted file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ConflictStage {
    /// Blob id.
    pub oid: String,
    pub mode: String,
    /// Size of the stored blob (of the pointer file for LFS).
    pub size: u64,
    pub binary: bool,
    pub lfs: Option<LfsPointer>,
    /// UTF-8 text without BOM; `None` for binary, LFS, non-UTF-8 or too large content.
    pub text: Option<String>,
}

/// Everything the conflict editor needs for one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ConflictFile {
    pub path: String,
    /// No unmerged entry any more (resolved meanwhile).
    pub resolved: bool,
    pub kind: ConflictKind,
    /// Stage 1 (common ancestor).
    pub base: Option<ConflictStage>,
    /// Stage 2 (HEAD's version).
    pub ours: Option<ConflictStage>,
    /// Stage 3 (the version being merged in / replayed).
    pub theirs: Option<ConflictStage>,
    /// Working tree file exists.
    pub working_exists: bool,
    pub working_size: Option<u64>,
    /// Working-tree text (with conflict markers), without BOM.
    pub working: Option<String>,
    /// Some side is binary or an LFS pointer: resolve by picking a whole side.
    pub binary: bool,
    /// Some side is too large to edit.
    pub too_large: bool,
    /// The result can be edited as text.
    pub editable: bool,
    /// The file uses CRLF line endings (kept when saving).
    pub crlf: bool,
    /// The file starts with a UTF-8 BOM (kept when saving).
    pub bom: bool,
    /// Conflict blocks (`<<<<<<<` … `>>>>>>>`) left in the working file.
    pub blocks: u32,
    pub image: bool,
}

/// What kind of operation produced the conflicts (for the explanation of ours/theirs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ConflictOrigin {
    Merge,
    /// `merge --squash` (no merge state, only `SQUASH_MSG`).
    MergeSquash,
    Rebase,
    CherryPick,
    Revert,
    /// `stash apply` / `stash pop`.
    Stash,
    ApplyMailbox,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ConflictCommit {
    pub oid: String,
    pub summary: String,
    /// Local / remote branches pointing at it.
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ConflictContext {
    pub origin: ConflictOrigin,
    /// Branch checked out (`None` when HEAD is detached, e.g. during a rebase).
    pub current_branch: Option<String>,
    /// HEAD (git's "ours").
    pub ours: Option<ConflictCommit>,
    /// The commit merged / replayed / reverted (git's "theirs").
    pub theirs: Option<ConflictCommit>,
    /// Merge: the branch name from `MERGE_MSG`.
    pub theirs_name: Option<String>,
    /// Rebase: the user's branch being rebased (`refs/heads/` stripped).
    pub rebase_branch: Option<String>,
    /// Rebase: branches pointing at the commit rebased onto.
    pub rebase_onto: Vec<String>,
    pub conflicts: Vec<String>,
}

/// Which side to keep for a whole file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ConflictSide {
    /// Stage 2 (git's `--ours`).
    Ours,
    /// Stage 3 (git's `--theirs`).
    Theirs,
}

/// Number of complete conflict blocks (`<<<<<<<`, optional `|||||||`, `=======`, `>>>>>>>`).
pub fn count_conflict_blocks(text: &str) -> u32 {
    #[derive(PartialEq)]
    enum S {
        Out,
        Ours,
        Base,
        Theirs,
    }
    let is = |line: &str, c: char| {
        let line = line.strip_suffix('\r').unwrap_or(line);
        line.len() >= 7
            && line.chars().take(7).all(|x| x == c)
            && (line.len() == 7 || (c != '=' && line.as_bytes()[7] == b' '))
    };
    let mut state = S::Out;
    let mut count = 0;
    for line in text.split('\n') {
        state = match state {
            S::Out if is(line, '<') => S::Ours,
            S::Ours if is(line, '|') => S::Base,
            S::Ours | S::Base if is(line, '=') => S::Theirs,
            S::Theirs if is(line, '>') => {
                count += 1;
                S::Out
            }
            s => s,
        };
    }
    count
}

fn op_invalid(what: impl Into<String>) -> OpError {
    OpError::rejected(FailureKind::InvalidInput, what)
}

/// Unmerged index entries of `path`: `(stage, mode, oid)`.
fn unmerged(
    git: &GitRunner,
    root: &Path,
    path: &str,
) -> Result<Vec<(u8, String, String)>, GitError> {
    let out = git.run(
        root,
        &AccountContext::default(),
        ["--literal-pathspecs", "ls-files", "-u", "-z", "--", path],
    )?;
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text
        .split('\0')
        .filter_map(|rec| {
            let (meta, p) = rec.split_once('\t')?;
            if p != path {
                return None;
            }
            let mut f = meta.split(' ');
            let mode = f.next()?.to_owned();
            let oid = f.next()?.to_owned();
            let stage = f.next()?.parse().ok()?;
            Some((stage, mode, oid))
        })
        .collect())
}

fn read_stage(git: &GitRunner, root: &Path, mode: String, oid: String) -> ConflictStage {
    let size = blob::object_size(git, root, &oid).unwrap_or(0);
    let mut stage = ConflictStage {
        oid,
        mode,
        size,
        binary: false,
        lfs: None,
        text: None,
    };
    if size > MAX_TEXT_BYTES {
        stage.binary = true;
        return stage;
    }
    let Some(bytes) = blob::object_bytes(git, root, &stage.oid) else {
        return stage;
    };
    stage.lfs = parse_lfs_pointer(&bytes);
    stage.binary = looks_binary(&bytes);
    if stage.lfs.is_none() && !stage.binary {
        stage.text = String::from_utf8(strip_bom(&bytes).to_vec()).ok();
    }
    stage
}

fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(BOM).unwrap_or(bytes)
}

/// More CRLF than lone LF line endings.
fn uses_crlf(bytes: &[u8]) -> bool {
    let crlf = bytes.windows(2).filter(|w| w == b"\r\n").count();
    let lf = bytes.iter().filter(|&&b| b == b'\n').count();
    crlf > 0 && crlf * 2 >= lf
}

/// Reads one conflicted file: its stages and the working-tree file.
pub fn conflict_file(git: &GitRunner, root: &Path, path: &str) -> Result<ConflictFile, GitError> {
    check_path(path)?;
    let mut base = None;
    let mut ours = None;
    let mut theirs = None;
    let entries = unmerged(git, root, path)?;
    let resolved = entries.is_empty();
    for (stage, mode, oid) in entries {
        let s = Some(read_stage(git, root, mode, oid));
        match stage {
            1 => base = s,
            2 => ours = s,
            3 => theirs = s,
            _ => {}
        }
    }
    let full = root.join(path);
    let meta = std::fs::metadata(&full).ok().filter(|m| m.is_file());
    let working_size = meta.as_ref().map(std::fs::Metadata::len);
    let too_large = [&base, &ours, &theirs]
        .into_iter()
        .flatten()
        .any(|s| s.size > MAX_TEXT_BYTES)
        || working_size.is_some_and(|s| s > MAX_TEXT_BYTES);
    let mut binary = [&base, &ours, &theirs]
        .into_iter()
        .flatten()
        .any(|s| s.binary || s.lfs.is_some());
    let mut working = None;
    let mut crlf = false;
    let mut bom = false;
    if !too_large && let Some(bytes) = meta.and_then(|_| std::fs::read(&full).ok()) {
        if parse_lfs_pointer(&bytes).is_some() || looks_binary(&bytes) {
            binary = true;
        } else {
            bom = bytes.starts_with(BOM);
            crlf = uses_crlf(&bytes);
            working = String::from_utf8(strip_bom(&bytes).to_vec()).ok();
        }
    }
    // Without a working file, line endings follow our (else their) version.
    if working_size.is_none()
        && let Some(text) = ours
            .as_ref()
            .or(theirs.as_ref())
            .and_then(|s| s.text.as_ref())
    {
        crlf = uses_crlf(text.as_bytes());
    }
    let text_sides_ok = [&ours, &theirs]
        .into_iter()
        .flatten()
        .all(|s| s.text.is_some());
    let editable =
        !binary && !too_large && text_sides_ok && (working.is_some() || working_size.is_none());
    Ok(ConflictFile {
        path: path.to_owned(),
        resolved,
        kind: ConflictKind::from_stages(base.is_some(), ours.is_some(), theirs.is_some()),
        blocks: working.as_deref().map_or(0, count_conflict_blocks),
        base,
        ours,
        theirs,
        working_exists: working_size.is_some(),
        working_size,
        working,
        binary,
        too_large,
        editable,
        crlf,
        bom,
        image: blob::is_image(path),
    })
}

fn read_trimmed(path: &Path) -> Option<String> {
    let s = std::fs::read_to_string(path).ok()?;
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_owned())
}

fn commit_info(git: &GitRunner, root: &Path, rev: &str) -> Option<ConflictCommit> {
    let ctx = AccountContext::default();
    let out = git
        .run_text(
            root,
            &ctx,
            [
                "-c",
                "log.showSignature=false",
                "log",
                "-1",
                "--format=%H%x00%s",
                "--end-of-options",
                rev,
                "--",
            ],
        )
        .ok()?;
    let (oid, summary) = out.split_once('\0')?;
    Some(ConflictCommit {
        oid: oid.to_owned(),
        summary: summary.to_owned(),
        refs: branches_at(git, root, oid),
    })
}

fn branches_at(git: &GitRunner, root: &Path, oid: &str) -> Vec<String> {
    git.run_text(
        root,
        &AccountContext::default(),
        [
            "for-each-ref",
            &format!("--points-at={oid}"),
            "--format=%(refname:short)",
            "refs/heads",
            "refs/remotes",
        ],
    )
    .map(|s| {
        s.lines()
            .filter(|l| !l.is_empty() && !l.ends_with("/HEAD"))
            .map(str::to_owned)
            .collect()
    })
    .unwrap_or_default()
}

/// First `'quoted'` name of a merge message ("Merge branch 'feature' into main").
fn merge_name(msg: &str) -> Option<String> {
    let first = msg.lines().next()?;
    let start = first.find('\'')? + 1;
    let len = first[start..].find('\'')?;
    Some(first[start..start + len].to_owned())
}

/// Which operation the conflicts come from and which commits are "ours" / "theirs".
pub fn conflict_context(git: &GitRunner, root: &Path) -> Result<ConflictContext, OpError> {
    let state = ops::operation_state(git, root)?;
    let dir = ops::git_dir(git, root)?;
    let head_file = |name: &str| read_trimmed(&dir.join(name));
    let mut cx = ConflictContext {
        origin: ConflictOrigin::Unknown,
        current_branch: ops::current_branch(git, root),
        ours: commit_info(git, root, "HEAD"),
        theirs: None,
        theirs_name: None,
        rebase_branch: None,
        rebase_onto: Vec::new(),
        conflicts: state.conflicts,
    };
    let theirs_of = |name: &str| {
        head_file(name)
            .filter(|oid| crate::commit::is_valid_oid(oid))
            .and_then(|oid| commit_info(git, root, &oid))
    };
    match state.operation {
        Some(RepoOperation::Merge) => {
            cx.origin = ConflictOrigin::Merge;
            cx.theirs = head_file("MERGE_HEAD")
                .and_then(|s| s.lines().next().map(str::to_owned))
                .filter(|oid| crate::commit::is_valid_oid(oid))
                .and_then(|oid| commit_info(git, root, &oid));
            cx.theirs_name = head_file("MERGE_MSG").and_then(|m| merge_name(&m));
        }
        Some(RepoOperation::Rebase) => {
            cx.origin = ConflictOrigin::Rebase;
            cx.theirs = theirs_of("REBASE_HEAD");
            let state_dir = ["rebase-merge", "rebase-apply"]
                .into_iter()
                .map(|d| dir.join(d))
                .find(|d| d.is_dir());
            if let Some(sd) = state_dir {
                cx.rebase_branch = read_trimmed(&sd.join("head-name"))
                    .map(|h| h.strip_prefix("refs/heads/").unwrap_or(&h).to_owned());
                if let Some(onto) =
                    read_trimmed(&sd.join("onto")).filter(|o| crate::commit::is_valid_oid(o))
                {
                    cx.rebase_onto = branches_at(git, root, &onto);
                }
            }
        }
        Some(RepoOperation::CherryPick) => {
            cx.origin = ConflictOrigin::CherryPick;
            cx.theirs = theirs_of("CHERRY_PICK_HEAD");
        }
        Some(RepoOperation::Revert) => {
            cx.origin = ConflictOrigin::Revert;
            cx.theirs = theirs_of("REVERT_HEAD");
        }
        Some(RepoOperation::ApplyMailbox) => cx.origin = ConflictOrigin::ApplyMailbox,
        Some(RepoOperation::Bisect) | None => {
            if dir.join("SQUASH_MSG").exists() && !cx.conflicts.is_empty() {
                cx.origin = ConflictOrigin::MergeSquash;
            } else if cx.conflicts.iter().take(20).any(|p| {
                std::fs::read(root.join(p)).is_ok_and(|b| {
                    b.len() as u64 <= MAX_TEXT_BYTES
                        && b.windows(23).any(|w| w == b">>>>>>> Stashed changes")
                })
            }) {
                cx.origin = ConflictOrigin::Stash;
            }
        }
    }
    Ok(cx)
}

/// Converts edited text back to the file's line endings and BOM.
pub fn encode_result(content: &str, crlf: bool, bom: bool) -> Vec<u8> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let lf = content.replace("\r\n", "\n");
    let text = if crlf { lf.replace('\n', "\r\n") } else { lf };
    let mut out = Vec::with_capacity(text.len() + 3);
    if bom {
        out.extend_from_slice(BOM);
    }
    out.extend_from_slice(text.as_bytes());
    out
}

fn checked(path: &str) -> Result<(), OpError> {
    check_path(path).map_err(|_| op_invalid(format!("invalid path: {path:?}")))
}

/// Writes the resolved content of `path` (keeping its line endings and BOM) and stages it.
pub fn resolve_with_content(
    git: &GitRunner,
    root: &Path,
    path: &str,
    content: &str,
) -> Result<(), OpError> {
    checked(path)?;
    let full = root.join(path);
    let (crlf, bom) = match std::fs::read(&full) {
        Ok(bytes) => (uses_crlf(&bytes), bytes.starts_with(BOM)),
        Err(_) => {
            let f = conflict_file(git, root, path)?;
            (f.crlf, f.bom)
        }
    };
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).map_err(GitError::from)?;
    }
    std::fs::write(&full, encode_result(content, crlf, bom)).map_err(GitError::from)?;
    index::stage(git, root, &[path.to_owned()])?;
    Ok(())
}

/// Resolves `path` by keeping one side as a whole: `git checkout --ours|--theirs` + `git add`,
/// or `git rm` when that side deleted the file.
pub fn resolve_take(
    git: &GitRunner,
    root: &Path,
    ctx: &AccountContext,
    path: &str,
    side: ConflictSide,
) -> Result<(), OpError> {
    checked(path)?;
    let entries = unmerged(git, root, path)?;
    if entries.is_empty() {
        return Err(op_invalid(format!("{path} has no conflict any more")));
    }
    let (stage, flag) = match side {
        ConflictSide::Ours => (2, "--ours"),
        ConflictSide::Theirs => (3, "--theirs"),
    };
    match entries.iter().find(|(s, ..)| *s == stage) {
        Some((_, _, oid)) => {
            // An LFS object that is not downloaded would make the smudge filter go to the
            // network (and fail offline): check out the pointer instead, `git add` keeps it.
            let pointer = blob::object_bytes(git, root, oid)
                .filter(|b| b.len() < 1024)
                .and_then(|b| parse_lfs_pointer(&b));
            let mut runner = git.clone();
            if pointer.is_some_and(|p| blob::lfs_object_path(git, root, &p).is_none()) {
                runner = runner.with_env("GIT_LFS_SKIP_SMUDGE", "1");
            }
            ops::run(
                &runner,
                root,
                ctx,
                &["--literal-pathspecs", "checkout", flag, "--", path],
            )?;
            index::stage(git, root, &[path.to_owned()])?;
        }
        None => {
            ops::run(
                git,
                root,
                ctx,
                &["--literal-pathspecs", "rm", "-q", "-f", "--", path],
            )?;
        }
    }
    Ok(())
}

/// Resolves a file whose final version should not exist (accept a deletion).
pub fn resolve_delete(
    git: &GitRunner,
    root: &Path,
    ctx: &AccountContext,
    path: &str,
) -> Result<(), OpError> {
    checked(path)?;
    ops::run(
        git,
        root,
        ctx,
        &[
            "--literal-pathspecs",
            "rm",
            "-q",
            "-f",
            "--ignore-unmatch",
            "--",
            path,
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blob::tests::Repo;

    #[test]
    fn counts_merge_diff3_and_crlf_blocks() {
        let merge = "a\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> feature\nb\n";
        assert_eq!(count_conflict_blocks(merge), 1);
        let diff3 = "<<<<<<< HEAD\no\n||||||| base\nb\n=======\nt\n>>>>>>> x\n<<<<<<< HEAD\n=======\n>>>>>>> x\n";
        assert_eq!(count_conflict_blocks(diff3), 2);
        let crlf = "<<<<<<< HEAD\r\no\r\n=======\r\nt\r\n>>>>>>> x\r\n";
        assert_eq!(count_conflict_blocks(crlf), 1);
        // Unfinished blocks and look-alikes do not count.
        assert_eq!(count_conflict_blocks("<<<<<<< HEAD\nx\n=======\n"), 0);
        assert_eq!(
            count_conflict_blocks("<<<<<<<< no\n========\n>>>>>>>> no\n"),
            0
        );
    }

    #[test]
    fn encodes_line_endings_and_bom() {
        assert_eq!(encode_result("a\nb\r\n", true, false), b"a\r\nb\r\n");
        assert_eq!(encode_result("a\r\nb\n", false, false), b"a\nb\n");
        assert_eq!(encode_result("x\n", false, true), b"\xEF\xBB\xBFx\n");
        assert_eq!(encode_result("\u{feff}x", false, false), b"x");
    }

    /// main and feature both change line 2 of f.txt.
    fn diverged(r: &Repo) {
        r.write("f.txt", "one\ntwo\nthree\n");
        r.commit_all("base");
        r.run(&["checkout", "-q", "-b", "feature"]);
        r.write("f.txt", "one\nTWO feature\nthree\n");
        r.commit_all("feature change");
        r.run(&["checkout", "-q", "main"]);
        r.write("f.txt", "one\nTWO main\nthree\n");
        r.commit_all("main change");
    }

    #[test]
    fn merge_conflict_with_diff3_markers_resolves_with_edited_content() {
        let r = Repo::new();
        r.run(&["config", "merge.conflictStyle", "diff3"]);
        diverged(&r);
        assert!(r.try_run(&["merge", "feature"]).is_err());

        let f = conflict_file(&r.git, r.p(), "f.txt").unwrap();
        assert_eq!(f.kind, ConflictKind::BothModified);
        assert!(!f.resolved && f.editable && !f.binary && !f.crlf);
        assert_eq!(f.blocks, 1);
        assert_eq!(f.base.unwrap().text.as_deref(), Some("one\ntwo\nthree\n"));
        assert_eq!(
            f.ours.unwrap().text.as_deref(),
            Some("one\nTWO main\nthree\n")
        );
        assert_eq!(
            f.theirs.unwrap().text.as_deref(),
            Some("one\nTWO feature\nthree\n")
        );
        let w = f.working.unwrap();
        assert!(w.contains("||||||| ") && w.contains("<<<<<<< HEAD"), "{w}");

        let cx = conflict_context(&r.git, r.p()).unwrap();
        assert_eq!(cx.origin, ConflictOrigin::Merge);
        assert_eq!(cx.current_branch.as_deref(), Some("main"));
        assert_eq!(cx.theirs_name.as_deref(), Some("feature"));
        assert_eq!(cx.theirs.unwrap().summary, "feature change");
        assert_eq!(cx.ours.unwrap().refs, vec!["main".to_owned()]);
        assert_eq!(cx.conflicts, vec!["f.txt".to_owned()]);

        resolve_with_content(&r.git, r.p(), "f.txt", "one\nboth\nthree\n").unwrap();
        assert_eq!(r.read("f.txt"), b"one\nboth\nthree\n");
        assert!(conflict_file(&r.git, r.p(), "f.txt").unwrap().resolved);
        assert!(ops::conflicted_paths(&r.git, r.p()).unwrap().is_empty());
    }

    #[test]
    fn crlf_working_file_keeps_its_line_endings() {
        let r = Repo::new();
        r.run(&["config", "core.autocrlf", "true"]);
        r.run(&["config", "merge.conflictStyle", "zdiff3"]);
        diverged(&r);
        assert!(r.try_run(&["merge", "feature"]).is_err());
        let f = conflict_file(&r.git, r.p(), "f.txt").unwrap();
        assert!(f.crlf, "{:?}", f.working);
        assert_eq!(f.blocks, 1);
        resolve_with_content(&r.git, r.p(), "f.txt", "one\nmerged\nthree\n").unwrap();
        assert_eq!(r.read("f.txt"), b"one\r\nmerged\r\nthree\r\n");
        // The index holds the normalised (LF) text.
        assert_eq!(r.run(&["show", ":f.txt"]), "one\nmerged\nthree");
    }

    #[test]
    fn rebase_context_names_the_onto_branch_and_the_replayed_commit() {
        let r = Repo::new();
        diverged(&r);
        r.run(&["checkout", "-q", "feature"]);
        assert!(r.try_run(&["rebase", "main"]).is_err());
        let cx = conflict_context(&r.git, r.p()).unwrap();
        assert_eq!(cx.origin, ConflictOrigin::Rebase);
        assert_eq!(cx.current_branch, None);
        assert_eq!(cx.rebase_branch.as_deref(), Some("feature"));
        assert!(
            cx.rebase_onto.contains(&"main".to_owned()),
            "{:?}",
            cx.rebase_onto
        );
        assert_eq!(cx.theirs.unwrap().summary, "feature change");
        assert_eq!(cx.ours.unwrap().summary, "main change");
        // During a rebase "ours" is the upstream version.
        let f = conflict_file(&r.git, r.p(), "f.txt").unwrap();
        assert_eq!(
            f.ours.unwrap().text.as_deref(),
            Some("one\nTWO main\nthree\n")
        );
        resolve_take(&r.git, r.p(), &r.ctx, "f.txt", ConflictSide::Theirs).unwrap();
        assert_eq!(r.read("f.txt"), b"one\nTWO feature\nthree\n");
        assert!(ops::conflicted_paths(&r.git, r.p()).unwrap().is_empty());
    }

    #[test]
    fn cherry_pick_and_stash_contexts() {
        let r = Repo::new();
        diverged(&r);
        let picked = r.run(&["rev-parse", "feature"]);
        assert!(r.try_run(&["cherry-pick", &picked]).is_err());
        let cx = conflict_context(&r.git, r.p()).unwrap();
        assert_eq!(cx.origin, ConflictOrigin::CherryPick);
        assert_eq!(cx.theirs.unwrap().oid, picked);
        resolve_take(&r.git, r.p(), &r.ctx, "f.txt", ConflictSide::Ours).unwrap();
        assert_eq!(r.read("f.txt"), b"one\nTWO main\nthree\n");
        r.run(&["cherry-pick", "--abort"]);

        r.write("f.txt", "one\nstashed\nthree\n");
        r.run(&["stash", "-q"]);
        r.write("f.txt", "one\nnew head\nthree\n");
        r.commit_all("head moves");
        assert!(r.try_run(&["stash", "pop"]).is_err());
        let cx = conflict_context(&r.git, r.p()).unwrap();
        assert_eq!(cx.origin, ConflictOrigin::Stash);
        assert_eq!(cx.conflicts, vec!["f.txt".to_owned()]);
    }

    #[test]
    fn modify_delete_and_add_add_conflicts() {
        let r = Repo::new();
        r.write("keep.txt", "k\n");
        r.write("gone.txt", "g\n");
        r.commit_all("base");
        r.run(&["checkout", "-q", "-b", "feature"]);
        r.run(&["rm", "-q", "gone.txt"]);
        r.write("keep.txt", "k feature\n");
        r.write("new.txt", "theirs new\n");
        r.commit_all("feature");
        r.run(&["checkout", "-q", "main"]);
        r.write("gone.txt", "g main\n");
        r.run(&["rm", "-q", "keep.txt"]);
        r.write("new.txt", "ours new\n");
        r.commit_all("main");
        assert!(r.try_run(&["merge", "feature"]).is_err());

        // Modified here, deleted there.
        let gone = conflict_file(&r.git, r.p(), "gone.txt").unwrap();
        assert_eq!(gone.kind, ConflictKind::DeletedByThem);
        assert!(gone.theirs.is_none() && gone.working_exists);
        // Deleted here, modified there.
        let keep = conflict_file(&r.git, r.p(), "keep.txt").unwrap();
        assert_eq!(keep.kind, ConflictKind::DeletedByUs);
        assert!(keep.ours.is_none());
        let new = conflict_file(&r.git, r.p(), "new.txt").unwrap();
        assert_eq!(new.kind, ConflictKind::BothAdded);
        assert!(new.base.is_none() && new.editable && new.blocks == 1);

        // Taking the deleting side removes the file; taking the other keeps it.
        resolve_take(&r.git, r.p(), &r.ctx, "gone.txt", ConflictSide::Theirs).unwrap();
        assert!(!r.p().join("gone.txt").exists());
        resolve_take(&r.git, r.p(), &r.ctx, "keep.txt", ConflictSide::Theirs).unwrap();
        assert_eq!(r.read("keep.txt"), b"k feature\n");
        resolve_take(&r.git, r.p(), &r.ctx, "new.txt", ConflictSide::Ours).unwrap();
        assert_eq!(r.read("new.txt"), b"ours new\n");
        assert!(ops::conflicted_paths(&r.git, r.p()).unwrap().is_empty());
        let status = r.run(&["status", "--porcelain"]);
        assert!(status.contains("D  gone.txt"), "{status}");
        // Taking a side of a resolved file is refused.
        assert!(resolve_take(&r.git, r.p(), &r.ctx, "new.txt", ConflictSide::Ours).is_err());
    }

    #[test]
    fn binary_conflict_takes_a_whole_side() {
        let r = Repo::new();
        r.write("img.png", [0x89u8, b'P', 0, 1]);
        r.commit_all("base");
        r.run(&["checkout", "-q", "-b", "feature"]);
        r.write("img.png", [0x89u8, b'P', 0, 2, 2]);
        r.commit_all("feature");
        r.run(&["checkout", "-q", "main"]);
        r.write("img.png", [0x89u8, b'P', 0, 3]);
        r.commit_all("main");
        assert!(r.try_run(&["merge", "feature"]).is_err());
        let f = conflict_file(&r.git, r.p(), "img.png").unwrap();
        assert!(f.binary && !f.editable && f.image);
        assert_eq!(f.theirs.as_ref().unwrap().size, 5);
        let preview = blob::blob_preview(
            &r.git,
            r.p(),
            &blob::BlobSource::Index { stage: 3 },
            "img.png",
        )
        .unwrap()
        .unwrap();
        assert_eq!(preview.size, 5);
        resolve_take(&r.git, r.p(), &r.ctx, "img.png", ConflictSide::Theirs).unwrap();
        assert_eq!(r.read("img.png"), [0x89u8, b'P', 0, 2, 2]);
    }

    #[test]
    fn lfs_conflict_previews_objects_and_takes_a_side_offline() {
        let r = Repo::new();
        if r.try_run(&["lfs", "version"]).is_err() {
            eprintln!("git-lfs not installed; skipping");
            return;
        }
        r.run(&["lfs", "install", "--local"]);
        r.run(&["lfs", "track", "*.bin"]);
        r.write("a.bin", [0u8, 1, 2, 3]);
        r.commit_all("base");
        r.run(&["checkout", "-q", "-b", "feature"]);
        r.write("a.bin", [0u8, 9, 9, 9, 9]);
        r.commit_all("feature");
        r.run(&["checkout", "-q", "main"]);
        r.write("a.bin", [0u8, 7]);
        r.commit_all("main");
        assert!(r.try_run(&["merge", "feature"]).is_err());

        let f = conflict_file(&r.git, r.p(), "a.bin").unwrap();
        assert!(f.binary && !f.editable);
        let theirs = f.theirs.unwrap();
        assert_eq!(theirs.lfs.as_ref().map(|p| p.size), Some(5));
        let preview = blob::blob_preview(
            &r.git,
            r.p(),
            &blob::BlobSource::Index { stage: 3 },
            "a.bin",
        )
        .unwrap()
        .unwrap();
        assert_eq!(preview.size, 5);
        assert!(!preview.lfs_missing);
        assert_eq!(
            preview.data.as_deref(),
            Some(blob::base64(&[0, 9, 9, 9, 9]).as_str())
        );

        // Without the object locally, taking that side keeps the pointer instead of downloading.
        let oid = theirs.lfs.unwrap().oid;
        let object = r
            .p()
            .join(".git/lfs/objects")
            .join(&oid[0..2])
            .join(&oid[2..4])
            .join(&oid);
        std::fs::remove_file(object).unwrap();
        let preview = blob::blob_preview(
            &r.git,
            r.p(),
            &blob::BlobSource::Index { stage: 3 },
            "a.bin",
        )
        .unwrap()
        .unwrap();
        assert!(preview.lfs_missing && preview.data.is_none());
        resolve_take(&r.git, r.p(), &r.ctx, "a.bin", ConflictSide::Theirs).unwrap();
        assert!(ops::conflicted_paths(&r.git, r.p()).unwrap().is_empty());
        let staged = r.run(&["cat-file", "-p", ":a.bin"]);
        assert!(staged.contains(&oid), "{staged}");
    }

    #[test]
    fn rejects_bad_paths() {
        let r = Repo::new();
        assert!(conflict_file(&r.git, r.p(), "--output=x").is_err());
        assert!(resolve_with_content(&r.git, r.p(), "../x", "").is_err());
        assert!(resolve_take(&r.git, r.p(), &r.ctx, "-f", ConflictSide::Ours).is_err());
    }
}
