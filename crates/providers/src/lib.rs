//! GitHub and GitLab API clients behind a common [`Provider`] trait.
//!
//! Both providers are spoken to with one small `reqwest` client (rustls + ring, HTTP/1.1, see
//! `docs/adr/0005-provider-http-client.md`). Everything is async and `Send`, so a Tauri command
//! can simply `.await` it; long listings take a [`CancellationToken`].
//!
//! Security rules this crate keeps:
//! - tokens are [`SecretToken`]s (zeroized on drop, redacted in `Debug`) and only ever travel in
//!   the `Authorization` header (marked sensitive) — never in URLs, query strings or errors;
//! - requests only go to URLs under the configured API base; pagination links are reduced to a
//!   page number instead of being followed blindly;
//! - error messages copy the provider's own `message` text (truncated), never request bodies.
//!
//! ```no_run
//! # async fn demo() -> Result<(), nanomgg_providers::ProviderError> {
//! use nanomgg_providers::*;
//! let http = build_http_client(&HttpConfig::default())?;
//! let client = ProviderClient::new(http, ProviderEndpoint::github(), SecretToken::new("ghp_..."));
//! let me = client.current_user().await?;
//! let all = list_all_repositories(&client, &ListOptions::default(), &CancellationToken::new(), |page| {
//!     println!("{} more repos", page.len());
//! })
//! .await?;
//! # let _ = (me, all); Ok(()) }
//! ```

pub mod api;
pub mod ci;
pub mod endpoint;
pub mod error;
pub mod forks;
pub mod github;
pub mod gitlab;
pub mod issues;
pub mod model;
pub mod mywork;
pub mod publish;
pub mod pulls;
pub mod remote;
pub mod secret;
pub mod timefmt;

use std::future::Future;

pub use api::{ApiClient, ApiResponse, HttpConfig, USER_AGENT, build_http_client, cancellable};
pub use endpoint::{ProviderEndpoint, ProviderFamily, ProviderKind};
pub use error::{AccessHint, ProviderError, split_scopes};
pub use github::{GithubClient, GithubTokenHeaders};
pub use gitlab::{GitlabClient, GitlabTokenSelf};
pub use model::{
    AccessLevel, ListOptions, ProviderEmail, ProviderUser, RepoListing, RepoPage, RepoPermission,
    Repository, ServerVersion, SshPublicKey,
};
pub use remote::{RemoteRef, parse_remote_url};
pub use secret::SecretToken;
pub use timefmt::{now_unix, parse_timestamp};
pub use tokio_util::sync::CancellationToken;

/// Operations every supported hosting provider offers.
///
/// Futures are `Send` so they can be awaited from Tauri commands. Repository names are the
/// provider's "full name": `owner/repo` on GitHub, `group/subgroup/project` on GitLab.
pub trait Provider: Send + Sync {
    fn kind(&self) -> ProviderKind;
    fn endpoint(&self) -> &ProviderEndpoint;

    /// The account that owns the token.
    fn current_user(&self) -> impl Future<Output = Result<ProviderUser, ProviderError>> + Send;

    /// Email addresses of the account with their verification state (Commit Guard input).
    /// GitHub needs the `user:email` (or `user`) scope.
    fn emails(&self) -> impl Future<Output = Result<Vec<ProviderEmail>, ProviderError>> + Send;

    /// One page (1-based) of every repository the account can access: owned, collaborator,
    /// organization/group member and shared ones, most recently updated first.
    fn list_repositories_page(
        &self,
        page: u32,
        per_page: u8,
    ) -> impl Future<Output = Result<RepoPage, ProviderError>> + Send;

    /// Provider-wide search (public repositories plus private ones the account can see).
    /// Filtering the account's own repositories is done on the cached [`list_all_repositories`]
    /// result instead.
    fn search_repositories(
        &self,
        query: &str,
        page: u32,
        per_page: u8,
    ) -> impl Future<Output = Result<RepoPage, ProviderError>> + Send;

    /// A single repository, including the fork parent (which listings do not always carry).
    fn repository(
        &self,
        full_name: &str,
    ) -> impl Future<Output = Result<Repository, ProviderError>> + Send;

    /// What the account may do in `full_name` (Push Guard). A private repository the account
    /// cannot see is reported as [`ProviderError::NotFound`] by both providers.
    fn repo_permission(
        &self,
        full_name: &str,
    ) -> impl Future<Output = Result<RepoPermission, ProviderError>> + Send {
        async move { Ok(self.repository(full_name).await?.permissions) }
    }

    /// SSH public keys registered on the account.
    fn list_ssh_keys(
        &self,
    ) -> impl Future<Output = Result<Vec<SshPublicKey>, ProviderError>> + Send;

    /// Registers an OpenSSH public key (`ssh-ed25519 AAAA... comment`). GitHub needs
    /// `admin:public_key`/`write:public_key`, GitLab needs `api`. A key that is already
    /// registered yields [`ProviderError::AlreadyExists`].
    fn add_ssh_key(
        &self,
        title: &str,
        public_key: &str,
    ) -> impl Future<Output = Result<SshPublicKey, ProviderError>> + Send;

    /// Server version: GitLab `/version`, GitHub Enterprise `/meta`. `None` for github.com.
    fn server_version(
        &self,
    ) -> impl Future<Output = Result<Option<ServerVersion>, ProviderError>> + Send;
}

/// A GitHub or GitLab client chosen from the endpoint kind. This is what the app stores per
/// account; it implements [`Provider`] by delegating.
#[derive(Debug, Clone)]
pub enum ProviderClient {
    Github(GithubClient),
    Gitlab(GitlabClient),
}

impl ProviderClient {
    /// Builds the right client for `endpoint.kind()`. `http` should come from
    /// [`build_http_client`] and be shared by all accounts.
    pub fn new(http: reqwest::Client, endpoint: ProviderEndpoint, token: SecretToken) -> Self {
        let api = ApiClient::new(http, endpoint, token);
        match api.endpoint().kind().family() {
            ProviderFamily::Github => Self::Github(GithubClient::from_api(api)),
            ProviderFamily::Gitlab => Self::Gitlab(GitlabClient::from_api(api)),
        }
    }

    /// The underlying authenticated API client (for endpoints this crate does not wrap).
    pub fn api(&self) -> &ApiClient {
        match self {
            Self::Github(c) => c.api(),
            Self::Gitlab(c) => c.api(),
        }
    }

    /// Permission check for a git remote URL (`https://`, `ssh://` or `git@host:path`).
    /// Fails with [`ProviderError::HostMismatch`] when the remote is on another host.
    pub async fn repo_permission_for_remote(
        &self,
        remote_url: &str,
    ) -> Result<RepoPermission, ProviderError> {
        let remote = parse_remote_url(remote_url).ok_or_else(|| ProviderError::InvalidInput {
            message: format!("not a recognised git remote URL: {remote_url}"),
        })?;
        // Compare host names only: SSH remotes (port 22 / custom) and HTTPS (443 / 8443) of the
        // same instance differ in port.
        let expected = self.endpoint().web_host();
        let host_only = |h: &str| {
            let h = match h.split_once(']') {
                Some((v6, _)) => v6, // `[::1]:8443` → `[::1`
                None => h.rsplit_once(':').map_or(h, |(h, _)| h),
            };
            h.to_ascii_lowercase()
        };
        if host_only(&remote.host) != host_only(&expected) {
            return Err(ProviderError::HostMismatch {
                expected,
                actual: remote.host,
            });
        }
        self.repo_permission(&remote.path).await
    }
}

macro_rules! delegate {
    ($self:ident, $c:ident => $e:expr) => {
        match $self {
            ProviderClient::Github($c) => $e,
            ProviderClient::Gitlab($c) => $e,
        }
    };
}

impl Provider for ProviderClient {
    fn kind(&self) -> ProviderKind {
        delegate!(self, c => c.kind())
    }
    fn endpoint(&self) -> &ProviderEndpoint {
        delegate!(self, c => c.endpoint())
    }
    async fn current_user(&self) -> Result<ProviderUser, ProviderError> {
        delegate!(self, c => c.current_user().await)
    }
    async fn emails(&self) -> Result<Vec<ProviderEmail>, ProviderError> {
        delegate!(self, c => c.emails().await)
    }
    async fn list_repositories_page(
        &self,
        page: u32,
        per_page: u8,
    ) -> Result<RepoPage, ProviderError> {
        delegate!(self, c => c.list_repositories_page(page, per_page).await)
    }
    async fn search_repositories(
        &self,
        query: &str,
        page: u32,
        per_page: u8,
    ) -> Result<RepoPage, ProviderError> {
        delegate!(self, c => c.search_repositories(query, page, per_page).await)
    }
    async fn repository(&self, full_name: &str) -> Result<Repository, ProviderError> {
        delegate!(self, c => c.repository(full_name).await)
    }
    async fn repo_permission(&self, full_name: &str) -> Result<RepoPermission, ProviderError> {
        delegate!(self, c => c.repo_permission(full_name).await)
    }
    async fn list_ssh_keys(&self) -> Result<Vec<SshPublicKey>, ProviderError> {
        delegate!(self, c => c.list_ssh_keys().await)
    }
    async fn add_ssh_key(
        &self,
        title: &str,
        public_key: &str,
    ) -> Result<SshPublicKey, ProviderError> {
        delegate!(self, c => c.add_ssh_key(title, public_key).await)
    }
    async fn server_version(&self) -> Result<Option<ServerVersion>, ProviderError> {
        delegate!(self, c => c.server_version().await)
    }
}

/// Walks every page of [`Provider::list_repositories_page`], calling `on_page` after each page
/// (for incremental UI updates) and stopping early on `cancel` ([`ProviderError::Cancelled`]) or
/// after `opts.max_pages` (`truncated = true`).
pub async fn list_all_repositories<P, F>(
    provider: &P,
    opts: &ListOptions,
    cancel: &CancellationToken,
    mut on_page: F,
) -> Result<RepoListing, ProviderError>
where
    P: Provider,
    F: FnMut(&[Repository]) + Send,
{
    let per_page = opts.per_page.clamp(1, 100);
    let mut repos = Vec::new();
    let mut page = 1u32;
    let mut fetched = 0u32;
    loop {
        if fetched >= opts.max_pages.max(1) {
            return Ok(RepoListing {
                repos,
                truncated: true,
            });
        }
        let result = cancellable(cancel, provider.list_repositories_page(page, per_page)).await?;
        fetched += 1;
        on_page(&result.repos);
        repos.extend(result.repos);
        match result.next_page {
            Some(next) if next > page => page = next,
            _ => {
                return Ok(RepoListing {
                    repos,
                    truncated: false,
                });
            }
        }
    }
}
