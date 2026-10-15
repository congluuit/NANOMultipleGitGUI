//! LFS maintenance (PLAN §6.7): local storage size, `git lfs prune` with a preview of what it
//! frees, and `git lfs fsck` (objects and pointers) with readable results.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use nanomgg_git_engine::{AccountContext, GitRunner};
use serde::Serialize;

use crate::LfsError;

/// The local LFS object store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct LfsStorage {
    /// `LocalMediaDir` of `git lfs env` (honours `lfs.storage`).
    pub media_dir: Option<String>,
    pub objects: u32,
    pub bytes: u64,
}

/// `LocalMediaDir` from `git lfs env`.
pub fn media_dir(git: &GitRunner, repo: &Path) -> Option<PathBuf> {
    let out = git
        .run_text(repo, &AccountContext::default(), ["lfs", "env"])
        .ok()?;
    out.lines()
        .find_map(|l| l.strip_prefix("LocalMediaDir="))
        .map(|p| PathBuf::from(p.trim()))
        .filter(|p| !p.as_os_str().is_empty())
}

fn is_oid(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Objects (`aa/bb/<oid>` files) under the media dir: `(oid, size)`.
fn walk(dir: &Path, depth: u32, out: &mut Vec<(String, u64)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let Ok(meta) = e.metadata() else { continue };
        let name = e.file_name().to_string_lossy().into_owned();
        if meta.is_dir() && depth < 2 {
            walk(&e.path(), depth + 1, out);
        } else if meta.is_file() && depth == 2 && is_oid(&name) {
            out.push((name, meta.len()));
        }
    }
}

fn local_objects(media: Option<&Path>) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    if let Some(d) = media {
        walk(d, 0, &mut out);
    }
    out
}

/// Size of the local LFS store.
pub fn storage(git: &GitRunner, repo: &Path) -> LfsStorage {
    let dir = media_dir(git, repo);
    let objects = local_objects(dir.as_deref());
    LfsStorage {
        media_dir: dir.map(|d| d.to_string_lossy().replace('\\', "/")),
        objects: objects.len() as u32,
        bytes: objects.iter().map(|(_, n)| n).sum(),
    }
}

fn object_size(media: Option<&Path>, oid: &str) -> Option<u64> {
    let m = media?;
    std::fs::metadata(m.join(&oid[0..2]).join(&oid[2..4]).join(oid))
        .ok()
        .map(|m| m.len())
}

/// One object `git lfs prune` would delete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PruneObject {
    pub oid: String,
    pub size: u64,
}

/// What `git lfs prune` would do (or did).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PruneReport {
    pub local_objects: u32,
    /// Objects kept (recent, unpushed, referenced by the checkout / stashes).
    pub retained: u32,
    /// Objects to delete (dry run) or deleted.
    pub objects: Vec<PruneObject>,
    /// Bytes freed by deleting `objects`.
    pub bytes: u64,
    /// Objects confirmed present on the remote (`--verify-remote`).
    pub verified: Option<u32>,
    /// `objects` / `bytes` are an upper bound: git-lfs did not say exactly which objects go
    /// (its verbose list is incomplete), so every local object not in the checkout is counted.
    pub approximate: bool,
}

/// Parses `prune: 4 local objects, 2 retained[, 2 verified with remote], done.` and the
/// verbose ` * <oid> (<size>)` lines of `git lfs prune --dry-run --verbose`.
pub fn parse_prune(output: &str, media: Option<&Path>) -> PruneReport {
    let mut r = PruneReport::default();
    for line in output.split(['\n', '\r']) {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("prune:") {
            for part in rest.split(',') {
                let mut w = part.split_whitespace();
                let (Some(n), Some(word)) = (w.next(), w.next()) else {
                    continue;
                };
                let Ok(n) = n.parse::<u32>() else { continue };
                match word {
                    "local" => r.local_objects = n,
                    "retained" => r.retained = n,
                    "verified" => r.verified = Some(n),
                    _ => {}
                }
            }
        } else if let Some(rest) = line.strip_prefix("* ") {
            let oid = rest.split_whitespace().next().unwrap_or("");
            if is_oid(oid) && !r.objects.iter().any(|o| o.oid == oid) {
                r.objects.push(PruneObject {
                    oid: oid.to_owned(),
                    size: object_size(media, oid).unwrap_or(0),
                });
            }
        }
    }
    r.bytes = r.objects.iter().map(|o| o.size).sum();
    r
}

fn prune_args(dry_run: bool, verify_remote: bool) -> Vec<&'static str> {
    let mut args = vec!["lfs", "prune", "--verbose"];
    if dry_run {
        args.push("--dry-run");
    }
    if verify_remote {
        args.push("--verify-remote");
    }
    args
}

/// `git lfs prune --dry-run --verbose`: what would be deleted and how much space it frees.
/// `verify_remote` asks the server first (network, runs as `ctx`).
///
/// git-lfs 3.5 prints the counts reliably but lists only some of the objects, so the list is
/// rebuilt: local objects minus those of the current checkout, when that matches the count
/// git-lfs reported (the usual case); otherwise it is an upper bound ([`PruneReport::approximate`]).
pub fn prune_preview(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    verify_remote: bool,
) -> Result<PruneReport, LfsError> {
    let out = git.run(repo, ctx, prune_args(true, verify_remote))?;
    let text = format!(
        "{}
{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let media = media_dir(git, repo);
    let mut report = parse_prune(&text, media.as_deref());
    let to_prune = report.local_objects.saturating_sub(report.retained) as usize;
    if report.objects.len() == to_prune {
        return Ok(report);
    }
    let checkout: HashSet<String> = crate::files::list(git, repo, &AccountContext::default())
        .map(|f| f.into_iter().map(|f| f.oid).collect())
        .unwrap_or_default();
    let candidates: Vec<PruneObject> = local_objects(media.as_deref())
        .into_iter()
        .filter(|(oid, _)| !checkout.contains(oid))
        .map(|(oid, size)| PruneObject { oid, size })
        .collect();
    report.approximate = candidates.len() != to_prune;
    if to_prune == 0 {
        report.objects.clear();
    } else if candidates.len() >= report.objects.len() {
        report.objects = candidates;
    }
    report.bytes = report.objects.iter().map(|o| o.size).sum();
    Ok(report)
}

/// `git lfs prune`: deletes old local copies that are safe to delete (pushed, not recent, not
/// in the checkout). Returns what was deleted (sizes measured before deleting).
pub fn prune(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    verify_remote: bool,
) -> Result<PruneReport, LfsError> {
    let preview = prune_preview(git, repo, ctx, verify_remote)?;
    let before = storage(git, repo);
    git.run(repo, ctx, prune_args(false, verify_remote))?;
    let after = storage(git, repo);
    let media = after.media_dir.as_deref().map(Path::new);
    Ok(PruneReport {
        approximate: false,
        bytes: before.bytes.saturating_sub(after.bytes),
        objects: preview
            .objects
            .into_iter()
            .filter(|o| object_size(media, &o.oid).is_none())
            .collect(),
        ..preview
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum FsckProblemKind {
    /// The object is missing from the local store (not downloaded, or deleted).
    Missing,
    /// The object's content does not match its hash.
    Corrupt,
    /// A pointer file is not in canonical form.
    NonCanonicalPointer,
    /// A file that should be an LFS pointer (matches an LFS pattern) was committed as a
    /// normal git file.
    NotAPointer,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FsckProblem {
    pub kind: FsckProblemKind,
    pub path: Option<String>,
    pub oid: Option<String>,
    /// The line git-lfs printed.
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FsckReport {
    pub ok: bool,
    pub problems: Vec<FsckProblem>,
}

/// `name (oid)` → `(name, oid)`.
fn name_and_oid(s: &str) -> (Option<String>, Option<String>) {
    let s = s.trim();
    if let Some(open) = s.find(" (") {
        let name = s[..open].trim().trim_matches('"').to_owned();
        let inner = &s[open + 2..];
        let oid = inner
            .split([')', ' '])
            .next()
            .filter(|o| is_oid(o))
            .map(str::to_owned);
        return ((!name.is_empty()).then_some(name), oid);
    }
    (None, None)
}

/// Parses `git lfs fsck` output (`objects: openError: a.bin (<oid>) could not be checked…`,
/// `pointer: unexpectedGitObject: "x.psd" (treeish …) should have been a pointer…`).
pub fn parse_fsck(output: &str) -> FsckReport {
    let mut problems = Vec::new();
    for line in output.lines().map(str::trim) {
        let Some((scope, rest)) = line.split_once(": ") else {
            continue;
        };
        if !matches!(scope, "objects" | "pointer" | "pointers") {
            continue;
        }
        let Some((code, detail)) = rest.split_once(": ") else {
            continue;
        };
        let kind = match code {
            "openError" => FsckProblemKind::Missing,
            "corruptObject" => FsckProblemKind::Corrupt,
            "nonCanonicalPointer" => FsckProblemKind::NonCanonicalPointer,
            "unexpectedGitObject" => FsckProblemKind::NotAPointer,
            "repair" => continue,
            _ => FsckProblemKind::Other,
        };
        let (path, oid) = name_and_oid(detail);
        let (path, oid) = match kind {
            // `Pointer for <oid> (blob <sha>) was not canonical`
            FsckProblemKind::NonCanonicalPointer => (
                None,
                detail
                    .strip_prefix("Pointer for ")
                    .and_then(|r| r.split_whitespace().next())
                    .filter(|o| is_oid(o))
                    .map(str::to_owned),
            ),
            _ => (path, oid),
        };
        problems.push(FsckProblem {
            kind,
            path,
            oid,
            detail: line.to_owned(),
        });
    }
    FsckReport {
        ok: problems.is_empty() && !output.to_ascii_lowercase().contains("error"),
        problems,
    }
}

/// `git lfs fsck --dry-run [--objects] [--pointers]`: read-only integrity check (corrupt
/// objects are reported, not moved to `.git/lfs/bad`).
pub fn fsck(
    git: &GitRunner,
    repo: &Path,
    objects: bool,
    pointers: bool,
) -> Result<FsckReport, LfsError> {
    let mut args = vec!["lfs", "fsck", "--dry-run"];
    if objects && !pointers {
        args.push("--objects");
    } else if pointers && !objects {
        args.push("--pointers");
    }
    // fsck exits 1 when it finds problems: read the output either way.
    let out = git
        .command(repo, &AccountContext::default(), &args)
        .env("LC_ALL", "C")
        .output()?;
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let mut report = parse_fsck(&text);
    if !out.status.success() && report.problems.is_empty() {
        return Err(LfsError::Git(nanomgg_git_engine::GitError::Failed {
            args: args.join(" "),
            status: out.status.to_string(),
            stderr: text.trim().to_owned(),
        }));
    }
    report.ok = out.status.success() && report.problems.is_empty();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_prune_dry_run() {
        // Captured from git-lfs 3.5.1 (`--dry-run --verbose`, stdout).
        let out = "prune: 4 local objects, 2 retained, done.\n\n * 92bd03c0759025a6e6141fc9f2cae764f3cac780a3b0882ae0036d913fe38952 (1.0 KB), done.\n * 4975e2df3de96eda21c421e8a84d7b35d3d770098d6fb2ff058bc98d52a3c483 (1.5 KB)\n";
        let media = tempfile::tempdir().unwrap();
        let oid = "92bd03c0759025a6e6141fc9f2cae764f3cac780a3b0882ae0036d913fe38952";
        let p = media.path().join(&oid[0..2]).join(&oid[2..4]);
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join(oid), vec![0u8; 1000]).unwrap();
        let r = parse_prune(out, Some(media.path()));
        assert_eq!((r.local_objects, r.retained, r.verified), (4, 2, None));
        assert_eq!(r.objects.len(), 2);
        assert_eq!(r.objects[0].size, 1000);
        assert_eq!(r.objects[1].size, 0, "missing locally");
        assert_eq!(r.bytes, 1000);
        let r = parse_prune(
            "prune: 4 local objects, 2 retained, 2 verified with remote, done.",
            None,
        );
        assert_eq!(r.verified, Some(2));
        assert!(parse_prune("", None).objects.is_empty());
    }

    #[test]
    fn parses_fsck_output() {
        let out = "objects: openError: a.bin (4fef548847c0c200527049b6bf2886080ae01604c7f979fc3414a5cb0d4d8a04) could not be checked: The system cannot find the file specified.\n\
objects: corruptObject: dir/b c.bin (92bd03c0759025a6e6141fc9f2cae764f3cac780a3b0882ae0036d913fe38952) is corrupt\n\
pointer: nonCanonicalPointer: Pointer for 4975e2df3de96eda21c421e8a84d7b35d3d770098d6fb2ff058bc98d52a3c483 (blob 1234) was not canonical\n\
pointer: unexpectedGitObject: \"x.psd\" (treeish 5678) should have been a pointer but was not\n\
objects: repair: moving corrupt objects to C:\\r\\.git\\lfs\\bad\n";
        let r = parse_fsck(out);
        assert!(!r.ok);
        assert_eq!(r.problems.len(), 4);
        assert_eq!(r.problems[0].kind, FsckProblemKind::Missing);
        assert_eq!(r.problems[0].path.as_deref(), Some("a.bin"));
        assert!(r.problems[0].oid.is_some());
        assert_eq!(r.problems[1].path.as_deref(), Some("dir/b c.bin"));
        assert_eq!(r.problems[1].kind, FsckProblemKind::Corrupt);
        assert_eq!(r.problems[2].kind, FsckProblemKind::NonCanonicalPointer);
        assert!(r.problems[2].oid.is_some());
        assert_eq!(r.problems[3].kind, FsckProblemKind::NotAPointer);
        assert_eq!(r.problems[3].path.as_deref(), Some("x.psd"));
        let ok = parse_fsck("Git LFS fsck OK\n");
        assert!(ok.ok && ok.problems.is_empty());
    }
}
