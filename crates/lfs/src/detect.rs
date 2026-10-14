//! Repository LFS status (PLAN §6.1): git-lfs availability, LFS patterns declared in
//! `.gitattributes`, LFS hooks in the effective hooks directory and the `filter.lfs` config.

use std::path::{Path, PathBuf};

use nanomgg_git_engine::version::{MIN_GIT_LFS, ToolInfo, parse_lfs_version};
use nanomgg_git_engine::{AccountContext, GitRunner};
use serde::Serialize;

use crate::LfsError;
use crate::attributes::{LfsPattern, parse_lfs_patterns};

/// Hooks written by `git lfs install` (git-lfs 3.x).
pub const LFS_HOOKS: [&str; 4] = ["pre-push", "post-checkout", "post-commit", "post-merge"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct LfsHook {
    pub name: String,
    pub exists: bool,
    /// The hook calls git-lfs (`git lfs …` / `git-lfs …`). A hook that exists but does not
    /// makes `git lfs install` refuse to overwrite it.
    pub lfs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct LfsStatus {
    pub git_lfs: ToolInfo,
    /// LFS patterns from every `.gitattributes` (root and nested, tracked or not) and
    /// `$GIT_DIR/info/attributes`, in file then line order.
    pub patterns: Vec<LfsPattern>,
    /// At least one LFS pattern is declared.
    pub uses_lfs: bool,
    /// Effective hooks directory (`git rev-parse --git-path hooks`, honours `core.hooksPath`).
    pub hooks_dir: String,
    pub hooks: Vec<LfsHook>,
    /// Every hook in [`LFS_HOOKS`] exists and calls git-lfs.
    pub hooks_installed: bool,
    /// `filter.lfs.process` or `filter.lfs.clean` is set (any config level).
    pub filter_configured: bool,
    /// The repo uses LFS, git-lfs is available and hooks or filters are missing: offer
    /// `git lfs install --local`.
    pub needs_install: bool,
}

/// `git lfs version`, parsed. Runs in `repo` so a repo-local PATH-less setup is irrelevant.
pub fn lfs_tool(git: &GitRunner, cwd: &Path) -> ToolInfo {
    let raw = git
        .run_text(cwd, &AccountContext::default(), ["lfs", "version"])
        .ok();
    let version = raw.as_deref().and_then(parse_lfs_version);
    ToolInfo {
        ok: version.is_some_and(|v| v >= MIN_GIT_LFS),
        version,
        raw,
        minimum: MIN_GIT_LFS,
    }
}

/// Resolves `git rev-parse --git-path <path>` (relative to `repo` or absolute).
pub fn git_path(git: &GitRunner, repo: &Path, path: &str) -> Result<PathBuf, LfsError> {
    let p = git.run_text(
        repo,
        &AccountContext::default(),
        ["rev-parse", "--git-path", path],
    )?;
    Ok(repo.join(p))
}

/// Repository-relative paths of all `.gitattributes` files: in the index or untracked and
/// not ignored.
pub fn attribute_files(git: &GitRunner, repo: &Path) -> Result<Vec<String>, LfsError> {
    let out = git.run(
        repo,
        &AccountContext::default(),
        [
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
            "--",
            ":(glob)**/.gitattributes",
        ],
    )?;
    let mut files: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    // Root first, then by depth and name.
    files.sort_by(|a, b| {
        (a.matches('/').count(), a.as_str()).cmp(&(b.matches('/').count(), b.as_str()))
    });
    files.dedup();
    Ok(files)
}

/// LFS patterns of the working tree's attributes files (files missing on disk are skipped).
pub fn read_patterns(git: &GitRunner, repo: &Path) -> Result<Vec<LfsPattern>, LfsError> {
    let mut patterns = Vec::new();
    for file in attribute_files(git, repo)? {
        if let Ok(content) = std::fs::read_to_string(repo.join(&file)) {
            patterns.extend(parse_lfs_patterns(&content, &file));
        }
    }
    let info = git_path(git, repo, "info/attributes")?;
    if let Ok(content) = std::fs::read_to_string(info) {
        patterns.extend(parse_lfs_patterns(&content, "$GIT_DIR/info/attributes"));
    }
    Ok(patterns)
}

/// Whether hook script content calls git-lfs.
pub fn is_lfs_hook(content: &str) -> bool {
    content.contains("git lfs") || content.contains("git-lfs")
}

/// State of the [`LFS_HOOKS`] in `dir`.
pub fn read_hooks(dir: &Path) -> Vec<LfsHook> {
    LFS_HOOKS
        .iter()
        .map(|&name| {
            let path = dir.join(name);
            let content = std::fs::read(&path).ok();
            LfsHook {
                name: name.to_owned(),
                exists: path.is_file(),
                lfs: content.is_some_and(|c| is_lfs_hook(&String::from_utf8_lossy(&c))),
            }
        })
        .collect()
}

fn filter_configured(git: &GitRunner, repo: &Path) -> bool {
    let ctx = AccountContext::default();
    ["filter.lfs.process", "filter.lfs.clean"]
        .iter()
        .any(|key| {
            git.run_text(repo, &ctx, ["config", "--get", key])
                .is_ok_and(|v| !v.is_empty())
        })
}

/// Full LFS status of the repository at `repo` (its working tree root).
pub fn detect(git: &GitRunner, repo: &Path) -> Result<LfsStatus, LfsError> {
    let git_lfs = lfs_tool(git, repo);
    let patterns = read_patterns(git, repo)?;
    let hooks_dir = git_path(git, repo, "hooks")?;
    let hooks = read_hooks(&hooks_dir);
    let hooks_installed = hooks.iter().all(|h| h.lfs);
    let filter_configured = filter_configured(git, repo);
    let uses_lfs = !patterns.is_empty();
    Ok(LfsStatus {
        needs_install: uses_lfs
            && git_lfs.version.is_some()
            && !(hooks_installed && filter_configured),
        git_lfs,
        patterns,
        uses_lfs,
        hooks_dir: hooks_dir.to_string_lossy().replace('\\', "/"),
        hooks,
        hooks_installed,
        filter_configured,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_lfs_hooks() {
        assert!(is_lfs_hook(
            "#!/bin/sh\ncommand -v git-lfs >/dev/null 2>&1 || { echo >&2 \"...\"; exit 2; }\ngit lfs pre-push \"$@\"\n"
        ));
        assert!(!is_lfs_hook("#!/bin/sh\nnpx lint-staged\n"));
    }

    #[test]
    fn reads_hooks_from_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pre-push"), "git lfs pre-push \"$@\"").unwrap();
        std::fs::write(dir.path().join("post-commit"), "echo hi").unwrap();
        let hooks = read_hooks(dir.path());
        assert_eq!(hooks.len(), 4);
        assert!(hooks[0].exists && hooks[0].lfs);
        assert!(!hooks[1].exists && !hooks[1].lfs);
        assert!(hooks[2].exists && !hooks[2].lfs, "foreign hook");
    }
}
