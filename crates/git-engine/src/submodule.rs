//! Submodules (PLAN §5.9): listing with the recorded vs checked-out commit and the state inside
//! each submodule, and the usual operations — init + update (with progress and cancel), update
//! to the recorded commit, sync URLs, add, deinit and remove.
//!
//! The list is built without parsing `git submodule status` (whose output is not NUL-safe):
//! `.gitmodules` is read with `git config -z --file`, the recorded commits come from the index
//! (`git ls-files --stage -z`, mode `160000`), the checked-out commit from the submodule itself
//! and dirtiness from the parent's `git status --porcelain=v2 -z` submodule flags. Nested
//! submodules are listed by recursing into populated submodules.
//!
//! Operations on a nested submodule run in its parent submodule (git only accepts paths of
//! direct submodules), see [`Submodule::parent`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::ops::{FailureKind, OpError, check_arg, writer};
use crate::progress::{CancelToken, Progress, run_streaming};
use crate::{AccountContext, GitError, GitRunner};

/// How deep nested submodules are listed.
const MAX_DEPTH: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum SubmoduleState {
    /// Not initialised / not checked out yet (`git submodule update --init` needed).
    Uninitialized,
    /// Checked out at exactly the commit recorded by the parent.
    UpToDate,
    /// Checked out at another commit than the recorded one.
    OutOfDate,
    /// The gitlink is in conflict in the parent (merge / rebase).
    Conflicted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Submodule {
    /// Name in `.gitmodules` (usually equal to the path); `None` for a gitlink without entry.
    pub name: Option<String>,
    /// Path relative to the repository the list was made for, with `/`.
    pub path: String,
    /// Path of the submodule containing this one (`None` = direct submodule).
    pub parent: Option<String>,
    /// Nesting depth (0 = direct submodule).
    pub depth: u32,
    /// URL from `.gitmodules`, credentials removed.
    pub url: Option<String>,
    /// `submodule.<name>.branch`.
    pub branch: Option<String>,
    /// Commit recorded by the parent (index); `None` if the parent does not track it.
    pub recorded: Option<String>,
    /// Commit checked out in the submodule; `None` when not initialised.
    pub head: Option<String>,
    /// Branch checked out in the submodule (`None` = detached, the usual state).
    pub head_branch: Option<String>,
    pub state: SubmoduleState,
    /// Modified tracked files inside.
    pub modified: bool,
    /// Untracked files inside.
    pub untracked: bool,
}

/// Removes `user:password@` from URLs (also `https://token@host`).
pub fn redact_url(url: &str) -> String {
    if let Some((scheme, rest)) = url.split_once("://") {
        let (authority, path) = match rest.find('/') {
            Some(i) => rest.split_at(i),
            None => (rest, ""),
        };
        if let Some((_, host)) = authority.rsplit_once('@')
            && !scheme.eq_ignore_ascii_case("ssh")
        {
            return format!("{scheme}://{host}{path}");
        }
        if let Some((user, host)) = authority.rsplit_once('@')
            && let Some((user, _)) = user.split_once(':')
        {
            return format!("{scheme}://{user}@{host}{path}");
        }
    }
    url.to_owned()
}

/// `.gitmodules` entries keyed by name: `(path, url, branch)`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ModuleConfig {
    pub name: String,
    pub path: Option<String>,
    pub url: Option<String>,
    pub branch: Option<String>,
}

/// Parses `git config -z --file .gitmodules --get-regexp '^submodule\.'` (`key\nvalue\0`).
pub fn parse_gitmodules(raw: &[u8]) -> Vec<ModuleConfig> {
    let mut out: Vec<ModuleConfig> = Vec::new();
    for entry in raw.split(|b| *b == 0).filter(|e| !e.is_empty()) {
        let text = String::from_utf8_lossy(entry);
        let (key, value) = text.split_once('\n').unwrap_or((&text, ""));
        let Some(rest) = key.strip_prefix("submodule.") else {
            continue;
        };
        // The name may itself contain dots: the variable is after the last one.
        let Some((name, var)) = rest.rsplit_once('.') else {
            continue;
        };
        let idx = match out.iter().position(|m| m.name == name) {
            Some(i) => i,
            None => {
                out.push(ModuleConfig {
                    name: name.to_owned(),
                    ..Default::default()
                });
                out.len() - 1
            }
        };
        let m = &mut out[idx];
        let value = Some(value.to_owned());
        match var.to_ascii_lowercase().as_str() {
            "path" => m.path = value,
            "url" => m.url = value,
            "branch" => m.branch = value,
            _ => {}
        }
    }
    out
}

/// Gitlinks (`mode 160000`) of `git ls-files --stage -z`: path → (recorded oid, conflicted).
pub fn parse_gitlinks(raw: &[u8]) -> HashMap<String, (String, bool)> {
    let mut out: HashMap<String, (String, bool)> = HashMap::new();
    for entry in raw.split(|b| *b == 0).filter(|e| !e.is_empty()) {
        let text = String::from_utf8_lossy(entry);
        let Some((meta, path)) = text.split_once('\t') else {
            continue;
        };
        let mut f = meta.split(' ');
        let (Some(mode), Some(oid), Some(stage)) = (f.next(), f.next(), f.next()) else {
            continue;
        };
        if mode != "160000" {
            continue;
        }
        let conflicted = stage != "0";
        out.entry(path.to_owned())
            .and_modify(|e| e.1 |= conflicted)
            .or_insert_with(|| (oid.to_owned(), conflicted));
        if stage == "2" {
            // "ours" is what the conflicted gitlink was before the operation.
            if let Some(e) = out.get_mut(path) {
                e.0 = oid.to_owned();
            }
        }
    }
    out
}

/// Submodule flags of `git status --porcelain=v2 -z`: path → (modified, untracked).
pub fn parse_status_flags(raw: &[u8]) -> HashMap<String, (bool, bool)> {
    let mut out = HashMap::new();
    let mut it = raw.split(|b| *b == 0).filter(|e| !e.is_empty());
    while let Some(entry) = it.next() {
        let text = String::from_utf8_lossy(entry);
        let kind = text.split(' ').next().unwrap_or("");
        let (sub, path) = match kind {
            "1" => {
                let f: Vec<&str> = text.splitn(9, ' ').collect();
                (f.get(2).copied(), f.get(8).copied())
            }
            "2" => {
                let f: Vec<&str> = text.splitn(10, ' ').collect();
                // The original path follows as its own NUL-separated field.
                let _ = it.next();
                (f.get(2).copied(), f.get(9).copied())
            }
            "u" => {
                let f: Vec<&str> = text.splitn(11, ' ').collect();
                (f.get(2).copied(), f.get(10).copied())
            }
            _ => (None, None),
        };
        if let (Some(sub), Some(path)) = (sub, path)
            && sub.starts_with('S')
        {
            let b = sub.as_bytes();
            out.insert(
                path.to_owned(),
                (b.get(2) == Some(&b'M'), b.get(3) == Some(&b'U')),
            );
        }
    }
    out
}

/// Whether `dir` holds a checked-out repository (`.git` file or directory).
fn populated(dir: &Path) -> bool {
    dir.join(".git").exists()
}

fn list_level(
    git: &GitRunner,
    repo: &Path,
    prefix: &str,
    depth: u32,
    out: &mut Vec<Submodule>,
) -> Result<(), GitError> {
    let none = AccountContext::default();
    let modules = if repo.join(".gitmodules").is_file() {
        let o = git
            .command(
                repo,
                &none,
                [
                    "config",
                    "-z",
                    "--file",
                    ".gitmodules",
                    "--get-regexp",
                    r"^submodule\.",
                ],
            )
            .output()?;
        parse_gitmodules(&o.stdout)
    } else {
        Vec::new()
    };
    let gitlinks = parse_gitlinks(&git.run(repo, &none, ["ls-files", "--stage", "-z"])?.stdout);
    if modules.is_empty() && gitlinks.is_empty() {
        return Ok(());
    }
    let flags = parse_status_flags(
        &git.run(
            repo,
            &none,
            [
                "status",
                "--porcelain=v2",
                "-z",
                "--ignore-submodules=none",
                "--untracked-files=no",
            ],
        )?
        .stdout,
    );
    // Untracked-only changes inside submodules are reported with `-unormal` only.
    let untracked = parse_status_flags(
        &git.run(
            repo,
            &none,
            [
                "status",
                "--porcelain=v2",
                "-z",
                "--ignore-submodules=none",
                "--untracked-files=normal",
            ],
        )?
        .stdout,
    );

    let mut paths: Vec<(Option<&ModuleConfig>, String)> = modules
        .iter()
        .filter_map(|m| m.path.clone().map(|p| (Some(m), p)))
        .collect();
    for path in gitlinks.keys() {
        if !paths.iter().any(|(_, p)| p == path) {
            paths.push((None, path.clone()));
        }
    }
    paths.sort_by(|a, b| a.1.cmp(&b.1));

    for (module, rel) in paths {
        let dir = repo.join(&rel);
        let (recorded, conflicted) = match gitlinks.get(&rel) {
            Some((oid, c)) => (Some(oid.clone()), *c),
            None => (None, false),
        };
        let (head, head_branch) = if populated(&dir) {
            (
                git.run_text(&dir, &none, ["rev-parse", "-q", "--verify", "HEAD"])
                    .ok()
                    .filter(|s| !s.is_empty()),
                git.run_text(&dir, &none, ["symbolic-ref", "-q", "--short", "HEAD"])
                    .ok()
                    .filter(|s| !s.is_empty()),
            )
        } else {
            (None, None)
        };
        let state = if conflicted {
            SubmoduleState::Conflicted
        } else if head.is_none() {
            SubmoduleState::Uninitialized
        } else if recorded.is_none() || head == recorded {
            SubmoduleState::UpToDate
        } else {
            SubmoduleState::OutOfDate
        };
        let (modified, _) = flags.get(&rel).copied().unwrap_or_default();
        let (_, has_untracked) = untracked.get(&rel).copied().unwrap_or_default();
        let full = if prefix.is_empty() {
            rel.clone()
        } else {
            format!("{prefix}/{rel}")
        };
        out.push(Submodule {
            name: module.map(|m| m.name.clone()),
            path: full.clone(),
            parent: (!prefix.is_empty()).then(|| prefix.to_owned()),
            depth,
            url: module.and_then(|m| m.url.as_deref()).map(redact_url),
            branch: module.and_then(|m| m.branch.clone()),
            recorded,
            head: head.clone(),
            head_branch,
            state,
            modified,
            untracked: has_untracked,
        });
        if head.is_some() && depth + 1 < MAX_DEPTH {
            list_level(git, &dir, &full, depth + 1, out)?;
        }
    }
    Ok(())
}

/// Every submodule of `repo`, nested ones included (depth-first, parents first).
pub fn list(git: &GitRunner, repo: &Path) -> Result<Vec<Submodule>, GitError> {
    let mut out = Vec::new();
    list_level(git, repo, "", 0, &mut out)?;
    Ok(out)
}

/// Looks up a listed submodule by its path; rejects anything that is not one.
pub fn find(git: &GitRunner, repo: &Path, path: &str) -> Result<Submodule, OpError> {
    check_arg(path, "submodule path")?;
    let wanted = path.replace('\\', "/");
    list(git, repo)?
        .into_iter()
        .find(|s| s.path == wanted)
        .ok_or_else(|| {
            OpError::rejected(FailureKind::NotFound, format!("no submodule at '{path}'"))
        })
}

/// Repository a submodule operation runs in (its parent) and the submodule's path there.
fn location(repo: &Path, s: &Submodule) -> (PathBuf, String) {
    match &s.parent {
        Some(parent) => (
            repo.join(parent),
            s.path
                .strip_prefix(&format!("{parent}/"))
                .unwrap_or(&s.path)
                .to_owned(),
        ),
        None => (repo.to_owned(), s.path.clone()),
    }
}

/// What [`update`] does.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdateOptions {
    /// Submodule paths (from [`list`]); empty = every direct submodule of `repo`.
    pub paths: Vec<String>,
    /// `--init`: initialise submodules that are not yet.
    pub init: bool,
    /// `--recursive`: nested submodules too.
    pub recursive: bool,
}

/// Summary of an update.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SubmoduleUpdateSummary {
    /// `Submodule path 'x': checked out '<oid>'` lines.
    pub checked_out: Vec<String>,
}

fn checked_out_paths(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter_map(|l| {
            let rest = l.trim().strip_prefix("Submodule path '")?;
            let (path, tail) = rest.split_once("': ")?;
            tail.starts_with("checked out").then(|| path.to_owned())
        })
        .collect()
}

/// Groups submodule paths by the repository the command has to run in.
fn grouped(
    git: &GitRunner,
    repo: &Path,
    paths: &[String],
) -> Result<Vec<(PathBuf, Vec<String>)>, OpError> {
    if paths.is_empty() {
        return Ok(vec![(repo.to_owned(), Vec::new())]);
    }
    let all = list(git, repo)?;
    let mut groups: Vec<(PathBuf, Vec<String>)> = Vec::new();
    for p in paths {
        check_arg(p, "submodule path")?;
        let wanted = p.replace('\\', "/");
        let s = all.iter().find(|s| s.path == wanted).ok_or_else(|| {
            OpError::rejected(FailureKind::NotFound, format!("no submodule at '{p}'"))
        })?;
        let (cwd, rel) = location(repo, s);
        match groups.iter_mut().find(|(c, _)| *c == cwd) {
            Some((_, list)) => list.push(rel),
            None => groups.push((cwd, vec![rel])),
        }
    }
    Ok(groups)
}

/// `git submodule update [--init] [--recursive] --progress -- <paths>`: checks submodules out
/// at the commit recorded by the parent (cloning them first with `--init`). Network access
/// runs as `ctx`; progress lines of the clones / fetches reach `on_progress`.
pub fn update<F: FnMut(Progress)>(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    opts: &UpdateOptions,
    cancel: &CancelToken,
    mut on_progress: F,
) -> Result<SubmoduleUpdateSummary, OpError> {
    let mut summary = SubmoduleUpdateSummary::default();
    for (cwd, paths) in grouped(git, repo, &opts.paths)? {
        let mut args: Vec<String> = vec!["submodule".into(), "update".into()];
        if opts.init {
            args.push("--init".into());
        }
        if opts.recursive {
            args.push("--recursive".into());
        }
        args.push("--progress".into());
        args.push("--".into());
        args.extend(paths);
        let out = run_streaming(&writer(git), &cwd, ctx, &args, cancel, &mut on_progress)?;
        let prefix = cwd
            .strip_prefix(repo)
            .ok()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .filter(|p| !p.is_empty());
        for p in checked_out_paths(&format!("{}\n{}", out.stdout, out.stderr)) {
            summary.checked_out.push(match &prefix {
                Some(pre) => format!("{pre}/{p}"),
                None => p,
            });
        }
    }
    Ok(summary)
}

/// `git submodule sync [--recursive] -- <paths>`: copies the URLs of `.gitmodules` into the
/// repository config (after the URL changed upstream).
pub fn sync(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    paths: &[String],
    recursive: bool,
) -> Result<(), OpError> {
    for (cwd, paths) in grouped(git, repo, paths)? {
        let mut args: Vec<String> = vec!["submodule".into(), "sync".into(), "--quiet".into()];
        if recursive {
            args.push("--recursive".into());
        }
        args.push("--".into());
        args.extend(paths);
        writer(git)
            .run(&cwd, ctx, &args)
            .map_err(crate::ops::classify)?;
    }
    Ok(())
}

/// Checks a URL for `git submodule add` (same rules as clone: not option-like, no
/// remote-helper syntax, no control characters; relative `../x` URLs are fine).
pub fn validate_url(url: &str) -> Result<(), OpError> {
    crate::clone::validate_url(url)
}

/// Checks a repository-relative path for a new submodule / worktree.
pub fn validate_rel_path(path: &str) -> Result<String, OpError> {
    let p = path.trim().replace('\\', "/");
    let p = p.trim_end_matches('/');
    let bad = p.is_empty()
        || p.starts_with('/')
        || p.starts_with('-')
        || p.as_bytes().get(1) == Some(&b':')
        || p.chars().any(char::is_control)
        || p.split('/')
            .any(|c| c == ".." || c == "." || c.is_empty() || c == ".git");
    if bad {
        return Err(OpError::rejected(
            FailureKind::InvalidInput,
            format!("invalid submodule path: {path:?}"),
        ));
    }
    Ok(p.to_owned())
}

/// `git submodule add [-b <branch>] -- <url> <path>` (clones it, stages `.gitmodules` and the
/// gitlink). Network access runs as `ctx`.
#[allow(clippy::too_many_arguments)]
pub fn add<F: FnMut(Progress)>(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    url: &str,
    path: &str,
    branch: Option<&str>,
    cancel: &CancelToken,
    on_progress: F,
) -> Result<Submodule, OpError> {
    let url = url.trim();
    validate_url(url)?;
    let path = validate_rel_path(path)?;
    let mut args: Vec<String> = vec!["submodule".into(), "add".into(), "--progress".into()];
    if let Some(b) = branch.map(str::trim).filter(|b| !b.is_empty()) {
        crate::branch::validate_branch_name(git, repo, b)?;
        args.extend(["-b".into(), b.to_owned()]);
    }
    args.extend(["--".into(), url.to_owned(), path.clone()]);
    run_streaming(&writer(git), repo, ctx, &args, cancel, on_progress)?;
    find(git, repo, &path)
}

/// `git submodule deinit [-f] -- <path>`: empties the submodule's folder and forgets its URL
/// in the repository config; `.gitmodules` and the gitlink stay. Local changes inside are
/// refused with [`FailureKind::DirtyWorktree`] unless `force`.
pub fn deinit(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    path: &str,
    force: bool,
) -> Result<(), OpError> {
    let s = find(git, repo, path)?;
    if !force && (s.modified || s.untracked) {
        return Err(OpError::rejected(
            FailureKind::DirtyWorktree,
            format!("submodule '{}' contains local modifications", s.path),
        ));
    }
    let (cwd, rel) = location(repo, &s);
    let mut args = vec!["submodule", "deinit"];
    if force {
        args.push("--force");
    }
    args.extend(["--", &rel]);
    writer(git)
        .run(&cwd, ctx, &args)
        .map_err(crate::ops::classify)?;
    Ok(())
}

/// Removes a submodule from the repository: deinit, then `git rm` (drops it from
/// `.gitmodules`, stages the removal). Commit afterwards. The clone kept under
/// `.git/modules/<name>` is left alone (re-adding the same path reuses it).
pub fn remove(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    path: &str,
    force: bool,
) -> Result<(), OpError> {
    let s = find(git, repo, path)?;
    if s.head.is_some() {
        deinit(git, repo, ctx, path, force)?;
    }
    let (cwd, rel) = location(repo, &s);
    let mut args = vec!["rm", "-q"];
    if force {
        args.push("-f");
    }
    args.extend(["--", &rel]);
    writer(git)
        .run(&cwd, ctx, &args)
        .map_err(crate::ops::classify)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::test_support::TestRepo;

    #[test]
    fn parses_gitmodules_with_dotted_names() {
        let raw = b"submodule.lib/my sub.path\nlib/my sub\0submodule.lib/my sub.url\nhttps://u:tok@host/a.git\0submodule.a.b.path\nab\0submodule.a.b.branch\nmain\0submodule.a.b.URL\n../ab\0";
        let m = parse_gitmodules(raw);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].name, "lib/my sub");
        assert_eq!(m[0].path.as_deref(), Some("lib/my sub"));
        assert_eq!(m[1].name, "a.b");
        assert_eq!(m[1].branch.as_deref(), Some("main"));
        assert_eq!(m[1].url.as_deref(), Some("../ab"));
    }

    #[test]
    fn parses_gitlinks_and_status_flags() {
        let raw = b"100644 aaaa 0\tREADME.md\x00160000 bbbb 0\tlib/my sub\x00160000 c1 1\tconf\x00160000 c2 2\tconf\x00160000 c3 3\tconf\x00";
        let g = parse_gitlinks(raw);
        assert_eq!(g.len(), 2);
        assert_eq!(g["lib/my sub"], ("bbbb".to_owned(), false));
        assert_eq!(g["conf"], ("c2".to_owned(), true));

        let raw = b"1 .M SC.U 160000 160000 160000 aa aa lib/my sub\x001 .M N... 100644 100644 100644 a b x y.txt\x002 R. SCM. 160000 160000 160000 a a R100 new path\0old path\0";
        let f = parse_status_flags(raw);
        assert_eq!(f["lib/my sub"], (false, true));
        assert_eq!(f["new path"], (true, false));
        assert!(!f.contains_key("x y.txt"));
    }

    #[test]
    fn redacts_and_validates() {
        assert_eq!(
            redact_url("https://user:ghp_secret@github.com/a/b.git"),
            "https://github.com/a/b.git"
        );
        assert_eq!(redact_url("https://tok@host/x"), "https://host/x");
        assert_eq!(
            redact_url("git@github.com:a/b.git"),
            "git@github.com:a/b.git"
        );
        assert_eq!(redact_url("ssh://git@host/x"), "ssh://git@host/x");
        assert_eq!(validate_rel_path("libs\\x/").unwrap(), "libs/x");
        for bad in ["", "/abs", "../x", "a/../b", "-x", "C:/x", ".git/x", "a//b"] {
            assert!(validate_rel_path(bad).is_err(), "{bad:?}");
        }
        assert!(validate_url("../sibling.git").is_ok());
        assert!(validate_url("--upload-pack=x").is_err());
        assert!(checked_out_paths("Submodule path 'a b': checked out 'abc'\nother").len() == 1);
    }

    /// Runner allowing `file://` submodules (git refuses them by default since 2.38).
    fn runner() -> GitRunner {
        GitRunner::default().with_env("GIT_CONFIG_PARAMETERS", "'protocol.file.allow'='always'")
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        top: TestRepo,
        lib_url: String,
    }

    /// `top` with submodule `lib/my sub` (from `lib`), which itself has submodule `nested`.
    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let mut nested = TestRepo::init_at(&dir.path().join("nested"));
        nested.git = runner();
        let mut lib = TestRepo::init_at(&dir.path().join("lib"));
        lib.git = runner();
        lib.git(&[
            "submodule",
            "add",
            "-q",
            "--",
            &url_of(&nested.path),
            "nested",
        ]);
        lib.git(&["commit", "-q", "-m", "add nested"]);
        let mut top = TestRepo::init_at(&dir.path().join("top"));
        top.git = runner();
        let lib_url = url_of(&lib.path);
        Fixture {
            _dir: dir,
            top,
            lib_url,
        }
    }

    fn url_of(p: &Path) -> String {
        p.to_string_lossy().replace('\\', "/")
    }

    #[test]
    fn add_list_update_sync_and_remove() {
        let f = fixture();
        let r = &f.top;
        assert!(list(&r.git, &r.path).unwrap().is_empty());
        let mut lines = 0;
        let added = add(
            &r.git,
            &r.path,
            &r.ctx,
            &f.lib_url,
            "lib/my sub",
            None,
            &CancelToken::new(),
            |_| lines += 1,
        )
        .unwrap();
        assert_eq!(added.path, "lib/my sub");
        assert_eq!(added.state, SubmoduleState::UpToDate);
        r.git(&["commit", "-q", "-m", "add lib"]);

        let subs = list(&r.git, &r.path).unwrap();
        assert_eq!(subs.len(), 2, "{subs:?}");
        assert_eq!(subs[1].path, "lib/my sub/nested");
        assert_eq!(subs[1].parent.as_deref(), Some("lib/my sub"));
        assert_eq!(subs[1].depth, 1);
        assert_eq!(subs[1].state, SubmoduleState::Uninitialized);

        // Init the nested one through its parent.
        let s = update(
            &r.git,
            &r.path,
            &r.ctx,
            &UpdateOptions {
                paths: vec!["lib/my sub/nested".into()],
                init: true,
                recursive: false,
            },
            &CancelToken::new(),
            |_| {},
        )
        .unwrap();
        assert_eq!(s.checked_out, vec!["lib/my sub/nested".to_owned()]);
        let nested = find(&r.git, &r.path, "lib/my sub/nested").unwrap();
        assert_eq!(nested.state, SubmoduleState::UpToDate);

        // Moving the submodule's HEAD makes it out of date; untracked files show up.
        let sub = TestRepo::open(&r.path.join("lib/my sub"));
        let recorded = sub.head();
        sub.commit("moved.txt", "m\n", "moved");
        sub.write("untracked.txt", "u\n");
        let s = find(&r.git, &r.path, "lib/my sub").unwrap();
        assert_eq!(s.state, SubmoduleState::OutOfDate);
        assert_eq!(s.recorded.as_deref(), Some(recorded.as_str()));
        assert_ne!(s.head, s.recorded);
        assert!(s.untracked && !s.modified, "{s:?}");

        // Update back to the recorded commit.
        update(
            &r.git,
            &r.path,
            &r.ctx,
            &UpdateOptions {
                paths: vec!["lib/my sub".into()],
                init: false,
                recursive: true,
            },
            &CancelToken::new(),
            |_| {},
        )
        .unwrap();
        let s = find(&r.git, &r.path, "lib/my sub").unwrap();
        assert_eq!(s.state, SubmoduleState::UpToDate);

        sync(&r.git, &r.path, &r.ctx, &[], true).unwrap();

        // Deinit refuses local changes unless forced.
        let err = deinit(&r.git, &r.path, &r.ctx, "lib/my sub", false).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::DirtyWorktree));
        let err = find(&r.git, &r.path, "--x").unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::InvalidName));
        let err = find(&r.git, &r.path, "nope").unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::NotFound));

        remove(&r.git, &r.path, &r.ctx, "lib/my sub", true).unwrap();
        assert!(list(&r.git, &r.path).unwrap().is_empty());
        assert!(!r.path.join("lib/my sub/README.md").exists());
    }

    #[test]
    fn fresh_clone_lists_uninitialized_then_init_recursive() {
        let f = fixture();
        let r = &f.top;
        add(
            &r.git,
            &r.path,
            &r.ctx,
            &f.lib_url,
            "lib",
            None,
            &CancelToken::new(),
            |_| {},
        )
        .unwrap();
        r.git(&["commit", "-q", "-m", "add lib"]);
        let clone_dir = r.path.parent().unwrap().join("clone");
        r.git(&["clone", "-q", &url_of(&r.path), &url_of(&clone_dir)]);
        let mut c = TestRepo::open(&clone_dir);
        c.git = runner();
        let subs = list(&c.git, &c.path).unwrap();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].state, SubmoduleState::Uninitialized);
        assert!(subs[0].recorded.is_some() && subs[0].head.is_none());

        let mut progress = 0;
        update(
            &c.git,
            &c.path,
            &c.ctx,
            &UpdateOptions {
                paths: Vec::new(),
                init: true,
                recursive: true,
            },
            &CancelToken::new(),
            |_| progress += 1,
        )
        .unwrap();
        let subs = list(&c.git, &c.path).unwrap();
        assert_eq!(subs.len(), 2);
        assert!(
            subs.iter().all(|s| s.state == SubmoduleState::UpToDate),
            "{subs:?}"
        );

        let cancelled = CancelToken::new();
        cancelled.cancel();
        let err = update(
            &c.git,
            &c.path,
            &c.ctx,
            &UpdateOptions::default(),
            &cancelled,
            |_| {},
        )
        .unwrap_err();
        assert!(matches!(err, OpError::Cancelled));
    }
}
