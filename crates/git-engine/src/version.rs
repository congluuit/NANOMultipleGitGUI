//! Detects the installed git / git-lfs and checks the minimum versions (PLAN §2.1).

use std::fmt;

use serde::Serialize;

use crate::{AccountContext, GitRunner};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// `--porcelain=v2`, `GIT_CONFIG_COUNT` and `safe.directory` all need git 2.35.
pub const MIN_GIT: Version = Version::new(2, 35, 0);
pub const MIN_GIT_LFS: Version = Version::new(3, 0, 0);

/// Parses the first `X.Y[.Z]` run of digits, e.g. in `git version 2.47.0.windows.1`.
fn parse_dotted(s: &str) -> Option<Version> {
    let start = s.find(|c: char| c.is_ascii_digit())?;
    let mut parts = s[start..]
        .split(|c: char| !c.is_ascii_digit())
        .take(3)
        .map(|p| p.parse::<u32>().ok());
    let major = parts.next()??;
    let minor = parts.next()??;
    let patch = parts.next().flatten().unwrap_or(0);
    Some(Version::new(major, minor, patch))
}

/// `git version 2.47.0.windows.1`, `git version 2.39.3 (Apple Git-146)`.
pub fn parse_git_version(output: &str) -> Option<Version> {
    parse_dotted(output.trim().strip_prefix("git version ")?)
}

/// `git-lfs/3.5.1 (GitHub; windows amd64; go 1.21.7; git e237bb3a)`.
pub fn parse_lfs_version(output: &str) -> Option<Version> {
    parse_dotted(output.trim().strip_prefix("git-lfs/")?)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub version: Option<Version>,
    /// Raw `--version` output, for display and bug reports.
    pub raw: Option<String>,
    pub minimum: Version,
    pub ok: bool,
}

impl ToolInfo {
    fn from_output(
        raw: Option<String>,
        parse: fn(&str) -> Option<Version>,
        minimum: Version,
    ) -> Self {
        let version = raw.as_deref().and_then(parse);
        Self {
            ok: version.is_some_and(|v| v >= minimum),
            version,
            raw,
            minimum,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Tools {
    pub git: ToolInfo,
    pub git_lfs: ToolInfo,
}

pub fn detect(git: &GitRunner) -> Tools {
    let cwd = std::env::temp_dir();
    let ctx = AccountContext::default();
    let run = |args: &[&str]| git.run_text(&cwd, &ctx, args).ok();
    Tools {
        git: ToolInfo::from_output(run(&["--version"]), parse_git_version, MIN_GIT),
        git_lfs: ToolInfo::from_output(run(&["lfs", "version"]), parse_lfs_version, MIN_GIT_LFS),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_git_versions_from_all_platforms() {
        assert_eq!(
            parse_git_version("git version 2.47.0.windows.1"),
            Some(Version::new(2, 47, 0))
        );
        assert_eq!(
            parse_git_version("git version 2.39.3 (Apple Git-146)\n"),
            Some(Version::new(2, 39, 3))
        );
        assert_eq!(
            parse_git_version("git version 2.43.0"),
            Some(Version::new(2, 43, 0))
        );
        assert_eq!(
            parse_git_version("git version 3.0"),
            Some(Version::new(3, 0, 0))
        );
        assert_eq!(parse_git_version("command not found"), None);
    }

    #[test]
    fn parses_lfs_version() {
        assert_eq!(
            parse_lfs_version("git-lfs/3.5.1 (GitHub; windows amd64; go 1.21.7; git e237bb3a)"),
            Some(Version::new(3, 5, 1))
        );
        assert_eq!(
            parse_lfs_version("git: 'lfs' is not a git command. See 'git --help'."),
            None
        );
    }

    #[test]
    fn minimum_version_check() {
        let check =
            |raw: &str| ToolInfo::from_output(Some(raw.into()), parse_git_version, MIN_GIT).ok;
        assert!(check("git version 2.35.0"));
        assert!(check("git version 2.47.0.windows.1"));
        assert!(!check("git version 2.34.9"));
        assert!(!ToolInfo::from_output(None, parse_git_version, MIN_GIT).ok);
    }

    #[test]
    fn detects_installed_git() {
        let tools = detect(&GitRunner::default());
        assert!(tools.git.ok, "{:?}", tools.git);
    }
}
