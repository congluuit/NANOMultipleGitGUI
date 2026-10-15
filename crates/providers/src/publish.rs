//! Publishing a local repository: create an empty repository / project on the provider so a
//! freshly initialised repository can be pushed for the first time.
//!
//! - GitHub: `POST /user/repos` for the account itself, `POST /orgs/{org}/repos` for an
//!   organization (`name`, `description`, `private`, `auto_init: false`).
//! - GitLab: `POST /projects` (`name`, `path`, `namespace_id`, `visibility`, `description`,
//!   `initialize_with_readme: false`); a group path is resolved with `GET /namespaces/:path`.
//!
//! The repository is always created empty so the first push of the local history is a plain
//! fast-forward. Destinations are listed with [`ProviderClient::fork_namespaces`].

use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::error::ProviderError;
use crate::github::GithubClient;
use crate::gitlab::GitlabClient;
use crate::{Provider, ProviderClient};

/// Options of [`ProviderClient::create_repository`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct NewRepoOptions {
    /// Repository name (GitHub) / project path (GitLab).
    pub name: String,
    /// Organization (GitHub) / group path (GitLab); `None` = the account itself.
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub private: bool,
}

/// A usable repository name: letters, digits, `.`, `_`, `-`; not `.` / `..`, at most 100.
pub fn valid_repo_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && name != "."
        && name != ".."
        && !name.starts_with('-')
        && !name.ends_with(".git")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

impl ProviderClient {
    /// Creates an empty repository and returns its full name (`owner/repo`, `group/project`).
    pub async fn create_repository(&self, opts: &NewRepoOptions) -> Result<String, ProviderError> {
        let name = opts.name.trim();
        if !valid_repo_name(name) {
            return Err(ProviderError::InvalidInput {
                message: format!(
                    "`{name}` is not a valid repository name (letters, digits, `.`, `_`, `-`)"
                ),
            });
        }
        let namespace = opts
            .namespace
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty());
        let description = opts
            .description
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty());
        match self {
            Self::Github(c) => {
                c.create_repository(name, namespace, description, opts.private)
                    .await
            }
            Self::Gitlab(c) => {
                c.create_repository(name, namespace, description, opts.private)
                    .await
            }
        }
    }
}

#[derive(Deserialize)]
struct RawFullName {
    full_name: String,
}

impl GithubClient {
    async fn create_repository(
        &self,
        name: &str,
        namespace: Option<&str>,
        description: Option<&str>,
        private: bool,
    ) -> Result<String, ProviderError> {
        let mut body = serde_json::json!({ "name": name, "private": private, "auto_init": false });
        if let Some(d) = description {
            body["description"] = d.into();
        }
        let org = match namespace {
            Some(ns) => {
                let me = self.current_user().await?.username;
                (!ns.eq_ignore_ascii_case(&me)).then(|| ns.to_owned())
            }
            None => None,
        };
        let resp = match &org {
            Some(org) => {
                self.api()
                    .send_json::<RawFullName, _>(Method::POST, &["orgs", org, "repos"], &body)
                    .await?
            }
            None => {
                self.api()
                    .send_json::<RawFullName, _>(Method::POST, &["user", "repos"], &body)
                    .await?
            }
        };
        Ok(resp.body.full_name)
    }
}

#[derive(Deserialize)]
struct RawNamespace {
    id: u64,
    kind: Option<String>,
}

#[derive(Deserialize)]
struct RawProjectPath {
    path_with_namespace: String,
}

impl GitlabClient {
    async fn create_repository(
        &self,
        name: &str,
        namespace: Option<&str>,
        description: Option<&str>,
        private: bool,
    ) -> Result<String, ProviderError> {
        let mut body = serde_json::json!({
            "name": name,
            "path": name,
            "visibility": if private { "private" } else { "public" },
            "initialize_with_readme": false,
        });
        if let Some(d) = description {
            body["description"] = d.into();
        }
        if let Some(ns) = namespace {
            let raw = self
                .api()
                .get_json::<RawNamespace>(&["namespaces", ns.trim_matches('/')], &[])
                .await?
                .body;
            // The user's own namespace is the default; only groups need the id.
            if raw.kind.as_deref() != Some("user") {
                body["namespace_id"] = raw.id.into();
            }
        }
        let resp = self
            .api()
            .send_json::<RawProjectPath, _>(Method::POST, &["projects"], &body)
            .await?;
        Ok(resp.body.path_with_namespace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_names() {
        for ok in ["app", "my-app", "my_app.v2", "A1"] {
            assert!(valid_repo_name(ok), "{ok}");
        }
        for bad in ["", ".", "..", "-x", "a b", "a/b", "x.git", "tiếng"] {
            assert!(!valid_repo_name(bad), "{bad:?}");
        }
        assert!(!valid_repo_name(&"a".repeat(101)));
    }
}
