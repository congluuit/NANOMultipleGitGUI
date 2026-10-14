//! Opening and creating repositories.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{AccountContext, GitError, GitRunner};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepoInfo {
    /// Top-level directory of the working tree (what the user thinks of as "the repo").
    pub root: PathBuf,
    /// Absolute git directory (differs from `<root>/.git` for worktrees and submodules).
    pub git_dir: PathBuf,
    pub name: String,
}

/// git prints paths with `/` on every platform; convert to the native form.
fn native_path(s: &str) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(s.replace('/', "\\"))
    } else {
        PathBuf::from(s)
    }
}

fn classify_failure(path: &Path, err: GitError) -> GitError {
    match &err {
        GitError::Failed { stderr, .. } if stderr.contains("not a git repository") => {
            GitError::NotARepository(path.to_owned())
        }
        GitError::Failed { stderr, .. } if stderr.contains("dubious ownership") => {
            GitError::DubiousOwnership(path.to_owned())
        }
        _ => err,
    }
}

/// Resolves `path` (the repo root or any folder inside it) to its repository.
pub fn discover(git: &GitRunner, path: &Path) -> Result<RepoInfo, GitError> {
    if !path.is_dir() {
        return Err(GitError::PathNotFound(path.to_owned()));
    }
    let ctx = AccountContext::default();
    let out = git
        .run_text(
            path,
            &ctx,
            ["rev-parse", "--is-bare-repository", "--absolute-git-dir"],
        )
        .map_err(|e| classify_failure(path, e))?;
    let mut lines = out.lines();
    if lines.next() == Some("true") {
        return Err(GitError::BareRepository(path.to_owned()));
    }
    let git_dir = native_path(lines.next().unwrap_or_default());
    let top = git
        .run_text(path, &ctx, ["rev-parse", "--show-toplevel"])
        .map_err(|e| classify_failure(path, e))?;
    let root = native_path(&top);
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| top.clone());
    Ok(RepoInfo {
        root,
        git_dir,
        name,
    })
}

/// `git init` in `path` (created if missing) with the given initial branch.
pub fn init(git: &GitRunner, path: &Path, initial_branch: &str) -> Result<RepoInfo, GitError> {
    std::fs::create_dir_all(path)?;
    git.run(
        path,
        &AccountContext::default(),
        ["init", "-q", "-b", initial_branch],
    )?;
    discover(git, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_root_from_subdirectory() {
        let dir = tempfile::tempdir().unwrap();
        let git = GitRunner::default();
        let created = init(&git, &dir.path().join("my-repo"), "main").unwrap();
        assert_eq!(created.name, "my-repo");
        let sub = created.root.join("src").join("deep");
        std::fs::create_dir_all(&sub).unwrap();

        let found = discover(&git, &sub).unwrap();
        assert_eq!(found.root, created.root);
        assert!(found.git_dir.ends_with(".git"), "{:?}", found.git_dir);
        assert!(found.git_dir.is_absolute());
    }

    #[test]
    fn reports_non_repositories_and_missing_paths() {
        let dir = tempfile::tempdir().unwrap();
        let git =
            GitRunner::default().with_env("GIT_CEILING_DIRECTORIES", dir.path().to_string_lossy());
        let plain = dir.path().join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        assert!(matches!(
            discover(&git, &plain),
            Err(GitError::NotARepository(_))
        ));
        assert!(matches!(
            discover(&git, &dir.path().join("missing")),
            Err(GitError::PathNotFound(_))
        ));
    }

    #[test]
    fn rejects_bare_repositories() {
        let dir = tempfile::tempdir().unwrap();
        let git = GitRunner::default();
        git.run(
            dir.path(),
            &AccountContext::default(),
            ["init", "-q", "--bare", "b.git"],
        )
        .unwrap();
        assert!(matches!(
            discover(&git, &dir.path().join("b.git")),
            Err(GitError::BareRepository(_))
        ));
    }
}
