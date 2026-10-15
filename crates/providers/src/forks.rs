//! Forks (PLAN §4.7 "Fork & clone", Phase 4.3): where the account may fork to, an existing
//! fork of the account, creating a fork and waiting until it can be cloned.
//!
//! - GitHub: `POST /repos/{o}/{r}/forks` (`organization`, `name`, `default_branch_only`) answers
//!   `202 Accepted` at once and copies the git data in the background — and returns the
//!   existing fork when the account already has one. Readiness = the fork's commits endpoint
//!   answers (`409 Git Repository is empty` / `404` while copying).
//! - GitLab: `POST /projects/:id/fork` (`namespace_path`, `name`, `path`); readiness is the
//!   project's `import_status` (`finished` / `none`; `failed` carries `import_error`).

use std::time::{Duration, Instant};

use reqwest::Method;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::error::ProviderError;
use crate::github::GithubClient;
use crate::gitlab::GitlabClient;
use crate::model::Repository;
use crate::{Provider, ProviderClient};

/// Organization / group listings read at most this many pages of 100.
const MAX_NAMESPACE_PAGES: u32 = 3;

/// Kind of a fork destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ForkNamespaceKind {
    /// The account's own namespace.
    User,
    /// GitHub organization.
    Organization,
    /// GitLab group / subgroup.
    Group,
}

/// A place the account may fork into.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ForkNamespace {
    pub kind: ForkNamespaceKind,
    /// Login / org login / group full path — what [`ForkOptions::namespace`] takes.
    pub path: String,
    /// Display name when the provider has one.
    pub name: Option<String>,
    pub avatar_url: Option<String>,
}

/// Options of [`ProviderClient::create_fork`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ForkOptions {
    /// Organization (GitHub) / namespace path (GitLab); `None` = the account itself.
    #[serde(default)]
    pub namespace: Option<String>,
    /// New repository name; `None` = same as the source.
    #[serde(default)]
    pub name: Option<String>,
    /// GitHub only: copy just the default branch.
    #[serde(default)]
    pub default_branch_only: bool,
}

/// Whether a fork can be cloned yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum ForkReadiness {
    Ready,
    /// Still copying (or not visible yet).
    Pending,
    /// GitLab import failed.
    #[serde(rename_all = "camelCase")]
    Failed {
        message: String,
    },
}

/// Why a fork request failed, when the provider's message says it (for a helpful UI text).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ForkProblem {
    /// The organization / project does not allow forking (often: private repositories).
    ForkingDisabled,
    /// A repository with that name already exists in the destination.
    NameTaken,
    /// Forking your own repository into the same namespace.
    OwnRepository,
}

/// Recognises [`ForkProblem`]s in a provider error.
pub fn fork_problem(e: &ProviderError) -> Option<ForkProblem> {
    let message = match e {
        ProviderError::Forbidden { message, .. }
        | ProviderError::Rejected { message, .. }
        | ProviderError::AlreadyExists { message } => message.to_ascii_lowercase(),
        _ => return None,
    };
    if (message.contains("fork")
        && (message.contains("disabled") || message.contains("not allowed")))
        || message.contains("forking is")
    {
        Some(ForkProblem::ForkingDisabled)
    } else if message.contains("cannot fork") && message.contains("own") {
        Some(ForkProblem::OwnRepository)
    } else if matches!(e, ProviderError::AlreadyExists { .. })
        || message.contains("already been taken")
        || message.contains("name already exists")
    {
        Some(ForkProblem::NameTaken)
    } else {
        None
    }
}

fn not_found(e: &ProviderError) -> bool {
    matches!(e, ProviderError::NotFound { .. })
}

fn eq_path(a: &str, b: &str) -> bool {
    a.trim_matches('/')
        .eq_ignore_ascii_case(b.trim_matches('/'))
}

impl ProviderClient {
    /// The account itself first, then organizations / groups it may create projects in.
    pub async fn fork_namespaces(&self) -> Result<Vec<ForkNamespace>, ProviderError> {
        match self {
            Self::Github(c) => c.fork_namespaces().await,
            Self::Gitlab(c) => c.fork_namespaces().await,
        }
    }

    /// The account's existing fork of `source` in `namespace` (`None` = the account itself),
    /// so the UI can offer it instead of forking again.
    pub async fn find_fork(
        &self,
        source: &str,
        namespace: Option<&str>,
    ) -> Result<Option<Repository>, ProviderError> {
        let namespace = match namespace.map(str::trim).filter(|n| !n.is_empty()) {
            Some(n) => n.to_owned(),
            None => self.current_user().await?.username,
        };
        match self {
            Self::Github(c) => c.find_fork(source, &namespace).await,
            Self::Gitlab(c) => c.find_fork(source, &namespace).await,
        }
    }

    /// Starts the fork and returns its full name. GitHub may still be copying the data: use
    /// [`Self::wait_fork_ready`] before cloning.
    pub async fn create_fork(
        &self,
        source: &str,
        opts: &ForkOptions,
    ) -> Result<String, ProviderError> {
        match self {
            Self::Github(c) => c.create_fork(source, opts).await,
            Self::Gitlab(c) => c.create_fork(source, opts).await,
        }
    }

    /// One readiness check of a fork.
    pub async fn fork_readiness(&self, full_name: &str) -> Result<ForkReadiness, ProviderError> {
        match self {
            Self::Github(c) => c.fork_readiness(full_name).await,
            Self::Gitlab(c) => c.fork_readiness(full_name).await,
        }
    }

    /// Polls [`Self::fork_readiness`] every `interval` until ready, failed or `timeout`
    /// (`Ok(false)` = still copying). Stops with [`ProviderError::Cancelled`] on `cancel`.
    pub async fn wait_fork_ready(
        &self,
        full_name: &str,
        timeout: Duration,
        interval: Duration,
        cancel: &CancellationToken,
    ) -> Result<bool, ProviderError> {
        let deadline = Instant::now() + timeout;
        loop {
            match crate::cancellable(cancel, self.fork_readiness(full_name)).await? {
                ForkReadiness::Ready => return Ok(true),
                ForkReadiness::Failed { message } => {
                    return Err(ProviderError::Rejected {
                        status: 0,
                        message: format!("the fork could not be created: {message}"),
                    });
                }
                ForkReadiness::Pending => {}
            }
            if Instant::now() + interval > deadline {
                return Ok(false);
            }
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(ProviderError::Cancelled),
                _ = tokio::time::sleep(interval) => {}
            }
        }
    }
}

// ----- GitHub --------------------------------------------------------------------------------

#[derive(Deserialize)]
struct RawLogin {
    login: String,
    name: Option<String>,
    avatar_url: Option<String>,
}

#[derive(Deserialize)]
struct RawFullName {
    full_name: String,
}

fn github_segments(full_name: &str) -> Result<(String, String), ProviderError> {
    match full_name.trim_matches('/').split_once('/') {
        Some((o, r)) if !o.is_empty() && !r.is_empty() && !r.contains('/') => {
            Ok((o.to_owned(), r.strip_suffix(".git").unwrap_or(r).to_owned()))
        }
        _ => Err(ProviderError::InvalidInput {
            message: format!("`{full_name}` is not an owner/repo name"),
        }),
    }
}

impl GithubClient {
    async fn fork_namespaces(&self) -> Result<Vec<ForkNamespace>, ProviderError> {
        let me = self.api().get_json::<RawLogin>(&["user"], &[]).await?.body;
        let mut out = vec![ForkNamespace {
            kind: ForkNamespaceKind::User,
            path: me.login,
            name: me.name.filter(|n| !n.trim().is_empty()),
            avatar_url: me.avatar_url,
        }];
        let mut page = 1u32;
        loop {
            let page_s = page.to_string();
            let resp = self
                .api()
                .get_json::<Vec<RawLogin>>(
                    &["user", "orgs"],
                    &[("per_page", "100"), ("page", &page_s)],
                )
                .await?;
            let next = resp.next_page();
            out.extend(resp.body.into_iter().map(|o| ForkNamespace {
                kind: ForkNamespaceKind::Organization,
                path: o.login,
                name: o.name.filter(|n| !n.trim().is_empty()),
                avatar_url: o.avatar_url,
            }));
            match next {
                Some(n) if n > page && page < MAX_NAMESPACE_PAGES => page = n,
                _ => return Ok(out),
            }
        }
    }

    async fn find_fork(
        &self,
        source: &str,
        namespace: &str,
    ) -> Result<Option<Repository>, ProviderError> {
        let (_, name) = github_segments(source)?;
        let candidate = format!("{namespace}/{name}");
        match self.repository(&candidate).await {
            Ok(repo)
                if repo.fork
                    && repo
                        .parent_full_name
                        .as_deref()
                        .is_some_and(|p| eq_path(p, source)) =>
            {
                Ok(Some(repo))
            }
            Ok(_) => Ok(None),
            Err(e) if not_found(&e) => Ok(None),
            Err(e) => Err(e),
        }
    }

    async fn create_fork(&self, source: &str, opts: &ForkOptions) -> Result<String, ProviderError> {
        let (owner, repo) = github_segments(source)?;
        let mut body = serde_json::json!({ "default_branch_only": opts.default_branch_only });
        if let Some(org) = opts.namespace.as_deref().filter(|o| !o.trim().is_empty()) {
            // `organization` must be omitted for the account itself.
            let me = self
                .api()
                .get_json::<RawLogin>(&["user"], &[])
                .await?
                .body
                .login;
            if !org.eq_ignore_ascii_case(&me) {
                body["organization"] = org.trim().into();
            }
        }
        if let Some(name) = opts.name.as_deref().filter(|n| !n.trim().is_empty()) {
            body["name"] = name.trim().into();
        }
        let resp = self
            .api()
            .send_json::<RawFullName, _>(Method::POST, &["repos", &owner, &repo, "forks"], &body)
            .await?;
        Ok(resp.body.full_name)
    }

    async fn fork_readiness(&self, full_name: &str) -> Result<ForkReadiness, ProviderError> {
        let (owner, repo) = github_segments(full_name)?;
        match self
            .api()
            .get_json::<serde_json::Value>(
                &["repos", &owner, &repo, "commits"],
                &[("per_page", "1")],
            )
            .await
        {
            Ok(_) => Ok(ForkReadiness::Ready),
            // 409 "Git Repository is empty." while the copy runs; 404 before it is visible.
            Err(ProviderError::Rejected { status: 409, .. }) => Ok(ForkReadiness::Pending),
            Err(e) if not_found(&e) => Ok(ForkReadiness::Pending),
            Err(e) => Err(e),
        }
    }
}

// ----- GitLab --------------------------------------------------------------------------------

#[derive(Deserialize)]
struct RawGitlabUser {
    username: String,
    name: Option<String>,
    avatar_url: Option<String>,
}

#[derive(Deserialize)]
struct RawGroup {
    full_path: String,
    full_name: Option<String>,
    avatar_url: Option<String>,
}

#[derive(Deserialize)]
struct RawNamespaceRef {
    full_path: String,
}

#[derive(Deserialize)]
struct RawForkProject {
    path_with_namespace: String,
    namespace: Option<RawNamespaceRef>,
    import_status: Option<String>,
    import_error: Option<String>,
}

fn gitlab_path(full_name: &str) -> Result<&str, ProviderError> {
    let path = full_name.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    if path.contains('/') {
        Ok(path)
    } else {
        Err(ProviderError::InvalidInput {
            message: format!("`{full_name}` is not a group/project path"),
        })
    }
}

impl GitlabClient {
    async fn fork_namespaces(&self) -> Result<Vec<ForkNamespace>, ProviderError> {
        let me = self
            .api()
            .get_json::<RawGitlabUser>(&["user"], &[])
            .await?
            .body;
        let mut out = vec![ForkNamespace {
            kind: ForkNamespaceKind::User,
            path: me.username,
            name: me.name.filter(|n| !n.trim().is_empty()),
            avatar_url: me.avatar_url,
        }];
        let mut page = 1u32;
        loop {
            let page_s = page.to_string();
            // Developer+ (30): the default role that may create projects in a group.
            let resp = self
                .api()
                .get_json::<Vec<RawGroup>>(
                    &["groups"],
                    &[
                        ("min_access_level", "30"),
                        ("order_by", "path"),
                        ("per_page", "100"),
                        ("page", &page_s),
                    ],
                )
                .await?;
            let next = resp.next_page();
            out.extend(resp.body.into_iter().map(|g| ForkNamespace {
                kind: ForkNamespaceKind::Group,
                path: g.full_path,
                name: g.full_name.filter(|n| !n.trim().is_empty()),
                avatar_url: g.avatar_url,
            }));
            match next {
                Some(n) if n > page && page < MAX_NAMESPACE_PAGES => page = n,
                _ => return Ok(out),
            }
        }
    }

    async fn find_fork(
        &self,
        source: &str,
        namespace: &str,
    ) -> Result<Option<Repository>, ProviderError> {
        let path = gitlab_path(source)?;
        let forks = self
            .api()
            .get_json::<Vec<RawForkProject>>(
                &["projects", path, "forks"],
                &[
                    ("min_access_level", "30"),
                    ("simple", "true"),
                    ("per_page", "100"),
                ],
            )
            .await?
            .body;
        let found = forks.into_iter().find(|f| {
            let ns = f.namespace.as_ref().map_or_else(
                || {
                    f.path_with_namespace
                        .rsplit_once('/')
                        .map_or("", |(o, _)| o)
                        .to_owned()
                },
                |n| n.full_path.clone(),
            );
            eq_path(&ns, namespace)
        });
        match found {
            Some(f) => Ok(Some(self.repository(&f.path_with_namespace).await?)),
            None => Ok(None),
        }
    }

    async fn create_fork(&self, source: &str, opts: &ForkOptions) -> Result<String, ProviderError> {
        let path = gitlab_path(source)?;
        let mut body = serde_json::json!({});
        if let Some(ns) = opts.namespace.as_deref().filter(|n| !n.trim().is_empty()) {
            body["namespace_path"] = ns.trim().into();
        }
        if let Some(name) = opts.name.as_deref().filter(|n| !n.trim().is_empty()) {
            body["name"] = name.trim().into();
            body["path"] = name.trim().into();
        }
        let resp = self
            .api()
            .send_json::<RawForkProject, _>(Method::POST, &["projects", path, "fork"], &body)
            .await?;
        Ok(resp.body.path_with_namespace)
    }

    async fn fork_readiness(&self, full_name: &str) -> Result<ForkReadiness, ProviderError> {
        let path = gitlab_path(full_name)?;
        match self
            .api()
            .get_json::<RawForkProject>(&["projects", path], &[])
            .await
        {
            Ok(resp) => Ok(match resp.body.import_status.as_deref() {
                None | Some("none" | "finished") => ForkReadiness::Ready,
                Some("failed") => ForkReadiness::Failed {
                    message: resp
                        .body
                        .import_error
                        .unwrap_or_else(|| "import failed".into()),
                },
                Some(_) => ForkReadiness::Pending,
            }),
            Err(e) if not_found(&e) => Ok(ForkReadiness::Pending),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_fork_problems() {
        let forbidden = |m: &str| ProviderError::Forbidden {
            message: m.into(),
            hint: None,
        };
        assert_eq!(
            fork_problem(&forbidden(
                "Repository forking is disabled for this organization"
            )),
            Some(ForkProblem::ForkingDisabled)
        );
        assert_eq!(
            fork_problem(&forbidden("Forking is not allowed for this project")),
            Some(ForkProblem::ForkingDisabled)
        );
        assert_eq!(
            fork_problem(&ProviderError::AlreadyExists {
                message: "name has already been taken".into()
            }),
            Some(ForkProblem::NameTaken)
        );
        assert_eq!(fork_problem(&forbidden("Must have admin rights")), None);
        assert_eq!(fork_problem(&ProviderError::Timeout), None);
    }

    #[test]
    fn names() {
        assert!(eq_path("Acme/API", "acme/api/"));
    }
}
