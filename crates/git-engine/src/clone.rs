//! `git clone` with an account context, progress, cancellation and cleanup (PLAN §4.7, §6.5).
//!
//! - The URL is validated (no option-like `-x`, no `<helper>::` remote-helper syntax, no
//!   control characters) and passed after `--end-of-options`.
//! - The destination must not exist or be an empty directory. Folders this module creates are
//!   removed again when the clone fails or is cancelled; a pre-existing empty folder is emptied
//!   but kept. Nothing outside the destination is ever touched.
//! - LFS files: [`LfsMode::All`] downloads them during checkout (git-lfs smudge filter),
//!   [`LfsMode::Later`] clones with `GIT_LFS_SKIP_SMUDGE=1` (pointer files; the caller starts
//!   `git lfs pull` in the background), [`LfsMode::Partial`] clones with pointers, stores
//!   `lfs.fetchinclude` / `lfs.fetchexclude` in the new repository and runs `git lfs pull`,
//!   which then only downloads the matching files (later pulls keep honouring the patterns).
//! - When git-lfs is installed but its filter is not configured globally, the filter is passed
//!   for this clone and `git lfs install --local` is run in the new repository, so LFS files
//!   are never silently left as pointers.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::ops::{FailureKind, OpError, check_arg, writer};
use crate::progress::{CancelToken, Progress, run_streaming_with};
use crate::{AccountContext, GitRunner};

/// How LFS files are downloaded by a clone.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum LfsMode {
    /// Download every LFS file during checkout (git-lfs default, with progress).
    #[default]
    All,
    /// Clone pointer files only (`GIT_LFS_SKIP_SMUDGE=1`); run `git lfs pull` afterwards.
    Later,
    /// Only download files matching `include` (and not `exclude`): comma-separated
    /// `lfs.fetchinclude` / `lfs.fetchexclude` patterns, e.g. `Assets/UI/**`.
    Partial {
        include: String,
        #[cfg_attr(feature = "specta", specta(optional))]
        #[serde(default)]
        exclude: Option<String>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CloneOptions {
    /// `https://…`, `ssh://…`, `git@host:owner/repo.git`, `file://…` or a local path.
    pub url: String,
    /// Folder to clone into; must not exist or be empty.
    pub dest: PathBuf,
    /// Branch (or tag) to check out instead of the remote HEAD.
    pub branch: Option<String>,
    /// Shallow clone with this many commits (`--depth`); `None` = full history.
    pub depth: Option<u32>,
    pub recurse_submodules: bool,
    pub lfs: LfsMode,
}

/// Result of a successful clone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloneOutcome {
    /// The cloned working tree (= `dest`).
    pub path: PathBuf,
    /// LFS files were left as pointers ([`LfsMode::Later`]); the caller should pull them.
    pub lfs_pending: bool,
    /// The repository has LFS attributes (`filter=lfs` in the top-level `.gitattributes`).
    pub uses_lfs: bool,
    /// The partial LFS download failed after the clone itself succeeded. The repository is
    /// kept (the download can be retried from the LFS panel).
    pub lfs_error: Option<String>,
}

fn invalid(what: &str, value: &str) -> OpError {
    OpError::rejected(
        FailureKind::InvalidInput,
        format!("invalid {what}: {value:?}"),
    )
}

/// Accepts URLs and local paths git can clone from; rejects option-like values, remote-helper
/// syntax (`ext::sh -c …`, `fd::…`) and control characters.
pub fn validate_url(url: &str) -> Result<(), OpError> {
    let trimmed = url.trim();
    if trimmed.is_empty()
        || trimmed != url
        || url.starts_with('-')
        || url.chars().any(char::is_control)
    {
        return Err(invalid("repository URL", url));
    }
    // `<transport>::<address>` runs the `git-remote-<transport>` helper.
    if let Some((helper, _)) = url.split_once("::")
        && !helper.is_empty()
        && helper
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+'))
    {
        return Err(invalid("repository URL", url));
    }
    Ok(())
}

/// State of a clone destination that may be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destination {
    /// Does not exist yet: it (and missing parents) will be created.
    Missing,
    /// An existing empty directory.
    EmptyDir,
}

/// Checks that `dest` does not exist or is an empty directory.
pub fn check_destination(dest: &Path) -> Result<Destination, OpError> {
    if dest.as_os_str().is_empty() || !dest.is_absolute() {
        return Err(invalid("destination", &dest.to_string_lossy()));
    }
    match std::fs::symlink_metadata(dest) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Destination::Missing),
        Err(e) => Err(OpError::Git(e.into())),
        Ok(meta) if meta.is_dir() => {
            let mut entries = std::fs::read_dir(dest).map_err(|e| OpError::Git(e.into()))?;
            if entries.next().is_none() {
                Ok(Destination::EmptyDir)
            } else {
                Err(OpError::rejected(
                    FailureKind::AlreadyExists,
                    format!("destination is not empty: {}", dest.display()),
                ))
            }
        }
        Ok(_) => Err(OpError::rejected(
            FailureKind::AlreadyExists,
            format!("destination exists and is not a folder: {}", dest.display()),
        )),
    }
}

/// Folders created for a clone, removed again unless [`Self::keep`] is called.
struct CreatedDest {
    dest: PathBuf,
    /// Topmost folder that did not exist before (removed entirely); `None` when `dest` existed
    /// (then only its contents are removed).
    created_root: Option<PathBuf>,
    keep: bool,
}

impl CreatedDest {
    fn prepare(dest: &Path) -> Result<Self, OpError> {
        let existed = check_destination(dest)? == Destination::EmptyDir;
        let created_root = if existed {
            None
        } else {
            let mut root = dest.to_owned();
            while let Some(parent) = root.parent() {
                if parent.as_os_str().is_empty() || parent.exists() {
                    break;
                }
                root = parent.to_owned();
            }
            Some(root)
        };
        let guard = Self {
            dest: dest.to_owned(),
            created_root,
            keep: false,
        };
        std::fs::create_dir_all(dest).map_err(|e| OpError::Git(e.into()))?;
        Ok(guard)
    }

    fn keep(mut self) {
        self.keep = true;
    }
}

/// `remove_dir_all` with retries: right after a killed git, Windows may still hold handles
/// (antivirus, the dying process) for a moment.
fn remove_with_retry(f: impl Fn() -> std::io::Result<()>) -> std::io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match f() {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) if Instant::now() >= deadline => return Err(e),
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

/// Makes files writable before deleting them (git marks pack files read-only, which
/// `remove_dir_all` cannot delete on Windows).
fn make_writable(path: &Path) {
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&p) else {
            continue;
        };
        if meta.is_dir() {
            make_writable(&p);
        } else {
            let mut perms = meta.permissions();
            if perms.readonly() {
                #[allow(clippy::permissions_set_readonly_false)]
                perms.set_readonly(false);
                let _ = std::fs::set_permissions(&p, perms);
            }
        }
    }
}

fn remove_contents(dir: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let p = entry?.path();
        if std::fs::symlink_metadata(&p)?.is_dir() {
            remove_with_retry(|| std::fs::remove_dir_all(&p))?;
        } else {
            remove_with_retry(|| std::fs::remove_file(&p))?;
        }
    }
    Ok(())
}

impl Drop for CreatedDest {
    fn drop(&mut self) {
        if self.keep {
            return;
        }
        make_writable(&self.dest);
        let result = match &self.created_root {
            Some(root) => remove_with_retry(|| std::fs::remove_dir_all(root)),
            None => remove_contents(&self.dest),
        };
        if let Err(e) = result {
            // Nothing else to do: the caller already reports the clone failure.
            eprintln!(
                "could not clean up {} after a failed clone: {e}",
                self.dest.display()
            );
        }
    }
}

/// Whether git-lfs is installed.
fn lfs_installed(git: &GitRunner, cwd: &Path) -> bool {
    git.run(cwd, &AccountContext::default(), ["lfs", "version"])
        .is_ok()
}

/// `-c filter.lfs.*` for a clone when git-lfs is installed but its filter is not configured
/// (no `git lfs install` in the global/system config).
fn lfs_filter_args(git: &GitRunner, cwd: &Path) -> Vec<String> {
    let configured = git
        .run_text(
            cwd,
            &AccountContext::default(),
            ["config", "--get", "filter.lfs.process"],
        )
        .is_ok_and(|v| !v.is_empty());
    if configured || !lfs_installed(git, cwd) {
        return Vec::new();
    }
    [
        "filter.lfs.process=git-lfs filter-process",
        "filter.lfs.smudge=git-lfs smudge -- %f",
        "filter.lfs.clean=git-lfs clean -- %f",
        "filter.lfs.required=true",
    ]
    .iter()
    .flat_map(|kv| ["-c".to_owned(), (*kv).to_owned()])
    .collect()
}

/// Comma-separated LFS fetch patterns: no option-like or multi-line values.
fn check_patterns(value: &str, what: &str) -> Result<String, OpError> {
    let v = value.trim();
    if v.is_empty()
        || v.starts_with('-')
        || v.chars().any(char::is_control)
        || v.split(',').any(|p| p.trim().is_empty())
    {
        return Err(invalid(what, value));
    }
    Ok(v.to_owned())
}

/// Whether the top-level `.gitattributes` routes files through git-lfs.
pub fn uses_lfs(repo: &Path) -> bool {
    std::fs::read_to_string(repo.join(".gitattributes"))
        .is_ok_and(|a| a.lines().any(|l| l.contains("filter=lfs")))
}

/// Builds the `git clone` arguments (without the `-c` prefix arguments).
fn clone_args(opts: &CloneOptions) -> Result<Vec<String>, OpError> {
    validate_url(&opts.url)?;
    let mut args = vec!["clone".to_owned(), "--progress".to_owned()];
    if let Some(branch) = &opts.branch {
        check_arg(branch, "branch name")?;
        if branch.chars().any(char::is_whitespace) {
            return Err(invalid("branch name", branch));
        }
        args.extend(["--branch".to_owned(), branch.clone()]);
    }
    if let Some(depth) = opts.depth {
        if depth == 0 {
            return Err(invalid("depth", "0"));
        }
        args.push(format!("--depth={depth}"));
    }
    if opts.recurse_submodules {
        args.push("--recurse-submodules".into());
        if opts.depth.is_some() {
            args.push("--shallow-submodules".into());
        }
    }
    args.extend([
        "--end-of-options".to_owned(),
        opts.url.clone(),
        opts.dest.to_string_lossy().into_owned(),
    ]);
    Ok(args)
}

/// Clones `opts.url` into `opts.dest` as the account of `ctx`, reporting git's progress (and,
/// for [`LfsMode::Partial`], the LFS download) to `on_progress`. On failure or cancel the
/// destination is cleaned up (see the module docs).
pub fn clone<F: FnMut(Progress)>(
    git: &GitRunner,
    ctx: &AccountContext,
    opts: &CloneOptions,
    cancel: &CancelToken,
    mut on_progress: F,
) -> Result<CloneOutcome, OpError> {
    let clone_args = clone_args(opts)?;
    let partial = match &opts.lfs {
        LfsMode::Partial { include, exclude } => Some((
            check_patterns(include, "LFS include pattern")?,
            exclude
                .as_deref()
                .filter(|e| !e.trim().is_empty())
                .map(|e| check_patterns(e, "LFS exclude pattern"))
                .transpose()?,
        )),
        _ => None,
    };
    if cancel.is_cancelled() {
        return Err(OpError::Cancelled);
    }
    let guard = CreatedDest::prepare(&opts.dest)?;
    let cwd = opts.dest.parent().unwrap_or(&opts.dest).to_owned();
    let git = writer(git);

    let filter = lfs_filter_args(&git, &cwd);
    let mut args = filter.clone();
    args.extend(clone_args);
    let mut env = vec![("GIT_LFS_FORCE_PROGRESS".to_owned(), "1".to_owned())];
    let skip_smudge = !matches!(opts.lfs, LfsMode::All);
    if skip_smudge {
        env.push(("GIT_LFS_SKIP_SMUDGE".into(), "1".into()));
    }
    run_streaming_with(
        &git,
        &cwd,
        ctx,
        &args,
        &env,
        cancel,
        &mut on_progress,
        || {},
    )?;

    let dest = opts.dest.clone();
    let uses_lfs = uses_lfs(&dest);
    let none = AccountContext::default();
    if !filter.is_empty() && uses_lfs {
        // Keep LFS working for later checkouts/pushes in this repository.
        if let Err(e) = git.run(&dest, &none, ["lfs", "install", "--local"]) {
            eprintln!("git lfs install --local failed after clone: {e}");
        }
    }
    let mut lfs_error = None;
    if let Some((include, exclude)) = partial {
        git.run(&dest, &none, ["config", "lfs.fetchinclude", &include])?;
        if let Some(exclude) = &exclude {
            git.run(&dest, &none, ["config", "lfs.fetchexclude", exclude])?;
        }
        if uses_lfs {
            let mut pull = filter.clone();
            pull.extend(["lfs".to_owned(), "pull".to_owned()]);
            let env = [("GIT_LFS_FORCE_PROGRESS".to_owned(), "1".to_owned())];
            match run_streaming_with(
                &git,
                &dest,
                ctx,
                &pull,
                &env,
                cancel,
                &mut on_progress,
                || {},
            ) {
                Ok(_) => {}
                Err(OpError::Cancelled) => return Err(OpError::Cancelled),
                Err(e) => lfs_error = Some(e.to_string()),
            }
        }
    }
    guard.keep();
    Ok(CloneOutcome {
        path: dest,
        lfs_pending: uses_lfs && matches!(opts.lfs, LfsMode::Later),
        uses_lfs,
        lfs_error,
    })
}

#[cfg(test)]
mod tests;
