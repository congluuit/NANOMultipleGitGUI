//! Where a provider lives: web base URL (OAuth pages, git remotes) and REST API base URL.

use serde::{Deserialize, Serialize};
use url::Url;

use crate::ProviderError;

/// Provider kinds as stored in the `Provider` table (PLAN 4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Github,
    GithubEnterprise,
    Gitlab,
    GitlabSelfHosted,
}

/// GitHub-flavoured or GitLab-flavoured API, regardless of hosting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum ProviderFamily {
    Github,
    Gitlab,
}

impl ProviderKind {
    pub fn family(self) -> ProviderFamily {
        match self {
            Self::Github | Self::GithubEnterprise => ProviderFamily::Github,
            Self::Gitlab | Self::GitlabSelfHosted => ProviderFamily::Gitlab,
        }
    }
}

/// Base URLs of one provider instance.
///
/// Invariants: both URLs are `http(s)`, have no query/fragment/credentials, and their path ends
/// with `/` so relative joins never drop the last segment (GitLab under a sub-path works).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderEndpoint {
    kind: ProviderKind,
    base_url: Url,
    api_url: Url,
}

impl ProviderEndpoint {
    /// github.com (`https://github.com`, API `https://api.github.com`).
    pub fn github() -> Self {
        Self {
            kind: ProviderKind::Github,
            base_url: Url::parse("https://github.com/").expect("static URL"),
            api_url: Url::parse("https://api.github.com/").expect("static URL"),
        }
    }

    /// gitlab.com (API `https://gitlab.com/api/v4`).
    pub fn gitlab() -> Self {
        Self {
            kind: ProviderKind::Gitlab,
            base_url: Url::parse("https://gitlab.com/").expect("static URL"),
            api_url: Url::parse("https://gitlab.com/api/v4/").expect("static URL"),
        }
    }

    /// GitHub Enterprise Server at `base_url` (API under `/api/v3`).
    pub fn github_enterprise(base_url: &str) -> Result<Self, ProviderError> {
        let base = normalize(base_url)?;
        let api = join(&base, "api/v3/")?;
        Ok(Self {
            kind: ProviderKind::GithubEnterprise,
            base_url: base,
            api_url: api,
        })
    }

    /// Self-hosted GitLab at `base_url` (API under `/api/v4`). Sub-path installs such as
    /// `https://example.com/gitlab` are supported.
    pub fn gitlab_self_hosted(base_url: &str) -> Result<Self, ProviderError> {
        let base = normalize(base_url)?;
        let api = join(&base, "api/v4/")?;
        Ok(Self {
            kind: ProviderKind::GitlabSelfHosted,
            base_url: base,
            api_url: api,
        })
    }

    /// Builds the endpoint for `kind`; `base_url` is ignored for github.com / gitlab.com when
    /// `None`. Convenient when loading provider rows from storage.
    pub fn for_kind(kind: ProviderKind, base_url: Option<&str>) -> Result<Self, ProviderError> {
        match (kind, base_url) {
            (ProviderKind::Github, _) => Ok(Self::github()),
            (ProviderKind::Gitlab, _) => Ok(Self::gitlab()),
            (ProviderKind::GithubEnterprise, Some(b)) => Self::github_enterprise(b),
            (ProviderKind::GitlabSelfHosted, Some(b)) => Self::gitlab_self_hosted(b),
            (_, None) => Err(ProviderError::InvalidInput {
                message: "a base URL is required for self-hosted providers".into(),
            }),
        }
    }

    /// Fully custom URLs (tests, unusual reverse-proxy setups).
    pub fn custom(
        kind: ProviderKind,
        base_url: &str,
        api_url: &str,
    ) -> Result<Self, ProviderError> {
        Ok(Self {
            kind,
            base_url: normalize(base_url)?,
            api_url: normalize(api_url)?,
        })
    }

    pub fn kind(&self) -> ProviderKind {
        self.kind
    }

    /// Web base URL, always ending with `/`.
    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    /// REST API base URL, always ending with `/`.
    pub fn api_url(&self) -> &Url {
        &self.api_url
    }

    /// Host of the web UI and git remotes (e.g. `github.com`). Includes a non-default port.
    pub fn web_host(&self) -> String {
        let host = self
            .base_url
            .host_str()
            .unwrap_or_default()
            .to_ascii_lowercase();
        match self.base_url.port() {
            Some(p) => format!("{host}:{p}"),
            None => host,
        }
    }

    /// True when the instance is reached over plain HTTP (the UI should warn).
    pub fn is_insecure(&self) -> bool {
        self.base_url.scheme() == "http" || self.api_url.scheme() == "http"
    }

    /// `base_url` + relative `path` (e.g. `login/device/code`, `oauth/token`).
    pub fn web(&self, path: &str) -> Result<Url, ProviderError> {
        join(&self.base_url, path)
    }

    /// API URL made of percent-encoded path `segments` (a segment may contain `/`, which is
    /// encoded as `%2F` — what GitLab expects for `projects/group%2Fproject`).
    pub fn api(&self, segments: &[&str]) -> Url {
        let mut url = self.api_url.clone();
        if let Ok(mut path) = url.path_segments_mut() {
            path.pop_if_empty();
            path.extend(segments);
        }
        url
    }
}

fn normalize(input: &str) -> Result<Url, ProviderError> {
    let invalid = |why: &str| ProviderError::InvalidInput {
        message: format!("invalid provider URL `{input}`: {why}"),
    };
    let trimmed = input.trim();
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("https://{trimmed}")
    };
    let mut url = Url::parse(&with_scheme).map_err(|e| invalid(&e.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(invalid("only http(s) is supported"));
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err(invalid("missing host"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid("credentials must not be part of the URL"));
    }
    url.set_query(None);
    url.set_fragment(None);
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
}

fn join(base: &Url, path: &str) -> Result<Url, ProviderError> {
    base.join(path.trim_start_matches('/'))
        .map_err(|e| ProviderError::InvalidInput {
            message: format!("cannot build URL from `{path}`: {e}"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_hosted_urls_are_normalized() {
        let e = ProviderEndpoint::gitlab_self_hosted("git.acme.vn/gitlab").unwrap();
        assert_eq!(e.base_url().as_str(), "https://git.acme.vn/gitlab/");
        assert_eq!(e.api_url().as_str(), "https://git.acme.vn/gitlab/api/v4/");
        assert_eq!(
            e.api(&["projects", "grp/sub/proj"]).as_str(),
            "https://git.acme.vn/gitlab/api/v4/projects/grp%2Fsub%2Fproj"
        );
        let g = ProviderEndpoint::github_enterprise("https://ghe.acme.com:8443?x=1").unwrap();
        assert_eq!(g.api_url().as_str(), "https://ghe.acme.com:8443/api/v3/");
        assert_eq!(g.web_host(), "ghe.acme.com:8443");
        assert_eq!(
            g.web("login/device/code").unwrap().as_str(),
            "https://ghe.acme.com:8443/login/device/code"
        );
        assert!(ProviderEndpoint::gitlab_self_hosted("ftp://x").is_err());
        assert!(ProviderEndpoint::gitlab_self_hosted("https://u:p@x").is_err());
        assert!(
            ProviderEndpoint::gitlab_self_hosted("http://x")
                .unwrap()
                .is_insecure()
        );
    }

    #[test]
    fn well_known_endpoints() {
        assert_eq!(
            ProviderEndpoint::github().api(&["user"]).as_str(),
            "https://api.github.com/user"
        );
        assert_eq!(
            ProviderEndpoint::gitlab().api(&["user", "emails"]).as_str(),
            "https://gitlab.com/api/v4/user/emails"
        );
        assert_eq!(
            ProviderKind::GitlabSelfHosted.family(),
            ProviderFamily::Gitlab
        );
    }
}
