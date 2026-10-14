//! Setting a repository up for LFS: `git lfs install --local`, `track` / `untrack` and
//! re-staging files so they become LFS pointers (PLAN §6.1, §6.2).

use std::path::Path;

use nanomgg_git_engine::{AccountContext, GitRunner};

use crate::LfsError;

/// `git lfs install --local`: hooks and `filter.lfs.*` for this repository only, never the
/// user's global config. Fails (stderr explains) when a non-LFS hook already exists.
pub fn install_local(git: &GitRunner, repo: &Path) -> Result<(), LfsError> {
    git.run(
        repo,
        &AccountContext::default(),
        ["lfs", "install", "--local"],
    )?;
    Ok(())
}

/// Checks that `pattern` is safe to hand to `git lfs track`: not empty, not option-like,
/// a single line without surrounding whitespace or quotes, and not a comment or negation in
/// `.gitattributes`.
pub fn validate_pattern(pattern: &str) -> Result<(), LfsError> {
    let bad = pattern.is_empty()
        || pattern.len() > 4096
        || pattern.trim() != pattern
        || pattern.starts_with(['-', '#', '!'])
        || pattern.contains('"')
        || pattern.chars().any(char::is_control);
    if bad {
        Err(LfsError::InvalidPattern(pattern.to_owned()))
    } else {
        Ok(())
    }
}

/// Checks a repository-relative path (`/` or `\` separators) and returns it with `/`.
/// Rejects absolute paths, `..` components and control characters.
pub fn normalize_path(path: &str) -> Result<String, LfsError> {
    let p = path.replace('\\', "/");
    let p = p.strip_prefix("./").unwrap_or(&p);
    let invalid = p.is_empty()
        || p.starts_with('/')
        || p.as_bytes().get(1) == Some(&b':')
        || p.chars().any(char::is_control)
        || p.split('/').any(|c| c == "..");
    if invalid {
        Err(LfsError::InvalidPath(path.to_owned()))
    } else {
        Ok(p.trim_end_matches('/').to_owned())
    }
}

fn read_root_attributes(repo: &Path) -> Option<Vec<u8>> {
    std::fs::read(repo.join(".gitattributes")).ok()
}

fn track_impl(
    git: &GitRunner,
    repo: &Path,
    pattern: &str,
    lockable: bool,
    untrack: bool,
) -> Result<bool, LfsError> {
    validate_pattern(pattern)?;
    let before = read_root_attributes(repo);
    let mut args = vec!["lfs", if untrack { "untrack" } else { "track" }];
    if lockable && !untrack {
        args.push("--lockable");
    }
    args.extend(["--", pattern]);
    git.run(repo, &AccountContext::default(), &args)?;
    Ok(read_root_attributes(repo) != before)
}

/// `git lfs track [--lockable] -- <pattern>` in the repository root. Returns whether the root
/// `.gitattributes` changed (`false` when the pattern was already tracked).
/// Note: git-lfs also installs its hooks in the repository as a side effect of `track`.
pub fn track(
    git: &GitRunner,
    repo: &Path,
    pattern: &str,
    lockable: bool,
) -> Result<bool, LfsError> {
    track_impl(git, repo, pattern, lockable, false)
}

/// `git lfs untrack -- <pattern>`. Returns whether the root `.gitattributes` changed.
/// Files already committed stay LFS pointers in history; only new content is affected.
pub fn untrack(git: &GitRunner, repo: &Path, pattern: &str) -> Result<bool, LfsError> {
    track_impl(git, repo, pattern, false, true)
}

/// NUL-separated `:(literal)` pathspecs for `--pathspec-from-file=- --pathspec-file-nul`
/// (no command-line length limit, no glob expansion).
fn pathspec_input(paths: &[String]) -> Result<Vec<u8>, LfsError> {
    let mut input = Vec::new();
    for p in paths {
        input.extend_from_slice(format!(":(literal){}\0", normalize_path(p)?).as_bytes());
    }
    Ok(input)
}

/// `git add --renormalize -- <paths>`: re-runs the clean filter on files git already tracks,
/// so content staged before a `track` becomes an LFS pointer. Untracked paths are ignored.
pub fn renormalize(git: &GitRunner, repo: &Path, paths: &[String]) -> Result<(), LfsError> {
    if paths.is_empty() {
        return Ok(());
    }
    let input = pathspec_input(paths)?;
    git.run_with_input(
        repo,
        &AccountContext::default(),
        [
            "add",
            "--renormalize",
            "--pathspec-from-file=-",
            "--pathspec-file-nul",
        ],
        &input,
    )?;
    Ok(())
}

/// One-click follow-up to [`track`] (PLAN §6.2): stages `.gitattributes`, renormalizes the
/// paths git already knows and adds the new ones, so all of them are staged as pointers.
pub fn stage_as_lfs(git: &GitRunner, repo: &Path, paths: &[String]) -> Result<(), LfsError> {
    let ctx = AccountContext::default();
    if repo.join(".gitattributes").is_file() {
        git.run(repo, &ctx, ["add", "--", ".gitattributes"])?;
    }
    if paths.is_empty() {
        return Ok(());
    }
    renormalize(git, repo, paths)?;
    let input = pathspec_input(paths)?;
    git.run_with_input(
        repo,
        &ctx,
        ["add", "--pathspec-from-file=-", "--pathspec-file-nul"],
        &input,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_validation() {
        for ok in [
            "*.psd",
            "Assets/**/*.fbx",
            "/big.bin",
            "a b.mp4",
            "[*]x.bin",
        ] {
            assert!(validate_pattern(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "-x",
            "--lockable",
            "#c",
            "!neg",
            " *.psd",
            "*.psd ",
            "a\nb",
            "a\0b",
            "\"q\"",
        ] {
            assert!(validate_pattern(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn path_normalization() {
        assert_eq!(normalize_path("a\\b\\c.bin").unwrap(), "a/b/c.bin");
        assert_eq!(normalize_path("./x.bin").unwrap(), "x.bin");
        assert_eq!(normalize_path("dir/").unwrap(), "dir");
        assert_eq!(normalize_path("-x.bin").unwrap(), "-x.bin");
        for bad in [
            "",
            "/etc/passwd",
            "C:/x",
            "c:\\x",
            "../x",
            "a/../../x",
            "a\nb",
        ] {
            assert!(normalize_path(bad).is_err(), "{bad:?}");
        }
    }
}
