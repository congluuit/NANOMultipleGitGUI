//! Default scopes, per-feature requirements and token health (PLAN 4.2 "Sức khỏe token").

use nanomgg_providers::ProviderFamily;
use serde::{Deserialize, Serialize};

/// Warn this many days before a token expires.
pub const EXPIRY_WARNING_DAYS: i64 = 7;

/// GitHub scope names.
pub mod github {
    pub const REPO: &str = "repo";
    pub const READ_USER: &str = "read:user";
    pub const USER_EMAIL: &str = "user:email";
    /// Needed to upload SSH keys (optional at login).
    pub const ADMIN_PUBLIC_KEY: &str = "admin:public_key";
    /// Needed to push changes under `.github/workflows` (optional at login).
    pub const WORKFLOW: &str = "workflow";
}

/// GitLab scope names.
pub mod gitlab {
    /// Full API access; needed to upload SSH keys (optional at login).
    pub const API: &str = "api";
    pub const READ_API: &str = "read_api";
    pub const READ_USER: &str = "read_user";
    pub const READ_REPOSITORY: &str = "read_repository";
    pub const WRITE_REPOSITORY: &str = "write_repository";
}

/// Scopes requested at login when the caller does not choose:
/// GitHub `repo read:user user:email`; GitLab `read_user read_api read_repository
/// write_repository`. Add `admin:public_key`/`workflow` (GitHub) or `api` (GitLab) to enable
/// SSH key upload / workflow edits.
pub fn default_scopes(family: ProviderFamily) -> Vec<String> {
    let list: &[&str] = match family {
        ProviderFamily::Github => &[github::REPO, github::READ_USER, github::USER_EMAIL],
        ProviderFamily::Gitlab => &[
            gitlab::READ_USER,
            gitlab::READ_API,
            gitlab::READ_REPOSITORY,
            gitlab::WRITE_REPOSITORY,
        ],
    };
    list.iter().map(|s| (*s).to_owned()).collect()
}

/// App features that need specific scopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum Feature {
    /// Clone/fetch/push private repositories over HTTPS (incl. LFS).
    GitHttps,
    /// Unified clone browser.
    ListRepositories,
    /// Verified emails for Commit Guard.
    ReadEmails,
    /// Profile (name, avatar).
    ReadUser,
    UploadSshKey,
    /// Push changes to GitHub Actions workflow files.
    EditWorkflows,
}

impl Feature {
    pub const ALL: [Feature; 6] = [
        Feature::GitHttps,
        Feature::ListRepositories,
        Feature::ReadEmails,
        Feature::ReadUser,
        Feature::UploadSshKey,
        Feature::EditWorkflows,
    ];
}

/// Scopes of which any one enables `feature` (empty = nothing required).
pub fn required_scopes(family: ProviderFamily, feature: Feature) -> &'static [&'static str] {
    match (family, feature) {
        (ProviderFamily::Github, Feature::GitHttps | Feature::ListRepositories) => &[github::REPO],
        (ProviderFamily::Github, Feature::ReadEmails) => &[github::USER_EMAIL],
        (ProviderFamily::Github, Feature::ReadUser) => &[github::READ_USER],
        (ProviderFamily::Github, Feature::UploadSshKey) => &["write:public_key"],
        (ProviderFamily::Github, Feature::EditWorkflows) => &[github::WORKFLOW],
        (ProviderFamily::Gitlab, Feature::GitHttps) => &[gitlab::WRITE_REPOSITORY],
        (ProviderFamily::Gitlab, Feature::ListRepositories) => &[gitlab::READ_API],
        (ProviderFamily::Gitlab, Feature::ReadEmails | Feature::ReadUser) => &[gitlab::READ_USER],
        (ProviderFamily::Gitlab, Feature::UploadSshKey) => &[gitlab::API],
        (ProviderFamily::Gitlab, Feature::EditWorkflows) => &[],
    }
}

/// Scopes that `scope` implies (parent → children), including itself.
fn implied(family: ProviderFamily, scope: &str) -> &'static [&'static str] {
    match (family, scope) {
        (ProviderFamily::Github, "repo") => &[
            "repo",
            "public_repo",
            "repo:status",
            "repo_deployment",
            "repo:invite",
            "security_events",
        ],
        (ProviderFamily::Github, "user") => &["user", "read:user", "user:email", "user:follow"],
        (ProviderFamily::Github, "admin:public_key") => {
            &["admin:public_key", "write:public_key", "read:public_key"]
        }
        (ProviderFamily::Github, "write:public_key") => &["write:public_key", "read:public_key"],
        (ProviderFamily::Github, "admin:org") => &["admin:org", "write:org", "read:org"],
        (ProviderFamily::Github, "write:org") => &["write:org", "read:org"],
        (ProviderFamily::Gitlab, "api") => &[
            "api",
            "read_api",
            "read_user",
            "read_repository",
            "write_repository",
        ],
        (ProviderFamily::Gitlab, "read_api") => &["read_api", "read_user"],
        (ProviderFamily::Gitlab, "write_repository") => &["write_repository", "read_repository"],
        _ => &[],
    }
}

/// True when `granted` contains `needed` directly or through a parent scope
/// (`repo` ⊃ `public_repo`, `api` ⊃ `read_api`, ...).
pub fn scope_satisfied(family: ProviderFamily, granted: &[String], needed: &str) -> bool {
    granted
        .iter()
        .any(|g| g == needed || implied(family, g).contains(&needed))
}

/// A feature the token cannot serve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct MissingScope {
    pub feature: Feature,
    /// Any one of these would do.
    pub any_of: Vec<String>,
}

/// Health of a stored token, shown on the account card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TokenHealth {
    /// `false` for fine-grained PATs / app tokens (no scope list): `missing` is then empty and
    /// permissions only show up as 403s.
    pub scopes_known: bool,
    pub missing: Vec<MissingScope>,
    pub expires_at: Option<i64>,
    pub expired: bool,
    /// Expires within [`EXPIRY_WARNING_DAYS`].
    pub expires_soon: bool,
    /// Whole days left (rounded down); `None` without expiry.
    pub days_left: Option<i64>,
}

impl TokenHealth {
    pub fn is_healthy(&self) -> bool {
        self.missing.is_empty() && !self.expired && !self.expires_soon
    }
}

/// True when `expires_at` falls within `days` days from `now` (or already passed).
pub fn expires_within(expires_at: Option<i64>, now: i64, days: i64) -> bool {
    expires_at.is_some_and(|e| e - now <= days * 86_400)
}

/// Computes [`TokenHealth`]. `scopes = None` means unknown (fine-grained PAT).
/// For tokens with a refresh token (GitLab OAuth) callers usually skip the expiry warning since
/// the app refreshes automatically.
pub fn token_health(
    family: ProviderFamily,
    scopes: Option<&[String]>,
    expires_at: Option<i64>,
    features: &[Feature],
    now: i64,
) -> TokenHealth {
    let missing = match scopes {
        None => Vec::new(),
        Some(granted) => features
            .iter()
            .filter_map(|&feature| {
                let any_of = required_scopes(family, feature);
                let ok =
                    any_of.is_empty() || any_of.iter().any(|s| scope_satisfied(family, granted, s));
                (!ok).then(|| MissingScope {
                    feature,
                    any_of: any_of.iter().map(|s| (*s).to_owned()).collect(),
                })
            })
            .collect(),
    };
    let expired = expires_at.is_some_and(|e| e <= now);
    TokenHealth {
        scopes_known: scopes.is_some(),
        missing,
        expires_at,
        expired,
        expires_soon: !expired && expires_within(expires_at, now, EXPIRY_WARNING_DAYS),
        days_left: expires_at.map(|e| (e - now).max(0) / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(list: &[&str]) -> Vec<String> {
        list.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn implications() {
        let gh = ProviderFamily::Github;
        assert!(scope_satisfied(gh, &s(&["repo"]), "public_repo"));
        assert!(scope_satisfied(gh, &s(&["user"]), "user:email"));
        assert!(scope_satisfied(
            gh,
            &s(&["admin:public_key"]),
            "write:public_key"
        ));
        assert!(!scope_satisfied(gh, &s(&["read:user"]), "repo"));
        let gl = ProviderFamily::Gitlab;
        assert!(scope_satisfied(gl, &s(&["api"]), "write_repository"));
        assert!(scope_satisfied(gl, &s(&["read_api"]), "read_user"));
        assert!(!scope_satisfied(
            gl,
            &s(&["read_repository"]),
            "write_repository"
        ));
    }

    #[test]
    fn health_reports_missing_scopes_and_expiry() {
        let now = 1_000_000;
        let h = token_health(
            ProviderFamily::Github,
            Some(&s(&["repo", "read:user", "user:email"])),
            Some(now + 3 * 86_400),
            &Feature::ALL,
            now,
        );
        assert!(h.scopes_known && h.expires_soon && !h.expired);
        assert_eq!(h.days_left, Some(3));
        let missing: Vec<Feature> = h.missing.iter().map(|m| m.feature).collect();
        assert_eq!(missing, [Feature::UploadSshKey, Feature::EditWorkflows]);
        assert!(!h.is_healthy());

        let h = token_health(
            ProviderFamily::Gitlab,
            Some(&s(&["api"])),
            None,
            &Feature::ALL,
            now,
        );
        assert!(h.is_healthy(), "{h:?}");

        let h = token_health(
            ProviderFamily::Github,
            None,
            Some(now - 1),
            &Feature::ALL,
            now,
        );
        assert!(!h.scopes_known && h.missing.is_empty() && h.expired && !h.expires_soon);
        assert!(expires_within(Some(now + 7 * 86_400), now, 7));
        assert!(!expires_within(Some(now + 8 * 86_400), now, 7));
        assert!(!expires_within(None, now, 7));
    }

    #[test]
    fn defaults() {
        assert_eq!(
            default_scopes(ProviderFamily::Github),
            ["repo", "read:user", "user:email"]
        );
        assert!(default_scopes(ProviderFamily::Gitlab).contains(&"write_repository".to_owned()));
    }
}
