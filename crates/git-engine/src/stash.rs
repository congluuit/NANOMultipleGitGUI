//! Stash: create, apply / pop / drop by index and show the files a stash entry holds.
//!
//! Listing stashes lives in [`crate::refs`] (`RepoRefs::stashes`). Entries are addressed by
//! their reflog index (`stash@{n}`); the index is a `u32`, so user input can never become a
//! git option or an arbitrary revision.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::commit::{ChangedFile, parse_name_status, parse_numstat};
use crate::{AccountContext, GitError, GitRunner};

/// Options for [`stash_create`]. Every field is optional on the wire.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", default)]
pub struct StashOptions {
    /// Stash message; git writes `WIP on <branch>: <commit>` when empty.
    #[cfg_attr(feature = "specta", specta(optional))]
    pub message: Option<String>,
    /// Also stash untracked (not ignored) files (`--include-untracked`).
    #[cfg_attr(feature = "specta", specta(optional))]
    pub include_untracked: bool,
    /// Leave staged changes in the index and working tree (`--keep-index`).
    #[cfg_attr(feature = "specta", specta(optional))]
    pub keep_index: bool,
    /// Stash only what is staged (`--staged`, git ≥ 2.35). Cannot be combined with
    /// `include_untracked` or `keep_index`.
    #[cfg_attr(feature = "specta", specta(optional))]
    pub staged_only: bool,
    /// Limit the stash to these repository-relative paths (matched literally). Empty = all.
    #[cfg_attr(feature = "specta", specta(optional))]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct StashCreated {
    /// `false` when there was nothing to stash (git exits 0 and prints
    /// "No local changes to save"); `oid` is then `None`.
    pub created: bool,
    /// Commit id of the new `stash@{0}`.
    pub oid: Option<String>,
}

/// Result of [`stash_apply`] / [`stash_pop`] when git got as far as merging the stash.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct StashApplyOutcome {
    /// Unmerged paths left in the working tree. Empty = applied cleanly.
    pub conflicts: Vec<String>,
    /// The entry was removed from the stash list (pop without conflicts). On conflicts git
    /// keeps the entry, so the user can retry or drop it after resolving.
    pub dropped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct StashDetails {
    pub index: u32,
    /// Commit id of the stash entry.
    pub oid: String,
    /// Commit the stash was created on (first parent).
    pub base: String,
    /// `On main: message` or `WIP on main: <commit> <subject>`.
    pub message: String,
    /// Seconds since the Unix epoch.
    pub time: i64,
    /// Tracked changes (staged and unstaged together) against `base`.
    pub files: Vec<ChangedFile>,
    /// Untracked files stored in the third parent (`--include-untracked`), all `Added`.
    pub untracked: Vec<ChangedFile>,
    pub additions: u32,
    pub deletions: u32,
}

/// `stash@{n}`.
pub fn stash_ref(index: u32) -> String {
    format!("stash@{{{index}}}")
}

/// Accepts `3` or `stash@{3}`; anything else (`stash@{-1}`, `stash@{now}`, `--x`) is `None`.
pub fn parse_stash_index(s: &str) -> Option<u32> {
    let s = s.trim();
    let n = s
        .strip_prefix("stash@{")
        .and_then(|r| r.strip_suffix('}'))
        .unwrap_or(s);
    if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    n.parse().ok()
}

fn rejected(args: &str, message: String) -> GitError {
    GitError::Failed {
        args: args.into(),
        status: "rejected".into(),
        stderr: message,
    }
}

/// Resolves `stash@{n}` to a commit id, with a clear error when the entry does not exist.
pub fn resolve_stash(git: &GitRunner, repo: &Path, index: u32) -> Result<String, GitError> {
    let r = stash_ref(index);
    git.run_text(
        repo,
        &AccountContext::default(),
        ["rev-parse", "-q", "--verify", &format!("{r}^{{commit}}")],
    )
    .ok()
    .filter(|s| !s.is_empty())
    .ok_or_else(|| rejected("stash", format!("no stash entry {r}")))
}

fn stash_tip(git: &GitRunner, repo: &Path) -> Option<String> {
    git.run_text(
        repo,
        &AccountContext::default(),
        ["rev-parse", "-q", "--verify", "refs/stash"],
    )
    .ok()
    .filter(|s| !s.is_empty())
}

/// Builds the `git stash push` arguments, validating option combinations.
pub fn push_args(opts: &StashOptions) -> Result<Vec<String>, GitError> {
    if opts.staged_only && (opts.include_untracked || opts.keep_index) {
        return Err(rejected(
            "stash push",
            "a staged-only stash cannot include untracked files or keep the index".into(),
        ));
    }
    if opts.paths.iter().any(|p| p.is_empty() || p.contains('\0')) {
        return Err(rejected("stash push", "invalid path".into()));
    }
    let mut args: Vec<String> = vec!["stash".into(), "push".into()];
    if let Some(m) = opts
        .message
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
    {
        // `--message=` keeps a message starting with `-` from being read as an option.
        args.push(format!("--message={m}"));
    }
    if opts.include_untracked {
        args.push("--include-untracked".into());
    }
    if opts.keep_index {
        args.push("--keep-index".into());
    }
    if opts.staged_only {
        args.push("--staged".into());
    }
    args.push("--".into());
    // Paths are literal: no `:(glob)` magic, no wildcard expansion. Per-path magic rather than
    // `--literal-pathspecs`, which would also break the `:/` pathspec stash uses internally.
    args.extend(opts.paths.iter().map(|p| format!(":(literal){p}")));
    Ok(args)
}

/// `git stash push`. "Nothing to stash" is detected by `refs/stash` not moving, which does
/// not depend on git's (translated) output.
pub fn stash_create(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    opts: &StashOptions,
) -> Result<StashCreated, GitError> {
    let args = push_args(opts)?;
    let before = stash_tip(git, repo);
    git.run(repo, ctx, &args)?;
    let after = stash_tip(git, repo);
    Ok(if after.is_some() && after != before {
        StashCreated {
            created: true,
            oid: after,
        }
    } else {
        StashCreated {
            created: false,
            oid: None,
        }
    })
}

/// Paths with unmerged index entries, i.e. conflicts left by a failed merge of the stash.
pub fn unmerged_paths(git: &GitRunner, repo: &Path) -> Result<Vec<String>, GitError> {
    let out = git.run(
        repo,
        &AccountContext::default(),
        ["diff", "--name-only", "-z", "--diff-filter=U"],
    )?;
    let mut paths: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    paths.dedup();
    Ok(paths)
}

fn apply_or_pop(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    index: u32,
    restore_index: bool,
    pop: bool,
) -> Result<StashApplyOutcome, GitError> {
    resolve_stash(git, repo, index)?;
    let r = stash_ref(index);
    let mut args = vec!["stash", if pop { "pop" } else { "apply" }];
    if restore_index {
        args.push("--index");
    }
    args.push(&r);
    match git.run(repo, ctx, &args) {
        Ok(_) => Ok(StashApplyOutcome {
            conflicts: Vec::new(),
            dropped: pop,
        }),
        Err(err @ GitError::Failed { .. }) => {
            // git refuses to start with unmerged entries in the index, so any found now come
            // from merging this stash. Other failures (local changes would be overwritten,
            // untracked file in the way, `--index` conflicts) leave nothing applied.
            let conflicts = unmerged_paths(git, repo).unwrap_or_default();
            if conflicts.is_empty() {
                Err(err)
            } else {
                Ok(StashApplyOutcome {
                    conflicts,
                    dropped: false,
                })
            }
        }
        Err(e) => Err(e),
    }
}

/// `git stash apply [--index] stash@{n}`; the entry stays in the list.
pub fn stash_apply(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    index: u32,
    restore_index: bool,
) -> Result<StashApplyOutcome, GitError> {
    apply_or_pop(git, repo, ctx, index, restore_index, false)
}

/// `git stash pop [--index] stash@{n}`; the entry is dropped only when it applied cleanly.
pub fn stash_pop(
    git: &GitRunner,
    repo: &Path,
    ctx: &AccountContext,
    index: u32,
    restore_index: bool,
) -> Result<StashApplyOutcome, GitError> {
    apply_or_pop(git, repo, ctx, index, restore_index, true)
}

/// `git stash drop stash@{n}`. Returns the commit id of the dropped entry (recoverable with
/// `git stash apply <oid>` until garbage-collected).
pub fn stash_drop(git: &GitRunner, repo: &Path, index: u32) -> Result<String, GitError> {
    let oid = resolve_stash(git, repo, index)?;
    git.run(
        repo,
        &AccountContext::default(),
        ["stash", "drop", "-q", &stash_ref(index)],
    )?;
    Ok(oid)
}

fn diff_files(git: &GitRunner, repo: &Path, range: &[&str]) -> Result<Vec<ChangedFile>, GitError> {
    let ctx = AccountContext::default();
    let diff = |kind: &str| -> Result<String, GitError> {
        let mut args = vec!["diff-tree", "-r", "-M", "-z", "--no-commit-id", kind];
        args.extend_from_slice(range);
        let out = git.run(repo, &ctx, &args)?;
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let stats = parse_numstat(&diff("--numstat")?);
    Ok(parse_name_status(&diff("--name-status")?)
        .into_iter()
        .map(|(status, path, old_path)| {
            let (additions, deletions) = stats.get(&path).copied().unwrap_or((None, None));
            ChangedFile {
                status,
                path,
                old_path,
                additions,
                deletions,
            }
        })
        .collect())
}

/// Files changed by `stash@{n}`: tracked changes against the base commit plus the untracked
/// files kept in the third parent (`git show --name-status stash@{n}^3`).
pub fn stash_show(git: &GitRunner, repo: &Path, index: u32) -> Result<StashDetails, GitError> {
    let oid = resolve_stash(git, repo, index)?;
    let ctx = AccountContext::default();
    let meta = git.run_text(
        repo,
        &ctx,
        [
            "-c",
            "log.showSignature=false",
            "show",
            "-s",
            "--format=%P%x00%ct%x00%s",
            &oid,
            "--",
        ],
    )?;
    let mut f = meta.splitn(3, '\0');
    let parents: Vec<&str> = f.next().unwrap_or("").split_whitespace().collect();
    let time = f.next().and_then(|t| t.trim().parse().ok()).unwrap_or(0);
    let message = f.next().unwrap_or("").trim().to_owned();
    let Some(&base) = parents.first() else {
        return Err(rejected(
            "stash show",
            format!("{oid} is not a stash commit"),
        ));
    };

    let files = diff_files(git, repo, &[base, &oid])?;
    let untracked = match parents.get(2) {
        Some(&u) => diff_files(git, repo, &["--root", u])?,
        None => Vec::new(),
    };
    let (mut additions, mut deletions) = (0, 0);
    for file in files.iter().chain(&untracked) {
        additions += file.additions.unwrap_or(0);
        deletions += file.deletions.unwrap_or(0);
    }
    Ok(StashDetails {
        index,
        base: base.to_owned(),
        oid,
        message,
        time,
        files,
        untracked,
        additions,
        deletions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Identity;
    use crate::commit::FileStatus;
    use crate::refs::read_refs;

    struct Repo {
        _dir: tempfile::TempDir,
        path: std::path::PathBuf,
        git: GitRunner,
        ctx: AccountContext,
    }

    impl Repo {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().to_path_buf();
            let repo = Self {
                _dir: dir,
                path,
                git: GitRunner::default(),
                ctx: AccountContext {
                    identity: Some(Identity {
                        name: "Stasher".into(),
                        email: "s@x.dev".into(),
                    }),
                    ..Default::default()
                },
            };
            repo.run(&["init", "-q", "-b", "main"]);
            // Byte-exact content regardless of the user's global autocrlf.
            repo.run(&["config", "core.autocrlf", "false"]);
            repo
        }
        fn run(&self, args: &[&str]) -> String {
            self.git.run_text(&self.path, &self.ctx, args).unwrap()
        }
        fn write(&self, name: &str, content: &str) {
            let p = self.path.join(name);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(p, content).unwrap();
        }
        fn read(&self, name: &str) -> String {
            std::fs::read_to_string(self.path.join(name)).unwrap()
        }
        fn stashes(&self) -> usize {
            read_refs(&self.git, &self.path).unwrap().stashes.len()
        }
        fn create(&self, opts: &StashOptions) -> Result<StashCreated, GitError> {
            stash_create(&self.git, &self.path, &self.ctx, opts)
        }
    }

    #[test]
    fn index_parsing_and_formatting() {
        assert_eq!(stash_ref(2), "stash@{2}");
        assert_eq!(parse_stash_index("0"), Some(0));
        assert_eq!(parse_stash_index("stash@{12}"), Some(12));
        assert_eq!(parse_stash_index(" stash@{3} "), Some(3));
        assert_eq!(parse_stash_index("stash@{-1}"), None);
        assert_eq!(parse_stash_index("stash@{now}"), None);
        assert_eq!(parse_stash_index("--output=x"), None);
        assert_eq!(parse_stash_index("+1"), None);
        assert_eq!(parse_stash_index(""), None);
        assert_eq!(parse_stash_index("99999999999"), None);
    }

    #[test]
    fn push_arguments_and_validation() {
        let args = push_args(&StashOptions {
            message: Some("--not-an-option".into()),
            include_untracked: true,
            paths: vec!["a b.txt".into()],
            ..Default::default()
        })
        .unwrap();
        assert_eq!(
            args,
            [
                "stash",
                "push",
                "--message=--not-an-option",
                "--include-untracked",
                "--",
                ":(literal)a b.txt"
            ]
        );
        let blank = push_args(&StashOptions {
            message: Some("  ".into()),
            ..Default::default()
        })
        .unwrap();
        assert!(!blank.iter().any(|a| a.starts_with("--message")));
        assert!(
            push_args(&StashOptions {
                staged_only: true,
                include_untracked: true,
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            push_args(&StashOptions {
                paths: vec![String::new()],
                ..Default::default()
            })
            .is_err()
        );
    }

    #[test]
    fn create_show_apply_pop_and_drop() {
        let r = Repo::new();
        r.write("a.txt", "1\n2\n");
        r.write("keep.txt", "k\n");
        r.run(&["add", "."]);
        r.run(&["commit", "-q", "-m", "base"]);

        // Clean tree: nothing to stash, recognisably.
        let none = r.create(&StashOptions::default()).unwrap();
        assert_eq!(
            none,
            StashCreated {
                created: false,
                oid: None
            }
        );
        assert_eq!(r.stashes(), 0);

        // Tracked edit + new untracked file, with untracked included.
        r.write("a.txt", "1\nhai\n3\n");
        r.write("dir/new file.txt", "x\ny\n");
        let made = r
            .create(&StashOptions {
                message: Some("work in progress".into()),
                include_untracked: true,
                ..Default::default()
            })
            .unwrap();
        assert!(made.created);
        assert_eq!(r.read("a.txt"), "1\n2\n", "tree reset");
        assert!(!r.path.join("dir/new file.txt").exists());
        assert_eq!(r.stashes(), 1);

        let d = stash_show(&r.git, &r.path, 0).unwrap();
        assert_eq!(Some(d.oid.clone()), made.oid);
        assert!(d.message.contains("work in progress"), "{}", d.message);
        assert_eq!(d.files.len(), 1);
        assert_eq!(d.files[0].path, "a.txt");
        assert_eq!(d.files[0].status, FileStatus::Modified);
        assert_eq!(
            (d.files[0].additions, d.files[0].deletions),
            (Some(2), Some(1))
        );
        assert_eq!(d.untracked.len(), 1);
        assert_eq!(d.untracked[0].path, "dir/new file.txt");
        assert_eq!(d.untracked[0].status, FileStatus::Added);
        assert_eq!((d.additions, d.deletions), (4, 1));

        // Second stash without untracked: no third parent.
        r.write("keep.txt", "changed\n");
        r.create(&StashOptions::default()).unwrap();
        assert_eq!(r.stashes(), 2);
        let d = stash_show(&r.git, &r.path, 0).unwrap();
        assert!(d.untracked.is_empty());
        assert_eq!(d.files[0].path, "keep.txt");

        // Apply the older one by index: entry stays.
        let out = stash_apply(&r.git, &r.path, &r.ctx, 1, false).unwrap();
        assert_eq!(out, StashApplyOutcome::default());
        assert_eq!(r.read("a.txt"), "1\nhai\n3\n");
        assert_eq!(r.read("dir/new file.txt"), "x\ny\n");
        assert_eq!(r.stashes(), 2);
        r.run(&["reset", "-q", "--hard"]);
        r.run(&["clean", "-qfd"]);

        // Pop stash@{0}: applied and dropped.
        let out = stash_pop(&r.git, &r.path, &r.ctx, 0, false).unwrap();
        assert!(out.dropped && out.conflicts.is_empty());
        assert_eq!(r.read("keep.txt"), "changed\n");
        assert_eq!(r.stashes(), 1);

        // Drop the remaining one; then index 0 no longer exists.
        let dropped = stash_drop(&r.git, &r.path, 0).unwrap();
        assert_eq!(Some(dropped), made.oid);
        assert_eq!(r.stashes(), 0);
        let err = stash_drop(&r.git, &r.path, 0).unwrap_err();
        assert!(
            err.to_string().contains("no stash entry stash@{0}"),
            "{err}"
        );
        assert!(stash_show(&r.git, &r.path, 3).is_err());
        assert!(stash_apply(&r.git, &r.path, &r.ctx, 7, false).is_err());
    }

    #[test]
    fn keep_index_staged_only_and_paths() {
        let r = Repo::new();
        r.write("a.txt", "a\n");
        r.write("b.txt", "b\n");
        r.run(&["add", "."]);
        r.run(&["commit", "-q", "-m", "base"]);

        // --staged: only the staged file is stashed.
        r.write("a.txt", "a2\n");
        r.write("b.txt", "b2\n");
        r.run(&["add", "a.txt"]);
        let made = r
            .create(&StashOptions {
                staged_only: true,
                ..Default::default()
            })
            .unwrap();
        assert!(made.created);
        assert_eq!(r.read("a.txt"), "a\n");
        assert_eq!(r.read("b.txt"), "b2\n", "unstaged change untouched");
        let d = stash_show(&r.git, &r.path, 0).unwrap();
        assert_eq!(d.files.len(), 1);
        assert_eq!(d.files[0].path, "a.txt");

        // --index restores the staged state.
        stash_pop(&r.git, &r.path, &r.ctx, 0, true).unwrap();
        let staged = r.run(&["diff", "--cached", "--name-only"]);
        assert_eq!(staged, "a.txt");

        // --keep-index: staged a.txt stays, both are in the stash.
        r.create(&StashOptions {
            keep_index: true,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(r.read("a.txt"), "a2\n");
        assert_eq!(r.read("b.txt"), "b\n");
        assert_eq!(stash_show(&r.git, &r.path, 0).unwrap().files.len(), 2);
        r.run(&["reset", "-q", "--hard"]);

        // Pathspec-limited stash, literal paths ("*.txt" is not a glob).
        r.write("a.txt", "a3\n");
        r.write("b.txt", "b3\n");
        r.create(&StashOptions {
            paths: vec!["b.txt".into()],
            ..Default::default()
        })
        .unwrap();
        assert_eq!(r.read("a.txt"), "a3\n");
        assert_eq!(r.read("b.txt"), "b\n");
        assert!(
            r.create(&StashOptions {
                paths: vec!["*.txt".into()],
                ..Default::default()
            })
            .is_err(),
            "no file literally named *.txt"
        );
    }

    #[test]
    fn conflicts_are_reported_and_pop_keeps_the_entry() {
        let r = Repo::new();
        r.write("a.txt", "base\n");
        r.run(&["add", "."]);
        r.run(&["commit", "-q", "-m", "base"]);
        r.write("a.txt", "mine\n");
        r.create(&StashOptions::default()).unwrap();
        r.write("a.txt", "theirs\n");
        r.run(&["commit", "-q", "-am", "theirs"]);

        let out = stash_pop(&r.git, &r.path, &r.ctx, 0, false).unwrap();
        assert_eq!(out.conflicts, ["a.txt"]);
        assert!(!out.dropped);
        assert_eq!(r.stashes(), 1, "git keeps the entry on conflicts");
        assert!(r.read("a.txt").contains("<<<<<<<"));

        // Local changes in the way: a plain failure, nothing applied.
        r.run(&["reset", "-q", "--hard"]);
        r.write("a.txt", "dirty\n");
        let err = stash_apply(&r.git, &r.path, &r.ctx, 0, false).unwrap_err();
        assert!(matches!(err, GitError::Failed { .. }));
        assert_eq!(r.read("a.txt"), "dirty\n");
    }
}
