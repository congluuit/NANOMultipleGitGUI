//! One-click fixes for git problems the app recognises in an error (the notice bar offers
//! them): a stale `*.lock` file, a corrupt index, Windows long paths, big pushes and a folder
//! git does not trust (`safe.directory`).

use std::path::{Path, PathBuf};

use nanomgg_git_engine::{AccountContext, GitRunner};
use serde::Deserialize;
use specta::Type;
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};
use crate::repos::repo_root;
use crate::state::AppState;

/// Repository settings the notice bar can turn on for one repository (`git config --local`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum RepoConfigFix {
    /// `core.longpaths true`: Windows paths longer than 260 characters ("Filename too long").
    LongPaths,
    /// `http.postBuffer 500 MB`: big HTTPS pushes that fail with "RPC failed" / HTTP 413.
    PostBuffer,
}

impl RepoConfigFix {
    fn setting(self) -> (&'static str, &'static str) {
        match self {
            Self::LongPaths => ("core.longpaths", "true"),
            Self::PostBuffer => ("http.postBuffer", "524288000"),
        }
    }
}

async fn with_repo<T: Send + 'static>(
    app: AppHandle,
    repo_id: i64,
    f: impl FnOnce(&GitRunner, &Path) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    let repo = repo_root(&app.state::<AppState>(), repo_id)?;
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppState>().git, &repo))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

/// The repository's own git directory and the common one (they differ in a linked worktree).
fn git_dirs(git: &GitRunner, repo: &Path) -> AppResult<Vec<PathBuf>> {
    let out = git.run_text(
        repo,
        &AccountContext::default(),
        [
            "rev-parse",
            "--path-format=absolute",
            "--git-dir",
            "--git-common-dir",
        ],
    )?;
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter_map(|l| std::fs::canonicalize(l).ok())
        .collect())
}

/// Checks that `path` (from git's own error message; relative paths are relative to the
/// repository) is a `*.lock` file inside the repository's git directory and returns it.
/// `None` when the file is already gone.
pub fn lock_file_in(git_dirs: &[PathBuf], root: &Path, path: &str) -> AppResult<Option<PathBuf>> {
    let raw = PathBuf::from(path.trim());
    let lock = if raw.is_absolute() {
        raw
    } else {
        root.join(raw)
    };
    let is_lock = lock
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(".lock") && n.len() > ".lock".len());
    if !is_lock {
        return Err(AppError::InvalidInput(format!(
            "not a git lock file: {}",
            lock.display()
        )));
    }
    let real = match std::fs::canonicalize(&lock) {
        Ok(real) => real,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(AppError::Git(e.to_string())),
    };
    if !git_dirs.iter().any(|dir| real.starts_with(dir)) || !real.is_file() {
        return Err(AppError::InvalidInput(format!(
            "{} is not a lock file of this repository",
            lock.display()
        )));
    }
    Ok(Some(real))
}

/// Deletes a lock file git left behind (`Unable to create '….lock': File exists`). Only a
/// `*.lock` file inside this repository's git directory is accepted; one already gone is fine.
#[tauri::command]
#[specta::specta]
pub async fn repair_remove_lock(app: AppHandle, repo_id: i64, path: String) -> AppResult<()> {
    with_repo(app, repo_id, move |git, repo| {
        let dirs = git_dirs(git, repo)?;
        if let Some(lock) = lock_file_in(&dirs, repo, &path)? {
            std::fs::remove_file(&lock).map_err(|e| AppError::Git(e.to_string()))?;
            tracing::info!(repo = repo_id, lock = %lock.display(), "stale lock file removed");
        }
        Ok(())
    })
    .await
}

/// Rebuilds a corrupt `.git/index` ("index file smaller than expected", "bad signature"):
/// the broken file is kept as `index.broken` and a fresh index is read from HEAD. Files in
/// the working tree are not touched; what was staged shows as unstaged again.
#[tauri::command]
#[specta::specta]
pub async fn repair_rebuild_index(app: AppHandle, repo_id: i64) -> AppResult<()> {
    with_repo(app, repo_id, move |git, repo| {
        let ctx = AccountContext::default();
        let git_dir =
            PathBuf::from(git.run_text(repo, &ctx, ["rev-parse", "--absolute-git-dir"])?);
        if git_dir.join("index.lock").exists() {
            return Err(AppError::Git(
                "index.lock exists: another git process is using this repository".into(),
            ));
        }
        let index = git_dir.join("index");
        if index.exists() {
            std::fs::rename(&index, git_dir.join("index.broken"))
                .map_err(|e| AppError::Git(e.to_string()))?;
        }
        let has_head = git
            .run(repo, &ctx, ["rev-parse", "-q", "--verify", "HEAD"])
            .is_ok();
        if has_head {
            git.run(repo, &ctx, ["reset", "-q", "--mixed"])?;
        }
        tracing::info!(repo = repo_id, "index rebuilt");
        Ok(())
    })
    .await
}

/// Turns on one [`RepoConfigFix`] for this repository only.
#[tauri::command]
#[specta::specta]
pub async fn repair_config(app: AppHandle, repo_id: i64, fix: RepoConfigFix) -> AppResult<()> {
    with_repo(app, repo_id, move |git, repo| {
        let (key, value) = fix.setting();
        git.run(
            repo,
            &AccountContext::default(),
            ["config", "--local", key, value],
        )?;
        tracing::info!(repo = repo_id, key, value, "repository setting fixed");
        Ok(())
    })
    .await
}

/// `safe.directory` entry for a folder, as git compares it (forward slashes on Windows).
pub fn safe_directory_value(path: &Path) -> String {
    let s = path.to_string_lossy();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    if cfg!(windows) {
        s.replace('\\', "/")
    } else {
        s.to_owned()
    }
}

/// Trusts a repository folder owned by another user (`git config --global --add
/// safe.directory <folder>`), after which git — and the app — can open it.
#[tauri::command]
#[specta::specta]
pub async fn repair_trust_folder(app: AppHandle, path: String) -> AppResult<()> {
    let git = app.state::<AppState>().git.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let folder = PathBuf::from(path.trim());
        if !folder.is_absolute() || !folder.is_dir() {
            return Err(AppError::PathNotFound(format!(
                "folder does not exist: {}",
                folder.display()
            )));
        }
        let value = safe_directory_value(&folder);
        let ctx = AccountContext::default();
        let cwd = std::env::temp_dir();
        let existing = git
            .run_text(
                &cwd,
                &ctx,
                ["config", "--global", "--get-all", "safe.directory"],
            )
            .unwrap_or_default();
        if !existing.lines().any(|l| l.trim() == value) {
            git.run(
                &cwd,
                &ctx,
                ["config", "--global", "--add", "safe.directory", &value],
            )?;
            tracing::info!(folder = %value, "folder added to safe.directory");
        }
        Ok(())
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_lock_files_inside_the_git_dir_are_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        let git_dir = root.join(".git");
        std::fs::create_dir_all(git_dir.join("refs/heads")).unwrap();
        std::fs::write(git_dir.join("index.lock"), "").unwrap();
        std::fs::write(git_dir.join("refs/heads/main.lock"), "").unwrap();
        std::fs::write(git_dir.join("config"), "").unwrap();
        std::fs::write(root.join("outside.lock"), "").unwrap();
        let dirs = vec![std::fs::canonicalize(&git_dir).unwrap()];

        let abs = git_dir.join("index.lock");
        assert!(
            lock_file_in(&dirs, &root, &abs.to_string_lossy())
                .unwrap()
                .is_some()
        );
        assert!(
            lock_file_in(&dirs, &root, ".git/refs/heads/main.lock")
                .unwrap()
                .is_some()
        );
        // Already gone: nothing to do.
        assert!(
            lock_file_in(&dirs, &root, ".git/HEAD.lock")
                .unwrap()
                .is_none()
        );
        // Not a lock file, outside the git dir, or escaping it.
        assert!(lock_file_in(&dirs, &root, ".git/config").is_err());
        assert!(lock_file_in(&dirs, &root, "outside.lock").is_err());
        assert!(lock_file_in(&dirs, &root, ".git/../outside.lock").is_err());
        assert!(lock_file_in(&dirs, &root, ".git/.lock").is_err());
    }

    #[test]
    fn safe_directory_uses_git_style_paths() {
        let value = safe_directory_value(Path::new(if cfg!(windows) {
            r"\\?\D:\Work\game"
        } else {
            "/Users/me/game"
        }));
        assert_eq!(
            value,
            if cfg!(windows) {
                "D:/Work/game"
            } else {
                "/Users/me/game"
            }
        );
    }
}
