//! GitHub.com and GitHub Enterprise Server REST API (v3, `X-GitHub-Api-Version: 2022-11-28`).

use reqwest::Method;
use serde::Deserialize;

use crate::api::{ApiClient, ApiResponse};
use crate::endpoint::{ProviderEndpoint, ProviderKind};
use crate::error::ProviderError;
use crate::model::*;
use crate::timefmt::parse_timestamp;
use crate::{Provider, SecretToken};

/// GitHub client for one account.
#[derive(Debug, Clone)]
pub struct GithubClient {
    api: ApiClient,
}

/// Token metadata GitHub returns in response headers of any authenticated call.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GithubTokenHeaders {
    /// `X-OAuth-Scopes`; `None` when the header is absent (fine-grained PATs, GitHub App tokens).
    pub scopes: Option<Vec<String>>,
    /// `github-authentication-token-expiration` as Unix seconds.
    pub expires_at: Option<i64>,
}

impl GithubTokenHeaders {
    pub fn from_response<T>(resp: &ApiResponse<T>) -> Self {
        Self {
            scopes: resp
                .header("x-oauth-scopes")
                .map(crate::error::split_scopes),
            expires_at: resp
                .header("github-authentication-token-expiration")
                .and_then(parse_timestamp),
        }
    }
}

impl GithubClient {
    pub fn new(http: reqwest::Client, endpoint: ProviderEndpoint, token: SecretToken) -> Self {
        Self::from_api(ApiClient::new(http, endpoint, token))
    }

    pub fn from_api(api: ApiClient) -> Self {
        Self { api }
    }

    pub fn api(&self) -> &ApiClient {
        &self.api
    }

    /// `GET /user` plus the token headers (scopes, expiry) — used for PAT validation.
    pub async fn current_user_with_token_info(
        &self,
    ) -> Result<(ProviderUser, GithubTokenHeaders), ProviderError> {
        let resp = self.api.get_json::<RawUser>(&["user"], &[]).await?;
        let headers = GithubTokenHeaders::from_response(&resp);
        Ok((resp.body.into(), headers))
    }

    fn repo_segments(full_name: &str) -> Result<[&str; 2], ProviderError> {
        match full_name.trim_matches('/').split_once('/') {
            Some((owner, name)) if !owner.is_empty() && !name.is_empty() && !name.contains('/') => {
                Ok([owner, name.strip_suffix(".git").unwrap_or(name)])
            }
            _ => Err(ProviderError::InvalidInput {
                message: format!("`{full_name}` is not an owner/repo name"),
            }),
        }
    }

    fn kind_of(&self) -> ProviderKind {
        self.api.endpoint().kind()
    }
}

impl Provider for GithubClient {
    fn kind(&self) -> ProviderKind {
        self.kind_of()
    }

    fn endpoint(&self) -> &ProviderEndpoint {
        self.api.endpoint()
    }

    async fn current_user(&self) -> Result<ProviderUser, ProviderError> {
        Ok(self.current_user_with_token_info().await?.0)
    }

    async fn emails(&self) -> Result<Vec<ProviderEmail>, ProviderError> {
        let resp = self
            .api
            .get_json::<Vec<RawEmail>>(&["user", "emails"], &[("per_page", "100")])
            .await?;
        Ok(resp
            .body
            .into_iter()
            .map(|e| ProviderEmail {
                email: e.email,
                verified: e.verified,
                primary: e.primary,
                commit_email: false,
            })
            .collect())
    }

    async fn list_repositories_page(
        &self,
        page: u32,
        per_page: u8,
    ) -> Result<RepoPage, ProviderError> {
        let page_s = page.max(1).to_string();
        let per_page_s = per_page.clamp(1, 100).to_string();
        let resp = self
            .api
            .get_json::<Vec<RawRepo>>(
                &["user", "repos"],
                &[
                    ("affiliation", "owner,collaborator,organization_member"),
                    ("sort", "updated"),
                    ("per_page", &per_page_s),
                    ("page", &page_s),
                ],
            )
            .await?;
        let next_page = resp.next_page();
        let kind = self.kind_of();
        Ok(RepoPage {
            repos: resp.body.into_iter().map(|r| r.into_repo(kind)).collect(),
            next_page,
            total: None,
        })
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
        let page_s = page.max(1).to_string();
        let per_page_s = per_page.clamp(1, 100).to_string();
        let resp = self
            .api
            .get_json::<RawSearch>(
                &["search", "repositories"],
                &[
                    ("q", query.trim()),
                    ("per_page", &per_page_s),
                    ("page", &page_s),
                ],
            )
            .await?;
        let next_page = resp.next_page();
        let kind = self.kind_of();
        Ok(RepoPage {
            total: Some(resp.body.total_count),
            repos: resp
                .body
                .items
                .into_iter()
                .map(|r| r.into_repo(kind))
                .collect(),
            next_page,
        })
    }

    async fn repository(&self, full_name: &str) -> Result<Repository, ProviderError> {
        let [owner, name] = Self::repo_segments(full_name)?;
        let resp = self
            .api
            .get_json::<RawRepo>(&["repos", owner, name], &[])
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
        if self.kind_of() == ProviderKind::Github {
            return Ok(None);
        }
        let resp = self.api.get_json::<RawMeta>(&["meta"], &[]).await?;
        Ok(resp
            .body
            .installed_version
            .map(|v| ServerVersion::parse(&v, None)))
    }
}

#[derive(Deserialize)]
struct RawUser {
    id: u64,
    login: String,
    name: Option<String>,
    avatar_url: Option<String>,
    html_url: Option<String>,
    email: Option<String>,
}

impl From<RawUser> for ProviderUser {
    fn from(u: RawUser) -> Self {
        Self {
            id: u.id.to_string(),
            username: u.login,
            display_name: u.name.filter(|n| !n.trim().is_empty()),
            avatar_url: u.avatar_url,
            profile_url: u.html_url,
            email: u.email.filter(|e| !e.is_empty()),
        }
    }
}

#[derive(Deserialize)]
struct RawEmail {
    email: String,
    #[serde(default)]
    primary: bool,
    #[serde(default)]
    verified: bool,
}

#[derive(Deserialize)]
struct RawOwner {
    login: String,
}

#[derive(Deserialize)]
struct RawParent {
    full_name: String,
}

#[derive(Deserialize, Default)]
struct RawPermissions {
    #[serde(default)]
    admin: bool,
    #[serde(default)]
    maintain: bool,
    #[serde(default)]
    push: bool,
    #[serde(default)]
    triage: bool,
    #[serde(default)]
    pull: bool,
}

impl RawPermissions {
    fn level(&self) -> AccessLevel {
        if self.admin {
            AccessLevel::Admin
        } else if self.maintain {
            AccessLevel::Maintain
        } else if self.push {
            AccessLevel::Write
        } else if self.triage {
            AccessLevel::Triage
        } else if self.pull {
            AccessLevel::Read
        } else {
            AccessLevel::None
        }
    }
}

#[derive(Deserialize)]
struct RawRepo {
    id: u64,
    name: String,
    full_name: String,
    owner: RawOwner,
    #[serde(default)]
    private: bool,
    visibility: Option<String>,
    description: Option<String>,
    #[serde(default)]
    fork: bool,
    parent: Option<RawParent>,
    #[serde(default)]
    archived: bool,
    default_branch: Option<String>,
    #[serde(default)]
    clone_url: String,
    #[serde(default)]
    ssh_url: String,
    #[serde(default)]
    html_url: String,
    updated_at: Option<String>,
    pushed_at: Option<String>,
    #[serde(default)]
    stargazers_count: u32,
    /// KiB.
    size: Option<u64>,
    permissions: Option<RawPermissions>,
}

impl RawRepo {
    fn into_repo(self, kind: ProviderKind) -> Repository {
        // A repository the API returned without `permissions` is at least readable.
        let level = self
            .permissions
            .as_ref()
            .map_or(AccessLevel::Read, RawPermissions::level);
        // `pushed_at` tracks code changes; `updated_at` also moves on metadata edits.
        let updated_at = [self.pushed_at.as_deref(), self.updated_at.as_deref()]
            .into_iter()
            .flatten()
            .filter_map(parse_timestamp)
            .max();
        Repository {
            provider: kind,
            id: self.id.to_string(),
            owner: self.owner.login,
            name: self.name,
            description: self.description.filter(|d| !d.is_empty()),
            visibility: self
                .visibility
                .unwrap_or_else(|| if self.private { "private" } else { "public" }.to_owned()),
            private: self.private,
            fork: self.fork,
            parent_full_name: self.parent.map(|p| p.full_name),
            archived: self.archived,
            default_branch: self.default_branch,
            clone_https: self.clone_url,
            clone_ssh: self.ssh_url,
            web_url: self.html_url,
            updated_at,
            stars: self.stargazers_count,
            size_bytes: self.size.map(|kib| kib.saturating_mul(1024)),
            lfs_enabled: None,
            lfs_size_bytes: None,
            permissions: RepoPermission::from_level(level),
            full_name: self.full_name,
        }
    }
}

#[derive(Deserialize)]
struct RawSearch {
    total_count: u64,
    items: Vec<RawRepo>,
}

#[derive(Deserialize)]
struct RawKey {
    id: u64,
    key: String,
    title: Option<String>,
    created_at: Option<String>,
}

impl From<RawKey> for SshPublicKey {
    fn from(k: RawKey) -> Self {
        Self {
            id: k.id.to_string(),
            title: k.title,
            key: k.key,
            created_at: k.created_at.as_deref().and_then(parse_timestamp),
            expires_at: None,
        }
    }
}

#[derive(Deserialize)]
struct RawMeta {
    installed_version: Option<String>,
}
