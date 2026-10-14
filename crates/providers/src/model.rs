//! Provider-neutral DTOs (serde camelCase, optional `specta::Type`). Timestamps are Unix seconds.

use serde::{Deserialize, Serialize};

use crate::endpoint::ProviderKind;

/// The account behind a token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ProviderUser {
    /// Numeric provider id, as a string (stable across renames).
    pub id: String,
    /// Login (`hoangnv`).
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub profile_url: Option<String>,
    /// Public (GitHub) or primary (GitLab) email when the API returns one.
    pub email: Option<String>,
}

/// One email address of the account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ProviderEmail {
    pub email: String,
    /// Commits authored with an unverified email are not linked to the profile (Commit Guard).
    pub verified: bool,
    pub primary: bool,
    /// GitLab `commit_email` — the address the web UI uses for commits.
    pub commit_email: bool,
}

/// Effective access of the account to a repository (highest of direct/team/group grants).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum AccessLevel {
    None,
    /// GitHub `pull`; GitLab Guest/Reporter (10/20).
    Read,
    /// GitHub `triage`.
    Triage,
    /// GitHub `push`; GitLab Developer (30).
    Write,
    /// GitHub `maintain`; GitLab Maintainer (40).
    Maintain,
    /// GitHub `admin`; GitLab Owner (50).
    Admin,
}

impl AccessLevel {
    /// GitLab numeric access level → [`AccessLevel`].
    pub fn from_gitlab(level: u32) -> Self {
        match level {
            50.. => Self::Admin,
            40..=49 => Self::Maintain,
            30..=39 => Self::Write,
            10..=29 => Self::Read,
            _ => Self::None,
        }
    }
}

/// Permission summary used by Push Guard and the clone browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RepoPermission {
    pub level: AccessLevel,
    /// GitHub `push`, GitLab access level ≥ 30 (Developer). Protected branches may still refuse
    /// a push — that is checked by git itself.
    pub can_push: bool,
    /// GitHub `admin`, GitLab ≥ 40 (Maintainer: project settings).
    pub can_admin: bool,
}

impl RepoPermission {
    pub fn from_level(level: AccessLevel) -> Self {
        Self {
            level,
            can_push: level >= AccessLevel::Write,
            can_admin: level >= AccessLevel::Maintain,
        }
    }
}

/// A repository row of the unified clone browser (PLAN 4.7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    pub provider: ProviderKind,
    /// Provider id as a string.
    pub id: String,
    /// `owner/name` (GitHub) or `group/sub/project` (GitLab `path_with_namespace`).
    pub full_name: String,
    /// User, organization or (sub)group path.
    pub owner: String,
    pub name: String,
    pub description: Option<String>,
    pub private: bool,
    /// `public`, `private` or `internal` (GitLab / GitHub Enterprise).
    pub visibility: String,
    pub fork: bool,
    /// `full_name` of the fork parent when the API returned it.
    pub parent_full_name: Option<String>,
    pub archived: bool,
    pub default_branch: Option<String>,
    pub clone_https: String,
    pub clone_ssh: String,
    pub web_url: String,
    pub updated_at: Option<i64>,
    pub stars: u32,
    /// Repository size in bytes (GitHub reports KiB; converted). `None` when unknown.
    pub size_bytes: Option<u64>,
    /// GitLab `lfs_enabled`; `None` when the provider does not say (GitHub).
    pub lfs_enabled: Option<bool>,
    /// LFS storage in bytes (GitLab project statistics, Reporter+ only).
    pub lfs_size_bytes: Option<u64>,
    pub permissions: RepoPermission,
}

/// One page of repositories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RepoPage {
    pub repos: Vec<Repository>,
    pub next_page: Option<u32>,
    /// Total hits for searches when the provider reports it.
    pub total: Option<u64>,
}

/// Result of [`crate::list_all_repositories`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RepoListing {
    pub repos: Vec<Repository>,
    /// Stopped at `max_pages` before the last page.
    pub truncated: bool,
}

/// Paging limits for [`crate::list_all_repositories`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListOptions {
    /// 1–100 (both providers cap at 100).
    pub per_page: u8,
    /// Safety cap: 50 pages × 100 = 5 000 repositories.
    pub max_pages: u32,
}

impl Default for ListOptions {
    fn default() -> Self {
        Self {
            per_page: 100,
            max_pages: 50,
        }
    }
}

/// A registered SSH public key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SshPublicKey {
    pub id: String,
    pub title: Option<String>,
    /// `ssh-ed25519 AAAA...` (providers strip the comment).
    pub key: String,
    pub created_at: Option<i64>,
    pub expires_at: Option<i64>,
}

impl SshPublicKey {
    /// True when `openssh` (`type base64 [comment]`) is the same key, ignoring the comment.
    pub fn matches(&self, openssh: &str) -> bool {
        fn core(k: &str) -> Option<(&str, &str)> {
            let mut it = k.split_whitespace();
            Some((it.next()?, it.next()?))
        }
        core(&self.key).is_some() && core(&self.key) == core(openssh)
    }
}

/// Server version (GitLab `/version`, GitHub Enterprise `/meta`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ServerVersion {
    /// Raw string, e.g. `17.4.1-ee` or `3.14.2`.
    pub version: String,
    pub major: u32,
    pub minor: u32,
    pub revision: Option<String>,
}

impl ServerVersion {
    pub fn parse(version: &str, revision: Option<String>) -> Self {
        let mut nums = version
            .split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty())
            .map(|s| s.parse::<u32>().unwrap_or(0));
        Self {
            version: version.to_owned(),
            major: nums.next().unwrap_or(0),
            minor: nums.next().unwrap_or(0),
            revision,
        }
    }

    /// `self >= major.minor`.
    pub fn at_least(&self, major: u32, minor: u32) -> bool {
        (self.major, self.minor) >= (major, minor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gitlab_levels_and_permissions() {
        assert_eq!(AccessLevel::from_gitlab(0), AccessLevel::None);
        assert_eq!(AccessLevel::from_gitlab(5), AccessLevel::None);
        assert_eq!(AccessLevel::from_gitlab(20), AccessLevel::Read);
        assert_eq!(AccessLevel::from_gitlab(30), AccessLevel::Write);
        assert_eq!(AccessLevel::from_gitlab(40), AccessLevel::Maintain);
        assert_eq!(AccessLevel::from_gitlab(50), AccessLevel::Admin);
        let p = RepoPermission::from_level(AccessLevel::Write);
        assert!(p.can_push && !p.can_admin);
        assert!(!RepoPermission::from_level(AccessLevel::Triage).can_push);
    }

    #[test]
    fn versions_and_keys() {
        let v = ServerVersion::parse("17.4.1-ee", None);
        assert_eq!((v.major, v.minor), (17, 4));
        assert!(v.at_least(17, 1) && !v.at_least(17, 5));
        let k = SshPublicKey {
            id: "1".into(),
            title: None,
            key: "ssh-ed25519 AAAAC3Nz".into(),
            created_at: None,
            expires_at: None,
        };
        assert!(k.matches("ssh-ed25519 AAAAC3Nz me@pc"));
        assert!(!k.matches("ssh-ed25519 AAAAother"));
    }
}
