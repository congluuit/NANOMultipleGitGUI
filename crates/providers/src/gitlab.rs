//! GitLab.com and self-hosted GitLab REST API v4.

use reqwest::Method;
use serde::Deserialize;

use crate::api::ApiClient;
use crate::endpoint::{ProviderEndpoint, ProviderKind};
use crate::error::ProviderError;
use crate::model::*;
use crate::timefmt::parse_timestamp;
use crate::{Provider, SecretToken};

/// GitLab client for one account.
#[derive(Debug, Clone)]
pub struct GitlabClient {
    api: ApiClient,
}

/// `GET /personal_access_tokens/self` (GitLab 15.5+). Also works for project/group tokens.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GitlabTokenSelf {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub revoked: bool,
    /// `YYYY-MM-DD` or null.
    pub expires_at: Option<String>,
    pub user_id: Option<u64>,
}

impl GitlabClient {
    pub fn new(http: reqwest::Client, endpoint: ProviderEndpoint, token: SecretToken) -> Self {
        Self::from_api(ApiClient::new(http, endpoint, token))
    }

    pub fn from_api(api: ApiClient) -> Self {
        Self { api }
    }

    pub fn api(&self) -> &ApiClient {
        &self.api
    }

    /// Introspects a personal/project/group access token. Fails with `NotFound`/`Unauthorized`
    /// for OAuth tokens and on GitLab < 15.5 — callers fall back to `/user`.
    pub async fn personal_access_token_self(&self) -> Result<GitlabTokenSelf, ProviderError> {
        Ok(self
            .api
            .get_json::<GitlabTokenSelf>(&["personal_access_tokens", "self"], &[])
            .await?
            .body)
    }

    fn kind_of(&self) -> ProviderKind {
        self.api.endpoint().kind()
    }

    async fn raw_user(&self) -> Result<RawUser, ProviderError> {
        Ok(self.api.get_json::<RawUser>(&["user"], &[]).await?.body)
    }

    async fn projects_page(
        &self,
        query: &[(&str, &str)],
        page: u32,
        per_page: u8,
    ) -> Result<RepoPage, ProviderError> {
        let page_s = page.max(1).to_string();
        let per_page_s = per_page.clamp(1, 100).to_string();
        let mut q: Vec<(&str, &str)> = query.to_vec();
        q.extend([
            ("order_by", "last_activity_at"),
            ("sort", "desc"),
            ("per_page", per_page_s.as_str()),
            ("page", page_s.as_str()),
        ]);
        let resp = self
            .api
            .get_json::<Vec<RawProject>>(&["projects"], &q)
            .await?;
        let next_page = resp.next_page();
        let total = resp.header("x-total").and_then(|t| t.parse().ok());
        let kind = self.kind_of();
        Ok(RepoPage {
            repos: resp.body.into_iter().map(|p| p.into_repo(kind)).collect(),
            next_page,
            total,
        })
    }
}

impl Provider for GitlabClient {
    fn kind(&self) -> ProviderKind {
        self.kind_of()
    }

    fn endpoint(&self) -> &ProviderEndpoint {
        self.api.endpoint()
    }

    async fn current_user(&self) -> Result<ProviderUser, ProviderError> {
        Ok(self.raw_user().await?.into())
    }

    /// Primary email (`/user`), commit email and secondary emails (`/user/emails`).
    async fn emails(&self) -> Result<Vec<ProviderEmail>, ProviderError> {
        let user = self.raw_user().await?;
        let extra = self
            .api
            .get_json::<Vec<RawEmail>>(&["user", "emails"], &[("per_page", "100")])
            .await?
            .body;
        let commit = user.commit_email.clone().filter(|e| !e.is_empty());
        let mut out: Vec<ProviderEmail> = Vec::new();
        let mut push = |email: String, verified: bool, primary: bool| {
            if let Some(existing) = out
                .iter_mut()
                .find(|e| e.email.eq_ignore_ascii_case(&email))
            {
                existing.verified |= verified;
                existing.primary |= primary;
                return;
            }
            let commit_email = commit
                .as_deref()
                .is_some_and(|c| c.eq_ignore_ascii_case(&email));
            out.push(ProviderEmail {
                email,
                verified,
                primary,
                commit_email,
            });
        };
        if let Some(primary) = user.email.clone().filter(|e| !e.is_empty()) {
            // The sign-in email; GitLab requires it confirmed unless the instance disables that.
            let verified = !matches!(user.confirmed_at, Some(None));
            push(primary, verified, true);
        }
        for e in extra {
            push(e.email, e.confirmed_at.is_some(), false);
        }
        if let Some(c) = user.commit_email.filter(|e| !e.is_empty()) {
            // Private commit email (`ID-user@users.noreply.gitlab.com`) is always valid.
            push(c, true, false);
        }
        Ok(out)
    }

    async fn list_repositories_page(
        &self,
        page: u32,
        per_page: u8,
    ) -> Result<RepoPage, ProviderError> {
        self.projects_page(
            &[
                ("membership", "true"),
                ("simple", "false"),
                ("statistics", "true"),
            ],
            page,
            per_page,
        )
        .await
    }

    async fn search_repositories(
        &self,
        query: &str,
        page: u32,
        per_page: u8,
    ) -> Result<RepoPage, ProviderError> {
        if query.trim().is_empty() {
            return Err(ProviderError::InvalidInput {
                message: "empty search query".into(),
            });
        }
        self.projects_page(
            &[("search", query.trim()), ("simple", "false")],
            page,
            per_page,
        )
        .await
    }

    async fn repository(&self, full_name: &str) -> Result<Repository, ProviderError> {
        let path = full_name.trim_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path);
        if !path.contains('/') {
            return Err(ProviderError::InvalidInput {
                message: format!("`{full_name}` is not a group/project path"),
            });
        }
        let resp = self
            .api
            .get_json::<RawProject>(&["projects", path], &[("statistics", "true")])
            .await?;
        Ok(resp.body.into_repo(self.kind_of()))
    }

    async fn list_ssh_keys(&self) -> Result<Vec<SshPublicKey>, ProviderError> {
        let resp = self
            .api
            .get_json::<Vec<RawKey>>(&["user", "keys"], &[("per_page", "100")])
            .await?;
        Ok(resp.body.into_iter().map(Into::into).collect())
    }

    async fn add_ssh_key(
        &self,
        title: &str,
        public_key: &str,
    ) -> Result<SshPublicKey, ProviderError> {
        let body = serde_json::json!({ "title": title, "key": public_key.trim() });
        let resp = self
            .api
            .send_json::<RawKey, _>(Method::POST, &["user", "keys"], &body)
            .await?;
        Ok(resp.body.into())
    }

    async fn server_version(&self) -> Result<Option<ServerVersion>, ProviderError> {
        let v = self
            .api
            .get_json::<RawVersion>(&["version"], &[])
            .await?
            .body;
        Ok(Some(ServerVersion::parse(&v.version, v.revision)))
    }
}

#[derive(Deserialize)]
struct RawUser {
    id: u64,
    username: String,
    name: Option<String>,
    avatar_url: Option<String>,
    web_url: Option<String>,
    email: Option<String>,
    commit_email: Option<String>,
    public_email: Option<String>,
    /// Outer `None`: field absent (older GitLab) — the primary is then assumed confirmed.
    /// `Some(None)`: explicitly null — unconfirmed.
    #[serde(default, deserialize_with = "present_option")]
    confirmed_at: Option<Option<String>>,
}

fn present_option<'de, D>(d: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(d).map(Some)
}

impl From<RawUser> for ProviderUser {
    fn from(u: RawUser) -> Self {
        Self {
            id: u.id.to_string(),
            username: u.username,
            display_name: u.name.filter(|n| !n.trim().is_empty()),
            avatar_url: u.avatar_url,
            profile_url: u.web_url,
            email: u
                .email
                .filter(|e| !e.is_empty())
                .or(u.public_email.filter(|e| !e.is_empty())),
        }
    }
}

#[derive(Deserialize)]
struct RawEmail {
    email: String,
    confirmed_at: Option<String>,
}

#[derive(Deserialize)]
struct RawNamespace {
    full_path: String,
}

#[derive(Deserialize)]
struct RawForkParent {
    path_with_namespace: String,
}

#[derive(Deserialize)]
struct RawAccess {
    access_level: u32,
}

#[derive(Deserialize)]
struct RawProjectPermissions {
    project_access: Option<RawAccess>,
    group_access: Option<RawAccess>,
}

#[derive(Deserialize)]
struct RawStatistics {
    repository_size: Option<u64>,
    lfs_objects_size: Option<u64>,
}

#[derive(Deserialize)]
struct RawProject {
    id: u64,
    name: String,
    path: Option<String>,
    path_with_namespace: String,
    namespace: Option<RawNamespace>,
    description: Option<String>,
    visibility: Option<String>,
    forked_from_project: Option<RawForkParent>,
    #[serde(default)]
    archived: bool,
    default_branch: Option<String>,
    #[serde(default)]
    http_url_to_repo: String,
    #[serde(default)]
    ssh_url_to_repo: String,
    #[serde(default)]
    web_url: String,
    last_activity_at: Option<String>,
    updated_at: Option<String>,
    #[serde(default)]
    star_count: u32,
    lfs_enabled: Option<bool>,
    statistics: Option<RawStatistics>,
    permissions: Option<RawProjectPermissions>,
}

impl RawProject {
    fn into_repo(self, kind: ProviderKind) -> Repository {
        let visibility = self.visibility.unwrap_or_else(|| "private".to_owned());
        let granted = self.permissions.as_ref().map_or(0, |p| {
            let project = p.project_access.as_ref().map_or(0, |a| a.access_level);
            let group = p.group_access.as_ref().map_or(0, |a| a.access_level);
            project.max(group)
        });
        let mut level = AccessLevel::from_gitlab(granted);
        if level == AccessLevel::None && visibility != "private" {
            // Public/internal project seen without membership: readable.
            level = AccessLevel::Read;
        }
        let owner = self
            .namespace
            .map(|n| n.full_path)
            .or_else(|| {
                self.path_with_namespace
                    .rsplit_once('/')
                    .map(|(o, _)| o.to_owned())
            })
            .unwrap_or_default();
        let updated_at = [self.last_activity_at.as_deref(), self.updated_at.as_deref()]
            .into_iter()
            .flatten()
            .filter_map(parse_timestamp)
            .max();
        Repository {
            provider: kind,
            id: self.id.to_string(),
            full_name: self.path_with_namespace,
            owner,
            name: self.path.unwrap_or(self.name),
            description: self.description.filter(|d| !d.is_empty()),
            private: visibility == "private",
            visibility,
            fork: self.forked_from_project.is_some(),
            parent_full_name: self.forked_from_project.map(|p| p.path_with_namespace),
            archived: self.archived,
            default_branch: self.default_branch,
            clone_https: self.http_url_to_repo,
            clone_ssh: self.ssh_url_to_repo,
            web_url: self.web_url,
            updated_at,
            stars: self.star_count,
            size_bytes: self.statistics.as_ref().and_then(|s| s.repository_size),
            lfs_enabled: self.lfs_enabled,
            lfs_size_bytes: self.statistics.as_ref().and_then(|s| s.lfs_objects_size),
            permissions: RepoPermission::from_level(level),
        }
    }
}

#[derive(Deserialize)]
struct RawKey {
    id: u64,
    title: Option<String>,
    key: String,
    created_at: Option<String>,
    expires_at: Option<String>,
}

impl From<RawKey> for SshPublicKey {
    fn from(k: RawKey) -> Self {
        Self {
            id: k.id.to_string(),
            title: k.title,
            key: k.key,
            created_at: k.created_at.as_deref().and_then(parse_timestamp),
            expires_at: k.expires_at.as_deref().and_then(parse_timestamp),
        }
    }
}

#[derive(Deserialize)]
struct RawVersion {
    version: String,
    revision: Option<String>,
}
