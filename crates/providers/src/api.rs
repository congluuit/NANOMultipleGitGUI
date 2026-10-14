//! Shared HTTP client, authenticated API calls, pagination headers and cancellation.

use std::future::Future;
use std::time::Duration;

use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue};
use reqwest::{Method, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::endpoint::{ProviderEndpoint, ProviderFamily};
use crate::error::{ProviderError, map_error_response};
use crate::secret::SecretToken;

/// `User-Agent` sent on every request (GitHub rejects requests without one).
pub const USER_AGENT: &str = concat!("NANOMultipleGitGUI/", env!("CARGO_PKG_VERSION"));

/// Client-wide settings.
#[derive(Debug, Clone)]
pub struct HttpConfig {
    /// Whole-request timeout (connect + send + read body).
    pub timeout: Duration,
    pub connect_timeout: Duration,
    pub user_agent: String,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(10),
            user_agent: USER_AGENT.to_owned(),
        }
    }
}

/// Builds the `reqwest::Client` shared by all providers and the auth flows.
///
/// TLS is rustls with the `ring` provider and the OS certificate store
/// (`rustls-platform-verifier`), so corporate root CAs installed in Windows/macOS work.
/// Installs `ring` as the process-wide rustls provider if none is installed yet.
/// Redirects are followed (max 5); reqwest drops `Authorization` on cross-host redirects.
pub fn build_http_client(config: &HttpConfig) -> Result<reqwest::Client, ProviderError> {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        // Err only means another thread installed one first — fine either way.
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    reqwest::Client::builder()
        .user_agent(config.user_agent.clone())
        .timeout(config.timeout)
        .connect_timeout(config.connect_timeout)
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| ProviderError::Network {
            message: format!("cannot initialise HTTP client: {e}"),
        })
}

/// Races `fut` against `cancel`; dropping the inner future aborts the HTTP request.
pub async fn cancellable<T, F>(cancel: &CancellationToken, fut: F) -> Result<T, ProviderError>
where
    F: Future<Output = Result<T, ProviderError>>,
{
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(ProviderError::Cancelled),
        r = fut => r,
    }
}

/// A decoded JSON body plus the response metadata pagination/token checks need.
#[derive(Debug)]
pub struct ApiResponse<T> {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: T,
}

impl<T> ApiResponse<T> {
    /// Next page number from `X-Next-Page` (GitLab) or `Link: <...page=N>; rel="next"` (GitHub).
    pub fn next_page(&self) -> Option<u32> {
        next_page(&self.headers)
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
    }
}

/// An authenticated client for one account on one provider instance. Cheap to clone.
#[derive(Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    endpoint: ProviderEndpoint,
    token: SecretToken,
}

impl std::fmt::Debug for ApiClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiClient")
            .field("endpoint", &self.endpoint)
            .field("token", &self.token)
            .finish_non_exhaustive()
    }
}

impl ApiClient {
    pub fn new(http: reqwest::Client, endpoint: ProviderEndpoint, token: SecretToken) -> Self {
        Self {
            http,
            endpoint,
            token,
        }
    }

    pub fn endpoint(&self) -> &ProviderEndpoint {
        &self.endpoint
    }

    pub fn family(&self) -> ProviderFamily {
        self.endpoint.kind().family()
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// API URL from path segments plus query pairs.
    pub fn url(&self, segments: &[&str], query: &[(&str, &str)]) -> Url {
        let mut url = self.endpoint.api(segments);
        if !query.is_empty() {
            let mut q = url.query_pairs_mut();
            for (k, v) in query {
                q.append_pair(k, v);
            }
        }
        url
    }

    /// GET `segments?query` and decode JSON.
    pub async fn get_json<T: DeserializeOwned>(
        &self,
        segments: &[&str],
        query: &[(&str, &str)],
    ) -> Result<ApiResponse<T>, ProviderError> {
        let url = self.url(segments, query);
        self.send::<T, ()>(Method::GET, url, None).await
    }

    /// Sends a JSON body (POST/PUT/PATCH) and decodes the JSON answer.
    pub async fn send_json<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        method: Method,
        segments: &[&str],
        body: &B,
    ) -> Result<ApiResponse<T>, ProviderError> {
        let url = self.url(segments, &[]);
        self.send(method, url, Some(body)).await
    }

    /// Low-level request to `url`, which must live under the endpoint's API or web base.
    pub async fn send<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        method: Method,
        url: Url,
        body: Option<&B>,
    ) -> Result<ApiResponse<T>, ProviderError> {
        if !self.is_own_url(&url) {
            return Err(ProviderError::InvalidInput {
                message: "refusing to send the token outside the provider base URL".into(),
            });
        }
        let mut req = self
            .http
            .request(method, url)
            .header(AUTHORIZATION, self.auth_header()?);
        req = match self.family() {
            ProviderFamily::Github => req
                .header(ACCEPT, "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28"),
            ProviderFamily::Gitlab => req.header(ACCEPT, "application/json"),
        };
        if let Some(body) = body {
            req = req.json(body);
        }
        let resp = req.send().await?;
        let status = resp.status();
        let headers = resp.headers().clone();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(map_error_response(self.family(), status, &headers, &text));
        }
        let body = serde_json::from_str(if text.trim().is_empty() {
            "null"
        } else {
            &text
        })
        .map_err(ProviderError::invalid_response)?;
        Ok(ApiResponse {
            status,
            headers,
            body,
        })
    }

    fn is_own_url(&self, url: &Url) -> bool {
        let under = |base: &Url| {
            url.scheme() == base.scheme()
                && url.host_str() == base.host_str()
                && url.port_or_known_default() == base.port_or_known_default()
                && url.path().starts_with(base.path())
        };
        under(self.endpoint.api_url()) || under(self.endpoint.base_url())
    }

    fn auth_header(&self) -> Result<HeaderValue, ProviderError> {
        if self.token.is_empty() {
            return Err(ProviderError::InvalidInput {
                message: "empty access token".into(),
            });
        }
        let mut value = HeaderValue::from_str(&format!("Bearer {}", self.token.expose().trim()))
            .map_err(|_| ProviderError::InvalidInput {
                message: "the access token contains invalid characters".into(),
            })?;
        value.set_sensitive(true);
        Ok(value)
    }
}

/// See [`ApiResponse::next_page`].
pub fn next_page(headers: &HeaderMap) -> Option<u32> {
    if let Some(v) = headers.get("x-next-page").and_then(|v| v.to_str().ok()) {
        return v.trim().parse().ok();
    }
    let link = headers.get("link")?.to_str().ok()?;
    link.split(',').find_map(|part| {
        let (target, params) = part.split_once(';')?;
        if !params
            .split(';')
            .any(|p| matches!(p.trim(), "rel=\"next\"" | "rel=next"))
        {
            return None;
        }
        let target = target.trim().trim_start_matches('<').trim_end_matches('>');
        let url = Url::parse(target).ok()?;
        url.query_pairs()
            .find(|(k, _)| k == "page")
            .and_then(|(_, v)| v.parse().ok())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pagination_headers() {
        let mut h = HeaderMap::new();
        h.insert(
            "link",
            HeaderValue::from_static(
                r#"<https://api.github.com/user/repos?per_page=100&page=3>; rel="next", <https://api.github.com/user/repos?per_page=100&page=9>; rel="last""#,
            ),
        );
        assert_eq!(next_page(&h), Some(3));
        let mut h = HeaderMap::new();
        h.insert(
            "link",
            HeaderValue::from_static(r#"<https://api.github.com/x?page=1>; rel="prev""#),
        );
        assert_eq!(next_page(&h), None);
        let mut h = HeaderMap::new();
        h.insert("x-next-page", HeaderValue::from_static(""));
        assert_eq!(next_page(&h), None);
        h.insert("x-next-page", HeaderValue::from_static("4"));
        assert_eq!(next_page(&h), Some(4));
    }

    #[test]
    fn refuses_foreign_urls_and_redacts_debug() {
        let http = build_http_client(&HttpConfig::default()).unwrap();
        let api = ApiClient::new(
            http,
            ProviderEndpoint::github(),
            SecretToken::new("ghp_secret"),
        );
        assert!(api.is_own_url(&Url::parse("https://api.github.com/user").unwrap()));
        assert!(!api.is_own_url(&Url::parse("https://evil.example/user").unwrap()));
        assert!(!api.is_own_url(&Url::parse("http://api.github.com/user").unwrap()));
        assert!(!format!("{api:?}").contains("ghp_secret"));
    }
}
