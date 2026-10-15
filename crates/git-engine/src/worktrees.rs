//! Linked worktrees (`git worktree`, PLAN §5.9) — not to be confused with the *working tree*
//! of [`crate::status`] / [`crate::index`]: list (`git worktree list --porcelain -z`), add on an
//! existing or a new branch, remove (force when it has changes), prune, lock / unlock.
//!
//! A branch can be checked out in only one worktree: checking it out elsewhere is refused with
//! [`FailureKind::CheckedOutElsewhere`](crate::ops::FailureKind::CheckedOutElsewhere) (see
//! [`crate::ops::failure_kind`]), and [`Worktree::branch`] tells the UI where it is.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::branch::validate_branch_name;
use crate::ops::{FailureKind, OpError, check_arg, classify, resolve_commit, writer};
use crate::{AccountContext, GitError, GitRunner};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    /// Absolute path as git prints it (`/` separators).
    pub path: String,
    /// Checked-out commit (`None` for a bare main repository).
    pub head: Option<String>,
    /// Checked-out branch (short name); `None` when detached.
    pub branch: Option<String>,
    pub detached: bool,
    pub bare: bool,
    /// The main worktree (first entry): cannot be removed, locked or pruned.
    pub is_main: bool,
    /// The worktree the list was requested from.
    pub is_current: bool,
    pub locked: bool,
    pub lock_reason: Option<String>,
    /// Its folder is gone: `git worktree prune` will drop it.
    pub prunable: bool,
    pub prunable_reason: Option<String>,
}

fn normalize(p: &str) -> String {
    let p = p.replace('\\', "/");
    let p = p.trim_end_matches('/');
    if cfg!(windows) {
        p.to_lowercase()
    } else {
        p.to_owned()
    }
}

/// Same folder? (case-insensitive and separator-agnostic on Windows).
pub fn same_path(a: &str, b: &str) -> bool {
    normalize(a) == normalize(b)
}

/// Parses `git worktree list --porcelain -z`.
pub fn parse_worktree_list(raw: &[u8], current: &str) -> Vec<Worktree> {
    let text = String::from_utf8_lossy(raw);
    let mut out: Vec<Worktree> = Vec::new();
    let mut cur: Option<Worktree> = None;
    let flush = |cur: &mut Option<Worktree>, out: &mut Vec<Worktree>| {
        if let Some(mut w) = cur.take() {
            w.is_main = out.is_empty();
            w.is_current = same_path(&w.path, current);
            out.push(w);
        }
    };
    for field in text.split('\0') {
        if field.is_empty() {
            flush(&mut cur, &mut out);
            continue;
        }
        let (key, value) = match field.split_once(' ') {
            Some((k, v)) => (k, Some(v)),
            None => (field, None),
        };
        if key == "worktree" {
            flush(&mut cur, &mut out);
            cur = Some(Worktree {
                path: value.unwrap_or_default().replace('\\', "/"),
                head: None,
                branch: None,
                detached: false,
                bare: false,
                is_main: false,
                is_current: false,
                locked: false,
                lock_reason: None,
                prunable: false,
                prunable_reason: None,
            });
            continue;
        }
        let Some(w) = cur.as_mut() else { continue };
        let value = value.map(str::to_owned).filter(|v| !v.is_empty());
        match key {
            "HEAD" => w.head = value,
            "branch" => {
                w.branch = value.map(|b| b.strip_prefix("refs/heads/").unwrap_or(&b).to_owned());
            }
            "detached" => w.detached = true,
            "bare" => w.bare = true,
            "locked" => {
                w.locked = true;
                w.lock_reason = value;
            }
            "prunable" => {
                w.prunable = true;
                w.prunable_reason = value;
            }
            _ => {}
        }
    }
    flush(&mut cur, &mut out);
    out
}

/// Worktrees of the repository `repo` belongs to (main first).
pub fn list(git: &GitRunner, repo: &Path) -> Result<Vec<Worktree>, GitError> {
    let out = git.run(
        repo,
        &AccountContext::default(),
        ["worktree", "list", "--porcelain", "-z"],
    )?;
    let current = git
        .run_text(
            repo,
            &AccountContext::default(),
            ["rev-parse", "--show-toplevel"],
        )
        .unwrap_or_else(|_| repo.to_string_lossy().into_owned());
    Ok(parse_worktree_list(&out.stdout, &current))
}

/// The listed worktree at `path`; anything else is rejected (the UI can only act on what git
/// lists, never on an arbitrary folder).
pub fn find(git: &GitRunner, repo: &Path, path: &str) -> Result<Worktree, OpError> {
    check_arg(path, "worktree path")?;
    list(git, repo)?
        .into_iter()
        .find(|w| same_path(&w.path, path))
        .ok_or_else(|| OpError::rejected(FailureKind::NotFound, format!("no worktree at '{path}'")))
}

/// Which branch a new worktree checks out.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WorktreeTarget {
    /// An existing local branch (must not be checked out elsewhere).
    Branch { name: String },
    /// A new branch `name` created at `start` (branch, tag, commit…; `None` = HEAD).
    NewBranch { name: String, start: Option<String> },
    /// Detached HEAD at a commit-ish.
    Detached { rev: String },
}

/// Checks the folder for a new worktree: absolute, and missing or an empty directory.
fn check_new_folder(path: &Path) -> Result<(), OpError> {
    let text = path.to_string_lossy();
    if !path.is_absolute() || text.starts_with('-') || text.chars().any(char::is_control) {
        return Err(OpError::rejected(
            FailureKind::InvalidInput,
            format!("worktree folder must be an absolute path: {text:?}"),
        ));
    }
    if path.exists() {
        let empty = path.is_dir()
            && std::fs::read_dir(path)
                .map(|mut d| d.next().is_none())
                .unwrap_or(false);
        if !empty {
            return Err(OpError::rejected(
                FailureKind::AlreadyExists,
                format!("'{text}' already exists and is not an empty folder"),
            ));
        }
    }
    Ok(())
}

/// `git worktree add` at `path` for `target`. Returns the new worktree.
pub fn add(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    path: &Path,
    target: &WorktreeTarget,
) -> Result<Worktree, OpError> {
    check_new_folder(path)?;
    let folder = path.to_string_lossy().replace('\\', "/");
    let mut args: Vec<String> = vec!["worktree".into(), "add".into()];
    match target {
        WorktreeTarget::Branch { name } => {
            validate_branch_name(git, repo, name)?;
            if !crate::ops::ref_exists(git, repo, &format!("refs/heads/{name}")) {
                return Err(OpError::rejected(
                    FailureKind::NotFound,
                    format!("branch '{name}' not found"),
                ));
            }
            args.extend(["--".into(), folder.clone(), name.clone()]);
        }
        WorktreeTarget::NewBranch { name, start } => {
            validate_branch_name(git, repo, name)?;
            if crate::ops::ref_exists(git, repo, &format!("refs/heads/{name}")) {
                return Err(OpError::rejected(
                    FailureKind::AlreadyExists,
                    format!("a branch named '{name}' already exists"),
                ));
            }
            let start = match start.as_deref().filter(|s| !s.trim().is_empty()) {
                Some(s) => resolve_commit(git, repo, s)?,
                None => resolve_commit(git, repo, "HEAD")?,
            };
            args.extend([
                "-b".into(),
                name.clone(),
                "--".into(),
                folder.clone(),
                start,
            ]);
        }
        WorktreeTarget::Detached { rev } => {
            let commit = resolve_commit(git, repo, rev)?;
            args.extend(["--detach".into(), "--".into(), folder.clone(), commit]);
        }
    }
    writer(git).run(repo, ctx, &args).map_err(classify)?;
    find(git, repo, &folder)
}

/// `git worktree remove [--force] -- <path>`. Changes inside are refused with
/// [`FailureKind::DirtyWorktree`] (retry with `force`), a locked worktree with
/// [`FailureKind::WorktreeLocked`].
pub fn remove(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    path: &str,
    force: bool,
) -> Result<(), OpError> {
    let w = find(git, repo, path)?;
    if w.is_main {
        return Err(OpError::rejected(
            FailureKind::InvalidInput,
            "the main worktree cannot be removed",
        ));
    }
    if w.is_current {
        return Err(OpError::rejected(
            FailureKind::CurrentBranch,
            "cannot remove the worktree that is open; open the main repository first",
        ));
    }
    if w.locked {
        return Err(OpError::rejected(
            FailureKind::WorktreeLocked,
            format!(
                "cannot remove a locked working tree{}",
                w.lock_reason
                    .as_deref()
                    .map(|r| format!(", lock reason: {r}"))
                    .unwrap_or_default()
            ),
        ));
    }
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.extend(["--", &w.path]);
    writer(git).run(repo, ctx, &args).map_err(classify)?;
    Ok(())
}

/// `git worktree prune [-n] -v`: the worktree entries whose folder is gone (with `dry_run`
/// only reported). Returns git's lines, e.g. `Removing worktrees/x: gitdir file points to
/// non-existent location`.
pub fn prune(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    dry_run: bool,
) -> Result<Vec<String>, OpError> {
    let mut args = vec!["worktree", "prune", "-v"];
    if dry_run {
        args.push("-n");
    }
    let out = writer(git).run(repo, ctx, &args).map_err(classify)?;
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect())
}

/// `git worktree lock [--reason <r>]` / `unlock`.
pub fn set_locked(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    path: &str,
    locked: bool,
    reason: Option<&str>,
) -> Result<(), OpError> {
    let w = find(git, repo, path)?;
    if w.is_main {
        return Err(OpError::rejected(
            FailureKind::InvalidInput,
            "the main worktree cannot be locked",
        ));
    }
    if w.locked == locked {
        return Ok(());
    }
    let mut args: Vec<String> = vec![
        "worktree".into(),
        if locked { "lock" } else { "unlock" }.into(),
    ];
    if locked && let Some(r) = reason.map(str::trim).filter(|r| !r.is_empty()) {
        if r.chars().any(char::is_control) {
            return Err(OpError::rejected(
                FailureKind::InvalidInput,
                "the lock reason must be a single line",
            ));
        }
        // `--reason` always consumes the next argument.
        args.extend(["--reason".into(), r.to_owned()]);
    }
    args.extend(["--".into(), w.path.clone()]);
    writer(git).run(repo, ctx, &args).map_err(classify)?;
    Ok(())
}

/// Folder of the main worktree (where the shared `.git` lives).
pub fn main_path(git: &GitRunner, repo: &Path) -> Result<PathBuf, GitError> {
    let list = list(git, repo)?;
    Ok(list
        .first()
        .map(|w| PathBuf::from(&w.path))
        .unwrap_or_else(|| repo.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::test_support::TestRepo;

    #[test]
    fn parses_porcelain_list() {
        let raw = b"worktree C:/r\0HEAD aaaa\0branch refs/heads/main\0\0worktree C:/r-wt b\0HEAD bbbb\0branch refs/heads/feature/x\0locked usb drive\0\0worktree C:/gone\0HEAD cccc\0detached\0prunable gitdir file points to non-existent location\0\0worktree C:/l2\0HEAD dddd\0detached\0locked\0\0";
        let w = parse_worktree_list(raw, "c:\\r-wt b\\");
        assert_eq!(w.len(), 4);
        assert!(w[0].is_main && !w[0].is_current);
        assert_eq!(w[0].branch.as_deref(), Some("main"));
        assert_eq!(w[1].branch.as_deref(), Some("feature/x"));
        assert!(w[1].locked && w[1].is_current == cfg!(windows));
        assert_eq!(w[1].lock_reason.as_deref(), Some("usb drive"));
        assert!(w[2].detached && w[2].prunable && w[2].branch.is_none());
        assert!(w[3].locked && w[3].lock_reason.is_none());
        assert!(parse_worktree_list(b"", "x").is_empty());
    }

    #[test]
    fn add_lock_remove_prune_and_checked_out_elsewhere() {
        let r = TestRepo::new();
        let base = r.path.parent().unwrap().to_owned();
        r.git(&["branch", "topic"]);

        let wt = base.join("wt topic");
        let w = add(
            &r.git,
            &r.path,
            &r.ctx,
            &wt,
            &WorktreeTarget::Branch {
                name: "topic".into(),
            },
        )
        .unwrap();
        assert_eq!(w.branch.as_deref(), Some("topic"));
        assert!(!w.is_main && !w.is_current);
        assert!(wt.join("README.md").exists());

        // The branch is in use there: checkout here is refused with a clear kind.
        let err = crate::branch::checkout_branch(&r.git, &r.path, &r.ctx, "topic").unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::CheckedOutElsewhere), "{err}");
        // …and a second worktree on it too.
        let err = add(
            &r.git,
            &r.path,
            &r.ctx,
            &base.join("wt2"),
            &WorktreeTarget::Branch {
                name: "topic".into(),
            },
        )
        .unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::CheckedOutElsewhere), "{err}");

        // New branch + detached.
        let nb = add(
            &r.git,
            &r.path,
            &r.ctx,
            &base.join("wt-new"),
            &WorktreeTarget::NewBranch {
                name: "feature/new".into(),
                start: Some("main".into()),
            },
        )
        .unwrap();
        assert_eq!(nb.branch.as_deref(), Some("feature/new"));
        let det = add(
            &r.git,
            &r.path,
            &r.ctx,
            &base.join("wt-det"),
            &WorktreeTarget::Detached { rev: r.head() },
        )
        .unwrap();
        assert!(det.detached);
        for bad in [
            WorktreeTarget::Branch {
                name: "--force".into(),
            },
            WorktreeTarget::NewBranch {
                name: "topic".into(),
                start: None,
            },
        ] {
            assert!(add(&r.git, &r.path, &r.ctx, &base.join("wt-bad"), &bad).is_err());
        }
        let err = add(
            &r.git,
            &r.path,
            &r.ctx,
            Path::new("relative/x"),
            &WorktreeTarget::Detached { rev: r.head() },
        )
        .unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::InvalidInput));
        let err = add(
            &r.git,
            &r.path,
            &r.ctx,
            &r.path,
            &WorktreeTarget::Detached { rev: r.head() },
        )
        .unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::AlreadyExists));

        // Lock: removal refused; unlock.
        let topic = wt.to_string_lossy().into_owned();
        set_locked(&r.git, &r.path, &r.ctx, &topic, true, Some("on usb")).unwrap();
        let w = find(&r.git, &r.path, &topic).unwrap();
        assert!(w.locked && w.lock_reason.as_deref() == Some("on usb"));
        let err = remove(&r.git, &r.path, &r.ctx, &topic, true).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::WorktreeLocked));
        set_locked(&r.git, &r.path, &r.ctx, &topic, false, None).unwrap();

        // Changes inside: needs force.
        std::fs::write(wt.join("new.txt"), "x").unwrap();
        let err = remove(&r.git, &r.path, &r.ctx, &topic, false).unwrap_err();
        assert_eq!(err.kind(), Some(FailureKind::DirtyWorktree), "{err}");
        remove(&r.git, &r.path, &r.ctx, &topic, true).unwrap();
        assert!(!wt.exists());
        // Now the branch can be checked out here.
        crate::branch::checkout_branch(&r.git, &r.path, &r.ctx, "topic").unwrap();

        // Main worktree is protected; unknown paths are rejected.
        let main = list(&r.git, &r.path).unwrap()[0].path.clone();
        assert_eq!(
            remove(&r.git, &r.path, &r.ctx, &main, true)
                .unwrap_err()
                .kind(),
            Some(FailureKind::InvalidInput)
        );
        assert_eq!(
            remove(&r.git, &r.path, &r.ctx, "C:/nowhere", true)
                .unwrap_err()
                .kind(),
            Some(FailureKind::NotFound)
        );

        // A deleted folder becomes prunable.
        std::fs::remove_dir_all(base.join("wt-det")).unwrap();
        let w = list(&r.git, &r.path).unwrap();
        assert!(w.iter().any(|w| w.prunable));
        let lines = prune(&r.git, &r.path, &r.ctx, true).unwrap();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(list(&r.git, &r.path).unwrap().iter().any(|w| w.prunable));
        prune(&r.git, &r.path, &r.ctx, false).unwrap();
        assert!(!list(&r.git, &r.path).unwrap().iter().any(|w| w.prunable));

        // Listing from a linked worktree marks it current and keeps main first.
        let from_new = list(&r.git, &base.join("wt-new")).unwrap();
        assert!(from_new[0].is_main && !from_new[0].is_current);
        assert!(
            from_new
                .iter()
                .any(|w| w.is_current && w.branch.as_deref() == Some("feature/new"))
        );
        assert!(same_path(
            &main_path(&r.git, &base.join("wt-new"))
                .unwrap()
                .to_string_lossy(),
            &main
        ));
    }
}
